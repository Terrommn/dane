//! T29 Lecturas: CRUD, vista por estado y aporte explícito (una vez) a objetivos de libros. Propietario: agente A-core.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::text::{required, validate_external_url};
use persistence::repos::readings::{self as repo, Reading, ReadingInput};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::api::goals::sync_source_contribution;
use crate::api::habits::{today, user_ctx};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_readings, create_reading))
        .routes(routes!(get_reading, update_reading, delete_reading))
}

#[derive(Deserialize, IntoParams)]
pub struct ReadingQuery {
    /// `want | reading | finished | abandoned`
    pub status: Option<String>,
    pub q: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct ReadingView {
    #[serde(flatten)]
    pub reading: Reading,
    /// Objetivos a los que aporta (enlace explícito).
    pub goal_ids: Vec<Uuid>,
}

async fn goal_ids(s: &AppState, user_id: Uuid, id: Uuid) -> ApiResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar("SELECT dst_id FROM entity_links WHERE user_id = $1 AND src_type = 'reading' AND src_id = $2 AND dst_type = 'goal' AND relation = 'contributes_to'")
        .bind(user_id)
        .bind(id)
        .fetch_all(&s.pool)
        .await?)
}

/// Lecturas (tabla y tarjetas por estado).
#[utoipa::path(get, path = "/readings", tag = "readings", params(ReadingQuery), responses((status = 200, body = Vec<Reading>)))]
async fn list_readings(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ReadingQuery>,
) -> ApiResult<Json<Vec<Reading>>> {
    Ok(Json(
        repo::list(
            &s.pool,
            auth.user_id,
            q.status.as_deref(),
            q.q.as_deref().map(str::trim).filter(|x| !x.is_empty()),
            q.archived.unwrap_or(false),
        )
        .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct ReadingBody {
    /// `book | article`
    #[serde(default = "book")]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    pub url: Option<String>,
    #[serde(default = "want")]
    pub status: String,
    pub rating: Option<i16>,
    pub started_on: Option<NaiveDate>,
    pub finished_on: Option<NaiveDate>,
    #[serde(default)]
    pub ideas_md: String,
    #[serde(default)]
    pub quotes_md: String,
    #[serde(default)]
    pub application_md: String,
    #[serde(default)]
    pub archived: bool,
    /// Objetivos a los que aporta al terminarse (reemplaza los enlaces actuales si se envía).
    pub goal_ids: Option<Vec<Uuid>>,
    pub version: Option<i32>,
}

fn book() -> String {
    "book".into()
}
fn want() -> String {
    "want".into()
}

struct Valid {
    title: String,
    url: Option<String>,
    finished_on: Option<NaiveDate>,
}

fn validate(b: &ReadingBody, today: NaiveDate) -> ApiResult<Valid> {
    let title = required("title", &b.title, 300)?;
    if !["book", "article"].contains(&b.kind.as_str()) {
        return Err(ApiError::validation("kind", "book o article"));
    }
    if !["want", "reading", "finished", "abandoned"].contains(&b.status.as_str()) {
        return Err(ApiError::validation(
            "status",
            "want, reading, finished o abandoned",
        ));
    }
    if b.rating.is_some_and(|r| !(1..=5).contains(&r)) {
        return Err(ApiError::validation("rating", "de 1 a 5"));
    }
    let url = b
        .url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .map(|u| validate_external_url("url", u))
        .transpose()?;
    let finished_on = if b.status == "finished" {
        Some(b.finished_on.unwrap_or(today))
    } else {
        b.finished_on
    };
    if let (Some(s), Some(f)) = (b.started_on, finished_on)
        && f < s
    {
        return Err(ApiError::validation("finished_on", "termina antes de empezar"));
    }
    Ok(Valid {
        title,
        url,
        finished_on,
    })
}

fn input<'a>(b: &'a ReadingBody, v: &'a Valid) -> ReadingInput<'a> {
    ReadingInput {
        kind: &b.kind,
        title: &v.title,
        author: b.author.trim(),
        url: v.url.as_deref(),
        status: &b.status,
        rating: b.rating,
        started_on: b.started_on,
        finished_on: v.finished_on,
        ideas_md: &b.ideas_md,
        quotes_md: &b.quotes_md,
        application_md: &b.application_md,
        archived: b.archived,
    }
}

/// Reemplaza enlaces (si se envían) y sincroniza el aporte: terminada → +1 una sola vez; si deja de estarlo, se quita.
async fn sync_goals(
    tx: &mut persistence::Tx<'_>,
    user_id: Uuid,
    tz: &str,
    id: Uuid,
    wanted: Option<&[Uuid]>,
) -> ApiResult<()> {
    if let Some(goals) = wanted {
        let current = persistence::hooks::contributing_goals(tx, user_id, "reading", id).await?;
        for g in current.iter().filter(|g| !goals.contains(g)) {
            sqlx::query("DELETE FROM entity_links WHERE user_id = $1 AND src_type = 'reading' AND src_id = $2 AND dst_type = 'goal' AND dst_id = $3 AND relation = 'contributes_to'")
                .bind(user_id)
                .bind(id)
                .bind(g)
                .execute(&mut **tx)
                .await?;
            persistence::hooks::remove_goal_progress(tx, user_id, "reading", id, Some(*g)).await?;
        }
        for g in goals {
            let ok: Option<String> =
                sqlx::query_scalar("SELECT progress_type FROM goals WHERE user_id = $1 AND id = $2")
                    .bind(user_id)
                    .bind(g)
                    .fetch_optional(&mut **tx)
                    .await?;
            match ok.as_deref() {
                None => return Err(ApiError::validation("goal_ids", "objetivo inexistente")),
                Some("current_value") => {
                    return Err(ApiError::validation(
                        "goal_ids",
                        "una lectura no aporta a un objetivo de valor actual",
                    ));
                }
                _ => {}
            }
            sqlx::query("INSERT INTO entity_links (user_id, src_type, src_id, dst_type, dst_id, relation) VALUES ($1, 'reading', $2, 'goal', $3, 'contributes_to') ON CONFLICT DO NOTHING")
                .bind(user_id)
                .bind(id)
                .bind(g)
                .execute(&mut **tx)
                .await?;
        }
    }
    let goals = persistence::hooks::contributing_goals(tx, user_id, "reading", id).await?;
    sync_source_contribution(tx, user_id, tz, "reading", id, &goals).await
}

#[utoipa::path(post, path = "/readings", tag = "readings", request_body = ReadingBody, responses((status = 201, body = ReadingView)))]
async fn create_reading(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<ReadingBody>,
) -> ApiResult<(StatusCode, Json<ReadingView>)> {
    let (tz, settings) = user_ctx(&s.pool, auth.user_id).await?;
    let v = validate(&b, today(&s, tz))?;
    let mut tx = s.pool.begin().await?;
    let r = repo::create(&mut tx, auth.user_id, &input(&b, &v)).await?;
    sync_goals(
        &mut tx,
        auth.user_id,
        &settings.timezone,
        r.id,
        b.goal_ids.as_deref(),
    )
    .await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "reading",
        Some(r.id),
        "create",
        serde_json::json!({ "title": r.title }),
    )
    .await;
    let goal_ids = goal_ids(&s, auth.user_id, r.id).await?;
    Ok((StatusCode::CREATED, Json(ReadingView { reading: r, goal_ids })))
}

#[utoipa::path(get, path = "/readings/{id}", tag = "readings", responses((status = 200, body = ReadingView)))]
async fn get_reading(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ReadingView>> {
    let r = repo::get(&s.pool, auth.user_id, id).await?;
    Ok(Json(ReadingView {
        goal_ids: goal_ids(&s, auth.user_id, id).await?,
        reading: r,
    }))
}

#[utoipa::path(put, path = "/readings/{id}", tag = "readings", request_body = ReadingBody,
    responses((status = 200, body = ReadingView), (status = 409, body = crate::error::ErrorBody)))]
async fn update_reading(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ReadingBody>,
) -> ApiResult<Json<ReadingView>> {
    let (tz, settings) = user_ctx(&s.pool, auth.user_id).await?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let v = validate(&b, today(&s, tz))?;
    let mut tx = s.pool.begin().await?;
    let r = repo::update(&mut tx, auth.user_id, id, version, &input(&b, &v)).await?;
    sync_goals(
        &mut tx,
        auth.user_id,
        &settings.timezone,
        id,
        b.goal_ids.as_deref(),
    )
    .await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "reading",
        Some(id),
        "update",
        serde_json::json!({ "status": r.status }),
    )
    .await;
    Ok(Json(ReadingView {
        goal_ids: goal_ids(&s, auth.user_id, id).await?,
        reading: r,
    }))
}

/// Borra la lectura y retira sus aportes a objetivos.
#[utoipa::path(delete, path = "/readings/{id}", tag = "readings", responses((status = 204)))]
async fn delete_reading(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let mut tx = s.pool.begin().await?;
    persistence::hooks::remove_goal_progress(&mut tx, auth.user_id, "reading", id, None).await?;
    sqlx::query("DELETE FROM entity_links WHERE user_id = $1 AND ((src_type = 'reading' AND src_id = $2) OR (dst_type = 'reading' AND dst_id = $2))")
        .bind(auth.user_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    repo::delete(&mut tx, auth.user_id, id).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "reading",
        Some(id),
        "delete",
        serde_json::json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}
