//! Propietario: agente B-modules. T34 Notas Markdown, tags, alias, enlaces/backlinks, favoritos, archivados y vista
//! Proyectos.base. T33: runbooks, TILs y snippets son `notes.kind`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::text::required;
use persistence::repos::areas::{self, ActivityItem};
use persistence::repos::dev::Adr;
use persistence::repos::notes::{self as repo, EntityRef, LinkView, Note, NoteFilter, NoteInput, TagCount};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::http::Page;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_notes, create_note))
        .routes(routes!(get_note, update_note))
        .routes(routes!(list_tags))
        .routes(routes!(get_links, add_link, remove_link))
        .routes(routes!(list_favorites))
        .routes(routes!(add_favorite, remove_favorite))
        .routes(routes!(list_archived))
        .routes(routes!(archive_entity))
        .routes(routes!(restore_entity))
        .routes(routes!(projects_view))
}

const KINDS: [&str; 4] = ["note", "runbook", "til", "snippet"];

#[derive(Deserialize, ToSchema)]
pub struct NoteBody {
    /// `note` (por defecto), `runbook`, `til`, `snippet`.
    #[serde(default = "default_kind")]
    pub kind: String,
    pub title: String,
    /// Markdown; la web lo renderiza sin HTML crudo.
    #[serde(default)]
    pub body_md: String,
    pub language: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Tags tipo `lang/rust`, `tool/git`.
    #[serde(default)]
    pub tags: Vec<String>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

fn default_kind() -> String {
    "note".into()
}

pub fn clean_tags(tags: &[String]) -> ApiResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t = t.trim().trim_start_matches('#').to_lowercase();
        if t.is_empty() {
            continue;
        }
        if t.len() > 60 || t.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ApiError::validation(
                "tags",
                "tags sin espacios, máximo 60 caracteres",
            ));
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    if out.len() > 30 {
        return Err(ApiError::validation("tags", "máximo 30 tags"));
    }
    Ok(out)
}

struct Valid {
    title: String,
    aliases: Vec<String>,
    tags: Vec<String>,
    metadata: serde_json::Value,
}

fn validate(b: &NoteBody) -> ApiResult<Valid> {
    if !KINDS.contains(&b.kind.as_str()) {
        return Err(ApiError::validation("kind", "note, runbook, til o snippet"));
    }
    let title = required("title", &b.title, 300)?;
    if b.body_md.len() > 1_000_000 {
        return Err(ApiError::validation("body_md", "máximo 1 MB"));
    }
    let metadata = match &b.metadata {
        serde_json::Value::Null => serde_json::json!({}),
        v @ serde_json::Value::Object(_) => v.clone(),
        _ => return Err(ApiError::validation("metadata", "debe ser un objeto")),
    };
    let aliases = b
        .aliases
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    Ok(Valid {
        title,
        aliases,
        tags: clean_tags(&b.tags)?,
        metadata,
    })
}

fn input<'a>(b: &'a NoteBody, v: &'a Valid) -> NoteInput<'a> {
    NoteInput {
        kind: &b.kind,
        title: &v.title,
        body_md: &b.body_md,
        language: b.language.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        aliases: &v.aliases,
        tags: &v.tags,
        area_id: b.area_id,
        project_id: b.project_id,
        metadata: &v.metadata,
        archived: b.archived,
    }
}

#[derive(Deserialize, IntoParams)]
pub struct NoteQuery {
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub q: Option<String>,
    /// `true`: solo archivadas (para recuperarlas).
    pub archived: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[utoipa::path(get, path = "/notes", tag = "notes", params(NoteQuery), responses((status = 200, body = Page<Note>)))]
async fn list_notes(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<NoteQuery>,
) -> ApiResult<Json<Page<Note>>> {
    let f = NoteFilter {
        kind: q.kind,
        tag: q.tag.map(|t| t.trim_start_matches('#').to_lowercase()),
        area_id: q.area_id,
        project_id: q.project_id,
        q: q.q.filter(|x| !x.trim().is_empty()),
        archived: q.archived.unwrap_or(false),
        limit: q.limit.unwrap_or(50).clamp(1, 200),
        offset: q.offset.unwrap_or(0).max(0),
    };
    let (items, total) = repo::list(&s.pool, auth.user_id, &f).await?;
    Ok(Json(Page {
        items,
        total,
        limit: f.limit,
        offset: f.offset,
    }))
}

#[utoipa::path(post, path = "/notes", tag = "notes", request_body = NoteBody, responses((status = 201, body = Note)))]
async fn create_note(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<NoteBody>,
) -> ApiResult<(StatusCode, Json<Note>)> {
    let v = validate(&b)?;
    let n = repo::create(&s.pool, auth.user_id, &input(&b, &v)).await?;
    audit(
        &s,
        auth.user_id,
        "note",
        Some(n.id),
        "create",
        serde_json::json!({ "kind": n.kind }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(n)))
}

#[derive(Serialize, ToSchema)]
pub struct NoteDetail {
    pub note: Note,
    pub links: Vec<LinkView>,
    pub backlinks: Vec<LinkView>,
    pub favorite: bool,
}

#[utoipa::path(get, path = "/notes/{id}", tag = "notes", responses((status = 200, body = NoteDetail)))]
async fn get_note(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<NoteDetail>> {
    let note = repo::get(&s.pool, auth.user_id, id).await?;
    let favorite: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM favorites WHERE user_id = $1 AND entity_type = 'note' AND entity_id = $2)")
        .bind(auth.user_id)
        .bind(id)
        .fetch_one(&s.pool)
        .await?;
    Ok(Json(NoteDetail {
        links: repo::outgoing(&s.pool, auth.user_id, "note", id).await?,
        backlinks: repo::backlinks(&s.pool, auth.user_id, "note", id).await?,
        favorite,
        note,
    }))
}

#[utoipa::path(put, path = "/notes/{id}", tag = "notes", request_body = NoteBody, responses((status = 200, body = Note), (status = 409, body = crate::error::ErrorBody)))]
async fn update_note(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<NoteBody>,
) -> ApiResult<Json<Note>> {
    let v = validate(&b)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let n = repo::update(&s.pool, auth.user_id, id, version, &input(&b, &v)).await?;
    audit(
        &s,
        auth.user_id,
        "note",
        Some(id),
        "update",
        serde_json::json!({}),
    )
    .await;
    Ok(Json(n))
}

#[utoipa::path(get, path = "/tags", tag = "notes", responses((status = 200, body = Vec<TagCount>)))]
async fn list_tags(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<TagCount>>> {
    Ok(Json(repo::tags(&s.pool, auth.user_id).await?))
}

#[derive(Deserialize, IntoParams)]
pub struct EntityQuery {
    #[serde(rename = "type")]
    #[param(rename = "type")]
    pub entity_type: String,
    pub id: Uuid,
}

#[derive(Serialize, ToSchema)]
pub struct EntityLinks {
    pub links: Vec<LinkView>,
    pub backlinks: Vec<LinkView>,
}

/// Enlaces salientes y backlinks de cualquier entidad del usuario.
#[utoipa::path(get, path = "/links", tag = "notes", params(EntityQuery), responses((status = 200, body = EntityLinks)))]
async fn get_links(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<EntityQuery>,
) -> ApiResult<Json<EntityLinks>> {
    if !repo::entity_exists(&s.pool, auth.user_id, &q.entity_type, q.id).await? {
        return Err(ApiError::not_found());
    }
    Ok(Json(EntityLinks {
        links: repo::outgoing(&s.pool, auth.user_id, &q.entity_type, q.id).await?,
        backlinks: repo::backlinks(&s.pool, auth.user_id, &q.entity_type, q.id).await?,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct LinkBody {
    pub src_type: String,
    pub src_id: Uuid,
    pub dst_type: String,
    pub dst_id: Uuid,
    /// `related` (por defecto) o `contributes_to` (aporte explícito a un objetivo).
    pub relation: Option<String>,
}

fn relation(b: &LinkBody) -> ApiResult<String> {
    let r = b.relation.clone().unwrap_or_else(|| "related".into());
    if !["related", "contributes_to", "mentions", "deliverable_of"].contains(&r.as_str()) {
        return Err(ApiError::validation("relation", "relación no válida"));
    }
    if r == "contributes_to" && b.dst_type != "goal" {
        return Err(ApiError::validation(
            "dst_type",
            "contributes_to solo apunta a objetivos",
        ));
    }
    Ok(r)
}

/// Crea un enlace validando que ambos extremos existan y sean del usuario.
#[utoipa::path(post, path = "/links", tag = "notes", request_body = LinkBody, responses((status = 204), (status = 404, body = crate::error::ErrorBody)))]
async fn add_link(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<LinkBody>,
) -> ApiResult<StatusCode> {
    let rel = relation(&b)?;
    if b.src_type == b.dst_type && b.src_id == b.dst_id {
        return Err(ApiError::validation(
            "dst_id",
            "no se puede enlazar consigo misma",
        ));
    }
    for (t, id) in [(&b.src_type, b.src_id), (&b.dst_type, b.dst_id)] {
        if repo::table_of(t).is_none() {
            return Err(ApiError::validation("type", format!("tipo desconocido: {t}")));
        }
        if !repo::entity_exists(&s.pool, auth.user_id, t, id).await? {
            return Err(ApiError::not_found());
        }
    }
    repo::add_link(
        &s.pool,
        auth.user_id,
        (&b.src_type, b.src_id),
        (&b.dst_type, b.dst_id),
        &rel,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/links", tag = "notes", request_body = LinkBody, responses((status = 204)))]
async fn remove_link(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<LinkBody>,
) -> ApiResult<StatusCode> {
    let rel = relation(&b)?;
    repo::remove_link(
        &s.pool,
        auth.user_id,
        (&b.src_type, b.src_id),
        (&b.dst_type, b.dst_id),
        &rel,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/favorites", tag = "notes", responses((status = 200, body = Vec<EntityRef>)))]
async fn list_favorites(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<EntityRef>>> {
    Ok(Json(repo::favorites(&s.pool, auth.user_id).await?))
}

#[utoipa::path(put, path = "/favorites/{entity_type}/{id}", tag = "notes", responses((status = 204)))]
async fn add_favorite(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((t, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    if !repo::entity_exists(&s.pool, auth.user_id, &t, id).await? {
        return Err(ApiError::not_found());
    }
    repo::set_favorite(&s.pool, auth.user_id, &t, id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/favorites/{entity_type}/{id}", tag = "notes", responses((status = 204)))]
async fn remove_favorite(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((t, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    repo::set_favorite(&s.pool, auth.user_id, &t, id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Elementos archivados de todos los módulos (para recuperarlos).
#[utoipa::path(get, path = "/archive", tag = "notes", responses((status = 200, body = Vec<EntityRef>)))]
async fn list_archived(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<EntityRef>>> {
    Ok(Json(repo::archived(&s.pool, auth.user_id).await?))
}

#[utoipa::path(post, path = "/archive/{entity_type}/{id}", tag = "notes", responses((status = 204)))]
async fn archive_entity(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((t, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    repo::set_archived(&s.pool, auth.user_id, &t, id, true).await?;
    audit(&s, auth.user_id, &t, Some(id), "archive", serde_json::json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/archive/{entity_type}/{id}/restore", tag = "notes", responses((status = 204)))]
async fn restore_entity(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((t, id)): Path<(String, Uuid)>,
) -> ApiResult<StatusCode> {
    repo::set_archived(&s.pool, auth.user_id, &t, id, false).await?;
    audit(&s, auth.user_id, &t, Some(id), "restore", serde_json::json!({})).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct ProjectsView {
    /// "Dev reciente" (excluye devocionales, clases, objetivos y finanzas).
    pub recent: Vec<ActivityItem>,
    pub adrs: Vec<Adr>,
}

/// Vista Proyectos.base: actividad reciente + ADRs.
#[utoipa::path(get, path = "/views/projects", tag = "notes", responses((status = 200, body = ProjectsView)))]
async fn projects_view(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<ProjectsView>> {
    Ok(Json(ProjectsView {
        recent: areas::recent_activity(&s.pool, auth.user_id, None, 50).await?,
        adrs: persistence::repos::dev::list_adrs(&s.pool, auth.user_id, None, None).await?,
    }))
}
