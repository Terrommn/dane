//! Propietario: agente D-telegram. Procesamiento de updates, outbox de mensajes, scheduler de recordatorios.

use crate::state::AppState;
use crate::workers::Shutdown;

pub fn workers(_state: &AppState, _shutdown: &Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    Vec::new()
}
