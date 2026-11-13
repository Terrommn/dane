//! Propietario: agente C-finance. Ver docs/CONVENTIONS.md.

use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
}

/// Tareas periódicas del módulo (materialización idempotente de recurrencias).
pub fn workers(_state: &AppState, _shutdown: &crate::workers::Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    Vec::new()
}
