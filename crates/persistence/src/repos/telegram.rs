//! Updates entrantes de Telegram y chats conocidos (propietario: D-telegram).
//!
//! Durabilidad: `ingest` confirma la fila `pending` antes de que el server responda al bot (y el bot a Telegram).
//! Un update repetido no reemplaza ni descarta el pendiente: se procesa exactamente una vez porque el procesador
//! lo marca `processed` en la misma transacción que sus efectos.

use chrono::{DateTime, Duration, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::{RepoResult, Tx};

/// Tras este número de fallos de procesamiento el update queda `failed` (visible en logs) y deja de reintentarse.
pub const MAX_UPDATE_ATTEMPTS: i32 = 8;

/// Devuelve `true` si el update ya existía (duplicado).
pub async fn ingest(
    pool: &PgPool,
    update_id: i64,
    payload: &serde_json::Value,
    now: DateTime<Utc>,
) -> RepoResult<bool> {
    let inserted = sqlx::query(
        "INSERT INTO telegram_updates (update_id, payload, received_at) VALUES ($1, $2, $3) ON CONFLICT (update_id) DO NOTHING",
    )
    .bind(update_id)
    .bind(payload)
    .bind(now)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(inserted == 0)
}

/// Toma el siguiente update pendiente y lo bloquea en la transacción del procesador (`SKIP LOCKED`).
pub async fn lock_next_pending(
    tx: &mut Tx<'_>,
    now: DateTime<Utc>,
) -> RepoResult<Option<(i64, serde_json::Value)>> {
    Ok(sqlx::query_as(
        "SELECT update_id, payload FROM telegram_updates
         WHERE status = 'pending' AND (locked_until IS NULL OR locked_until <= $1)
         ORDER BY update_id LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .bind(now)
    .fetch_optional(&mut **tx)
    .await?)
}

pub async fn mark_processed(tx: &mut Tx<'_>, update_id: i64, now: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query(
        "UPDATE telegram_updates SET status = 'processed', processed_at = $2, attempts = attempts + 1, locked_until = NULL, last_error = NULL
         WHERE update_id = $1",
    )
    .bind(update_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Registra un fallo de procesamiento (fuera de la transacción revertida) con backoff.
pub async fn mark_failure(pool: &PgPool, update_id: i64, error: &str, now: DateTime<Utc>) -> RepoResult<()> {
    sqlx::query(
        "UPDATE telegram_updates SET attempts = attempts + 1, last_error = $2,
            locked_until = $3 + make_interval(secs => least(3600, 5 * power(2, attempts))),
            status = CASE WHEN attempts + 1 >= $4 THEN 'failed' ELSE 'pending' END
         WHERE update_id = $1 AND status = 'pending'",
    )
    .bind(update_id)
    .bind(error.chars().take(500).collect::<String>())
    .bind(now)
    .bind(MAX_UPDATE_ATTEMPTS)
    .execute(pool)
    .await?;
    Ok(())
}

/// Registra o reactiva el chat privado del usuario.
pub async fn upsert_chat(tx: &mut Tx<'_>, user_id: Uuid, chat_id: i64) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO telegram_chats (user_id, chat_id) VALUES ($1, $2)
         ON CONFLICT (user_id) DO UPDATE SET chat_id = EXCLUDED.chat_id, can_message = true, last_error = NULL",
    )
    .bind(user_id)
    .bind(chat_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn purge_processed(db: impl PgExecutor<'_>, older_than: DateTime<Utc>) -> RepoResult<u64> {
    Ok(
        sqlx::query("DELETE FROM telegram_updates WHERE status = 'processed' AND processed_at < $1")
            .bind(older_than)
            .execute(db)
            .await?
            .rows_affected(),
    )
}

pub fn retention() -> Duration {
    Duration::days(30)
}
