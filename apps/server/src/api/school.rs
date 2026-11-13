//! Propietario: agente B-modules. T32 Materias, notas de clase por fecha y entregas como tareas.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::text::required;
use persistence::repos::school::{self as repo, ClassNote, ClassNoteInput, Subject};
use persistence::repos::tasks::{self as tasks_repo, Task, TaskFilter, TaskInput};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::tasks::user_today;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_subjects, create_subject))
        .routes(routes!(update_subject))
        .routes(routes!(list_class_notes, create_class_note))
        .routes(routes!(get_class_note, update_class_note))
        .routes(routes!(add_deliverable))
}

#[derive(Deserialize, IntoParams)]
pub struct ArchivedQuery {
    pub archived: Option<bool>,
}

#[utoipa::path(get, path = "/subjects", tag = "school", params(ArchivedQuery), responses((status = 200, body = Vec<Subject>)))]
async fn list_subjects(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ArchivedQuery>,
) -> ApiResult<Json<Vec<Subject>>> {
    Ok(Json(
        repo::list_subjects(&s.pool, auth.user_id, q.archived.unwrap_or(false)).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct SubjectBody {
    pub name: String,
    /// Por defecto el área con slug `escuela` si existe.
    pub area_id: Option<Uuid>,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

async fn school_area(s: &AppState, user_id: Uuid, area: Option<Uuid>) -> ApiResult<Option<Uuid>> {
    if area.is_some() {
        return Ok(area);
    }
    Ok(
        sqlx::query_scalar("SELECT id FROM areas WHERE user_id = $1 AND slug = 'escuela'")
            .bind(user_id)
            .fetch_optional(&s.pool)
            .await?,
    )
}

#[utoipa::path(post, path = "/subjects", tag = "school", request_body = SubjectBody, responses((status = 201, body = Subject)))]
async fn create_subject(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<SubjectBody>,
) -> ApiResult<(StatusCode, Json<Subject>)> {
    let name = required("name", &b.name, 120)?;
    let area = school_area(&s, auth.user_id, b.area_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(repo::create_subject(&s.pool, auth.user_id, &name, area).await?),
    ))
}

#[utoipa::path(put, path = "/subjects/{id}", tag = "school", request_body = SubjectBody, responses((status = 200, body = Subject)))]
async fn update_subject(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<SubjectBody>,
) -> ApiResult<Json<Subject>> {
    let name = required("name", &b.name, 120)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    Ok(Json(
        repo::update_subject(&s.pool, auth.user_id, id, version, &name, b.area_id, b.archived).await?,
    ))
}

#[derive(Deserialize, IntoParams)]
pub struct ClassNoteQuery {
    pub subject_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

#[utoipa::path(get, path = "/class-notes", tag = "school", params(ClassNoteQuery), responses((status = 200, body = Vec<ClassNote>)))]
async fn list_class_notes(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ClassNoteQuery>,
) -> ApiResult<Json<Vec<ClassNote>>> {
    Ok(Json(
        repo::list_notes(&s.pool, auth.user_id, q.subject_id, q.from, q.to).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct ClassNoteBody {
    pub subject_id: Uuid,
    pub on_date: Option<NaiveDate>,
    pub topic: String,
    #[serde(default)]
    pub topics_md: String,
    #[serde(default)]
    pub questions_md: String,
    pub version: Option<i32>,
}

#[utoipa::path(post, path = "/class-notes", tag = "school", request_body = ClassNoteBody, responses((status = 201, body = ClassNote)))]
async fn create_class_note(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<ClassNoteBody>,
) -> ApiResult<(StatusCode, Json<ClassNote>)> {
    let topic = required("topic", &b.topic, 200)?;
    let date = match b.on_date {
        Some(d) => d,
        None => user_today(&s, auth.user_id).await?.1,
    };
    let n = repo::create_note(
        &s.pool,
        auth.user_id,
        &ClassNoteInput {
            subject_id: b.subject_id,
            on_date: date,
            topic: &topic,
            topics_md: &b.topics_md,
            questions_md: &b.questions_md,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(n)))
}

#[derive(Serialize, ToSchema)]
pub struct ClassNoteDetail {
    pub note: ClassNote,
    /// Entregas/tareas relacionadas.
    pub deliverables: Vec<Task>,
}

#[utoipa::path(get, path = "/class-notes/{id}", tag = "school", responses((status = 200, body = ClassNoteDetail)))]
async fn get_class_note(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ClassNoteDetail>> {
    let note = repo::get_note(&s.pool, auth.user_id, id).await?;
    let (deliverables, _) = tasks_repo::list(
        &s.pool,
        auth.user_id,
        &TaskFilter {
            class_note_id: Some(id),
            limit: 200,
            ..Default::default()
        },
    )
    .await?;
    Ok(Json(ClassNoteDetail { note, deliverables }))
}

#[utoipa::path(put, path = "/class-notes/{id}", tag = "school", request_body = ClassNoteBody, responses((status = 200, body = ClassNote)))]
async fn update_class_note(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ClassNoteBody>,
) -> ApiResult<Json<ClassNote>> {
    let topic = required("topic", &b.topic, 200)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let cur = repo::get_note(&s.pool, auth.user_id, id).await?;
    Ok(Json(
        repo::update_note(
            &s.pool,
            auth.user_id,
            id,
            version,
            &ClassNoteInput {
                subject_id: b.subject_id,
                on_date: b.on_date.unwrap_or(cur.on_date),
                topic: &topic,
                topics_md: &b.topics_md,
                questions_md: &b.questions_md,
            },
        )
        .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct DeliverableBody {
    pub title: String,
    pub due_on: Option<NaiveDate>,
    pub priority: Option<i16>,
}

/// Crea una entrega como tarea ligada a la clase (área de la materia, aparece en la daily el día de entrega).
#[utoipa::path(post, path = "/class-notes/{id}/deliverables", tag = "school", request_body = DeliverableBody, responses((status = 201, body = Task)))]
async fn add_deliverable(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<DeliverableBody>,
) -> ApiResult<(StatusCode, Json<Task>)> {
    let title = required("title", &b.title, 300)?;
    let note = repo::get_note(&s.pool, auth.user_id, id).await?;
    let subject = repo::get_subject(&s.pool, auth.user_id, note.subject_id).await?;
    let mut tx = s.pool.begin().await?;
    let tid = tasks_repo::create(
        &mut tx,
        auth.user_id,
        &TaskInput {
            title: &title,
            notes_md: "",
            area_id: subject.area_id,
            project_id: None,
            priority: b.priority,
            due_on: b.due_on,
            planned_on: b.due_on,
            class_note_id: Some(id),
            person_ids: &[],
            archived: false,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(tasks_repo::get(&s.pool, auth.user_id, tid).await?),
    ))
}
