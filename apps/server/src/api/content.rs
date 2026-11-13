//! Propietario: agente B-modules. T30 Contenido (pipeline, hooks, guion, shot list, métricas) y T31 Swipe.
//! Los medios pesados son enlaces externos validados; nunca se descargan.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::tasks::days_until;
use domain::text::{required, validate_external_url};
use persistence::repos::content::{self as repo, ContentInput, ContentPiece, SwipeInput, SwipeRef};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::api::notes::clean_tags;
use crate::api::tasks::user_today;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_content, create_content))
        .routes(routes!(content_board))
        .routes(routes!(content_published))
        .routes(routes!(content_ai_context))
        .routes(routes!(get_content, update_content))
        .routes(routes!(list_swipe, create_swipe))
        .routes(routes!(get_swipe, update_swipe))
}

pub const STATUSES: [&str; 5] = ["idea", "script", "recorded", "edited", "published"];

#[derive(Serialize, ToSchema)]
pub struct ContentView {
    #[serde(flatten)]
    pub piece: ContentPiece,
    /// Días para publicar (negativo si ya pasó). Fórmula de Pipeline.base.
    pub days_to_publish: Option<i64>,
    pub chosen_hook_text: Option<String>,
}

fn view(p: ContentPiece, today: NaiveDate) -> ContentView {
    ContentView {
        days_to_publish: if p.status == "published" {
            None
        } else {
            days_until(p.publish_on, today)
        },
        chosen_hook_text: p.chosen_hook.and_then(|i| p.hooks.get(i as usize).cloned()),
        piece: p,
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ShotRow {
    pub n: Option<i32>,
    #[serde(default)]
    pub shot: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub motion: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ContentBody {
    pub title: String,
    #[serde(default)]
    pub idea: String,
    #[serde(default = "idea")]
    pub status: String,
    #[serde(default = "reel")]
    pub format: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub pillar: Option<String>,
    /// Escribe hasta 10 y elige 1.
    #[serde(default)]
    pub hooks: Vec<String>,
    pub chosen_hook: Option<i16>,
    #[serde(default)]
    pub script_hook: String,
    #[serde(default)]
    pub script_body: String,
    #[serde(default)]
    pub script_cta: String,
    #[serde(default)]
    pub shot_list: Vec<ShotRow>,
    #[serde(default)]
    pub edit_notes: String,
    #[serde(default)]
    pub result_worked: String,
    #[serde(default)]
    pub result_change: String,
    pub publish_on: Option<NaiveDate>,
    pub published_on: Option<NaiveDate>,
    /// Enlace al original (Drive, etc.). No se descarga.
    pub raw_url: Option<String>,
    pub render_url: Option<String>,
    pub views: Option<i64>,
    #[schema(value_type = Option<String>)]
    pub retention_pct: Option<Decimal>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

fn idea() -> String {
    "idea".into()
}
fn reel() -> String {
    "reel".into()
}

struct Valid {
    title: String,
    hooks: Vec<String>,
    platforms: Vec<String>,
    shots: serde_json::Value,
    raw: Option<String>,
    render: Option<String>,
}

fn validate(b: &ContentBody) -> ApiResult<Valid> {
    let title = required("title", &b.title, 200)?;
    if !STATUSES.contains(&b.status.as_str()) {
        return Err(ApiError::validation(
            "status",
            "idea, script, recorded, edited o published",
        ));
    }
    let hooks: Vec<String> = b
        .hooks
        .iter()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .collect();
    if hooks.len() > 10 {
        return Err(ApiError::validation("hooks", "máximo 10 hooks"));
    }
    if let Some(i) = b.chosen_hook
        && (i < 0 || i as usize >= hooks.len())
    {
        return Err(ApiError::validation(
            "chosen_hook",
            "elige uno de los hooks escritos",
        ));
    }
    if b.views.is_some_and(|v| v < 0) {
        return Err(ApiError::validation("views", "no puede ser negativo"));
    }
    if b.retention_pct
        .is_some_and(|r| r < Decimal::ZERO || r > Decimal::from(100))
    {
        return Err(ApiError::validation("retention_pct", "0 a 100"));
    }
    if b.status == "published" && b.published_on.is_none() && b.publish_on.is_none() {
        return Err(ApiError::validation(
            "published_on",
            "indica la fecha de publicación",
        ));
    }
    if b.shot_list.len() > 200 {
        return Err(ApiError::validation("shot_list", "máximo 200 tomas"));
    }
    let shots = serde_json::Value::Array(
        b.shot_list
            .iter()
            .enumerate()
            .map(|(i, r)| serde_json::json!({ "n": r.n.unwrap_or(i as i32 + 1), "shot": r.shot, "text": r.text, "motion": r.motion }))
            .collect(),
    );
    let url = |f: &str, v: &Option<String>| {
        v.as_deref()
            .filter(|x| !x.trim().is_empty())
            .map(|x| validate_external_url(f, x))
            .transpose()
    };
    let mut platforms: Vec<String> = b
        .platforms
        .iter()
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    platforms.dedup();
    Ok(Valid {
        title,
        hooks,
        platforms,
        shots,
        raw: url("raw_url", &b.raw_url)?,
        render: url("render_url", &b.render_url)?,
    })
}

fn input<'a>(b: &'a ContentBody, v: &'a Valid) -> ContentInput<'a> {
    ContentInput {
        title: &v.title,
        idea: &b.idea,
        status: &b.status,
        format: b.format.trim(),
        platforms: &v.platforms,
        pillar: b.pillar.as_deref().filter(|p| !p.trim().is_empty()),
        hooks: &v.hooks,
        chosen_hook: b.chosen_hook,
        script_hook: &b.script_hook,
        script_body: &b.script_body,
        script_cta: &b.script_cta,
        shot_list: &v.shots,
        edit_notes: &b.edit_notes,
        result_worked: &b.result_worked,
        result_change: &b.result_change,
        publish_on: b.publish_on,
        published_on: if b.status == "published" {
            b.published_on.or(b.publish_on)
        } else {
            b.published_on
        },
        raw_url: v.raw.as_deref(),
        render_url: v.render.as_deref(),
        views: b.views,
        retention_pct: b.retention_pct,
        area_id: b.area_id,
        project_id: b.project_id,
        archived: b.archived,
    }
}

#[derive(Deserialize, IntoParams)]
pub struct ContentQuery {
    pub status: Option<String>,
    pub q: Option<String>,
    pub archived: Option<bool>,
}

/// Tabla del pipeline (Pipeline.base), con días para publicar.
#[utoipa::path(get, path = "/content", tag = "content", params(ContentQuery), responses((status = 200, body = Vec<ContentView>)))]
async fn list_content(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ContentQuery>,
) -> ApiResult<Json<Vec<ContentView>>> {
    let (_, today) = user_today(&s, auth.user_id).await?;
    let rows = repo::list(
        &s.pool,
        auth.user_id,
        q.status.as_deref(),
        q.q.as_deref().filter(|x| !x.trim().is_empty()),
        q.archived.unwrap_or(false),
    )
    .await?;
    Ok(Json(rows.into_iter().map(|p| view(p, today)).collect()))
}

#[derive(Serialize, ToSchema)]
pub struct BoardColumn {
    pub status: String,
    pub items: Vec<ContentView>,
}

/// Kanban por estado (idea → guion → grabado → editado → publicado).
#[utoipa::path(get, path = "/content/board", tag = "content", responses((status = 200, body = Vec<BoardColumn>)))]
async fn content_board(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<BoardColumn>>> {
    let (_, today) = user_today(&s, auth.user_id).await?;
    let mut cols: Vec<BoardColumn> = STATUSES
        .iter()
        .map(|st| BoardColumn {
            status: st.to_string(),
            items: vec![],
        })
        .collect();
    for p in repo::list(&s.pool, auth.user_id, None, None, false).await? {
        if let Some(c) = cols.iter_mut().find(|c| c.status == p.status) {
            c.items.push(view(p, today));
        }
    }
    Ok(Json(cols))
}

#[derive(Deserialize, IntoParams)]
pub struct PublishedQuery {
    /// `views` (por defecto) o `retention`.
    pub sort: Option<String>,
    pub limit: Option<i64>,
}

/// Publicados ordenados por métricas.
#[utoipa::path(get, path = "/content/published", tag = "content", params(PublishedQuery), responses((status = 200, body = Vec<ContentPiece>)))]
async fn content_published(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PublishedQuery>,
) -> ApiResult<Json<Vec<ContentPiece>>> {
    let by_ret = q.sort.as_deref() == Some("retention");
    Ok(Json(
        repo::published(
            &s.pool,
            auth.user_id,
            by_ret,
            q.limit.unwrap_or(100).clamp(1, 500),
        )
        .await?,
    ))
}

#[derive(Serialize, ToSchema)]
pub struct AiContentContext {
    /// Las 5 piezas publicadas más recientes (views, retención y "Resultado") a considerar antes de proponer un guion.
    pub recent_published: Vec<ContentPiece>,
}

#[utoipa::path(get, path = "/content/ai-context", tag = "content", responses((status = 200, body = AiContentContext)))]
async fn content_ai_context(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<AiContentContext>> {
    Ok(Json(AiContentContext {
        recent_published: repo::recent_published(&s.pool, auth.user_id, 5).await?,
    }))
}

#[utoipa::path(post, path = "/content", tag = "content", request_body = ContentBody, responses((status = 201, body = ContentPiece)))]
async fn create_content(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<ContentBody>,
) -> ApiResult<(StatusCode, Json<ContentPiece>)> {
    let v = validate(&b)?;
    let p = repo::create(&s.pool, auth.user_id, &input(&b, &v)).await?;
    audit(
        &s,
        auth.user_id,
        "content",
        Some(p.id),
        "create",
        serde_json::json!({ "status": p.status }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(p)))
}

#[utoipa::path(get, path = "/content/{id}", tag = "content", responses((status = 200, body = ContentView)))]
async fn get_content(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ContentView>> {
    let (_, today) = user_today(&s, auth.user_id).await?;
    Ok(Json(view(repo::get(&s.pool, auth.user_id, id).await?, today)))
}

#[utoipa::path(put, path = "/content/{id}", tag = "content", request_body = ContentBody, responses((status = 200, body = ContentPiece), (status = 409, body = crate::error::ErrorBody)))]
async fn update_content(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ContentBody>,
) -> ApiResult<Json<ContentPiece>> {
    let v = validate(&b)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let p = repo::update(&s.pool, auth.user_id, id, version, &input(&b, &v)).await?;
    audit(
        &s,
        auth.user_id,
        "content",
        Some(id),
        "update",
        serde_json::json!({ "status": p.status }),
    )
    .await;
    Ok(Json(p))
}

// ---------- Swipe ----------

#[derive(Deserialize, ToSchema)]
pub struct SwipeBody {
    #[serde(default)]
    pub title: String,
    pub url: String,
    pub creator: Option<String>,
    pub platform: Option<String>,
    #[serde(default)]
    pub why_it_works: String,
    #[serde(default)]
    pub hook: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

#[derive(Deserialize, IntoParams)]
pub struct SwipeQuery {
    pub tag: Option<String>,
    pub q: Option<String>,
    pub archived: Option<bool>,
}

#[utoipa::path(get, path = "/swipe", tag = "content", params(SwipeQuery), responses((status = 200, body = Vec<SwipeRef>)))]
async fn list_swipe(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<SwipeQuery>,
) -> ApiResult<Json<Vec<SwipeRef>>> {
    Ok(Json(
        repo::list_swipe(
            &s.pool,
            auth.user_id,
            q.tag.as_deref(),
            q.q.as_deref().filter(|x| !x.trim().is_empty()),
            q.archived.unwrap_or(false),
        )
        .await?,
    ))
}

fn swipe_valid(b: &SwipeBody) -> ApiResult<(String, String, Vec<String>)> {
    let url = validate_external_url("url", &b.url)?;
    let title = if b.title.trim().is_empty() {
        url.chars().take(120).collect()
    } else {
        b.title.trim().chars().take(200).collect()
    };
    Ok((title, url, clean_tags(&b.tags)?))
}

fn swipe_input<'a>(b: &'a SwipeBody, v: &'a (String, String, Vec<String>)) -> SwipeInput<'a> {
    SwipeInput {
        title: &v.0,
        url: &v.1,
        creator: b.creator.as_deref().filter(|x| !x.trim().is_empty()),
        platform: b.platform.as_deref().filter(|x| !x.trim().is_empty()),
        why_it_works: &b.why_it_works,
        hook: &b.hook,
        tags: &v.2,
        archived: b.archived,
    }
}

#[utoipa::path(post, path = "/swipe", tag = "content", request_body = SwipeBody, responses((status = 201, body = SwipeRef)))]
async fn create_swipe(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<SwipeBody>,
) -> ApiResult<(StatusCode, Json<SwipeRef>)> {
    let v = swipe_valid(&b)?;
    Ok((
        StatusCode::CREATED,
        Json(repo::create_swipe(&s.pool, auth.user_id, &swipe_input(&b, &v)).await?),
    ))
}

#[utoipa::path(get, path = "/swipe/{id}", tag = "content", responses((status = 200, body = SwipeRef)))]
async fn get_swipe(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SwipeRef>> {
    Ok(Json(repo::get_swipe(&s.pool, auth.user_id, id).await?))
}

#[utoipa::path(put, path = "/swipe/{id}", tag = "content", request_body = SwipeBody, responses((status = 200, body = SwipeRef)))]
async fn update_swipe(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<SwipeBody>,
) -> ApiResult<Json<SwipeRef>> {
    let v = swipe_valid(&b)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    Ok(Json(
        repo::update_swipe(&s.pool, auth.user_id, id, version, &swipe_input(&b, &v)).await?,
    ))
}
