//! Objetivos mensuales/semanales, eventos de progreso y enlaces que aportan. Propietario: agente A-core.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Goal {
    pub id: Uuid,
    /// `month | week`
    pub kind: String,
    /// `YYYY-MM` o `GGGG-Www`
    pub period_key: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub title: String,
    pub area_id: Option<Uuid>,
    pub motivation: String,
    pub success_criteria: String,
    /// `active | achieved | not_achieved`
    pub status: String,
    pub parent_goal_id: Option<Uuid>,
    /// `none | cumulative | current_value | milestone`
    pub progress_type: String,
    pub unit: Option<String>,
    #[schema(value_type = Option<String>)]
    pub baseline: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub target: Option<Decimal>,
    pub direction: Option<String>,
    /// `manual | readings | habit | weight | debt | finance`
    pub progress_source: String,
    pub source_ref: Option<Uuid>,
    /// Cierre: `{achievement, learning, difficulties, adjustments}`.
    #[schema(value_type = Option<Object>)]
    pub closing: Option<serde_json::Value>,
    pub closed_at: Option<DateTime<Utc>>,
    pub position: i32,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct ProgressEventRow {
    pub id: Uuid,
    pub goal_id: Uuid,
    pub occurred_on: NaiveDate,
    #[schema(value_type = Option<String>)]
    pub amount: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub value: Option<Decimal>,
    pub completed: Option<bool>,
    pub source_type: String,
    pub source_id: Option<Uuid>,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct GoalLink {
    pub src_type: String,
    pub src_id: Uuid,
    pub created_at: DateTime<Utc>,
}

const COLS: &str = "id, kind, period_key, starts_on, ends_on, title, area_id, motivation, success_criteria, status, parent_goal_id,
    progress_type, unit, baseline, target, direction, progress_source, source_ref, closing, closed_at, position, archived_at,
    version, updated_at";

pub struct GoalFilter {
    pub kind: Option<String>,
    pub period_key: Option<String>,
    pub status: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub include_archived: bool,
}

pub async fn list(db: impl PgExecutor<'_>, user_id: Uuid, f: &GoalFilter) -> RepoResult<Vec<Goal>> {
    Ok(sqlx::query_as::<_, Goal>(crate::sqlf!(
        "SELECT {COLS} FROM goals WHERE user_id = $1
           AND ($2::text IS NULL OR kind = $2) AND ($3::text IS NULL OR period_key = $3) AND ($4::text IS NULL OR status = $4)
           AND ($5::date IS NULL OR ends_on >= $5) AND ($6::date IS NULL OR starts_on <= $6)
           AND ($7 OR archived_at IS NULL)
         ORDER BY starts_on DESC, kind, position, created_at"
    ))
    .bind(user_id)
    .bind(&f.kind)
    .bind(&f.period_key)
    .bind(&f.status)
    .bind(f.from)
    .bind(f.to)
    .bind(f.include_archived)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Goal> {
    sqlx::query_as::<_, Goal>(crate::sqlf!(
        "SELECT {COLS} FROM goals WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct GoalInput<'a> {
    pub kind: &'a str,
    pub period_key: &'a str,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
    pub title: &'a str,
    pub area_id: Option<Uuid>,
    pub motivation: &'a str,
    pub success_criteria: &'a str,
    pub status: &'a str,
    pub parent_goal_id: Option<Uuid>,
    pub progress_type: &'a str,
    pub unit: Option<&'a str>,
    pub baseline: Option<Decimal>,
    pub target: Option<Decimal>,
    pub direction: Option<&'a str>,
    pub progress_source: &'a str,
    pub source_ref: Option<Uuid>,
    pub position: i32,
    pub archived: bool,
}

pub async fn create(db: impl PgExecutor<'_>, user_id: Uuid, g: &GoalInput<'_>) -> RepoResult<Goal> {
    Ok(sqlx::query_as::<_, Goal>(crate::sqlf!(
        "INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, area_id, motivation, success_criteria, status,
            parent_goal_id, progress_type, unit, baseline, target, direction, progress_source, source_ref, position)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19) RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(g.kind)
    .bind(g.period_key)
    .bind(g.starts_on)
    .bind(g.ends_on)
    .bind(g.title)
    .bind(g.area_id)
    .bind(g.motivation)
    .bind(g.success_criteria)
    .bind(g.status)
    .bind(g.parent_goal_id)
    .bind(g.progress_type)
    .bind(g.unit)
    .bind(g.baseline)
    .bind(g.target)
    .bind(g.direction)
    .bind(g.progress_source)
    .bind(g.source_ref)
    .bind(g.position)
    .fetch_one(db)
    .await?)
}

pub async fn update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    g: &GoalInput<'_>,
) -> RepoResult<Goal> {
    let row = sqlx::query_as::<_, Goal>(crate::sqlf!(
        "UPDATE goals SET title = $4, area_id = $5, motivation = $6, success_criteria = $7, status = $8, parent_goal_id = $9,
            progress_type = $10, unit = $11, baseline = $12, target = $13, direction = $14, progress_source = $15,
            source_ref = $16, position = $17, archived_at = CASE WHEN $18 THEN coalesce(archived_at, now()) ELSE NULL END,
            version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(g.title)
    .bind(g.area_id)
    .bind(g.motivation)
    .bind(g.success_criteria)
    .bind(g.status)
    .bind(g.parent_goal_id)
    .bind(g.progress_type)
    .bind(g.unit)
    .bind(g.baseline)
    .bind(g.target)
    .bind(g.direction)
    .bind(g.progress_source)
    .bind(g.source_ref)
    .bind(g.position)
    .bind(g.archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}

/// Cierre del objetivo: estado final + reflexión. Acción separada de editar y de archivar.
pub async fn close(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    status: &str,
    closing: &serde_json::Value,
) -> RepoResult<Goal> {
    let row = sqlx::query_as::<_, Goal>(crate::sqlf!(
        "UPDATE goals SET status = $4, closing = $5, closed_at = now(), version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(status)
    .bind(closing)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}

const EV_COLS: &str =
    "id, goal_id, occurred_on, amount, value, completed, source_type, source_id, note, created_at";

pub async fn events(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    goal_ids: &[Uuid],
) -> RepoResult<Vec<ProgressEventRow>> {
    Ok(sqlx::query_as::<_, ProgressEventRow>(crate::sqlf!(
        "SELECT {EV_COLS} FROM goal_progress_events WHERE user_id = $1 AND goal_id = ANY($2) ORDER BY occurred_on, created_at"
    ))
    .bind(user_id)
    .bind(goal_ids)
    .fetch_all(db)
    .await?)
}

pub async fn delete_manual_event(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    goal_id: Uuid,
    event_id: Uuid,
) -> RepoResult<()> {
    let n = sqlx::query("DELETE FROM goal_progress_events WHERE user_id = $1 AND goal_id = $2 AND id = $3 AND source_type = 'manual'")
        .bind(user_id)
        .bind(goal_id)
        .bind(event_id)
        .execute(db)
        .await?
        .rows_affected();
    if n == 0 { Err(RepoError::NotFound) } else { Ok(()) }
}

pub async fn links(db: impl PgExecutor<'_>, user_id: Uuid, goal_id: Uuid) -> RepoResult<Vec<GoalLink>> {
    Ok(sqlx::query_as::<_, GoalLink>(
        "SELECT src_type, src_id, created_at FROM entity_links
         WHERE user_id = $1 AND dst_type = 'goal' AND dst_id = $2 AND relation = 'contributes_to' ORDER BY created_at",
    )
    .bind(user_id)
    .bind(goal_id)
    .fetch_all(db)
    .await?)
}

/// Mediciones de peso de los check-ins (para objetivos con fuente `weight`).
pub async fn weight_points(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<(NaiveDate, Decimal)>> {
    Ok(sqlx::query_as(
        "SELECT on_date, weight_kg FROM daily_checkins WHERE user_id = $1 AND weight_kg IS NOT NULL AND on_date BETWEEN $2 AND $3 ORDER BY on_date",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

/// Cantidades de entradas `done` de un hábito (para objetivos con fuente `habit` en la misma unidad).
pub async fn habit_quantities(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    habit_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> RepoResult<Vec<(NaiveDate, Option<Decimal>)>> {
    Ok(sqlx::query_as(
        "SELECT on_date, quantity FROM habit_entries WHERE user_id = $1 AND habit_id = $2 AND status = 'done' AND on_date BETWEEN $3 AND $4 ORDER BY on_date",
    )
    .bind(user_id)
    .bind(habit_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}
