//! Check-in diario (un registro por usuario y fecha local), log y reflexiones. Propietario: agente A-core.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Checkin {
    pub id: Uuid,
    pub on_date: NaiveDate,
    #[schema(value_type = Option<String>)]
    pub weight_kg: Option<Decimal>,
    pub energy: Option<i16>,
    pub planned_spend_text: Option<String>,
    pub planned_spend_cents: Option<i64>,
    pub planned_spend_none: bool,
    pub notes_md: String,
    pub morning_done_at: Option<DateTime<Utc>>,
    pub evening_done_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct LogEntry {
    pub id: Uuid,
    pub on_date: NaiveDate,
    pub text: String,
    pub position: i32,
    pub source: String,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Reflection {
    pub id: Uuid,
    pub on_date: NaiveDate,
    /// `good | bad`
    pub polarity: String,
    pub text: String,
    pub position: i32,
}

const COLS: &str =
    "id, on_date, weight_kg, energy, planned_spend_text, planned_spend_cents, planned_spend_none, notes_md,
    morning_done_at, evening_done_at, version, updated_at";

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, on: NaiveDate) -> RepoResult<Option<Checkin>> {
    Ok(sqlx::query_as::<_, Checkin>(crate::sqlf!(
        "SELECT {COLS} FROM daily_checkins WHERE user_id = $1 AND on_date = $2"
    ))
    .bind(user_id)
    .bind(on)
    .fetch_optional(db)
    .await?)
}

/// Bloquea (o crea vacío) el check-in del día dentro de la transacción. Evita carreras entre dos dispositivos.
pub async fn lock_or_create(tx: &mut Tx<'_>, user_id: Uuid, on: NaiveDate) -> RepoResult<(Checkin, bool)> {
    let inserted = sqlx::query("INSERT INTO daily_checkins (user_id, on_date) VALUES ($1, $2) ON CONFLICT (user_id, on_date) DO NOTHING")
        .bind(user_id)
        .bind(on)
        .execute(&mut **tx)
        .await?
        .rows_affected()
        == 1;
    let row = sqlx::query_as::<_, Checkin>(crate::sqlf!(
        "SELECT {COLS} FROM daily_checkins WHERE user_id = $1 AND on_date = $2 FOR UPDATE"
    ))
    .bind(user_id)
    .bind(on)
    .fetch_one(&mut **tx)
    .await?;
    Ok((row, inserted))
}

pub async fn save(tx: &mut Tx<'_>, user_id: Uuid, c: &Checkin) -> RepoResult<Checkin> {
    sqlx::query_as::<_, Checkin>(crate::sqlf!(
        "UPDATE daily_checkins SET weight_kg = $3, energy = $4, planned_spend_text = $5, planned_spend_cents = $6,
            planned_spend_none = $7, notes_md = $8, morning_done_at = $9, evening_done_at = $10, version = version + 1
         WHERE user_id = $1 AND id = $2 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(c.id)
    .bind(c.weight_kg)
    .bind(c.energy)
    .bind(&c.planned_spend_text)
    .bind(c.planned_spend_cents)
    .bind(c.planned_spend_none)
    .bind(&c.notes_md)
    .bind(c.morning_done_at)
    .bind(c.evening_done_at)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn list(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<Checkin>> {
    Ok(sqlx::query_as::<_, Checkin>(crate::sqlf!(
        "SELECT {COLS} FROM daily_checkins WHERE user_id = $1 AND on_date BETWEEN $2 AND $3 ORDER BY on_date"
    ))
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

pub async fn log_entries(db: impl PgExecutor<'_>, user_id: Uuid, on: NaiveDate) -> RepoResult<Vec<LogEntry>> {
    Ok(sqlx::query_as::<_, LogEntry>(
        "SELECT id, on_date, text, position, source FROM daily_log_entries WHERE user_id = $1 AND on_date = $2 ORDER BY position, created_at",
    )
    .bind(user_id)
    .bind(on)
    .fetch_all(db)
    .await?)
}

/// Reemplaza el log del día (lista completa, en orden).
pub async fn replace_log(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    on: NaiveDate,
    items: &[String],
    source: &str,
) -> RepoResult<()> {
    sqlx::query("DELETE FROM daily_log_entries WHERE user_id = $1 AND on_date = $2")
        .bind(user_id)
        .bind(on)
        .execute(&mut **tx)
        .await?;
    for (i, t) in items.iter().enumerate() {
        sqlx::query("INSERT INTO daily_log_entries (user_id, on_date, text, position, source) VALUES ($1, $2, $3, $4, $5)")
            .bind(user_id)
            .bind(on)
            .bind(t)
            .bind(i as i32)
            .bind(source)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn reflections(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<Reflection>> {
    Ok(sqlx::query_as::<_, Reflection>(
        "SELECT id, on_date, polarity, text, position FROM reflection_items WHERE user_id = $1 AND on_date BETWEEN $2 AND $3
         ORDER BY on_date, polarity, position",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

pub async fn replace_reflections(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    on: NaiveDate,
    polarity: &str,
    items: &[String],
) -> RepoResult<()> {
    sqlx::query("DELETE FROM reflection_items WHERE user_id = $1 AND on_date = $2 AND polarity = $3")
        .bind(user_id)
        .bind(on)
        .bind(polarity)
        .execute(&mut **tx)
        .await?;
    for (i, t) in items.iter().enumerate() {
        sqlx::query("INSERT INTO reflection_items (user_id, on_date, polarity, text, position) VALUES ($1, $2, $3, $4, $5)")
            .bind(user_id)
            .bind(on)
            .bind(polarity)
            .bind(t)
            .bind(i as i32)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
