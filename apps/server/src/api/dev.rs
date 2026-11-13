//! Propietario: agente B-modules. T33 Devlog diario con sesiones, importación de texto (log-session) y ADRs.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{NaiveDate, NaiveTime};
use domain::tasks::{ParsedSession, parse_devlog_text};
use domain::text::required;
use persistence::repos::dev::{self as repo, Adr, AdrInput, DevSession, Devlog, SessionInput};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::api::tasks::user_today;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_devlogs))
        .routes(routes!(get_devlog, set_summary))
        .routes(routes!(add_session))
        .routes(routes!(update_session, delete_session))
        .routes(routes!(preview_import))
        .routes(routes!(import_text))
        .routes(routes!(list_adrs, create_adr))
        .routes(routes!(get_adr, update_adr))
}

#[derive(Deserialize, IntoParams)]
pub struct RangeQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

#[utoipa::path(get, path = "/devlogs", tag = "dev", params(RangeQuery), responses((status = 200, body = Vec<Devlog>)))]
async fn list_devlogs(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<Devlog>>> {
    Ok(Json(
        repo::list_devlogs(&s.pool, auth.user_id, q.from, q.to).await?,
    ))
}

#[derive(Serialize, ToSchema)]
pub struct DevlogDay {
    pub date: NaiveDate,
    pub devlog: Option<Devlog>,
    pub sessions: Vec<DevSession>,
}

#[utoipa::path(get, path = "/devlogs/{date}", tag = "dev", responses((status = 200, body = DevlogDay)))]
async fn get_devlog(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
) -> ApiResult<Json<DevlogDay>> {
    let devlog = repo::get_devlog(&s.pool, auth.user_id, date).await?;
    let sessions = match &devlog {
        Some(d) => repo::sessions(&s.pool, auth.user_id, d.id).await?,
        None => vec![],
    };
    Ok(Json(DevlogDay {
        date,
        devlog,
        sessions,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct SummaryBody {
    pub summary_md: String,
    pub version: Option<i32>,
}

#[utoipa::path(put, path = "/devlogs/{date}", tag = "dev", request_body = SummaryBody, responses((status = 200, body = Devlog)))]
async fn set_summary(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
    Json(b): Json<SummaryBody>,
) -> ApiResult<Json<Devlog>> {
    Ok(Json(
        repo::set_summary(&s.pool, auth.user_id, date, b.version, &b.summary_md).await?,
    ))
}

#[derive(Deserialize, ToSchema, Default)]
pub struct SessionBody {
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub branch: String,
    /// `HH:MM`
    pub at_time: Option<String>,
    #[serde(default)]
    pub changes_md: String,
    #[serde(default)]
    pub bugs_md: String,
    #[serde(default)]
    pub decisions_md: String,
    #[serde(default)]
    pub next_steps_md: String,
    #[serde(default)]
    pub project_ids: Vec<Uuid>,
    pub version: Option<i32>,
}

fn parse_time(t: Option<&str>) -> ApiResult<Option<NaiveTime>> {
    match t.map(str::trim).filter(|x| !x.is_empty()) {
        None => Ok(None),
        Some(x) => NaiveTime::parse_from_str(x, "%H:%M")
            .or_else(|_| NaiveTime::parse_from_str(x, "%H:%M:%S"))
            .map(Some)
            .map_err(|_| ApiError::validation("at_time", "usa HH:MM")),
    }
}

fn input<'a>(b: &'a SessionBody, at: Option<NaiveTime>, raw: Option<&'a str>) -> SessionInput<'a> {
    SessionInput {
        repo: b.repo.trim(),
        branch: b.branch.trim(),
        at_time: at,
        changes_md: &b.changes_md,
        bugs_md: &b.bugs_md,
        decisions_md: &b.decisions_md,
        next_steps_md: &b.next_steps_md,
        raw_text: raw,
        project_ids: &b.project_ids,
    }
}

/// Agrega una sesión al devlog del día (varias por día).
#[utoipa::path(post, path = "/devlogs/{date}/sessions", tag = "dev", request_body = SessionBody, responses((status = 201, body = DevSession)))]
async fn add_session(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
    Json(b): Json<SessionBody>,
) -> ApiResult<(StatusCode, Json<DevSession>)> {
    let at = parse_time(b.at_time.as_deref())?;
    let mut tx = s.pool.begin().await?;
    let id = repo::add_session(&mut tx, auth.user_id, date, &input(&b, at, None)).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(repo::get_session(&s.pool, auth.user_id, id).await?),
    ))
}

#[utoipa::path(put, path = "/devlog-sessions/{id}", tag = "dev", request_body = SessionBody, responses((status = 200, body = DevSession)))]
async fn update_session(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<SessionBody>,
) -> ApiResult<Json<DevSession>> {
    let at = parse_time(b.at_time.as_deref())?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    Ok(Json(
        repo::update_session(&s.pool, auth.user_id, id, version, &input(&b, at, None)).await?,
    ))
}

#[utoipa::path(delete, path = "/devlog-sessions/{id}", tag = "dev", responses((status = 204)))]
async fn delete_session(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    repo::delete_session(&s.pool, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct ImportBody {
    /// Texto pegado (resumen de sesión al estilo /log-session). Se trata como datos.
    pub text: String,
    /// Fecha del devlog (por defecto hoy).
    pub date: Option<NaiveDate>,
}

#[derive(Serialize, ToSchema)]
pub struct ImportPreview {
    pub parsed: ParsedSessionDto,
    /// Proyectos reconocidos por slug.
    pub project_ids: Vec<Uuid>,
    /// Slugs mencionados que no existen (no se crean).
    pub unknown_projects: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ParsedSessionDto {
    pub repo: String,
    pub branch: String,
    pub at_time: Option<String>,
    pub changes_md: String,
    pub bugs_md: String,
    pub decisions_md: String,
    pub next_steps_md: String,
    pub project_slugs: Vec<String>,
}

impl From<ParsedSession> for ParsedSessionDto {
    fn from(p: ParsedSession) -> Self {
        Self {
            repo: p.repo,
            branch: p.branch,
            at_time: p.at_time,
            changes_md: p.changes_md,
            bugs_md: p.bugs_md,
            decisions_md: p.decisions_md,
            next_steps_md: p.next_steps_md,
            project_slugs: p.project_slugs,
        }
    }
}

async fn preview(s: &AppState, user_id: Uuid, text: &str) -> ApiResult<ImportPreview> {
    if text.trim().is_empty() || text.len() > 100_000 {
        return Err(ApiError::validation("text", "pega entre 1 y 100 000 caracteres"));
    }
    let parsed = parse_devlog_text(text);
    let mut ids = vec![];
    let mut unknown = vec![];
    for slug in &parsed.project_slugs {
        let id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM projects WHERE user_id = $1 AND slug = $2")
            .bind(user_id)
            .bind(slug)
            .fetch_optional(&s.pool)
            .await?;
        match id {
            Some(i) => ids.push(i),
            None => unknown.push(slug.clone()),
        }
    }
    Ok(ImportPreview {
        parsed: parsed.into(),
        project_ids: ids,
        unknown_projects: unknown,
    })
}

/// Vista previa del texto pegado sin guardar nada.
#[utoipa::path(post, path = "/devlogs/import/preview", tag = "dev", request_body = ImportBody, responses((status = 200, body = ImportPreview)))]
async fn preview_import(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<ImportBody>,
) -> ApiResult<Json<ImportPreview>> {
    Ok(Json(preview(&s, auth.user_id, &b.text).await?))
}

/// Crea una sesión desde el texto pegado (guarda también el texto original).
#[utoipa::path(post, path = "/devlogs/import", tag = "dev", request_body = ImportBody, responses((status = 201, body = DevSession)))]
async fn import_text(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<ImportBody>,
) -> ApiResult<(StatusCode, Json<DevSession>)> {
    let p = preview(&s, auth.user_id, &b.text).await?;
    let date = match b.date {
        Some(d) => d,
        None => user_today(&s, auth.user_id).await?.1,
    };
    let at = parse_time(p.parsed.at_time.as_deref()).ok().flatten();
    let body = SessionBody {
        repo: p.parsed.repo,
        branch: p.parsed.branch,
        changes_md: p.parsed.changes_md,
        bugs_md: p.parsed.bugs_md,
        decisions_md: p.parsed.decisions_md,
        next_steps_md: p.parsed.next_steps_md,
        project_ids: p.project_ids,
        ..Default::default()
    };
    let mut tx = s.pool.begin().await?;
    let id = repo::add_session(&mut tx, auth.user_id, date, &input(&body, at, Some(&b.text))).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "devlog_session",
        Some(id),
        "import",
        serde_json::json!({}),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(repo::get_session(&s.pool, auth.user_id, id).await?),
    ))
}

#[derive(Deserialize, IntoParams)]
pub struct AdrQuery {
    pub project_id: Option<Uuid>,
    pub status: Option<String>,
}

#[utoipa::path(get, path = "/adrs", tag = "dev", params(AdrQuery), responses((status = 200, body = Vec<Adr>)))]
async fn list_adrs(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<AdrQuery>,
) -> ApiResult<Json<Vec<Adr>>> {
    Ok(Json(
        repo::list_adrs(&s.pool, auth.user_id, q.project_id, q.status.as_deref()).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct AdrBody {
    pub project_id: Uuid,
    pub title: String,
    pub decided_on: Option<NaiveDate>,
    #[serde(default)]
    pub context_md: String,
    #[serde(default)]
    pub decision_md: String,
    #[serde(default)]
    pub alternatives_md: String,
    #[serde(default)]
    pub consequences_md: String,
    /// Al actualizar: `proposed | accepted | superseded`.
    pub status: Option<String>,
    /// Obligatorio con `superseded`: ADR que lo reemplaza (mismo usuario).
    pub superseded_by_id: Option<Uuid>,
    pub version: Option<i32>,
}

/// Crea un ADR con el siguiente número libre del proyecto (estado inicial: propuesta).
#[utoipa::path(post, path = "/adrs", tag = "dev", request_body = AdrBody, responses((status = 201, body = Adr)))]
async fn create_adr(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<AdrBody>,
) -> ApiResult<(StatusCode, Json<Adr>)> {
    let title = required("title", &b.title, 200)?;
    let date = match b.decided_on {
        Some(d) => d,
        None => user_today(&s, auth.user_id).await?.1,
    };
    let a = repo::create_adr(
        &s.pool,
        auth.user_id,
        b.project_id,
        &AdrInput {
            title: &title,
            decided_on: date,
            context_md: &b.context_md,
            decision_md: &b.decision_md,
            alternatives_md: &b.alternatives_md,
            consequences_md: &b.consequences_md,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "adr",
        Some(a.id),
        "create",
        serde_json::json!({ "number": a.number }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(a)))
}

#[utoipa::path(get, path = "/adrs/{id}", tag = "dev", responses((status = 200, body = Adr)))]
async fn get_adr(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<Json<Adr>> {
    Ok(Json(repo::get_adr(&s.pool, auth.user_id, id).await?))
}

#[utoipa::path(put, path = "/adrs/{id}", tag = "dev", request_body = AdrBody, responses((status = 200, body = Adr), (status = 409, body = crate::error::ErrorBody)))]
async fn update_adr(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<AdrBody>,
) -> ApiResult<Json<Adr>> {
    let title = required("title", &b.title, 200)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let cur = repo::get_adr(&s.pool, auth.user_id, id).await?;
    let status = b.status.clone().unwrap_or(cur.status.clone());
    if !["proposed", "accepted", "superseded"].contains(&status.as_str()) {
        return Err(ApiError::validation("status", "proposed, accepted o superseded"));
    }
    let by = if status == "superseded" {
        let by = b
            .superseded_by_id
            .ok_or_else(|| ApiError::validation("superseded_by_id", "indica el ADR que lo reemplaza"))?;
        if by == id {
            return Err(ApiError::validation(
                "superseded_by_id",
                "un ADR no puede reemplazarse a sí mismo",
            ));
        }
        repo::get_adr(&s.pool, auth.user_id, by)
            .await
            .map_err(|_| ApiError::validation("superseded_by_id", "ADR inexistente"))?;
        Some(by)
    } else {
        None
    };
    let a = repo::update_adr(
        &s.pool,
        auth.user_id,
        id,
        version,
        &AdrInput {
            title: &title,
            decided_on: b.decided_on.unwrap_or(cur.decided_on),
            context_md: &b.context_md,
            decision_md: &b.decision_md,
            alternatives_md: &b.alternatives_md,
            consequences_md: &b.consequences_md,
        },
        &status,
        by,
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "adr",
        Some(id),
        "update",
        serde_json::json!({ "status": status }),
    )
    .await;
    Ok(Json(a))
}
