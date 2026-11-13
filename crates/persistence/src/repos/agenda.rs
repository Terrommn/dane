//! Propietario: agente B-modules. Agenda del día: eventos manuales (CRUD) + lectura de los sincronizados de Google.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct CalendarEvent {
    pub id: Uuid,
    /// `manual | google`
    pub source: String,
    pub calendar_id: String,
    pub on_date: NaiveDate,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub title: String,
    pub location: Option<String>,
    /// Invitados `[{email, name}]` (propuesta de personas; no prueban asistencia).
    #[schema(value_type = Vec<Object>)]
    pub attendees: serde_json::Value,
    pub status: String,
    pub html_link: Option<String>,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str = "id, source, calendar_id, on_date, starts_at, ends_at, all_day, title, location, attendees, status, html_link, updated_at";

pub async fn for_range(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<CalendarEvent>> {
    Ok(sqlx::query_as::<_, CalendarEvent>(crate::sqlf!(
        "SELECT {COLS} FROM calendar_events WHERE user_id = $1 AND on_date BETWEEN $2 AND $3 AND status <> 'cancelled'
         ORDER BY on_date, all_day DESC, starts_at NULLS FIRST, title"
    ))
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<CalendarEvent> {
    sqlx::query_as::<_, CalendarEvent>(crate::sqlf!(
        "SELECT {COLS} FROM calendar_events WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct EventInput<'a> {
    pub on_date: NaiveDate,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    pub title: &'a str,
    pub location: Option<&'a str>,
}

pub async fn create_manual(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    e: &EventInput<'_>,
) -> RepoResult<CalendarEvent> {
    Ok(sqlx::query_as::<_, CalendarEvent>(crate::sqlf!(
        "INSERT INTO calendar_events (user_id, source, on_date, starts_at, ends_at, all_day, title, location)
         VALUES ($1, 'manual', $2, $3, $4, $5, $6, $7) RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(e.on_date)
    .bind(e.starts_at)
    .bind(e.ends_at)
    .bind(e.all_day)
    .bind(e.title)
    .bind(e.location)
    .fetch_one(db)
    .await?)
}

/// Solo eventos manuales: los de Google son de lectura y se actualizan por sincronización.
pub async fn update_manual(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    e: &EventInput<'_>,
) -> RepoResult<CalendarEvent> {
    sqlx::query_as::<_, CalendarEvent>(crate::sqlf!(
        "UPDATE calendar_events SET on_date = $3, starts_at = $4, ends_at = $5, all_day = $6, title = $7, location = $8
         WHERE user_id = $1 AND id = $2 AND source = 'manual' RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(e.on_date)
    .bind(e.starts_at)
    .bind(e.ends_at)
    .bind(e.all_day)
    .bind(e.title)
    .bind(e.location)
    .fetch_optional(pool)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn delete_manual(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<()> {
    let n = sqlx::query("DELETE FROM calendar_events WHERE user_id = $1 AND id = $2 AND source = 'manual'")
        .bind(user_id)
        .bind(id)
        .execute(db)
        .await?
        .rows_affected();
    if n == 0 { Err(RepoError::NotFound) } else { Ok(()) }
}
