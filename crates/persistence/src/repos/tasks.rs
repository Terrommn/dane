//! Propietario: agente B-modules. Tareas, historial, personas de la tarea y reglas recurrentes.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub notes_md: String,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// `open | done | cancelled`
    pub status: String,
    pub priority: Option<i16>,
    pub due_on: Option<NaiveDate>,
    pub planned_on: Option<NaiveDate>,
    pub done_at: Option<DateTime<Utc>>,
    pub recurrence_rule_id: Option<Uuid>,
    pub recurrence_period: Option<String>,
    pub class_note_id: Option<Uuid>,
    pub external_source: Option<String>,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
    pub external_status: Option<String>,
    /// `open | need_info | closed`
    pub external_status_category: Option<String>,
    pub external_priority: Option<String>,
    pub ease_hint: Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Personas relacionadas (task_people).
    pub person_ids: Vec<Uuid>,
}

const COLS: &str = "t.id, t.title, t.notes_md, t.area_id, t.project_id, t.status, t.priority, t.due_on, t.planned_on,
    t.done_at, t.recurrence_rule_id, t.recurrence_period, t.class_note_id, t.external_source, t.external_id,
    t.external_url, t.external_status, t.external_status_category, t.external_priority, t.ease_hint, t.archived_at,
    t.version, t.created_at, t.updated_at,
    coalesce((SELECT array_agg(tp.person_id) FROM task_people tp WHERE tp.task_id = t.id), '{}') AS person_ids";

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct TaskEvent {
    pub id: i64,
    pub at: DateTime<Utc>,
    /// `created | carried_over | completed | reopened | edited | cancelled | external_sync`
    pub kind: String,
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
    #[schema(value_type = Object)]
    pub data: serde_json::Value,
}

pub struct TaskInput<'a> {
    pub title: &'a str,
    pub notes_md: &'a str,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub priority: Option<i16>,
    pub due_on: Option<NaiveDate>,
    pub planned_on: Option<NaiveDate>,
    pub class_note_id: Option<Uuid>,
    pub person_ids: &'a [Uuid],
    pub archived: bool,
}

#[derive(Debug, Default, Clone)]
pub struct TaskFilter {
    pub status: Option<String>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub class_note_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub q: Option<String>,
    pub external: Option<bool>,
    pub archived: bool,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list(db: &PgPool, user_id: Uuid, f: &TaskFilter) -> RepoResult<(Vec<Task>, i64)> {
    let where_ = "t.user_id = $1 AND ($2::text IS NULL OR t.status = $2) AND ($3::uuid IS NULL OR t.area_id = $3)
        AND ($4::uuid IS NULL OR t.project_id = $4) AND ($5::date IS NULL OR coalesce(t.planned_on, t.due_on) >= $5)
        AND ($6::date IS NULL OR coalesce(t.planned_on, t.due_on) <= $6)
        AND ($7::text IS NULL OR immutable_unaccent(lower(t.title)) LIKE '%' || immutable_unaccent(lower($7)) || '%')
        AND ($8::bool IS NULL OR (t.external_source IS NOT NULL) = $8) AND ($9 OR t.archived_at IS NULL)
        AND ($10::uuid IS NULL OR t.class_note_id = $10)";
    let rows = sqlx::query_as::<_, Task>(crate::sqlf!(
        "SELECT {COLS} FROM tasks t WHERE {where_}
         ORDER BY t.status = 'open' DESC, coalesce(t.planned_on, t.due_on) NULLS LAST, t.priority NULLS LAST, t.created_at
         LIMIT $11 OFFSET $12"
    ))
    .bind(user_id)
    .bind(&f.status)
    .bind(f.area_id)
    .bind(f.project_id)
    .bind(f.from)
    .bind(f.to)
    .bind(&f.q)
    .bind(f.external)
    .bind(f.archived)
    .bind(f.class_note_id)
    .bind(f.limit)
    .bind(f.offset)
    .fetch_all(db)
    .await?;
    let total: i64 = sqlx::query_scalar(crate::sqlf!("SELECT count(*) FROM tasks t WHERE {where_}"))
        .bind(user_id)
        .bind(&f.status)
        .bind(f.area_id)
        .bind(f.project_id)
        .bind(f.from)
        .bind(f.to)
        .bind(&f.q)
        .bind(f.external)
        .bind(f.archived)
        .bind(f.class_note_id)
        .fetch_one(db)
        .await?;
    Ok((rows, total))
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Task> {
    sqlx::query_as::<_, Task>(crate::sqlf!(
        "SELECT {COLS} FROM tasks t WHERE t.user_id = $1 AND t.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn events(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Vec<TaskEvent>> {
    Ok(sqlx::query_as::<_, TaskEvent>(
        "SELECT id, at, kind, from_date, to_date, data FROM task_events WHERE user_id = $1 AND task_id = $2 ORDER BY at, id",
    )
    .bind(user_id)
    .bind(id)
    .fetch_all(db)
    .await?)
}

pub async fn add_event(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    task_id: Uuid,
    kind: &str,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    data: serde_json::Value,
) -> RepoResult<()> {
    sqlx::query("INSERT INTO task_events (user_id, task_id, kind, from_date, to_date, data) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(user_id)
        .bind(task_id)
        .bind(kind)
        .bind(from)
        .bind(to)
        .bind(data)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn set_people(tx: &mut Tx<'_>, user_id: Uuid, task_id: Uuid, people: &[Uuid]) -> RepoResult<()> {
    sqlx::query("DELETE FROM task_people WHERE user_id = $1 AND task_id = $2")
        .bind(user_id)
        .bind(task_id)
        .execute(&mut **tx)
        .await?;
    for p in people {
        // FK compuesta: una persona de otro usuario falla.
        sqlx::query("INSERT INTO task_people (user_id, task_id, person_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(task_id)
            .bind(p)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn create(tx: &mut Tx<'_>, user_id: Uuid, t: &TaskInput<'_>) -> RepoResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tasks (user_id, title, notes_md, area_id, project_id, priority, due_on, planned_on, class_note_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(user_id)
    .bind(t.title)
    .bind(t.notes_md)
    .bind(t.area_id)
    .bind(t.project_id)
    .bind(t.priority)
    .bind(t.due_on)
    .bind(t.planned_on)
    .bind(t.class_note_id)
    .fetch_one(&mut **tx)
    .await?;
    set_people(tx, user_id, id, t.person_ids).await?;
    add_event(
        tx,
        user_id,
        id,
        "created",
        None,
        t.planned_on,
        serde_json::json!({}),
    )
    .await?;
    Ok(id)
}

pub async fn update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    t: &TaskInput<'_>,
) -> RepoResult<Task> {
    let mut tx = pool.begin().await?;
    let before = get(&mut *tx, user_id, id).await?;
    let n = sqlx::query(
        "UPDATE tasks SET title = $4, notes_md = $5, area_id = $6, project_id = $7, priority = $8, due_on = $9,
            planned_on = $10, class_note_id = $11,
            archived_at = CASE WHEN $12 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3",
    )
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(t.title)
    .bind(t.notes_md)
    .bind(t.area_id)
    .bind(t.project_id)
    .bind(t.priority)
    .bind(t.due_on)
    .bind(t.planned_on)
    .bind(t.class_note_id)
    .bind(t.archived)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(conflict_or_missing(Ok(before)));
    }
    set_people(&mut tx, user_id, id, t.person_ids).await?;
    add_event(
        &mut tx,
        user_id,
        id,
        "edited",
        before.planned_on,
        t.planned_on,
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    get(pool, user_id, id).await
}

/// Cambia el estado local. Para tareas externas (ClickUp) solo cambia la copia local: nunca llama al proveedor.
pub async fn set_status(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: Option<i32>,
    status: &str,
) -> RepoResult<Task> {
    let mut tx = pool.begin().await?;
    let before = get(&mut *tx, user_id, id).await?;
    let n = sqlx::query(
        "UPDATE tasks SET status = $4, done_at = CASE WHEN $4 = 'done' THEN now() ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND ($3::int IS NULL OR version = $3)",
    )
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(status)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(conflict_or_missing(Ok(before)));
    }
    let kind = match status {
        "done" => "completed",
        "cancelled" => "cancelled",
        _ => "reopened",
    };
    add_event(
        &mut tx,
        user_id,
        id,
        kind,
        None,
        None,
        serde_json::json!({ "from": before.status }),
    )
    .await?;
    tx.commit().await?;
    get(pool, user_id, id).await
}

/// Arrastre: las pendientes abiertas con `planned_on < to` pasan a `to`. Conserva el ID y registra un evento.
/// Idempotente: repetir con la misma fecha no mueve nada.
pub async fn carry_over(pool: &PgPool, user_id: Uuid, to: NaiveDate) -> RepoResult<Vec<Uuid>> {
    let mut tx = pool.begin().await?;
    let moved: Vec<(Uuid, NaiveDate)> = sqlx::query_as(
        "WITH m AS (
            SELECT id, planned_on FROM tasks
            WHERE user_id = $1 AND status = 'open' AND archived_at IS NULL AND planned_on < $2
            FOR UPDATE
         )
         UPDATE tasks t SET planned_on = $2, version = t.version + 1 FROM m
         WHERE t.id = m.id RETURNING t.id, m.planned_on",
    )
    .bind(user_id)
    .bind(to)
    .fetch_all(&mut *tx)
    .await?;
    for (id, from) in &moved {
        add_event(
            &mut tx,
            user_id,
            *id,
            "carried_over",
            Some(*from),
            Some(to),
            serde_json::json!({}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(moved.into_iter().map(|(id, _)| id).collect())
}

/// Tareas de un día: abiertas planeadas hasta ese día (incluye arrastradas), las que vencen ese día y las
/// completadas ese día (fecha local). Orden dentro del área: ClickUp fáciles primero, `need_info` al final.
pub async fn for_day(db: &PgPool, user_id: Uuid, date: NaiveDate, tz: &str) -> RepoResult<Vec<Task>> {
    Ok(sqlx::query_as::<_, Task>(crate::sqlf!(
        "SELECT {COLS} FROM tasks t
         WHERE t.user_id = $1 AND t.archived_at IS NULL AND (
            (t.status = 'open' AND (t.planned_on <= $2 OR t.due_on = $2))
            OR (t.status = 'done' AND (t.done_at AT TIME ZONE $3)::date = $2)
         )
         ORDER BY t.status = 'done', CASE WHEN t.external_status_category = 'need_info' THEN 2
                  WHEN t.ease_hint IS NOT NULL THEN 0 ELSE 1 END, t.priority NULLS LAST, t.created_at"
    ))
    .bind(user_id)
    .bind(date)
    .bind(tz)
    .fetch_all(db)
    .await?)
}

// ---------- Reglas recurrentes ----------

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct RecurrenceRule {
    pub id: Uuid,
    pub title: String,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// `day | last | penultimate`
    pub day_kind: String,
    pub day: Option<i16>,
    pub active: bool,
    pub starts_on: NaiveDate,
    pub version: i32,
}

const RCOLS: &str = "id, title, area_id, project_id, day_kind, day, active, starts_on, version";

pub async fn list_rules(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<RecurrenceRule>> {
    Ok(sqlx::query_as::<_, RecurrenceRule>(crate::sqlf!(
        "SELECT {RCOLS} FROM task_recurrence_rules WHERE user_id = $1 ORDER BY active DESC, day_kind, day, title"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn get_rule(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<RecurrenceRule> {
    sqlx::query_as::<_, RecurrenceRule>(crate::sqlf!(
        "SELECT {RCOLS} FROM task_recurrence_rules WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct RuleInput<'a> {
    pub title: &'a str,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub day_kind: &'a str,
    pub day: Option<i16>,
    pub active: bool,
    pub starts_on: NaiveDate,
}

pub async fn create_rule(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    r: &RuleInput<'_>,
) -> RepoResult<RecurrenceRule> {
    Ok(sqlx::query_as::<_, RecurrenceRule>(crate::sqlf!(
        "INSERT INTO task_recurrence_rules (user_id, title, area_id, project_id, day_kind, day, active, starts_on)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {RCOLS}"
    ))
    .bind(user_id)
    .bind(r.title)
    .bind(r.area_id)
    .bind(r.project_id)
    .bind(r.day_kind)
    .bind(r.day)
    .bind(r.active)
    .bind(r.starts_on)
    .fetch_one(db)
    .await?)
}

pub async fn update_rule(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    r: &RuleInput<'_>,
) -> RepoResult<RecurrenceRule> {
    let row = sqlx::query_as::<_, RecurrenceRule>(crate::sqlf!(
        "UPDATE task_recurrence_rules SET title = $4, area_id = $5, project_id = $6, day_kind = $7, day = $8,
            active = $9, starts_on = $10, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {RCOLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(r.title)
    .bind(r.area_id)
    .bind(r.project_id)
    .bind(r.day_kind)
    .bind(r.day)
    .bind(r.active)
    .bind(r.starts_on)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_rule(pool, user_id, id).await)),
    }
}

/// Inserta la ocurrencia si no existe (UNIQUE user+regla+periodo). Devuelve true si la creó.
pub async fn insert_occurrence(
    pool: &PgPool,
    user_id: Uuid,
    rule: &RecurrenceRule,
    period: &str,
    date: NaiveDate,
) -> RepoResult<bool> {
    let mut tx = pool.begin().await?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO tasks (user_id, title, area_id, project_id, planned_on, due_on, recurrence_rule_id, recurrence_period)
         VALUES ($1, $2, $3, $4, $5, $5, $6, $7)
         ON CONFLICT (user_id, recurrence_rule_id, recurrence_period) DO NOTHING RETURNING id",
    )
    .bind(user_id)
    .bind(&rule.title)
    .bind(rule.area_id)
    .bind(rule.project_id)
    .bind(date)
    .bind(rule.id)
    .bind(period)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(id) = id {
        add_event(
            &mut tx,
            user_id,
            id,
            "created",
            None,
            Some(date),
            serde_json::json!({ "recurrence_period": period }),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(id.is_some())
}

/// Usuarios con reglas activas y su zona horaria (para el worker).
pub async fn users_with_active_rules(db: impl PgExecutor<'_>) -> RepoResult<Vec<(Uuid, String)>> {
    Ok(sqlx::query_as(
        "SELECT DISTINCT r.user_id, coalesce(s.timezone, 'America/Monterrey')
         FROM task_recurrence_rules r LEFT JOIN user_settings s ON s.user_id = r.user_id WHERE r.active",
    )
    .fetch_all(db)
    .await?)
}
