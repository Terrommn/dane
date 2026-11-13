//! Propietario: agente B-modules. Inbox de captura rápida con propuesta revisable.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct InboxItem {
    pub id: Uuid,
    pub text: String,
    /// `app | telegram | import`
    pub source: String,
    /// `pending | processed | discarded | archived`
    pub status: String,
    pub captured_at: DateTime<Utc>,
    /// Propuesta revisable `{destination, title, area_slug, reason, …}`.
    #[schema(value_type = Option<Object>)]
    pub proposal: Option<serde_json::Value>,
    pub proposal_source: Option<String>,
    pub processed_at: Option<DateTime<Utc>>,
    pub result_type: Option<String>,
    pub result_id: Option<Uuid>,
    pub client_id: Option<String>,
    pub version: i32,
}

const COLS: &str = "id, text, source, status, captured_at, proposal, proposal_source, processed_at, result_type, result_id, client_id, version";

pub async fn list(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    status: Option<&str>,
) -> RepoResult<Vec<InboxItem>> {
    Ok(sqlx::query_as::<_, InboxItem>(crate::sqlf!(
        "SELECT {COLS} FROM inbox_items WHERE user_id = $1 AND ($2::text IS NULL OR status = $2) ORDER BY captured_at DESC LIMIT 500"
    ))
    .bind(user_id)
    .bind(status)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<InboxItem> {
    sqlx::query_as::<_, InboxItem>(crate::sqlf!(
        "SELECT {COLS} FROM inbox_items WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

/// Captura idempotente: con `client_id` repetido devuelve la existente (`created = false`).
pub async fn capture(
    pool: &PgPool,
    user_id: Uuid,
    text: &str,
    source: &str,
    client_id: Option<&str>,
) -> RepoResult<(InboxItem, bool)> {
    let row = sqlx::query_as::<_, InboxItem>(crate::sqlf!(
        "INSERT INTO inbox_items (user_id, text, source, client_id) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, client_id) DO NOTHING RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(text)
    .bind(source)
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok((r, true)),
        None => {
            let r = sqlx::query_as::<_, InboxItem>(crate::sqlf!(
                "SELECT {COLS} FROM inbox_items WHERE user_id = $1 AND client_id = $2"
            ))
            .bind(user_id)
            .bind(client_id)
            .fetch_one(pool)
            .await?;
            Ok((r, false))
        }
    }
}

pub async fn set_proposal(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: Option<i32>,
    text: Option<&str>,
    proposal: &serde_json::Value,
    source: &str,
) -> RepoResult<InboxItem> {
    let row = sqlx::query_as::<_, InboxItem>(crate::sqlf!(
        "UPDATE inbox_items SET text = coalesce($4, text), proposal = $5, proposal_source = $6, version = version + 1
         WHERE user_id = $1 AND id = $2 AND ($3::int IS NULL OR version = $3) AND status = 'pending' RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(text)
    .bind(proposal)
    .bind(source)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}

/// Cierra el elemento (solo si sigue pendiente y con la versión esperada) dentro de la transacción del destino.
pub async fn resolve(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    status: &str,
    proposal: &serde_json::Value,
    result: Option<(&str, Uuid)>,
) -> RepoResult<bool> {
    Ok(sqlx::query(
        "UPDATE inbox_items SET status = $4, proposal = $5, processed_at = now(), result_type = $6, result_id = $7, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 AND status = 'pending'",
    )
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(status)
    .bind(proposal)
    .bind(result.map(|r| r.0))
    .bind(result.map(|r| r.1))
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1)
}

pub async fn reopen(pool: &PgPool, user_id: Uuid, id: Uuid) -> RepoResult<InboxItem> {
    let row = sqlx::query_as::<_, InboxItem>(crate::sqlf!(
        "UPDATE inbox_items SET status = 'pending', processed_at = NULL, version = version + 1
         WHERE user_id = $1 AND id = $2 AND status IN ('archived', 'discarded') RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}
