//! Utilidades HTTP compartidas: idempotencia, paginación y validación de entradas.

pub mod idempotency;

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Parámetros de lista comunes. `limit` máximo 200.
#[derive(Debug, Clone, Deserialize, IntoParams, Default)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// Búsqueda de texto (tolerante a acentos donde aplique).
    pub q: Option<String>,
    /// Incluir archivados.
    pub archived: Option<bool>,
}

impl ListParams {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(50).clamp(1, 200)
    }
    pub fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
    pub fn archived(&self) -> bool {
        self.archived.unwrap_or(false)
    }
    pub fn q(&self) -> Option<String> {
        self.q
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(200).collect())
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}
