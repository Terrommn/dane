//! Propietario: agente E-integrations. Ejecuciones de importación (dry-run → aplicada) y su plan.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct ImportRun {
    pub id: Uuid,
    pub status: String,
    pub source_name: String,
    #[schema(value_type = Object)]
    pub summary: serde_json::Value,
    #[schema(value_type = Vec<String>)]
    pub warnings: serde_json::Value,
    #[schema(value_type = Vec<Object>)]
    pub conflicts: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub applied_at: Option<DateTime<Utc>>,
}

const COLS: &str = "id, status, source_name, summary, warnings, conflicts, created_at, applied_at";

pub async fn create(db: impl PgExecutor<'_>, user_id: Uuid, source_name: &str, plan: &serde_json::Value, summary: &serde_json::Value, warnings: &serde_json::Value, conflicts: &serde_json::Value) -> RepoResult<ImportRun> {
    Ok(sqlx::query_as::<_, ImportRun>(crate::sqlf!(
        "INSERT INTO import_runs (user_id, status, source_name, plan, summary, warnings, conflicts) VALUES ($1, 'dry_run', $2, $3, $4, $5, $6) RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(source_name)
    .bind(plan)
    .bind(summary)
    .bind(warnings)
    .bind(conflicts)
    .fetch_one(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<ImportRun> {
    sqlx::query_as::<_, ImportRun>(crate::sqlf!("SELECT {COLS} FROM import_runs WHERE user_id = $1 AND id = $2"))
        .bind(user_id)
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(RepoError::NotFound)
}

pub async fn plan(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<(String, serde_json::Value)> {
    sqlx::query_as("SELECT status, plan FROM import_runs WHERE user_id = $1 AND id = $2")
        .bind(user_id)
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(RepoError::NotFound)
}

pub async fn list(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<ImportRun>> {
    Ok(sqlx::query_as::<_, ImportRun>(crate::sqlf!("SELECT {COLS} FROM import_runs WHERE user_id = $1 ORDER BY created_at DESC LIMIT 50"))
        .bind(user_id)
        .fetch_all(db)
        .await?)
}

pub async fn mark(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid, status: &str, summary: &serde_json::Value, warnings: &serde_json::Value) -> RepoResult<ImportRun> {
    sqlx::query_as::<_, ImportRun>(crate::sqlf!(
        "UPDATE import_runs SET status = $3, summary = $4, warnings = $5, applied_at = CASE WHEN $3 = 'applied' THEN now() END
         WHERE user_id = $1 AND id = $2 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(status)
    .bind(summary)
    .bind(warnings)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}
