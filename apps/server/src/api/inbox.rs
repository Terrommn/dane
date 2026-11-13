//! Propietario: agente B-modules. T25 Inbox: captura rápida, propuesta revisable y procesamiento solo con confirmación.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::tasks::propose_inbox;
use domain::text::required;
use persistence::repos::inbox::{self as repo, InboxItem};
use persistence::repos::notes::{self as notes_repo, NoteInput};
use persistence::repos::people::{self as people_repo, PersonInput};
use persistence::repos::tasks::{self as tasks_repo, TaskInput};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_inbox, capture))
        .routes(routes!(edit_item))
        .routes(routes!(propose))
        .routes(routes!(apply))
        .routes(routes!(reopen))
}

#[derive(Deserialize, IntoParams)]
pub struct InboxQuery {
    /// `pending` (por defecto), `processed`, `discarded`, `archived`, `all`.
    pub status: Option<String>,
}

#[utoipa::path(get, path = "/inbox", tag = "inbox", params(InboxQuery), responses((status = 200, body = Vec<InboxItem>)))]
async fn list_inbox(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<InboxQuery>,
) -> ApiResult<Json<Vec<InboxItem>>> {
    let st = match q.status.as_deref() {
        Some("all") => None,
        Some(x) => Some(x),
        None => Some("pending"),
    };
    Ok(Json(repo::list(&s.pool, auth.user_id, st).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct CaptureBody {
    pub text: String,
    /// ID generado en el dispositivo: reenviar la misma captura (offline) no duplica.
    pub client_id: Option<String>,
    /// `app` (por defecto) o `telegram`.
    pub source: Option<String>,
}

/// Captura sin clasificar. 201 si es nueva, 200 si el `client_id` ya existía.
#[utoipa::path(post, path = "/inbox", tag = "inbox", request_body = CaptureBody, responses((status = 201, body = InboxItem), (status = 200, body = InboxItem)))]
async fn capture(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<CaptureBody>,
) -> ApiResult<(StatusCode, Json<InboxItem>)> {
    let text = required("text", &b.text, 4000)?;
    let source = b.source.as_deref().unwrap_or("app");
    if !["app", "telegram"].contains(&source) {
        return Err(ApiError::validation("source", "app o telegram"));
    }
    if let Some(c) = &b.client_id
        && (c.is_empty() || c.len() > 100)
    {
        return Err(ApiError::validation("client_id", "1 a 100 caracteres"));
    }
    let (item, created) = repo::capture(&s.pool, auth.user_id, &text, source, b.client_id.as_deref()).await?;
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(item),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct EditBody {
    pub text: Option<String>,
    /// Propuesta editada por el usuario (se guarda como `manual`).
    #[schema(value_type = Option<Object>)]
    pub proposal: Option<serde_json::Value>,
    pub version: i32,
}

#[utoipa::path(put, path = "/inbox/{id}", tag = "inbox", request_body = EditBody, responses((status = 200, body = InboxItem), (status = 409, body = crate::error::ErrorBody)))]
async fn edit_item(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<EditBody>,
) -> ApiResult<Json<InboxItem>> {
    let text = b.text.as_deref().map(|t| required("text", t, 4000)).transpose()?;
    let cur = repo::get(&s.pool, auth.user_id, id).await?;
    let proposal = b.proposal.or(cur.proposal).unwrap_or(serde_json::json!(null));
    Ok(Json(
        repo::set_proposal(
            &s.pool,
            auth.user_id,
            id,
            Some(b.version),
            text.as_deref(),
            &proposal,
            "manual",
        )
        .await?,
    ))
}

/// Calcula una propuesta determinista (no mueve nada). La IA, si está configurada, la propone el módulo de capturas.
#[utoipa::path(post, path = "/inbox/{id}/propose", tag = "inbox", responses((status = 200, body = InboxItem)))]
async fn propose(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<InboxItem>> {
    let item = repo::get(&s.pool, auth.user_id, id).await?;
    if item.status != "pending" {
        return Err(ApiError::validation("status", "el elemento ya se procesó"));
    }
    let p = propose_inbox(&item.text);
    let area_id: Option<Uuid> = match &p.area_slug {
        Some(slug) => {
            sqlx::query_scalar("SELECT id FROM areas WHERE user_id = $1 AND slug = $2")
                .bind(auth.user_id)
                .bind(slug)
                .fetch_optional(&s.pool)
                .await?
        }
        None => None,
    };
    let mut json = serde_json::to_value(&p).map_err(|_| ApiError::internal())?;
    json["area_id"] = serde_json::json!(area_id);
    Ok(Json(
        repo::set_proposal(&s.pool, auth.user_id, id, None, None, &json, "rule").await?,
    ))
}

#[derive(Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InboxAction {
    Task {
        title: String,
        area_id: Option<Uuid>,
        project_id: Option<Uuid>,
        planned_on: Option<NaiveDate>,
        due_on: Option<NaiveDate>,
        priority: Option<i16>,
        #[serde(default)]
        notes_md: String,
    },
    Note {
        title: String,
        #[serde(default)]
        body_md: String,
        #[serde(default)]
        tags: Vec<String>,
        area_id: Option<Uuid>,
        project_id: Option<Uuid>,
    },
    Person {
        name: String,
        #[serde(default)]
        aliases: Vec<String>,
        area_id: Option<Uuid>,
        #[serde(default)]
        context_md: String,
    },
    Discard,
    Archive,
}

#[derive(Deserialize, ToSchema)]
pub struct InboxApplyBody {
    /// Versión vista por el usuario al revisar.
    pub version: i32,
    /// Acción aceptada o editada explícitamente por el usuario.
    pub action: InboxAction,
}

#[derive(Serialize, ToSchema)]
pub struct ApplyResult {
    pub item: InboxItem,
    pub result_type: Option<String>,
    pub result_id: Option<Uuid>,
}

/// Aplica la decisión del usuario: crea el destino y cierra el elemento en una sola transacción.
#[utoipa::path(post, path = "/inbox/{id}/apply", tag = "inbox", request_body = InboxApplyBody, responses((status = 200, body = ApplyResult), (status = 409, body = crate::error::ErrorBody)))]
async fn apply(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<InboxApplyBody>,
) -> ApiResult<Json<ApplyResult>> {
    let item = repo::get(&s.pool, auth.user_id, id).await?;
    if item.status != "pending" || item.version != b.version {
        return Err(ApiError::conflict(
            serde_json::to_value(&item).unwrap_or_default(),
        ));
    }
    let mut tx = s.pool.begin().await?;
    let (status, result, decision): (&str, Option<(&str, Uuid)>, serde_json::Value) = match &b.action {
        InboxAction::Task {
            title,
            area_id,
            project_id,
            planned_on,
            due_on,
            priority,
            notes_md,
        } => {
            let title = required("title", title, 300)?;
            let tid = tasks_repo::create(
                &mut tx,
                auth.user_id,
                &TaskInput {
                    title: &title,
                    notes_md,
                    area_id: *area_id,
                    project_id: *project_id,
                    priority: *priority,
                    due_on: *due_on,
                    planned_on: *planned_on,
                    class_note_id: None,
                    person_ids: &[],
                    archived: false,
                },
            )
            .await?;
            (
                "processed",
                Some(("task", tid)),
                serde_json::json!({ "destination": "task", "title": title }),
            )
        }
        InboxAction::Note {
            title,
            body_md,
            tags,
            area_id,
            project_id,
        } => {
            let title = required("title", title, 300)?;
            let tags = crate::api::notes::clean_tags(tags)?;
            let body = if body_md.trim().is_empty() {
                item.text.clone()
            } else {
                body_md.clone()
            };
            let n = notes_repo::create(
                &mut *tx,
                auth.user_id,
                &NoteInput {
                    kind: "note",
                    title: &title,
                    body_md: &body,
                    language: None,
                    aliases: &[],
                    tags: &tags,
                    area_id: *area_id,
                    project_id: *project_id,
                    metadata: &serde_json::json!({ "from_inbox": id }),
                    archived: false,
                },
            )
            .await?;
            (
                "processed",
                Some(("note", n.id)),
                serde_json::json!({ "destination": "note", "title": title }),
            )
        }
        InboxAction::Person {
            name,
            aliases,
            area_id,
            context_md,
        } => {
            let name = required("name", name, 120)?;
            let p = people_repo::create(
                &mut *tx,
                auth.user_id,
                &PersonInput {
                    name: &name,
                    aliases,
                    area_id: *area_id,
                    relationship: None,
                    org: None,
                    role: None,
                    contact: None,
                    email: None,
                    context_md,
                    birthday: (None, None, None),
                    archived: false,
                },
            )
            .await?;
            (
                "processed",
                Some(("person", p.id)),
                serde_json::json!({ "destination": "person", "name": name }),
            )
        }
        InboxAction::Discard => ("discarded", None, serde_json::json!({ "destination": "discard" })),
        InboxAction::Archive => ("archived", None, serde_json::json!({ "destination": "archive" })),
    };
    if !repo::resolve(&mut tx, auth.user_id, id, b.version, status, &decision, result).await? {
        // Otro dispositivo lo procesó entre la lectura y ahora: se revierte todo.
        tx.rollback().await?;
        let cur = repo::get(&s.pool, auth.user_id, id).await?;
        return Err(ApiError::conflict(serde_json::to_value(&cur).unwrap_or_default()));
    }
    tx.commit().await?;
    audit(&s, auth.user_id, "inbox", Some(id), "apply", decision).await;
    Ok(Json(ApplyResult {
        item: repo::get(&s.pool, auth.user_id, id).await?,
        result_type: result.map(|r| r.0.to_string()),
        result_id: result.map(|r| r.1),
    }))
}

/// Recupera un elemento archivado o descartado a pendiente.
#[utoipa::path(post, path = "/inbox/{id}/reopen", tag = "inbox", responses((status = 200, body = InboxItem)))]
async fn reopen(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<InboxItem>> {
    Ok(Json(repo::reopen(&s.pool, auth.user_id, id).await?))
}
