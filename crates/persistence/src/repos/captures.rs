//! Propietario: agente E-integrations. Propuestas de captura libre (texto → propuesta tipada → confirmación).

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use uuid::Uuid;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CaptureRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub input_text: String,
    pub source: String,
    pub intent: String,
    pub reference_date: NaiveDate,
    pub status: String,
    pub mode: String,
    pub proposal: serde_json::Value,
    pub questions: serde_json::Value,
    pub errors: serde_json::Value,
    pub applied: Option<serde_json::Value>,
    pub meta: serde_json::Value,
    pub llm_model: Option<String>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str = "id, user_id, input_text, source, intent, reference_date, status, mode, proposal, questions, errors, applied, meta, llm_model, version, created_at, updated_at";

#[allow(clippy::too_many_arguments)]
pub async fn create(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    text: &str,
    source: &str,
    intent: &str,
    reference_date: NaiveDate,
    meta: &serde_json::Value,
) -> RepoResult<CaptureRow> {
    Ok(sqlx::query_as::<_, CaptureRow>(crate::sqlf!(
        "INSERT INTO capture_proposals (user_id, input_text, source, intent, reference_date, meta) VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(text)
    .bind(source)
    .bind(intent)
    .bind(reference_date)
    .bind(meta)
    .fetch_one(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<CaptureRow> {
    sqlx::query_as::<_, CaptureRow>(crate::sqlf!(
        "SELECT {COLS} FROM capture_proposals WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn list(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    status: Option<&str>,
    limit: i64,
) -> RepoResult<Vec<CaptureRow>> {
    Ok(sqlx::query_as::<_, CaptureRow>(crate::sqlf!(
        "SELECT {COLS} FROM capture_proposals WHERE user_id = $1 AND ($2::text IS NULL OR status = $2) ORDER BY created_at DESC LIMIT $3"
    ))
    .bind(user_id)
    .bind(status)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

/// Guarda propuesta/preguntas/errores. Con `expected_version` aplica control optimista.
#[allow(clippy::too_many_arguments)]
pub async fn save_proposal(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    id: Uuid,
    expected_version: Option<i32>,
    status: &str,
    mode: &str,
    proposal: &serde_json::Value,
    questions: &serde_json::Value,
    errors: &serde_json::Value,
    llm_model: Option<&str>,
) -> RepoResult<Option<CaptureRow>> {
    Ok(sqlx::query_as::<_, CaptureRow>(crate::sqlf!(
        "UPDATE capture_proposals SET status = $4, mode = $5, proposal = $6, questions = $7, errors = $8,
            llm_model = coalesce($9, llm_model), version = version + 1
         WHERE user_id = $1 AND id = $2 AND ($3::int IS NULL OR version = $3) AND status IN ('pending', 'needs_input', 'failed')
         RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(expected_version)
    .bind(status)
    .bind(mode)
    .bind(proposal)
    .bind(questions)
    .bind(errors)
    .bind(llm_model)
    .fetch_optional(db)
    .await?)
}

pub async fn finish(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    id: Uuid,
    status: &str,
    applied: Option<&serde_json::Value>,
) -> RepoResult<Option<CaptureRow>> {
    Ok(sqlx::query_as::<_, CaptureRow>(crate::sqlf!(
        "UPDATE capture_proposals SET status = $3, applied = $4, version = version + 1
         WHERE user_id = $1 AND id = $2 AND status IN ('pending', 'needs_input', 'failed') RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(status)
    .bind(applied)
    .fetch_optional(db)
    .await?)
}
