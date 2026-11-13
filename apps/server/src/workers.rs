//! Tareas de fondo del servidor (scheduler de recordatorios/recurrencias, procesador de updates, sincronizaciones).
//! Todas son seguras con varias instancias: la coordinación es por UNIQUE y `SKIP LOCKED` en PostgreSQL.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use crate::state::AppState;

/// Señal de cancelación cooperativa.
#[derive(Clone, Default)]
pub struct Shutdown(Arc<(AtomicBool, Notify)>);

impl Shutdown {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.0.store(true, Ordering::SeqCst);
        self.0.1.notify_waiters();
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.0.load(Ordering::SeqCst)
    }
    pub async fn cancelled(&self) {
        while !self.is_cancelled() {
            let n = self.0.1.notified();
            if self.is_cancelled() {
                break;
            }
            n.await;
        }
    }
    /// Espera `d` o hasta la cancelación; devuelve `false` si se canceló.
    pub async fn sleep(&self, d: std::time::Duration) -> bool {
        tokio::select! {
            _ = tokio::time::sleep(d) => !self.is_cancelled(),
            _ = self.cancelled() => false,
        }
    }
}

pub fn spawn_all(state: AppState, shutdown: Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();
    if !state.config.scheduler_enabled {
        tracing::info!("SCHEDULER_ENABLED=false: sin tareas de fondo");
        return handles;
    }
    handles.extend(crate::api::tasks::workers(&state, &shutdown));
    handles.extend(crate::api::finance::workers(&state, &shutdown));
    handles.extend(crate::telegram::workers(&state, &shutdown));
    handles.extend(crate::integrations::workers(&state, &shutdown));
    handles
}
