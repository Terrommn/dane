//! T49–T51 Integraciones (solo lectura): estado, conexión/desconexión, configuración, sincronización y sugerencias.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::NaiveDate;
use persistence::repos::integrations::{self as repo, ConnectionStatus, InteractionSuggestion, PersonSuggestion, SyncRun};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::config::AppEnv;
use crate::error::{ApiError, ApiResult};
use crate::integrations::{self, Endpoints, SyncReport, clickup, google, llm};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(overview))
        .routes(routes!(google_connect))
        .routes(routes!(google_callback))
        .routes(routes!(google_calendars))
        .routes(routes!(google_config))
        .routes(routes!(google_sync))
        .routes(routes!(google_disconnect))
        .routes(routes!(clickup_connect))
        .routes(routes!(clickup_sync))
        .routes(routes!(clickup_tasks))
        .routes(routes!(clickup_disconnect))
        .routes(routes!(suggestions))
        .routes(routes!(accept_interaction))
        .routes(routes!(dismiss_interaction))
        .routes(routes!(accept_person))
        .routes(routes!(dismiss_person))
}

#[derive(Serialize, ToSchema)]
pub struct LlmStatus {
    pub configured: bool,
    pub model: Option<String>,
    /// `llm` si hay credencial; `manual` si no (formularios completos sin IA).
    pub mode: String,
}

#[derive(Serialize, ToSchema)]
pub struct IntegrationsOverview {
    pub google: ConnectionStatus,
    pub clickup: ConnectionStatus,
    /// El servidor tiene credenciales OAuth de Google (GOOGLE_CLIENT_ID).
    pub google_available: bool,
    pub llm: LlmStatus,
    pub recent_runs: Vec<SyncRun>,
    pub google_redirect_uri: String,
}

fn disconnected(provider: &str) -> ConnectionStatus {
    ConnectionStatus {
        provider: provider.into(),
        status: "disconnected".into(),
        scopes: vec![],
        account_label: None,
        config: serde_json::json!({}),
        last_sync_at: None,
        last_error: None,
        last_error_at: None,
        version: 0,
    }
}

/// Estado de todas las integraciones: conexión, última sincronización y errores.
#[utoipa::path(get, path = "/integrations", tag = "integrations", responses((status = 200, body = IntegrationsOverview)))]
async fn overview(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<IntegrationsOverview>> {
    let conns = repo::list(&s.pool, auth.user_id).await?;
    let find = |p: &str| conns.iter().find(|c| c.provider == p).map(|c| c.status()).unwrap_or_else(|| disconnected(p));
    Ok(Json(IntegrationsOverview {
        google: find("google"),
        clickup: find("clickup"),
        google_available: s.config.google.is_some(),
        llm: LlmStatus {
            configured: llm::configured(&s),
            model: llm::configured(&s).then(|| s.config.llm_model.clone()),
            mode: if llm::configured(&s) { "llm".into() } else { "manual".into() },
        },
        recent_runs: repo::recent_runs(&s.pool, auth.user_id, 20).await?,
        google_redirect_uri: s.config.google_redirect_uri(),
    }))
}

#[derive(Deserialize, IntoParams)]
pub struct ConnectQuery {
    pub redirect_to: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct AuthorizeUrl {
    /// Abre esta URL (en el navegador, no dentro de un WebView) para autorizar.
    pub url: String,
}

fn safe_redirect(p: Option<&str>) -> String {
    match p {
        Some(p) if p.starts_with('/') && !p.starts_with("//") && !p.contains('\\') && p.len() < 300 => p.to_string(),
        _ => "/ajustes".into(),
    }
}

/// Inicia la conexión con Google (OAuth + PKCE, scopes de solo lectura).
#[utoipa::path(get, path = "/integrations/google/connect", tag = "integrations", params(ConnectQuery), responses((status = 200, body = AuthorizeUrl)))]
async fn google_connect(State(s): State<AppState>, auth: AuthUser, Query(q): Query<ConnectQuery>) -> ApiResult<Json<AuthorizeUrl>> {
    let ep = Endpoints::resolve(&s, None);
    let url = google::authorize_url(&s, auth.user_id, &ep, &safe_redirect(q.redirect_to.as_deref())).await?;
    Ok(Json(AuthorizeUrl { url }))
}

#[derive(Deserialize, IntoParams)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Callback de Google. El `state` (un solo uso) identifica al usuario; no requiere sesión.
#[utoipa::path(get, path = "/integrations/google/callback", tag = "integrations", params(CallbackQuery), responses((status = 302)))]
async fn google_callback(State(s): State<AppState>, Query(q): Query<CallbackQuery>) -> Response {
    let (Some(code), Some(st)) = (q.code, q.state) else {
        return Redirect::to(&format!("/ajustes?google={}", if q.error.is_some() { "cancelled" } else { "error" })).into_response();
    };
    let ep = Endpoints::resolve(&s, None);
    match google::complete(&s, &ep, &st, &code).await {
        Ok((user, redirect)) => {
            audit(&s, user, "integration", None, "connect", serde_json::json!({ "provider": "google" })).await;
            let sep = if redirect.contains('?') { '&' } else { '?' };
            Redirect::to(&format!("{redirect}{sep}google=connected")).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "conexión Google fallida");
            Redirect::to("/ajustes?google=error").into_response()
        }
    }
}

/// Calendarios disponibles; los de festivos/observancias se marcan como excluidos siempre.
#[utoipa::path(get, path = "/integrations/google/calendars", tag = "integrations", responses((status = 200, body = Vec<google::CalendarInfo>)))]
async fn google_calendars(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<google::CalendarInfo>>> {
    let conn = repo::get(&s.pool, auth.user_id, "google").await?;
    let ep = Endpoints::resolve(&s, conn.as_ref().map(|c| &c.config));
    Ok(Json(google::list_calendars(&s, auth.user_id, &ep).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct GoogleConfigBody {
    pub version: i32,
    /// IDs de calendarios a leer (los de festivos se ignoran aunque se incluyan).
    pub calendars: Option<Vec<String>>,
    pub calendar_enabled: Option<bool>,
    pub gmail_enabled: Option<bool>,
}

#[utoipa::path(put, path = "/integrations/google/config", tag = "integrations", request_body = GoogleConfigBody, responses((status = 200, body = ConnectionStatus), (status = 409, body = crate::error::ErrorBody)))]
async fn google_config(State(s): State<AppState>, auth: AuthUser, Json(b): Json<GoogleConfigBody>) -> ApiResult<Json<ConnectionStatus>> {
    let mut patch = serde_json::Map::new();
    if let Some(c) = b.calendars {
        let c: Vec<String> = c.into_iter().filter(|x| !google::excluded_calendar(x)).collect();
        patch.insert("calendars".into(), serde_json::json!(c));
    }
    if let Some(v) = b.calendar_enabled {
        patch.insert("calendar_enabled".into(), v.into());
    }
    if let Some(v) = b.gmail_enabled {
        patch.insert("gmail_enabled".into(), v.into());
    }
    let ok = repo::patch_config(&s.pool, auth.user_id, "google", b.version, &serde_json::Value::Object(patch)).await?;
    let conn = repo::get(&s.pool, auth.user_id, "google").await?.ok_or_else(ApiError::not_found)?;
    if !ok {
        return Err(ApiError::conflict(serde_json::to_value(conn.status()).unwrap_or_default()));
    }
    Ok(Json(conn.status()))
}

#[derive(Deserialize, ToSchema, Default)]
pub struct SyncBody {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

/// Sincroniza ahora (agenda y correos enviados). Ventana máxima de 62 días.
#[utoipa::path(post, path = "/integrations/google/sync", tag = "integrations", request_body = SyncBody, responses((status = 200, body = Vec<SyncReport>)))]
async fn google_sync(State(s): State<AppState>, auth: AuthUser, body: Option<Json<SyncBody>>) -> ApiResult<Json<Vec<SyncReport>>> {
    let b = body.map(|b| b.0).unwrap_or_default();
    let window = match (b.from, b.to) {
        (Some(f), Some(t)) if t >= f && (t - f).num_days() <= 62 => Some((f, t)),
        (None, None) => None,
        _ => return Err(ApiError::validation("to", "rango inválido (máximo 62 días)")),
    };
    repo::get(&s.pool, auth.user_id, "google").await?.ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "integration_not_ready", "Google no está conectado"))?;
    Ok(Json(google::sync_all(&s, auth.user_id, window).await))
}

/// Revoca el acceso en Google y borra las credenciales.
#[utoipa::path(delete, path = "/integrations/google", tag = "integrations", responses((status = 204)))]
async fn google_disconnect(State(s): State<AppState>, auth: AuthUser) -> ApiResult<StatusCode> {
    let conn = repo::get(&s.pool, auth.user_id, "google").await?;
    let ep = Endpoints::resolve(&s, conn.as_ref().map(|c| &c.config));
    google::revoke(&s, auth.user_id, &ep).await?;
    audit(&s, auth.user_id, "integration", None, "revoke", serde_json::json!({ "provider": "google" })).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct ClickupBody {
    /// Token personal (pk_…). Si se omite, solo se actualiza la configuración.
    pub token: Option<String>,
    pub view_id: Option<String>,
    pub area_id: Option<Uuid>,
    /// Estado de ClickUp → `open | need_info | closed`.
    pub status_map: Option<std::collections::BTreeMap<String, String>>,
    pub version: Option<i32>,
    /// Solo desarrollo/pruebas: URL base simulada.
    pub endpoints_override: Option<String>,
}

#[utoipa::path(put, path = "/integrations/clickup", tag = "integrations", request_body = ClickupBody, responses((status = 200, body = ConnectionStatus)))]
async fn clickup_connect(State(s): State<AppState>, auth: AuthUser, Json(b): Json<ClickupBody>) -> ApiResult<Json<ConnectionStatus>> {
    if let Some(m) = &b.status_map {
        if m.values().any(|v| !["open", "need_info", "closed"].contains(&v.as_str())) {
            return Err(ApiError::validation("status_map", "cada estado debe mapear a open, need_info o closed"));
        }
    }
    if b.endpoints_override.is_some() && s.config.env == AppEnv::Production {
        return Err(ApiError::validation("endpoints_override", "no permitido en producción"));
    }
    let mut extra = serde_json::Map::new();
    if let Some(o) = &b.endpoints_override {
        extra.insert("endpoints_override".into(), o.clone().into());
    }
    match b.token.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(token) => {
            let ep = match &b.endpoints_override {
                Some(o) => Endpoints::mock(o),
                None => Endpoints::resolve(&s, None),
            };
            let cfg = clickup::Config {
                view_id: b.view_id.clone(),
                clickup_user_id: None,
                status_map: b.status_map.clone().unwrap_or_default(),
                area_id: b.area_id,
            };
            clickup::connect(&s, auth.user_id, &ep, token, cfg, serde_json::Value::Object(extra)).await?;
            audit(&s, auth.user_id, "integration", None, "connect", serde_json::json!({ "provider": "clickup" })).await;
        }
        None => {
            let version = b.version.ok_or_else(|| ApiError::validation("version", "obligatoria al actualizar la configuración"))?;
            let mut patch = extra;
            if let Some(v) = b.view_id {
                patch.insert("view_id".into(), v.into());
            }
            if let Some(a) = b.area_id {
                patch.insert("area_id".into(), a.to_string().into());
            }
            if let Some(m) = b.status_map {
                patch.insert("status_map".into(), serde_json::json!(m));
            }
            if !repo::patch_config(&s.pool, auth.user_id, "clickup", version, &serde_json::Value::Object(patch)).await? {
                let cur = repo::get(&s.pool, auth.user_id, "clickup").await?.ok_or_else(ApiError::not_found)?;
                return Err(ApiError::conflict(serde_json::to_value(cur.status()).unwrap_or_default()));
            }
        }
    }
    let conn = repo::get(&s.pool, auth.user_id, "clickup").await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(conn.status()))
}

/// Lee la vista configurada de ClickUp ahora (no modifica nada en ClickUp).
#[utoipa::path(post, path = "/integrations/clickup/sync", tag = "integrations", responses((status = 200, body = SyncReport)))]
async fn clickup_sync(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<SyncReport>> {
    let started = s.now();
    let r = clickup::sync(&s, auth.user_id).await;
    integrations::record_outcome(&s, auth.user_id, "clickup", "tasks", started, &r).await;
    Ok(Json(r?))
}

/// Tareas abiertas de ClickUp en el orden de la daily: fáciles (con razón), resto, need info.
#[utoipa::path(get, path = "/integrations/clickup/tasks", tag = "integrations", responses((status = 200, body = Vec<clickup::OrderedTask>)))]
async fn clickup_tasks(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<clickup::OrderedTask>>> {
    Ok(Json(clickup::ordered_tasks(&s, auth.user_id).await?))
}

#[utoipa::path(delete, path = "/integrations/clickup", tag = "integrations", responses((status = 204)))]
async fn clickup_disconnect(State(s): State<AppState>, auth: AuthUser) -> ApiResult<StatusCode> {
    clickup::disconnect(&s, auth.user_id).await?;
    audit(&s, auth.user_id, "integration", None, "disconnect", serde_json::json!({ "provider": "clickup" })).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct Suggestions {
    pub interactions: Vec<InteractionSuggestion>,
    pub people: Vec<PersonSuggestion>,
}

/// Interacciones y personas propuestas desde Calendar/Gmail/dictado (nada se registra sin aceptar).
#[utoipa::path(get, path = "/integrations/suggestions", tag = "integrations", responses((status = 200, body = Suggestions)))]
async fn suggestions(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Suggestions>> {
    Ok(Json(Suggestions {
        interactions: repo::pending_interaction_suggestions(&s.pool, auth.user_id).await?,
        people: repo::pending_person_suggestions(&s.pool, auth.user_id).await?,
    }))
}

#[derive(Deserialize, ToSchema, Default)]
pub struct AcceptInteraction {
    /// Tema editado (opcional).
    pub topic: Option<String>,
}

#[utoipa::path(post, path = "/integrations/suggestions/interactions/{id}/accept", tag = "integrations", request_body = AcceptInteraction, responses((status = 200, body = InteractionSuggestion)))]
async fn accept_interaction(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>, body: Option<Json<AcceptInteraction>>) -> ApiResult<Json<serde_json::Value>> {
    let topic = body.and_then(|b| b.0.topic);
    let mut tx = s.pool.begin().await?;
    let sug: Option<(Uuid, NaiveDate, String, String, String)> = sqlx::query_as(
        "UPDATE interaction_suggestions SET status = 'accepted' WHERE user_id = $1 AND id = $2 AND status = 'pending'
         RETURNING person_id, on_date, topic, source, source_ref",
    )
    .bind(auth.user_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (person, date, t, source, sref) = sug.ok_or_else(ApiError::not_found)?;
    let iid: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO interactions (user_id, person_id, on_date, topic, source, source_ref) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (user_id, person_id, source, source_ref) DO NOTHING RETURNING id",
    )
    .bind(auth.user_id)
    .bind(person)
    .bind(date)
    .bind(topic.unwrap_or(t))
    .bind(&source)
    .bind(&sref)
    .fetch_optional(&mut *tx)
    .await?;
    sqlx::query("UPDATE interaction_suggestions SET interaction_id = $3 WHERE user_id = $1 AND id = $2").bind(auth.user_id).bind(id).bind(iid).execute(&mut *tx).await?;
    sqlx::query("UPDATE people SET last_contact_on = greatest(coalesce(last_contact_on, $3), $3) WHERE user_id = $1 AND id = $2")
        .bind(auth.user_id)
        .bind(person)
        .bind(date)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "interaction_id": iid })))
}

#[utoipa::path(post, path = "/integrations/suggestions/interactions/{id}/dismiss", tag = "integrations", responses((status = 204)))]
async fn dismiss_interaction(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let n = sqlx::query("UPDATE interaction_suggestions SET status = 'dismissed' WHERE user_id = $1 AND id = $2 AND status = 'pending'")
        .bind(auth.user_id)
        .bind(id)
        .execute(&s.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema, Default)]
pub struct AcceptPerson {
    pub name: Option<String>,
    pub area_id: Option<Uuid>,
}

/// Crea la persona sugerida (decisión explícita del usuario).
#[utoipa::path(post, path = "/integrations/suggestions/people/{id}/accept", tag = "integrations", request_body = AcceptPerson, responses((status = 200)))]
async fn accept_person(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>, body: Option<Json<AcceptPerson>>) -> ApiResult<Json<serde_json::Value>> {
    let b = body.map(|b| b.0).unwrap_or_default();
    let mut tx = s.pool.begin().await?;
    let sug: Option<(String, Option<String>, Option<NaiveDate>, String)> =
        sqlx::query_as("SELECT name, email, on_date, context FROM person_suggestions WHERE user_id = $1 AND id = $2 AND status = 'pending' FOR UPDATE")
            .bind(auth.user_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let (name, email, date, _ctx) = sug.ok_or_else(ApiError::not_found)?;
    let name = domain::text::required("name", b.name.as_deref().unwrap_or(&name), 120)?;
    let pid: Uuid = sqlx::query_scalar("INSERT INTO people (user_id, name, email, area_id, last_contact_on) VALUES ($1, $2, $3, $4, $5) RETURNING id")
        .bind(auth.user_id)
        .bind(&name)
        .bind(&email)
        .bind(b.area_id)
        .bind(date)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("UPDATE person_suggestions SET status = 'accepted', person_id = $3 WHERE user_id = $1 AND id = $2").bind(auth.user_id).bind(id).bind(pid).execute(&mut *tx).await?;
    tx.commit().await?;
    audit(&s, auth.user_id, "person", Some(pid), "create_from_suggestion", serde_json::json!({ "name": name })).await;
    Ok(Json(serde_json::json!({ "person_id": pid })))
}

#[utoipa::path(post, path = "/integrations/suggestions/people/{id}/dismiss", tag = "integrations", responses((status = 204)))]
async fn dismiss_person(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    let n = sqlx::query("UPDATE person_suggestions SET status = 'dismissed' WHERE user_id = $1 AND id = $2 AND status = 'pending'")
        .bind(auth.user_id)
        .bind(id)
        .execute(&s.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}
