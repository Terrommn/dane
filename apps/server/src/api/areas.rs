//! T16 Áreas y proyectos. Patrón de referencia de un módulo CRUD:
//! entrada validada → repositorio con `user_id` del `AuthUser` → `version` para conflictos → auditoría.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::text::{required, slugify, validate_external_url};
use persistence::repos::areas::{self, ActivityItem, Area, AreaInput, Project, ProjectInput};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::http::ListParams;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_areas, create_area))
        .routes(routes!(get_area, update_area))
        .routes(routes!(list_projects, create_project))
        .routes(routes!(get_project, update_project))
        .routes(routes!(project_activity))
        .routes(routes!(recent_activity))
}

#[derive(Deserialize, ToSchema)]
pub struct AreaBody {
    pub name: String,
    /// Opcional: se deriva del nombre si falta.
    pub slug: Option<String>,
    #[serde(default)]
    pub description: String,
    pub color: Option<String>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub archived: bool,
    /// Obligatorio al actualizar.
    pub version: Option<i32>,
}

fn validate_area(b: &AreaBody) -> ApiResult<(String, String)> {
    let name = required("name", &b.name, 80)?;
    let slug = b.slug.as_deref().map(slugify).unwrap_or_else(|| slugify(&name));
    Ok((name, slug))
}

/// Lista de áreas.
#[utoipa::path(get, path = "/areas", tag = "areas", params(ListParams), responses((status = 200, body = Vec<Area>)))]
async fn list_areas(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(p): Query<ListParams>,
) -> ApiResult<Json<Vec<Area>>> {
    Ok(Json(
        areas::list_areas(&s.pool, auth.user_id, p.archived()).await?,
    ))
}

#[utoipa::path(post, path = "/areas", tag = "areas", request_body = AreaBody, responses((status = 201, body = Area)))]
async fn create_area(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<AreaBody>,
) -> ApiResult<(StatusCode, Json<Area>)> {
    let (name, slug) = validate_area(&b)?;
    let a = areas::create_area(
        &s.pool,
        auth.user_id,
        &AreaInput {
            name: &name,
            slug: &slug,
            description: &b.description,
            color: b.color.as_deref(),
            position: b.position,
            archived: false,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "area",
        Some(a.id),
        "create",
        serde_json::json!({ "name": name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(a)))
}

#[utoipa::path(get, path = "/areas/{id}", tag = "areas", responses((status = 200, body = Area), (status = 404, body = crate::error::ErrorBody)))]
async fn get_area(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<Json<Area>> {
    Ok(Json(areas::get_area(&s.pool, auth.user_id, id).await?))
}

/// Renombrar o archivar un área conserva su ID y sus relaciones.
#[utoipa::path(put, path = "/areas/{id}", tag = "areas", request_body = AreaBody,
    responses((status = 200, body = Area), (status = 409, body = crate::error::ErrorBody)))]
async fn update_area(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<AreaBody>,
) -> ApiResult<Json<Area>> {
    let (name, slug) = validate_area(&b)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let a = areas::update_area(
        &s.pool,
        auth.user_id,
        id,
        version,
        &AreaInput {
            name: &name,
            slug: &slug,
            description: &b.description,
            color: b.color.as_deref(),
            position: b.position,
            archived: b.archived,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "area",
        Some(id),
        "update",
        serde_json::json!({ "name": name, "archived": b.archived }),
    )
    .await;
    Ok(Json(a))
}

#[derive(Deserialize, ToSchema)]
pub struct ProjectBody {
    pub area_id: Option<Uuid>,
    pub name: String,
    pub slug: Option<String>,
    #[serde(default)]
    pub description: String,
    /// `active | paused | done | archived`
    #[serde(default = "default_status")]
    pub status: String,
    pub repo_url: Option<String>,
    /// Lista de `{ "label": "...", "url": "https://..." }`.
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    pub links: serde_json::Value,
    #[serde(default)]
    pub notes_md: String,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

fn default_status() -> String {
    "active".into()
}

struct ValidProject {
    name: String,
    slug: String,
    repo: Option<String>,
    links: serde_json::Value,
}

fn validate_project(b: &ProjectBody) -> ApiResult<ValidProject> {
    let name = required("name", &b.name, 120)?;
    let slug = b.slug.as_deref().map(slugify).unwrap_or_else(|| slugify(&name));
    if !["active", "paused", "done", "archived"].contains(&b.status.as_str()) {
        return Err(ApiError::validation("status", "estado inválido"));
    }
    let repo = b
        .repo_url
        .as_deref()
        .filter(|r| !r.trim().is_empty())
        .map(|r| validate_external_url("repo_url", r))
        .transpose()?;
    let links = match &b.links {
        serde_json::Value::Null => serde_json::json!([]),
        serde_json::Value::Array(items) => {
            let mut out = Vec::new();
            for it in items {
                let url = it.get("url").and_then(|v| v.as_str()).unwrap_or_default();
                let label = it.get("label").and_then(|v| v.as_str()).unwrap_or(url);
                out.push(serde_json::json!({ "label": label.chars().take(120).collect::<String>(), "url": validate_external_url("links", url)? }));
            }
            serde_json::Value::Array(out)
        }
        _ => return Err(ApiError::validation("links", "debe ser una lista")),
    };
    Ok(ValidProject {
        name,
        slug,
        repo,
        links,
    })
}

#[derive(Deserialize, IntoParams)]
pub struct ProjectFilter {
    pub area_id: Option<Uuid>,
    pub archived: Option<bool>,
}

#[utoipa::path(get, path = "/projects", tag = "areas", params(ProjectFilter), responses((status = 200, body = Vec<Project>)))]
async fn list_projects(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(f): Query<ProjectFilter>,
) -> ApiResult<Json<Vec<Project>>> {
    Ok(Json(
        areas::list_projects(&s.pool, auth.user_id, f.area_id, f.archived.unwrap_or(false)).await?,
    ))
}

#[utoipa::path(post, path = "/projects", tag = "areas", request_body = ProjectBody, responses((status = 201, body = Project)))]
async fn create_project(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<ProjectBody>,
) -> ApiResult<(StatusCode, Json<Project>)> {
    let v = validate_project(&b)?;
    let p = areas::create_project(
        &s.pool,
        auth.user_id,
        &ProjectInput {
            area_id: b.area_id,
            name: &v.name,
            slug: &v.slug,
            description: &b.description,
            status: &b.status,
            repo_url: v.repo.as_deref(),
            links: &v.links,
            notes_md: &b.notes_md,
            archived: false,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "project",
        Some(p.id),
        "create",
        serde_json::json!({ "name": v.name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(p)))
}

#[utoipa::path(get, path = "/projects/{id}", tag = "areas", responses((status = 200, body = Project)))]
async fn get_project(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Project>> {
    Ok(Json(areas::get_project(&s.pool, auth.user_id, id).await?))
}

#[utoipa::path(put, path = "/projects/{id}", tag = "areas", request_body = ProjectBody,
    responses((status = 200, body = Project), (status = 409, body = crate::error::ErrorBody)))]
async fn update_project(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ProjectBody>,
) -> ApiResult<Json<Project>> {
    let v = validate_project(&b)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let p = areas::update_project(
        &s.pool,
        auth.user_id,
        id,
        version,
        &ProjectInput {
            area_id: b.area_id,
            name: &v.name,
            slug: &v.slug,
            description: &b.description,
            status: &b.status,
            repo_url: v.repo.as_deref(),
            links: &v.links,
            notes_md: &b.notes_md,
            archived: b.archived,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "project",
        Some(id),
        "update",
        serde_json::json!({ "name": v.name }),
    )
    .await;
    Ok(Json(p))
}

/// Actividad reciente relacionada con un proyecto (tareas, notas, ADRs, devlog, contenido).
#[utoipa::path(get, path = "/projects/{id}/activity", tag = "areas", responses((status = 200, body = Vec<ActivityItem>)))]
async fn project_activity(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ActivityItem>>> {
    areas::get_project(&s.pool, auth.user_id, id).await?;
    Ok(Json(
        areas::recent_activity(&s.pool, auth.user_id, Some(id), 50).await?,
    ))
}

/// "Dev reciente" (Proyectos.base): actividad de todos los proyectos.
#[utoipa::path(get, path = "/activity", tag = "areas", responses((status = 200, body = Vec<ActivityItem>)))]
async fn recent_activity(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<ActivityItem>>> {
    Ok(Json(
        areas::recent_activity(&s.pool, auth.user_id, None, 50).await?,
    ))
}
