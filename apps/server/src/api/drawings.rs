//! Propietario: agente B-modules. T35 Persistencia de notas visuales (Excalidraw) y enlace a notas.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::text::required;
use persistence::repos::drawings::{self as repo, Drawing, DrawingSummary};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_drawings, create_drawing))
        .routes(routes!(get_drawing, update_drawing))
}

/// Límite de la escena serializada (los dibujos con imágenes embebidas pueden crecer; ver límite del body en api/mod.rs).
pub const MAX_SCENE_BYTES: usize = 1_900_000;

#[derive(Deserialize, ToSchema)]
pub struct DrawingBody {
    pub title: String,
    /// Escena Excalidraw serializada (objeto).
    #[serde(default)]
    #[schema(value_type = Object)]
    pub scene: serde_json::Value,
    pub note_id: Option<Uuid>,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

fn validate(b: &DrawingBody) -> ApiResult<(String, serde_json::Value)> {
    let title = required("title", &b.title, 200)?;
    let scene = match &b.scene {
        serde_json::Value::Null => serde_json::json!({ "elements": [] }),
        v @ serde_json::Value::Object(_) => v.clone(),
        _ => return Err(ApiError::validation("scene", "debe ser un objeto")),
    };
    if scene.to_string().len() > MAX_SCENE_BYTES {
        return Err(ApiError::validation(
            "scene",
            "el dibujo supera el tamaño máximo (≈1.9 MB)",
        ));
    }
    if let Some(el) = scene.get("elements")
        && !el.is_array()
    {
        return Err(ApiError::validation("scene", "elements debe ser una lista"));
    }
    Ok((title, scene))
}

#[derive(Deserialize, IntoParams)]
pub struct DrawingQuery {
    pub note_id: Option<Uuid>,
    pub archived: Option<bool>,
}

#[utoipa::path(get, path = "/drawings", tag = "drawings", params(DrawingQuery), responses((status = 200, body = Vec<DrawingSummary>)))]
async fn list_drawings(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<DrawingQuery>,
) -> ApiResult<Json<Vec<DrawingSummary>>> {
    Ok(Json(
        repo::list(&s.pool, auth.user_id, q.note_id, q.archived.unwrap_or(false)).await?,
    ))
}

#[utoipa::path(post, path = "/drawings", tag = "drawings", request_body = DrawingBody, responses((status = 201, body = Drawing)))]
async fn create_drawing(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<DrawingBody>,
) -> ApiResult<(StatusCode, Json<Drawing>)> {
    let (title, scene) = validate(&b)?;
    Ok((
        StatusCode::CREATED,
        Json(repo::create(&s.pool, auth.user_id, &title, &scene, b.note_id).await?),
    ))
}

#[utoipa::path(get, path = "/drawings/{id}", tag = "drawings", responses((status = 200, body = Drawing)))]
async fn get_drawing(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Drawing>> {
    Ok(Json(repo::get(&s.pool, auth.user_id, id).await?))
}

#[utoipa::path(put, path = "/drawings/{id}", tag = "drawings", request_body = DrawingBody, responses((status = 200, body = Drawing), (status = 409, body = crate::error::ErrorBody)))]
async fn update_drawing(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<DrawingBody>,
) -> ApiResult<Json<Drawing>> {
    let (title, scene) = validate(&b)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    Ok(Json(
        repo::update(
            &s.pool,
            auth.user_id,
            id,
            version,
            &title,
            &scene,
            b.note_id,
            b.archived,
        )
        .await?,
    ))
}
