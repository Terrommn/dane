//! DTOs de la API interna entre `apps/server` y `apps/telegram-bot` (`/internal/v1`, Bearer `INTERNAL_API_TOKEN`).
//!
//! * `POST /internal/v1/telegram/updates` — [`UpdateIngest`] → [`UpdateAccepted`]. Aceptación durable: el server
//!   responde 200 solo después de confirmar la transacción.
//! * `POST /internal/v1/jobs/claim` — [`ClaimRequest`] → [`ClaimResponse`] (lease con `lease_token`).
//! * `POST /internal/v1/jobs/{id}/ack` — [`AckRequest`] → [`AckResponse`]; 409 si el lease ya no es del worker.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Update de Telegram sin modificar (JSON crudo) e identificador.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateIngest {
    pub update_id: i64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateAccepted {
    pub update_id: i64,
    /// Ya existía (pendiente o procesado). No se pierde: un pendiente sigue pendiente hasta procesarse.
    pub duplicate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRequest {
    /// Identificador del proceso del bot (para diagnóstico).
    pub worker_id: String,
    /// Máximo de trabajos a reclamar (1–50).
    pub limit: u32,
    /// Duración del lease en segundos (10–600).
    pub lease_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaimResponse {
    pub jobs: Vec<ClaimedJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub lease_token: Uuid,
    pub kind: String,
    /// Número de intento (1 = primero).
    pub attempt: i32,
    pub message: OutboundMessage,
}

/// Mensaje breve a enviar. La edición detallada ocurre en la app (botones).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutboundMessage {
    pub chat_id: i64,
    pub text: String,
    /// Filas de botones inline.
    #[serde(default)]
    pub buttons: Vec<Vec<Button>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Button {
    /// Abre la Mini App en la URL (HTTPS obligatorio para Telegram).
    WebApp { text: String, url: String },
    /// Enlace normal (se usa cuando el origen público no es HTTPS, p. ej. desarrollo local).
    Url { text: String, url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckRequest {
    pub lease_token: Uuid,
    pub outcome: AckOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum AckOutcome {
    /// Telegram aceptó el mensaje.
    Sent { telegram_message_id: Option<i64> },
    /// Error transitorio (red, 5xx, 429). `retry_after_secs` viene de Telegram en 429.
    Retry {
        retry_after_secs: Option<u64>,
        error: String,
    },
    /// Rechazo permanente (400/403). `chat_blocked` marca el chat como no disponible.
    Failed { error: String, chat_blocked: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AckResponse {
    /// Estado resultante del job: `sent | pending | failed | cancelled | expired`.
    pub status: String,
    pub next_attempt_at: Option<DateTime<Utc>>,
    /// El ack repetía uno ya aplicado (idempotente).
    pub replayed: bool,
}

pub const PATH_UPDATES: &str = "/internal/v1/telegram/updates";
pub const PATH_CLAIM: &str = "/internal/v1/jobs/claim";

pub fn path_ack(id: Uuid) -> String {
    format!("/internal/v1/jobs/{id}/ack")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_stable() {
        let a = AckOutcome::Retry {
            retry_after_secs: Some(5),
            error: "429".into(),
        };
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::json!({"result": "retry", "retry_after_secs": 5, "error": "429"})
        );
        let b = Button::WebApp {
            text: "Abrir".into(),
            url: "https://x/hoy".into(),
        };
        assert_eq!(serde_json::to_value(&b).unwrap()["type"], "web_app");
        let m: OutboundMessage =
            serde_json::from_value(serde_json::json!({"chat_id": 1, "text": "hola"})).unwrap();
        assert!(m.buttons.is_empty());
    }
}

/// Comandos del bot (nombres válidos de Telegram) con su descripción y el comando equivalente del vault.
/// Fuente única para `setMyCommands` (bot) y `/help` (server).
pub const COMMANDS: &[(&str, &str, &str)] = &[
    ("start", "Conectar el bot y abrir la app", ""),
    ("daily", "Check-in de hoy (o dicta tu día)", "daily"),
    ("checkin", "Abrir el check-in (opcional: ayer o AAAA-MM-DD)", ""),
    ("objetivos", "Objetivos del mes y la semana", "objetivos"),
    ("dump", "Vaciar pendientes en texto libre", "dump"),
    ("gasto", "Registrar gastos", "gasto"),
    ("ingreso", "Registrar ingresos", "ingreso"),
    ("weekly_review", "Revisión semanal", "weekly-review"),
    ("log_session", "Registrar una sesión de código", "log-session"),
    ("dashboard", "Ver tu dashboard", ""),
    ("settings", "Ajustes y recordatorios", ""),
    ("help", "Ayuda y comandos", ""),
    ("pause", "Pausar recordatorios (opcional: días)", ""),
    ("resume", "Reanudar recordatorios", ""),
];
