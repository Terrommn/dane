//! Lecturas agregadas (solo lectura) de tablas de varios módulos para Hoy, Weekly y Dashboard. Propietario: agente A-core.
//! No crea ni modifica registros de otros módulos.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::RepoResult;

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct AgendaItem {
    pub id: Uuid,
    pub source: String,
    pub title: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub location: Option<String>,
    #[schema(value_type = Vec<Object>)]
    pub attendees: serde_json::Value,
}

pub async fn agenda(db: impl PgExecutor<'_>, user_id: Uuid, on: NaiveDate) -> RepoResult<Vec<AgendaItem>> {
    Ok(sqlx::query_as::<_, AgendaItem>(
        "SELECT id, source, title, starts_at, ends_at, all_day, location, attendees FROM calendar_events
         WHERE user_id = $1 AND on_date = $2 AND status <> 'cancelled' ORDER BY all_day DESC, starts_at NULLS FIRST, title",
    )
    .bind(user_id)
    .bind(on)
    .fetch_all(db)
    .await?)
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct TaskBrief {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub priority: Option<i16>,
    pub area_id: Option<Uuid>,
    pub area_name: Option<String>,
    pub project_id: Option<Uuid>,
    pub planned_on: Option<NaiveDate>,
    pub due_on: Option<NaiveDate>,
    pub done_on: Option<NaiveDate>,
    pub external_url: Option<String>,
    pub external_status_category: Option<String>,
    pub ease_hint: Option<String>,
}

const TASK_SELECT: &str = "SELECT t.id, t.title, t.status, t.priority, t.area_id, a.name AS area_name, t.project_id, t.planned_on,
    t.due_on, (t.done_at AT TIME ZONE $2)::date AS done_on, t.external_url, t.external_status_category, t.ease_hint
    FROM tasks t LEFT JOIN areas a ON a.id = t.area_id";

/// Pendientes del día: abiertos planeados/vencidos hasta la fecha y los cerrados ese día. Sin crear tareas.
pub async fn tasks_for_day(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    tz: &str,
    on: NaiveDate,
) -> RepoResult<Vec<TaskBrief>> {
    Ok(sqlx::query_as::<_, TaskBrief>(crate::sqlf!(
        "{TASK_SELECT} WHERE t.user_id = $1 AND t.archived_at IS NULL AND (
            (t.status = 'open' AND coalesce(t.planned_on, t.due_on) <= $3)
            OR (t.status = 'done' AND (t.done_at AT TIME ZONE $2)::date = $3))
         ORDER BY a.position NULLS LAST, t.status, t.priority NULLS LAST, t.created_at"
    ))
    .bind(user_id)
    .bind(tz)
    .bind(on)
    .fetch_all(db)
    .await?)
}

/// Tareas cerradas en un rango (log derivado y weekly).
pub async fn tasks_done_between(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    tz: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<TaskBrief>> {
    Ok(sqlx::query_as::<_, TaskBrief>(crate::sqlf!(
        "{TASK_SELECT} WHERE t.user_id = $1 AND t.status = 'done' AND (t.done_at AT TIME ZONE $2)::date BETWEEN $3 AND $4
         ORDER BY t.done_at"
    ))
    .bind(user_id)
    .bind(tz)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

/// Tareas abiertas con fecha planeada/vencimiento en el rango y aún sin cerrar ("qué no pasó").
pub async fn tasks_open_planned_between(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    tz: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<TaskBrief>> {
    Ok(sqlx::query_as::<_, TaskBrief>(crate::sqlf!(
        "{TASK_SELECT} WHERE t.user_id = $1 AND t.status = 'open' AND t.archived_at IS NULL
           AND coalesce(t.planned_on, t.due_on) BETWEEN $3 AND $4 ORDER BY coalesce(t.planned_on, t.due_on)"
    ))
    .bind(user_id)
    .bind(tz)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct InteractionBrief {
    pub id: Uuid,
    pub person_id: Uuid,
    pub person_name: String,
    pub on_date: NaiveDate,
    pub topic: String,
    pub source: String,
}

pub async fn interactions_between(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<InteractionBrief>> {
    Ok(sqlx::query_as::<_, InteractionBrief>(
        "SELECT i.id, i.person_id, p.name AS person_name, i.on_date, i.topic, i.source FROM interactions i
         JOIN people p ON p.id = i.person_id WHERE i.user_id = $1 AND i.on_date BETWEEN $2 AND $3 ORDER BY i.on_date, p.name",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct DevlogBrief {
    pub id: Uuid,
    pub on_date: NaiveDate,
    pub repo: String,
    pub branch: String,
    pub changes_md: String,
}

pub async fn devlog_between(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<DevlogBrief>> {
    Ok(sqlx::query_as::<_, DevlogBrief>(
        "SELECT s.id, d.on_date, s.repo, s.branch, s.changes_md FROM devlog_sessions s JOIN devlogs d ON d.id = s.devlog_id
         WHERE s.user_id = $1 AND d.on_date BETWEEN $2 AND $3 ORDER BY d.on_date, s.position",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct ContentBrief {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub published_on: Option<NaiveDate>,
    pub platforms: Vec<String>,
    pub views: Option<i64>,
    #[schema(value_type = Option<String>)]
    pub retention_pct: Option<rust_decimal::Decimal>,
}

pub async fn content_published_between(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<ContentBrief>> {
    Ok(sqlx::query_as::<_, ContentBrief>(
        "SELECT id, title, status, published_on, platforms, views, retention_pct FROM content_pieces
         WHERE user_id = $1 AND status = 'published' AND coalesce(published_on, publish_on) BETWEEN $2 AND $3
         ORDER BY coalesce(published_on, publish_on)",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

/// Gastos confirmados (kind expense) en moneda base: total y número, por rango. Fuente de datos financiera común.
pub async fn expenses_between(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    currency: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<(i64, i64)> {
    let row: (Option<i64>, i64) = sqlx::query_as(
        "SELECT sum(amount_cents)::bigint, count(*) FROM transactions
         WHERE user_id = $1 AND kind = 'expense' AND status = 'confirmed' AND currency = $2 AND occurred_on BETWEEN $3 AND $4",
    )
    .bind(user_id)
    .bind(currency)
    .bind(from)
    .bind(to)
    .fetch_one(db)
    .await?;
    Ok((row.0.unwrap_or(0), row.1))
}

/// Inbox pendiente (para la revisión semanal: se propone, no se mueve aquí).
pub async fn inbox_pending_count(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM inbox_items WHERE user_id = $1 AND status = 'pending'")
            .bind(user_id)
            .fetch_one(db)
            .await?,
    )
}
