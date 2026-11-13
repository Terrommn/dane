//! Propietario: agente D-telegram. Procesamiento de updates, outbox de mensajes, scheduler de recordatorios.
//!
//! El bot (`apps/telegram-bot`) solo transporta: entrega updates a `/internal/v1/telegram/updates` y reclama/confirma
//! mensajes en `/internal/v1/jobs/*`. Las reglas viven aquí.

pub mod commands;
pub mod internal_api;
pub mod messages;
pub mod processor;
pub mod scheduler;

use std::time::Duration;

use crate::state::AppState;
use crate::workers::Shutdown;

pub fn workers(state: &AppState, shutdown: &Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    let updates = {
        let (state, shutdown) = (state.clone(), shutdown.clone());
        tokio::spawn(async move {
            tracing::info!("procesador de updates de Telegram iniciado");
            while !shutdown.is_cancelled() {
                let busy = match processor::process_next(&state).await {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!(error = %e, "procesador de updates");
                        false
                    }
                };
                if !busy && !shutdown.sleep(Duration::from_secs(1)).await {
                    break;
                }
            }
        })
    };
    let reminders = {
        let (state, shutdown) = (state.clone(), shutdown.clone());
        tokio::spawn(async move {
            tracing::info!("scheduler de recordatorios iniciado");
            let mut ticks: u64 = 0;
            loop {
                match scheduler::run_once(&state).await {
                    Ok(n) if n > 0 => tracing::info!(creados = n, "recordatorios programados"),
                    Ok(_) => {}
                    Err(e) => tracing::error!(error = %e, "scheduler de recordatorios"),
                }
                if ticks % 1440 == 0 {
                    retention(&state).await;
                }
                ticks += 1;
                if !shutdown.sleep(Duration::from_secs(60)).await {
                    break;
                }
            }
        })
    };
    vec![updates, reminders]
}

async fn retention(state: &AppState) {
    let now = state.now();
    let r1 = persistence::repos::telegram::purge_processed(
        &state.pool,
        now - persistence::repos::telegram::retention(),
    )
    .await;
    let r2 = persistence::repos::jobs::purge_finished(&state.pool, now - chrono::Duration::days(90)).await;
    if let (Ok(a), Ok(b)) = (r1, r2) {
        tracing::info!(updates = a, jobs = b, "retención aplicada");
    }
}
