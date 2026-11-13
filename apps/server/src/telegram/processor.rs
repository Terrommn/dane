//! Procesador de updates pendientes. Cada update se procesa en una transacción que incluye sus efectos, las respuestas
//! encoladas y la marca `processed`; un fallo revierte todo y el update se reintenta con backoff (también tras reinicio).

use persistence::repos::telegram;

use super::commands;
use crate::state::AppState;

/// Procesa como máximo un update. Devuelve `true` si había uno.
pub async fn process_next(state: &AppState) -> anyhow::Result<bool> {
    let now = state.now();
    let mut tx = state.pool.begin().await?;
    let Some((update_id, payload)) = telegram::lock_next_pending(&mut tx, now).await? else {
        return Ok(false);
    };
    match commands::handle_update(state, &mut tx, update_id, &payload, now).await {
        Ok(()) => {
            telegram::mark_processed(&mut tx, update_id, now).await?;
            tx.commit().await?;
        }
        Err(e) => {
            tx.rollback().await?;
            tracing::warn!(update_id, error = %e, "fallo al procesar update; se reintentará");
            telegram::mark_failure(&state.pool, update_id, &e.to_string(), now).await?;
        }
    }
    Ok(true)
}

/// Procesa todos los pendientes disponibles (útil en pruebas y al arrancar).
pub async fn drain(state: &AppState) -> anyhow::Result<usize> {
    let mut n = 0;
    while process_next(state).await? {
        n += 1;
    }
    Ok(n)
}
