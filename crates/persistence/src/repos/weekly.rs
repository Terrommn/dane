//! Revisión semanal. Propietario: agente A-core.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct WeeklyReview {
    pub id: Uuid,
    pub iso_week: String,
    pub week_start: NaiveDate,
    /// Paz de la semana 1–5 (la aporta el usuario).
    pub peace: Option<i16>,
    pub what_happened: String,
    pub what_didnt: String,
    pub content_md: String,
    pub habits_md: String,
    pub goals_md: String,
    pub went_well: String,
    pub went_badly: String,
    pub areas_md: String,
    pub people_md: String,
    /// Hasta 3: `[{text, reason}]`.
    #[schema(value_type = Vec<Object>)]
    pub priorities: serde_json::Value,
    /// Snapshot del resumen calculado al guardar.
    #[schema(value_type = Option<Object>)]
    pub summary: Option<serde_json::Value>,
    #[schema(value_type = Vec<Object>)]
    pub evidence: serde_json::Value,
    pub completed_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str =
    "id, iso_week, week_start, peace, what_happened, what_didnt, content_md, habits_md, goals_md, went_well,
    went_badly, areas_md, people_md, priorities, summary, evidence, completed_at, version, updated_at";

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, iso_week: &str) -> RepoResult<Option<WeeklyReview>> {
    Ok(sqlx::query_as::<_, WeeklyReview>(crate::sqlf!(
        "SELECT {COLS} FROM weekly_reviews WHERE user_id = $1 AND iso_week = $2"
    ))
    .bind(user_id)
    .bind(iso_week)
    .fetch_optional(db)
    .await?)
}

pub async fn list(db: impl PgExecutor<'_>, user_id: Uuid, limit: i64) -> RepoResult<Vec<WeeklyReview>> {
    Ok(sqlx::query_as::<_, WeeklyReview>(crate::sqlf!(
        "SELECT {COLS} FROM weekly_reviews WHERE user_id = $1 ORDER BY week_start DESC LIMIT $2"
    ))
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

pub struct WeeklyInput<'a> {
    pub peace: Option<i16>,
    pub what_happened: &'a str,
    pub what_didnt: &'a str,
    pub content_md: &'a str,
    pub habits_md: &'a str,
    pub goals_md: &'a str,
    pub went_well: &'a str,
    pub went_badly: &'a str,
    pub areas_md: &'a str,
    pub people_md: &'a str,
    pub priorities: &'a serde_json::Value,
    pub summary: Option<&'a serde_json::Value>,
    pub evidence: &'a serde_json::Value,
    pub completed: bool,
}

pub async fn upsert(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    iso_week: &str,
    week_start: NaiveDate,
    expected_version: Option<i32>,
    w: &WeeklyInput<'_>,
) -> RepoResult<WeeklyReview> {
    let row = sqlx::query_as::<_, WeeklyReview>(crate::sqlf!(
        "INSERT INTO weekly_reviews (user_id, iso_week, week_start, peace, what_happened, what_didnt, content_md, habits_md,
            goals_md, went_well, went_badly, areas_md, people_md, priorities, summary, evidence, completed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, CASE WHEN $17 THEN now() END)
         ON CONFLICT (user_id, iso_week) DO UPDATE SET peace = EXCLUDED.peace, what_happened = EXCLUDED.what_happened,
            what_didnt = EXCLUDED.what_didnt, content_md = EXCLUDED.content_md, habits_md = EXCLUDED.habits_md,
            goals_md = EXCLUDED.goals_md, went_well = EXCLUDED.went_well, went_badly = EXCLUDED.went_badly,
            areas_md = EXCLUDED.areas_md, people_md = EXCLUDED.people_md, priorities = EXCLUDED.priorities,
            summary = coalesce(EXCLUDED.summary, weekly_reviews.summary), evidence = EXCLUDED.evidence,
            completed_at = CASE WHEN $17 THEN coalesce(weekly_reviews.completed_at, now()) ELSE NULL END,
            version = weekly_reviews.version + 1
         WHERE weekly_reviews.version = coalesce($18, 0)
         RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(iso_week)
    .bind(week_start)
    .bind(w.peace)
    .bind(w.what_happened)
    .bind(w.what_didnt)
    .bind(w.content_md)
    .bind(w.habits_md)
    .bind(w.goals_md)
    .bind(w.went_well)
    .bind(w.went_badly)
    .bind(w.areas_md)
    .bind(w.people_md)
    .bind(w.priorities)
    .bind(w.summary)
    .bind(w.evidence)
    .bind(w.completed)
    .bind(expected_version)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => match get(pool, user_id, iso_week).await? {
            Some(cur) => Err(RepoError::Conflict(serde_json::to_value(cur).unwrap_or_default())),
            None => Err(RepoError::NotFound),
        },
    }
}

/// Paz por semana en un rango (para el dashboard).
pub async fn peace_series(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<(String, NaiveDate, i16)>> {
    Ok(sqlx::query_as(
        "SELECT iso_week, week_start, peace FROM weekly_reviews WHERE user_id = $1 AND peace IS NOT NULL AND week_start BETWEEN $2 AND $3 ORDER BY week_start",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}
