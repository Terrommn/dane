//! Propietario: agente B-modules. T24 Tareas, arrastre, vista diaria por área y pendientes recurrentes.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use chrono_tz::Tz;
use domain::tasks::due_occurrences;
use domain::text::required;
use domain::time::{DayOfMonth, local_date, parse_tz};
use persistence::repos::areas;
use persistence::repos::tasks::{
    self as repo, RecurrenceRule, RuleInput, Task, TaskEvent, TaskFilter, TaskInput,
};
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
        .routes(routes!(list_tasks, create_task))
        .routes(routes!(get_task, update_task, archive_task))
        .routes(routes!(complete_task))
        .routes(routes!(reopen_task))
        .routes(routes!(cancel_task))
        .routes(routes!(carry_over))
        .routes(routes!(tasks_for_day))
        .routes(routes!(list_rules, create_rule))
        .routes(routes!(update_rule))
        .routes(routes!(materialize_rules))
}

/// Zona y fecha local de hoy del usuario.
pub async fn user_today(s: &AppState, user_id: Uuid) -> ApiResult<(Tz, NaiveDate)> {
    let tz_name: Option<String> = sqlx::query_scalar("SELECT timezone FROM user_settings WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&s.pool)
        .await?;
    let tz = parse_tz(tz_name.as_deref().unwrap_or("America/Monterrey"))?;
    Ok((tz, local_date(s.now(), tz)))
}

#[derive(Deserialize, ToSchema)]
pub struct TaskBody {
    pub title: String,
    #[serde(default)]
    pub notes_md: String,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// 1 (alta) a 4 (baja).
    pub priority: Option<i16>,
    pub due_on: Option<NaiveDate>,
    /// Día en que aparece en la daily; las abiertas se arrastran sin duplicarse.
    pub planned_on: Option<NaiveDate>,
    pub class_note_id: Option<Uuid>,
    #[serde(default)]
    pub person_ids: Vec<Uuid>,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

fn validate(b: &TaskBody) -> ApiResult<String> {
    let title = required("title", &b.title, 300)?;
    if let Some(p) = b.priority
        && !(1..=4).contains(&p)
    {
        return Err(ApiError::validation("priority", "1 a 4"));
    }
    if b.person_ids.len() > 50 {
        return Err(ApiError::validation("person_ids", "máximo 50 personas"));
    }
    Ok(title)
}

fn input<'a>(b: &'a TaskBody, title: &'a str) -> TaskInput<'a> {
    TaskInput {
        title,
        notes_md: &b.notes_md,
        area_id: b.area_id,
        project_id: b.project_id,
        priority: b.priority,
        due_on: b.due_on,
        planned_on: b.planned_on,
        class_note_id: b.class_note_id,
        person_ids: &b.person_ids,
        archived: b.archived,
    }
}

#[derive(Deserialize, IntoParams, Default)]
pub struct TaskQuery {
    /// `open | done | cancelled`
    pub status: Option<String>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub class_note_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub q: Option<String>,
    /// Solo externas (true) o solo locales (false).
    pub external: Option<bool>,
    pub archived: Option<bool>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[utoipa::path(get, path = "/tasks", tag = "tasks", params(TaskQuery), responses((status = 200, body = Page<Task>)))]
async fn list_tasks(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<TaskQuery>,
) -> ApiResult<Json<Page<Task>>> {
    let f = TaskFilter {
        status: q.status,
        area_id: q.area_id,
        project_id: q.project_id,
        class_note_id: q.class_note_id,
        from: q.from,
        to: q.to,
        q: q.q.filter(|x| !x.trim().is_empty()),
        external: q.external,
        archived: q.archived.unwrap_or(false),
        limit: q.limit.unwrap_or(100).clamp(1, 500),
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

#[utoipa::path(post, path = "/tasks", tag = "tasks", request_body = TaskBody, responses((status = 201, body = Task)))]
async fn create_task(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<TaskBody>,
) -> ApiResult<(StatusCode, Json<Task>)> {
    let title = validate(&b)?;
    let mut tx = s.pool.begin().await?;
    let id = repo::create(&mut tx, auth.user_id, &input(&b, &title)).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "task",
        Some(id),
        "create",
        serde_json::json!({ "title": title }),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(repo::get(&s.pool, auth.user_id, id).await?),
    ))
}

#[derive(Serialize, ToSchema)]
pub struct TaskDetail {
    pub task: Task,
    pub events: Vec<TaskEvent>,
}

#[utoipa::path(get, path = "/tasks/{id}", tag = "tasks", responses((status = 200, body = TaskDetail), (status = 404, body = crate::error::ErrorBody)))]
async fn get_task(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<TaskDetail>> {
    let task = repo::get(&s.pool, auth.user_id, id).await?;
    let events = repo::events(&s.pool, auth.user_id, id).await?;
    Ok(Json(TaskDetail { task, events }))
}

#[utoipa::path(put, path = "/tasks/{id}", tag = "tasks", request_body = TaskBody,
    responses((status = 200, body = Task), (status = 409, body = crate::error::ErrorBody)))]
async fn update_task(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<TaskBody>,
) -> ApiResult<Json<Task>> {
    let title = validate(&b)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let t = repo::update(&s.pool, auth.user_id, id, version, &input(&b, &title)).await?;
    audit(
        &s,
        auth.user_id,
        "task",
        Some(id),
        "update",
        serde_json::json!({ "title": title }),
    )
    .await;
    Ok(Json(t))
}

/// Archiva (no borra). Se recupera con `POST /archive/task/{id}/restore`.
#[utoipa::path(delete, path = "/tasks/{id}", tag = "tasks", responses((status = 204)))]
async fn archive_task(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let n = sqlx::query("UPDATE tasks SET archived_at = coalesce(archived_at, now()), version = version + 1 WHERE user_id = $1 AND id = $2")
        .bind(auth.user_id)
        .bind(id)
        .execute(&s.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::not_found());
    }
    audit(
        &s,
        auth.user_id,
        "task",
        Some(id),
        "archive",
        serde_json::json!({}),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema, Default)]
pub struct TaskVersionBody {
    /// Opcional: si se envía, se valida contra la versión actual.
    pub version: Option<i32>,
}

/// Marca como hecha. En tareas de ClickUp solo cambia el estado local (no modifica ClickUp).
#[utoipa::path(post, path = "/tasks/{id}/complete", tag = "tasks", request_body = TaskVersionBody, responses((status = 200, body = Task)))]
async fn complete_task(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    b: Option<Json<TaskVersionBody>>,
) -> ApiResult<Json<Task>> {
    let v = b.and_then(|b| b.0.version);
    Ok(Json(
        repo::set_status(&s.pool, auth.user_id, id, v, "done").await?,
    ))
}

#[utoipa::path(post, path = "/tasks/{id}/reopen", tag = "tasks", request_body = TaskVersionBody, responses((status = 200, body = Task)))]
async fn reopen_task(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    b: Option<Json<TaskVersionBody>>,
) -> ApiResult<Json<Task>> {
    let v = b.and_then(|b| b.0.version);
    Ok(Json(
        repo::set_status(&s.pool, auth.user_id, id, v, "open").await?,
    ))
}

#[utoipa::path(post, path = "/tasks/{id}/cancel", tag = "tasks", request_body = TaskVersionBody, responses((status = 200, body = Task)))]
async fn cancel_task(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    b: Option<Json<TaskVersionBody>>,
) -> ApiResult<Json<Task>> {
    let v = b.and_then(|b| b.0.version);
    Ok(Json(
        repo::set_status(&s.pool, auth.user_id, id, v, "cancelled").await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct CarryOverBody {
    /// Fecha destino; por defecto hoy en la zona del usuario.
    pub to_date: Option<NaiveDate>,
}

#[derive(Serialize, ToSchema)]
pub struct CarryOverResult {
    pub to_date: NaiveDate,
    /// IDs movidos (los mismos de antes: no se crean tareas nuevas).
    pub moved: Vec<Uuid>,
}

/// Arrastra las pendientes abiertas de días anteriores al día indicado, conservando ID e historial.
#[utoipa::path(post, path = "/tasks/carry-over", tag = "tasks", request_body = CarryOverBody, responses((status = 200, body = CarryOverResult)))]
async fn carry_over(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<CarryOverBody>,
) -> ApiResult<Json<CarryOverResult>> {
    let to = match b.to_date {
        Some(d) => d,
        None => user_today(&s, auth.user_id).await?.1,
    };
    let moved = repo::carry_over(&s.pool, auth.user_id, to).await?;
    Ok(Json(CarryOverResult { to_date: to, moved }))
}

#[derive(Serialize, ToSchema)]
pub struct TaskDayArea {
    pub area_id: Option<Uuid>,
    pub area_name: String,
    pub tasks: Vec<Task>,
}

#[derive(Serialize, ToSchema)]
pub struct DayTasks {
    pub date: NaiveDate,
    pub groups: Vec<TaskDayArea>,
    pub open_count: usize,
    pub done_count: usize,
}

/// Pendientes del día agrupados por área (Personal/Trabajo/Negocio/Escuela + las que agregue el usuario).
#[utoipa::path(get, path = "/tasks/day/{date}", tag = "tasks", responses((status = 200, body = DayTasks)))]
async fn tasks_for_day(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
) -> ApiResult<Json<DayTasks>> {
    let (tz, _) = user_today(&s, auth.user_id).await?;
    let tasks = repo::for_day(&s.pool, auth.user_id, date, tz.name()).await?;
    let area_list = areas::list_areas(&s.pool, auth.user_id, true).await?;
    let mut groups: Vec<TaskDayArea> = area_list
        .iter()
        .filter(|a| a.archived_at.is_none())
        .map(|a| TaskDayArea {
            area_id: Some(a.id),
            area_name: a.name.clone(),
            tasks: Vec::new(),
        })
        .collect();
    let mut none = TaskDayArea {
        area_id: None,
        area_name: "Sin área".into(),
        tasks: Vec::new(),
    };
    let (mut open_count, mut done_count) = (0, 0);
    for t in tasks {
        if t.status == "done" {
            done_count += 1;
        } else {
            open_count += 1;
        }
        match groups
            .iter_mut()
            .find(|g| g.area_id.is_some() && g.area_id == t.area_id)
        {
            Some(g) => g.tasks.push(t),
            None => {
                // Área archivada: se muestra con su nombre para no esconder pendientes.
                if let Some(a) = area_list.iter().find(|a| Some(a.id) == t.area_id) {
                    groups.push(TaskDayArea {
                        area_id: Some(a.id),
                        area_name: a.name.clone(),
                        tasks: vec![t],
                    });
                } else {
                    none.tasks.push(t);
                }
            }
        }
    }
    if !none.tasks.is_empty() {
        groups.push(none);
    }
    Ok(Json(DayTasks {
        date,
        groups,
        open_count,
        done_count,
    }))
}

// ---------- Recurrentes ----------

#[derive(Deserialize, ToSchema)]
pub struct TaskRuleBody {
    pub title: String,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// `day` (con `day` 1–31), `last` o `penultimate`.
    pub day_kind: String,
    pub day: Option<i16>,
    #[serde(default = "yes")]
    pub active: bool,
    /// Por defecto hoy: no se generan ocurrencias anteriores a la creación.
    pub starts_on: Option<NaiveDate>,
    pub version: Option<i32>,
}

fn yes() -> bool {
    true
}

#[utoipa::path(get, path = "/task-recurrences", tag = "tasks", responses((status = 200, body = Vec<RecurrenceRule>)))]
async fn list_rules(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<RecurrenceRule>>> {
    Ok(Json(repo::list_rules(&s.pool, auth.user_id).await?))
}

#[utoipa::path(post, path = "/task-recurrences", tag = "tasks", request_body = TaskRuleBody, responses((status = 201, body = RecurrenceRule)))]
async fn create_rule(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<TaskRuleBody>,
) -> ApiResult<(StatusCode, Json<RecurrenceRule>)> {
    let title = required("title", &b.title, 300)?;
    DayOfMonth::from_parts(&b.day_kind, b.day)?;
    let starts_on = match b.starts_on {
        Some(d) => d,
        None => user_today(&s, auth.user_id).await?.1,
    };
    let r = repo::create_rule(
        &s.pool,
        auth.user_id,
        &RuleInput {
            title: &title,
            area_id: b.area_id,
            project_id: b.project_id,
            day_kind: &b.day_kind,
            day: b.day,
            active: b.active,
            starts_on,
        },
    )
    .await?;
    materialize_user(&s, auth.user_id).await?;
    Ok((StatusCode::CREATED, Json(r)))
}

/// Editar o desactivar una regla no modifica las tareas ya generadas.
#[utoipa::path(put, path = "/task-recurrences/{id}", tag = "tasks", request_body = TaskRuleBody, responses((status = 200, body = RecurrenceRule)))]
async fn update_rule(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<TaskRuleBody>,
) -> ApiResult<Json<RecurrenceRule>> {
    let title = required("title", &b.title, 300)?;
    DayOfMonth::from_parts(&b.day_kind, b.day)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let current = repo::get_rule(&s.pool, auth.user_id, id).await?;
    let r = repo::update_rule(
        &s.pool,
        auth.user_id,
        id,
        version,
        &RuleInput {
            title: &title,
            area_id: b.area_id,
            project_id: b.project_id,
            day_kind: &b.day_kind,
            day: b.day,
            active: b.active,
            starts_on: b.starts_on.unwrap_or(current.starts_on),
        },
    )
    .await?;
    Ok(Json(r))
}

#[derive(Serialize, ToSchema)]
pub struct TaskMaterializeResult {
    pub created: u32,
}

/// Genera las ocurrencias pendientes hasta hoy (idempotente; también lo hace el worker periódicamente).
#[utoipa::path(post, path = "/task-recurrences/materialize", tag = "tasks", responses((status = 200, body = TaskMaterializeResult)))]
async fn materialize_rules(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<TaskMaterializeResult>> {
    Ok(Json(TaskMaterializeResult {
        created: materialize_user(&s, auth.user_id).await?,
    }))
}

/// Materializa las ocurrencias de todas las reglas activas del usuario hasta su "hoy" local.
pub async fn materialize_user(s: &AppState, user_id: Uuid) -> ApiResult<u32> {
    let (_, today) = user_today(s, user_id).await?;
    let mut created = 0;
    for rule in repo::list_rules(&s.pool, user_id)
        .await?
        .into_iter()
        .filter(|r| r.active)
    {
        let dom = DayOfMonth::from_parts(&rule.day_kind, rule.day)?;
        for o in due_occurrences(dom, rule.starts_on, today) {
            if repo::insert_occurrence(&s.pool, user_id, &rule, &o.period, o.date).await? {
                created += 1;
            }
        }
    }
    Ok(created)
}

/// Tareas periódicas del módulo (materialización idempotente de recurrencias).
pub fn workers(state: &AppState, shutdown: &crate::workers::Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    let s = state.clone();
    let sd = shutdown.clone();
    vec![tokio::spawn(async move {
        loop {
            match repo::users_with_active_rules(&s.pool).await {
                Ok(users) => {
                    for (u, _) in users {
                        if let Err(e) = materialize_user(&s, u).await {
                            tracing::warn!(user = %u, error = ?e.message, "no se pudieron materializar recurrentes");
                        }
                    }
                }
                Err(e) => tracing::warn!(error = %e, "worker de recurrentes"),
            }
            if !sd.sleep(std::time::Duration::from_secs(600)).await {
                break;
            }
        }
    })]
}
