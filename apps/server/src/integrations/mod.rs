//! Propietario: agente E-integrations. Google Calendar, Gmail, ClickUp, LLM, importación/exportación.

use crate::state::AppState;
use crate::workers::Shutdown;

pub fn workers(_state: &AppState, _shutdown: &Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    Vec::new()
}
