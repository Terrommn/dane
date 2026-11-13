//! Propietario: agente E-integrations. Conexiones a proveedores externos (credenciales cifradas por el server),
//! historial de sincronizaciones y sugerencias de interacción/personas.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::RepoResult;

#[derive(Debug, Clone, FromRow)]
pub struct Connection {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub status: String,
    pub scopes: Vec<String>,
    pub account_label: Option<String>,
    pub credentials_enc: Option<Vec<u8>>,
    pub config: serde_json::Value,
    pub cursor: serde_json::Value,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_error_at: Option<DateTime<Utc>>,
    pub version: i32,
}

/// Estado visible de una conexión (sin credenciales).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ConnectionStatus {
    pub provider: String,
    /// `connected | error | disconnected | revoked`
    pub status: String,
    pub scopes: Vec<String>,
    pub account_label: Option<String>,
    #[schema(value_type = Object)]
    pub config: serde_json::Value,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_error_at: Option<DateTime<Utc>>,
    pub version: i32,
}

impl Connection {
    pub fn status(&self) -> ConnectionStatus {
        let mut config = self.config.clone();
        if let Some(o) = config.as_object_mut() {
            o.remove("endpoints_override");
        }
        ConnectionStatus {
            provider: self.provider.clone(),
            status: self.status.clone(),
            scopes: self.scopes.clone(),
            account_label: self.account_label.clone(),
            config,
            last_sync_at: self.last_sync_at,
            last_error: self.last_error.clone(),
            last_error_at: self.last_error_at,
            version: self.version,
        }
    }
}

const COLS: &str = "id, user_id, provider, status, scopes, account_label, credentials_enc, config, cursor, last_sync_at, last_error, last_error_at, version";

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, provider: &str) -> RepoResult<Option<Connection>> {
    Ok(sqlx::query_as::<_, Connection>(crate::sqlf!(
        "SELECT {COLS} FROM integration_connections WHERE user_id = $1 AND provider = $2"
    ))
    .bind(user_id)
    .bind(provider)
    .fetch_optional(db)
    .await?)
}

pub async fn list(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<Connection>> {
    Ok(sqlx::query_as::<_, Connection>(crate::sqlf!(
        "SELECT {COLS} FROM integration_connections WHERE user_id = $1 ORDER BY provider"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// Crea o reemplaza una conexión como conectada, conservando la configuración previa si existe.
pub async fn upsert_connected(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    provider: &str,
    scopes: &[String],
    account_label: Option<&str>,
    credentials_enc: &[u8],
    config_patch: &serde_json::Value,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO integration_connections (user_id, provider, status, scopes, account_label, credentials_enc, config)
         VALUES ($1, $2, 'connected', $3, $4, $5, $6)
         ON CONFLICT (user_id, provider) DO UPDATE SET status = 'connected', scopes = EXCLUDED.scopes,
            account_label = coalesce(EXCLUDED.account_label, integration_connections.account_label),
            credentials_enc = EXCLUDED.credentials_enc, config = integration_connections.config || EXCLUDED.config,
            last_error = NULL, last_error_at = NULL, cursor = '{}', version = integration_connections.version + 1",
    )
    .bind(user_id)
    .bind(provider)
    .bind(scopes)
    .bind(account_label)
    .bind(credentials_enc)
    .bind(config_patch)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn save_credentials(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    provider: &str,
    credentials_enc: &[u8],
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE integration_connections SET credentials_enc = $3 WHERE user_id = $1 AND provider = $2",
    )
    .bind(user_id)
    .bind(provider)
    .bind(credentials_enc)
    .execute(db)
    .await?;
    Ok(())
}

/// Actualiza la configuración (merge) con control de versión. Devuelve `false` si la versión no coincide.
pub async fn patch_config(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    provider: &str,
    version: i32,
    patch: &serde_json::Value,
) -> RepoResult<bool> {
    Ok(sqlx::query(
        "UPDATE integration_connections SET config = config || $4, version = version + 1
         WHERE user_id = $1 AND provider = $2 AND version = $3",
    )
    .bind(user_id)
    .bind(provider)
    .bind(version)
    .bind(patch)
    .execute(db)
    .await?
    .rows_affected()
        == 1)
}

pub async fn mark_sync_ok(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    provider: &str,
    at: DateTime<Utc>,
    next_sync_at: DateTime<Utc>,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE integration_connections SET last_sync_at = $3, last_error = NULL, last_error_at = NULL,
            cursor = (cursor - 'lease_until' - 'failures') || jsonb_build_object('next_sync_at', $4::timestamptz)
         WHERE user_id = $1 AND provider = $2",
    )
    .bind(user_id)
    .bind(provider)
    .bind(at)
    .bind(next_sync_at)
    .execute(db)
    .await?;
    Ok(())
}

/// Registra un error. `fatal` = requiere reconectar (status `error`); si no, solo backoff exponencial.
pub async fn mark_error(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    provider: &str,
    error: &str,
    fatal: bool,
    at: DateTime<Utc>,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE integration_connections SET last_error = left($3, 500), last_error_at = $5,
            status = CASE WHEN $4 THEN 'error' ELSE status END,
            cursor = (cursor - 'lease_until') || jsonb_build_object(
                'failures', coalesce((cursor->>'failures')::int, 0) + 1,
                'next_sync_at', $5::timestamptz + make_interval(mins => least(240, 15 * power(2, coalesce((cursor->>'failures')::int, 0))::int)))
         WHERE user_id = $1 AND provider = $2",
    )
    .bind(user_id)
    .bind(provider)
    .bind(error)
    .bind(fatal)
    .bind(at)
    .execute(db)
    .await?;
    Ok(())
}

/// Desconecta: borra credenciales cifradas y deja la configuración no sensible.
pub async fn disconnect(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    provider: &str,
    status: &str,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE integration_connections SET status = $3, credentials_enc = NULL, cursor = '{}', version = version + 1
         WHERE user_id = $1 AND provider = $2",
    )
    .bind(user_id)
    .bind(provider)
    .bind(status)
    .execute(db)
    .await?;
    Ok(())
}

/// Reclama conexiones vencidas para sincronizar (lease de 10 min en `cursor`), seguro con varias instancias.
pub async fn claim_due(
    db: impl PgExecutor<'_>,
    provider: &str,
    now: DateTime<Utc>,
    limit: i64,
) -> RepoResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "UPDATE integration_connections SET cursor = cursor || jsonb_build_object('lease_until', $2::timestamptz + interval '10 minutes')
         WHERE id IN (
            SELECT id FROM integration_connections
            WHERE provider = $1 AND status = 'connected' AND credentials_enc IS NOT NULL
              AND coalesce((cursor->>'lease_until')::timestamptz, 'epoch') < $2
              AND coalesce((cursor->>'next_sync_at')::timestamptz, 'epoch') <= $2
            ORDER BY last_sync_at NULLS FIRST LIMIT $3 FOR UPDATE SKIP LOCKED)
         RETURNING user_id",
    )
    .bind(provider)
    .bind(now)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct SyncRun {
    pub provider: String,
    pub kind: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub ok: Option<bool>,
    pub items: i32,
    pub error: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn log_run(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    provider: &str,
    kind: &str,
    started: DateTime<Utc>,
    ok: bool,
    items: i32,
    error: Option<&str>,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO integration_sync_runs (user_id, provider, kind, started_at, finished_at, ok, items, error)
         VALUES ($1, $2, $3, $4, now(), $5, $6, left($7, 500))",
    )
    .bind(user_id)
    .bind(provider)
    .bind(kind)
    .bind(started)
    .bind(ok)
    .bind(items)
    .bind(error)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn recent_runs(db: impl PgExecutor<'_>, user_id: Uuid, limit: i64) -> RepoResult<Vec<SyncRun>> {
    Ok(sqlx::query_as::<_, SyncRun>(
        "SELECT provider, kind, started_at, finished_at, ok, items, error FROM integration_sync_runs
         WHERE user_id = $1 ORDER BY started_at DESC LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct InteractionSuggestion {
    pub id: Uuid,
    pub person_id: Uuid,
    pub person_name: String,
    pub on_date: NaiveDate,
    pub topic: String,
    pub source: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, FromRow, ToSchema)]
pub struct PersonSuggestion {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    pub source: String,
    pub on_date: Option<NaiveDate>,
    pub context: String,
    pub status: String,
}

pub async fn pending_interaction_suggestions(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> RepoResult<Vec<InteractionSuggestion>> {
    Ok(sqlx::query_as::<_, InteractionSuggestion>(
        "SELECT s.id, s.person_id, p.name AS person_name, s.on_date, s.topic, s.source, s.status
         FROM interaction_suggestions s JOIN people p ON p.id = s.person_id
         WHERE s.user_id = $1 AND s.status = 'pending' ORDER BY s.on_date DESC, p.name LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn pending_person_suggestions(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
) -> RepoResult<Vec<PersonSuggestion>> {
    Ok(sqlx::query_as::<_, PersonSuggestion>(
        "SELECT id, name, email, source, on_date, context, status FROM person_suggestions
         WHERE user_id = $1 AND status = 'pending' ORDER BY on_date DESC NULLS LAST, name LIMIT 200",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}
