//! Outbox de mensajes salientes (`jobs`) con leases (propietario: D-telegram, D-011).
//!
//! * Encolar: `UNIQUE NULLS NOT DISTINCT (user_id, kind, occurrence_key)` → nunca dos jobs para la misma ocurrencia.
//! * Reclamar: transacción corta con `FOR UPDATE SKIP LOCKED`; asigna `lease_token` y `lease_expires_at`. El worker
//!   llama a Telegram **sin** locks abiertos. Un lease vencido vuelve a ser reclamable con un token nuevo.
//! * Confirmar: solo con el `lease_token` vigente (o el último aplicado, para acks repetidos idempotentes).
//!   Un token obsoleto recibe `AckError::StaleLease` (409).
//! * Entrega *al menos una vez*: si el envío tuvo éxito pero el ack se pierde, el lease vence y puede reenviarse.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{RepoResult, Tx};

/// Reintentos por defecto para mensajes.
pub const DEFAULT_MAX_ATTEMPTS: i32 = 6;

/// Encola un job; devuelve `None` si ya existía la ocurrencia.
#[allow(clippy::too_many_arguments)]
pub async fn enqueue(
    db: impl PgExecutor<'_>,
    user_id: Option<Uuid>,
    kind: &str,
    occurrence_key: &str,
    payload: &serde_json::Value,
    run_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    max_attempts: i32,
) -> RepoResult<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "INSERT INTO jobs (user_id, kind, occurrence_key, payload, run_at, expires_at, max_attempts)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (user_id, kind, occurrence_key) DO NOTHING RETURNING id",
    )
    .bind(user_id)
    .bind(kind)
    .bind(occurrence_key)
    .bind(payload)
    .bind(run_at)
    .bind(expires_at)
    .bind(max_attempts)
    .fetch_optional(db)
    .await?)
}

/// Asegura un recordatorio. Si ya existe y no se ha intentado, recalcula hora/caducidad/texto (cambio de
/// preferencias o de zona); si ya se envió o se reclamó, no se toca. Devuelve `true` si lo creó.
pub async fn upsert_reminder(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    kind: &str,
    occurrence_key: &str,
    payload: &serde_json::Value,
    run_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> RepoResult<bool> {
    let inserted: Option<bool> = sqlx::query_scalar(
        "INSERT INTO jobs (user_id, kind, occurrence_key, payload, run_at, expires_at, max_attempts)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (user_id, kind, occurrence_key) DO UPDATE
           SET run_at = EXCLUDED.run_at, expires_at = EXCLUDED.expires_at, payload = EXCLUDED.payload
           WHERE jobs.status = 'pending' AND jobs.attempts = 0
             AND (jobs.run_at, jobs.expires_at, jobs.payload) IS DISTINCT FROM (EXCLUDED.run_at, EXCLUDED.expires_at, EXCLUDED.payload)
         RETURNING (xmax = 0)",
    )
    .bind(user_id)
    .bind(kind)
    .bind(occurrence_key)
    .bind(payload)
    .bind(run_at)
    .bind(expires_at)
    .bind(DEFAULT_MAX_ATTEMPTS)
    .fetch_optional(db)
    .await?;
    Ok(inserted.unwrap_or(false))
}

#[derive(Debug, Clone, FromRow)]
pub struct ClaimedRow {
    pub id: Uuid,
    pub lease_token: Uuid,
    pub kind: String,
    pub attempts: i32,
    pub payload: serde_json::Value,
}

/// Limpieza previa al claim, sin esperar locks (`SKIP LOCKED`) para no bloquear ni provocar deadlocks:
/// caduca vencidos, falla los que agotaron intentos y cancela recordatorios que ya no aplican
/// (check-in nocturno terminado, recordatorios pausados, chat no disponible).
pub async fn housekeeping(pool: &PgPool, now: DateTime<Utc>) -> RepoResult<()> {
    let reclaimable = "(status = 'pending' OR (status = 'leased' AND lease_expires_at < $1))";
    sqlx::query(crate::sqlf!(
        "UPDATE jobs SET status = 'expired', last_error = coalesce(last_error, 'caducó sin enviarse')
         WHERE id IN (SELECT id FROM jobs WHERE expires_at IS NOT NULL AND expires_at < $1 AND {reclaimable}
                      FOR UPDATE SKIP LOCKED)"
    ))
    .bind(now)
    .execute(pool)
    .await?;
    sqlx::query(crate::sqlf!(
        "UPDATE jobs SET status = 'failed', last_error = coalesce(last_error, 'reintentos agotados')
         WHERE id IN (SELECT id FROM jobs WHERE attempts >= max_attempts AND {reclaimable} FOR UPDATE SKIP LOCKED)"
    ))
    .bind(now)
    .execute(pool)
    .await?;
    sqlx::query(crate::sqlf!(
        "UPDATE jobs SET status = 'cancelled', last_error = 'check-in del día ya terminado'
         WHERE id IN (
           SELECT j.id FROM jobs j JOIN daily_checkins c
             ON c.user_id = j.user_id AND c.on_date = (j.payload ->> 'date')::date AND c.evening_done_at IS NOT NULL
           WHERE j.kind = 'reminder_daily' AND {}
           FOR UPDATE OF j SKIP LOCKED)",
        reclaimable.replace("status", "j.status").replace("lease_expires_at", "j.lease_expires_at")
    ))
    .bind(now)
    .execute(pool)
    .await?;
    sqlx::query(crate::sqlf!(
        "UPDATE jobs SET status = 'cancelled', last_error = 'recordatorios pausados o chat no disponible'
         WHERE id IN (
           SELECT j.id FROM jobs j
             JOIN user_settings s ON s.user_id = j.user_id
             LEFT JOIN telegram_chats c ON c.user_id = j.user_id
           WHERE j.kind LIKE 'reminder\\_%' AND {}
             AND (s.reminders_paused OR (s.paused_until IS NOT NULL AND s.paused_until > $1)
                  OR c.user_id IS NULL OR NOT c.can_message)
           FOR UPDATE OF j SKIP LOCKED)",
        reclaimable
            .replace("status", "j.status")
            .replace("lease_expires_at", "j.lease_expires_at")
    ))
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reclama hasta `limit` jobs listos. Transacción corta; nada queda bloqueado al devolver.
pub async fn claim(
    pool: &PgPool,
    worker_id: &str,
    limit: i64,
    lease: Duration,
    now: DateTime<Utc>,
) -> RepoResult<Vec<ClaimedRow>> {
    housekeeping(pool, now).await?;
    let mut tx = pool.begin().await?;
    let rows = sqlx::query_as::<_, ClaimedRow>(
        "WITH c AS (
            SELECT id FROM jobs
            WHERE run_at <= $1 AND attempts < max_attempts
              AND (expires_at IS NULL OR expires_at >= $1)
              AND (status = 'pending' OR (status = 'leased' AND lease_expires_at < $1))
            ORDER BY run_at, created_at
            LIMIT $2
            FOR UPDATE SKIP LOCKED)
         UPDATE jobs j SET status = 'leased', lease_token = gen_random_uuid(), lease_expires_at = $3,
                leased_by = $4, attempts = j.attempts + 1
         FROM c WHERE j.id = c.id
         RETURNING j.id, j.lease_token, j.kind, j.attempts, j.payload",
    )
    .bind(now)
    .bind(limit)
    .bind(now + lease)
    .bind(worker_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows)
}

/// Resultado solicitado por el worker.
#[derive(Debug, Clone)]
pub enum AckKind {
    Sent {
        telegram_message_id: Option<i64>,
    },
    Retry {
        retry_after: Option<Duration>,
        error: String,
    },
    Failed {
        error: String,
        chat_blocked: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AckOutcome {
    pub status: String,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub replayed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AckError {
    #[error("job no encontrado")]
    NotFound,
    #[error("lease obsoleto")]
    StaleLease,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Retraso del siguiente intento: `retry_after` de Telegram si viene (≥ 1 s), si no backoff exponencial
/// 30 s · 2^(intentos−1) acotado a 1 h.
pub fn retry_delay(attempts: i32, retry_after: Option<Duration>) -> Duration {
    if let Some(r) = retry_after {
        return r.max(Duration::seconds(1)).min(Duration::hours(1));
    }
    let exp = attempts.clamp(1, 12) - 1;
    (Duration::seconds(30) * 2i32.pow(exp as u32)).min(Duration::hours(1))
}

pub async fn ack(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    kind: AckKind,
    now: DateTime<Utc>,
) -> Result<AckOutcome, AckError> {
    let mut tx = pool.begin().await?;
    // (status, lease_token, attempts, max_attempts, user_id, run_at)
    type AckRow = (String, Option<Uuid>, i32, i32, Option<Uuid>, DateTime<Utc>);
    let row: Option<AckRow> = sqlx::query_as(
        "SELECT status, lease_token, attempts, max_attempts, user_id, run_at FROM jobs WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((status, lease_token, attempts, max_attempts, user_id, run_at)) = row else {
        return Err(AckError::NotFound);
    };
    if lease_token != Some(token) {
        return Err(AckError::StaleLease);
    }
    if status != "leased" {
        // Mismo token ya aplicado: ack repetido.
        let next = (status == "pending").then_some(run_at);
        return Ok(AckOutcome {
            status,
            next_attempt_at: next,
            replayed: true,
        });
    }
    let out = match kind {
        AckKind::Sent { telegram_message_id } => {
            sqlx::query(
                "UPDATE jobs SET status = 'sent', sent_at = $2, telegram_message_id = $3, lease_expires_at = NULL, last_error = NULL
                 WHERE id = $1",
            )
            .bind(id)
            .bind(now)
            .bind(telegram_message_id)
            .execute(&mut *tx)
            .await?;
            AckOutcome {
                status: "sent".into(),
                next_attempt_at: None,
                replayed: false,
            }
        }
        AckKind::Retry { retry_after, error } if attempts < max_attempts => {
            let next = now + retry_delay(attempts, retry_after);
            sqlx::query("UPDATE jobs SET status = 'pending', run_at = $2, lease_expires_at = NULL, last_error = $3 WHERE id = $1")
                .bind(id)
                .bind(next)
                .bind(truncate(&error))
                .execute(&mut *tx)
                .await?;
            AckOutcome {
                status: "pending".into(),
                next_attempt_at: Some(next),
                replayed: false,
            }
        }
        AckKind::Retry { error, .. } => {
            mark_failed(&mut tx, id, &format!("reintentos agotados: {}", truncate(&error))).await?;
            AckOutcome {
                status: "failed".into(),
                next_attempt_at: None,
                replayed: false,
            }
        }
        AckKind::Failed { error, chat_blocked } => {
            mark_failed(&mut tx, id, &truncate(&error)).await?;
            if let (true, Some(u)) = (chat_blocked, user_id) {
                sqlx::query(
                    "UPDATE telegram_chats SET can_message = false, last_error = $2 WHERE user_id = $1",
                )
                .bind(u)
                .bind(truncate(&error))
                .execute(&mut *tx)
                .await?;
            }
            AckOutcome {
                status: "failed".into(),
                next_attempt_at: None,
                replayed: false,
            }
        }
    };
    tx.commit().await?;
    Ok(out)
}

async fn mark_failed(tx: &mut Tx<'_>, id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE jobs SET status = 'failed', lease_expires_at = NULL, last_error = $2 WHERE id = $1")
        .bind(id)
        .bind(error)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn truncate(s: &str) -> String {
    s.chars().take(500).collect()
}

/// Cancela recordatorios pendientes del usuario (al pausar).
pub async fn cancel_pending_reminders(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    reason: &str,
) -> RepoResult<u64> {
    Ok(sqlx::query(
        "UPDATE jobs SET status = 'cancelled', last_error = $2
         WHERE user_id = $1 AND kind LIKE 'reminder\\_%' AND status = 'pending'",
    )
    .bind(user_id)
    .bind(reason)
    .execute(db)
    .await?
    .rows_affected())
}

/// Entrega visible para el usuario (sin payload).
#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Delivery {
    pub id: Uuid,
    pub kind: String,
    pub occurrence_key: String,
    /// `pending | leased | sent | failed | cancelled | expired`
    pub status: String,
    pub attempts: i32,
    pub run_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

pub async fn recent_deliveries(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    limit: i64,
) -> RepoResult<Vec<Delivery>> {
    Ok(sqlx::query_as::<_, Delivery>(
        "SELECT id, kind, occurrence_key, status, attempts, run_at, sent_at, last_error
         FROM jobs WHERE user_id = $1 ORDER BY run_at DESC, created_at DESC LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

/// Retención: borra jobs terminados viejos.
pub async fn purge_finished(db: impl PgExecutor<'_>, older_than: DateTime<Utc>) -> RepoResult<u64> {
    Ok(sqlx::query(
        "DELETE FROM jobs WHERE status IN ('sent','failed','cancelled','expired') AND updated_at < $1",
    )
    .bind(older_than)
    .execute(db)
    .await?
    .rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_exponential_bounded_and_honors_retry_after() {
        assert_eq!(retry_delay(1, None), Duration::seconds(30));
        assert_eq!(retry_delay(2, None), Duration::seconds(60));
        assert_eq!(retry_delay(4, None), Duration::seconds(240));
        assert_eq!(retry_delay(20, None), Duration::hours(1));
        assert_eq!(retry_delay(3, Some(Duration::seconds(7))), Duration::seconds(7));
        assert_eq!(retry_delay(3, Some(Duration::zero())), Duration::seconds(1));
    }
}
