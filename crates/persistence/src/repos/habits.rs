//! Hábitos (identidad), versiones, pausas, entradas por fecha y vínculos explícitos. Propietario: agente A-core.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Habit {
    pub id: Uuid,
    pub name: String,
    /// `fixed | goal | custom`
    pub kind: String,
    /// `exercise | reading` para hábitos fijos.
    pub fixed_key: Option<String>,
    pub goal_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub unit: Option<String>,
    pub description: String,
    pub position: i32,
    pub created_on: NaiveDate,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct HabitVersionRow {
    pub id: Uuid,
    pub habit_id: Uuid,
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
    /// `daily | weekdays | times_per_week`
    pub frequency: String,
    /// Bitmask ISO (lunes = 1 … domingo = 64).
    pub weekdays: Option<i16>,
    pub times_per_week: Option<i16>,
    #[schema(value_type = Option<String>)]
    pub target_quantity: Option<Decimal>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct HabitPause {
    pub id: Uuid,
    pub habit_id: Uuid,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub reason: String,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct HabitEntry {
    pub habit_id: Uuid,
    pub on_date: NaiveDate,
    /// `done | not_done | not_applicable`
    pub status: String,
    #[schema(value_type = Option<String>)]
    pub quantity: Option<Decimal>,
    pub reason: Option<String>,
    pub source: String,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct HabitLink {
    /// `weight_logged | expense_logged | positive_reflection`
    pub trigger: String,
    pub habit_id: Uuid,
    pub habit_name: String,
}

const HABIT_COLS: &str = "id, name, kind, fixed_key, goal_id, area_id, unit, description, position, created_on, archived_at, version, updated_at";
const VERSION_COLS: &str =
    "id, habit_id, valid_from, valid_to, frequency, weekdays, times_per_week, target_quantity";

pub async fn list(db: impl PgExecutor<'_>, user_id: Uuid, include_archived: bool) -> RepoResult<Vec<Habit>> {
    Ok(sqlx::query_as::<_, Habit>(crate::sqlf!(
        "SELECT {HABIT_COLS} FROM habits WHERE user_id = $1 AND ($2 OR archived_at IS NULL) ORDER BY kind, position, name"
    ))
    .bind(user_id)
    .bind(include_archived)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Habit> {
    sqlx::query_as::<_, Habit>(crate::sqlf!(
        "SELECT {HABIT_COLS} FROM habits WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct NewHabit<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub fixed_key: Option<&'a str>,
    pub goal_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub unit: Option<&'a str>,
    pub description: &'a str,
    pub position: i32,
    pub created_on: NaiveDate,
}

pub struct NewVersion {
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
    pub frequency: &'static str,
    pub weekdays: Option<i16>,
    pub times_per_week: Option<i16>,
    pub target_quantity: Option<Decimal>,
}

pub async fn create(tx: &mut Tx<'_>, user_id: Uuid, h: &NewHabit<'_>, v: &NewVersion) -> RepoResult<Habit> {
    let habit = sqlx::query_as::<_, Habit>(crate::sqlf!(
        "INSERT INTO habits (user_id, name, kind, fixed_key, goal_id, area_id, unit, description, position, created_on)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING {HABIT_COLS}"
    ))
    .bind(user_id)
    .bind(h.name)
    .bind(h.kind)
    .bind(h.fixed_key)
    .bind(h.goal_id)
    .bind(h.area_id)
    .bind(h.unit)
    .bind(h.description)
    .bind(h.position)
    .bind(h.created_on)
    .fetch_one(&mut **tx)
    .await?;
    insert_version(tx, user_id, habit.id, v).await?;
    Ok(habit)
}

async fn insert_version(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    habit_id: Uuid,
    v: &NewVersion,
) -> RepoResult<HabitVersionRow> {
    Ok(sqlx::query_as::<_, HabitVersionRow>(crate::sqlf!(
        "INSERT INTO habit_versions (user_id, habit_id, valid_from, valid_to, frequency, weekdays, times_per_week, target_quantity)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {VERSION_COLS}"
    ))
    .bind(user_id)
    .bind(habit_id)
    .bind(v.valid_from)
    .bind(v.valid_to)
    .bind(v.frequency)
    .bind(v.weekdays)
    .bind(v.times_per_week)
    .bind(v.target_quantity)
    .fetch_one(&mut **tx)
    .await?)
}

/// Nueva versión desde `valid_from`: recorta las versiones que la cubren (terminan el día anterior) y elimina las que
/// empiezan en o después de esa fecha (quedarían reemplazadas). El historial anterior no cambia.
pub async fn add_version(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    habit_id: Uuid,
    v: &NewVersion,
) -> RepoResult<HabitVersionRow> {
    sqlx::query("DELETE FROM habit_versions WHERE user_id = $1 AND habit_id = $2 AND valid_from >= $3")
        .bind(user_id)
        .bind(habit_id)
        .bind(v.valid_from)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "UPDATE habit_versions SET valid_to = $3::date - 1
         WHERE user_id = $1 AND habit_id = $2 AND valid_from < $3 AND (valid_to IS NULL OR valid_to >= $3)",
    )
    .bind(user_id)
    .bind(habit_id)
    .bind(v.valid_from)
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE habits SET version = version + 1 WHERE user_id = $1 AND id = $2")
        .bind(user_id)
        .bind(habit_id)
        .execute(&mut **tx)
        .await?;
    insert_version(tx, user_id, habit_id, v).await
}

pub struct HabitUpdate<'a> {
    pub name: &'a str,
    pub area_id: Option<Uuid>,
    pub unit: Option<&'a str>,
    pub description: &'a str,
    pub position: i32,
    pub archived: bool,
}

pub async fn update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    u: &HabitUpdate<'_>,
) -> RepoResult<Habit> {
    let row = sqlx::query_as::<_, Habit>(crate::sqlf!(
        "UPDATE habits SET name = $4, area_id = $5, unit = $6, description = $7, position = $8,
            archived_at = CASE WHEN $9 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {HABIT_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(u.name)
    .bind(u.area_id)
    .bind(u.unit)
    .bind(u.description)
    .bind(u.position)
    .bind(u.archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}

pub async fn versions(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<HabitVersionRow>> {
    Ok(sqlx::query_as::<_, HabitVersionRow>(crate::sqlf!(
        "SELECT {VERSION_COLS} FROM habit_versions WHERE user_id = $1 ORDER BY habit_id, valid_from"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn pauses(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<HabitPause>> {
    Ok(sqlx::query_as::<_, HabitPause>(
        "SELECT id, habit_id, starts_on, ends_on, reason FROM habit_pauses WHERE user_id = $1 ORDER BY habit_id, starts_on",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn add_pause(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    habit_id: Uuid,
    starts_on: NaiveDate,
    ends_on: Option<NaiveDate>,
    reason: &str,
) -> RepoResult<HabitPause> {
    Ok(sqlx::query_as::<_, HabitPause>(
        "INSERT INTO habit_pauses (user_id, habit_id, starts_on, ends_on, reason) VALUES ($1, $2, $3, $4, $5)
         RETURNING id, habit_id, starts_on, ends_on, reason",
    )
    .bind(user_id)
    .bind(habit_id)
    .bind(starts_on)
    .bind(ends_on)
    .bind(reason)
    .fetch_one(db)
    .await?)
}

pub async fn delete_pause(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    habit_id: Uuid,
    pause_id: Uuid,
) -> RepoResult<()> {
    let n = sqlx::query("DELETE FROM habit_pauses WHERE user_id = $1 AND habit_id = $2 AND id = $3")
        .bind(user_id)
        .bind(habit_id)
        .bind(pause_id)
        .execute(db)
        .await?
        .rows_affected();
    if n == 0 { Err(RepoError::NotFound) } else { Ok(()) }
}

const ENTRY_COLS: &str = "habit_id, on_date, status, quantity, reason, source, version, updated_at";

pub async fn entries(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    habit_id: Option<Uuid>,
) -> RepoResult<Vec<HabitEntry>> {
    Ok(sqlx::query_as::<_, HabitEntry>(crate::sqlf!(
        "SELECT {ENTRY_COLS} FROM habit_entries WHERE user_id = $1 AND on_date BETWEEN $2 AND $3
         AND ($4::uuid IS NULL OR habit_id = $4) ORDER BY on_date, habit_id"
    ))
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(habit_id)
    .fetch_all(db)
    .await?)
}

pub async fn get_entry(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    habit_id: Uuid,
    on: NaiveDate,
) -> RepoResult<HabitEntry> {
    sqlx::query_as::<_, HabitEntry>(crate::sqlf!(
        "SELECT {ENTRY_COLS} FROM habit_entries WHERE user_id = $1 AND habit_id = $2 AND on_date = $3"
    ))
    .bind(user_id)
    .bind(habit_id)
    .bind(on)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

/// Inserta o actualiza la entrada de un día. Con `expected_version` exige coincidencia (409 si no).
#[allow(clippy::too_many_arguments)]
pub async fn upsert_entry(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    habit_id: Uuid,
    on: NaiveDate,
    status: &str,
    quantity: Option<Decimal>,
    reason: Option<&str>,
    source: &str,
    expected_version: Option<i32>,
) -> RepoResult<HabitEntry> {
    let row = sqlx::query_as::<_, HabitEntry>(crate::sqlf!(
        "INSERT INTO habit_entries (user_id, habit_id, on_date, status, quantity, reason, source)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (habit_id, on_date) DO UPDATE
           SET status = EXCLUDED.status, quantity = EXCLUDED.quantity, reason = EXCLUDED.reason, source = EXCLUDED.source,
               version = habit_entries.version + 1
           WHERE habit_entries.user_id = $1 AND ($8::int IS NULL OR habit_entries.version = $8)
         RETURNING {ENTRY_COLS}"
    ))
    .bind(user_id)
    .bind(habit_id)
    .bind(on)
    .bind(status)
    .bind(quantity)
    .bind(reason)
    .bind(source)
    .bind(expected_version)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(
            get_entry(&mut **tx, user_id, habit_id, on).await,
        )),
    }
}

/// Vuelve el día a "sin registrar".
pub async fn delete_entry(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    habit_id: Uuid,
    on: NaiveDate,
) -> RepoResult<bool> {
    Ok(
        sqlx::query("DELETE FROM habit_entries WHERE user_id = $1 AND habit_id = $2 AND on_date = $3")
            .bind(user_id)
            .bind(habit_id)
            .bind(on)
            .execute(db)
            .await?
            .rows_affected()
            > 0,
    )
}

pub async fn links(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<HabitLink>> {
    Ok(sqlx::query_as::<_, HabitLink>(
        "SELECT l.trigger, l.habit_id, h.name AS habit_name FROM habit_links l JOIN habits h ON h.id = l.habit_id
         WHERE l.user_id = $1 AND l.trigger <> 'devotional_content' ORDER BY l.trigger",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn set_link(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    trigger: &str,
    habit_id: Uuid,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO habit_links (user_id, trigger, habit_id) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, trigger) DO UPDATE SET habit_id = EXCLUDED.habit_id",
    )
    .bind(user_id)
    .bind(trigger)
    .bind(habit_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn delete_link(db: impl PgExecutor<'_>, user_id: Uuid, trigger: &str) -> RepoResult<()> {
    sqlx::query("DELETE FROM habit_links WHERE user_id = $1 AND trigger = $2")
        .bind(user_id)
        .bind(trigger)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn fixed_habit(db: impl PgExecutor<'_>, user_id: Uuid, key: &str) -> RepoResult<Option<Habit>> {
    Ok(sqlx::query_as::<_, Habit>(crate::sqlf!(
        "SELECT {HABIT_COLS} FROM habits WHERE user_id = $1 AND fixed_key = $2"
    ))
    .bind(user_id)
    .bind(key)
    .fetch_optional(db)
    .await?)
}
