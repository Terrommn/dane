//! Reglas de negocio puras: sin IO, sin Telegram, sin UI. Todo lo dependiente del tiempo recibe fechas o un `Clock`.

pub mod capture;
pub mod checkin;
pub mod finance;
pub mod goals;
pub mod habits;
pub mod metrics;
pub mod money;
pub mod reminders;
pub mod tasks;
pub mod text;
pub mod time;
pub mod vault;

use serde::Serialize;

/// Error de validación o de regla de dominio, con campo y mensaje legibles (español).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[error("{field}: {message}")]
pub struct DomainError {
    pub field: String,
    pub message: String,
}

impl DomainError {
    pub fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

pub type DomainResult<T> = Result<T, DomainError>;
