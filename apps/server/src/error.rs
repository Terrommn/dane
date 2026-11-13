//! Errores HTTP consistentes: `{"error": {"code", "message", "field?", "current?"}}`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use domain::DomainError;
use persistence::RepoError;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Código estable: `bad_request`, `unauthorized`, `forbidden`, `not_found`, `conflict`, `validation`,
    /// `idempotency_mismatch`, `rate_limited`, `payload_too_large`, `unavailable`, `internal`.
    pub code: String,
    /// Mensaje en español para mostrar al usuario.
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// En `conflict`: estado actual del recurso para resolver explícitamente.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub current: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub field: Option<String>,
    pub current: Option<serde_json::Value>,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            field: None,
            current: None,
        }
    }
    pub fn bad_request(m: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", m)
    }
    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Inicia sesión para continuar",
        )
    }
    pub fn forbidden(m: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", m)
    }
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "No encontrado")
    }
    pub fn validation(field: &str, m: impl Into<String>) -> Self {
        Self {
            field: Some(field.into()),
            ..Self::new(StatusCode::UNPROCESSABLE_ENTITY, "validation", m)
        }
    }
    pub fn conflict(current: serde_json::Value) -> Self {
        Self {
            current: Some(current),
            ..Self::new(
                StatusCode::CONFLICT,
                "conflict",
                "El registro cambió en otro dispositivo; revisa la versión actual",
            )
        }
    }
    pub fn unavailable(m: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, "unavailable", m)
    }
    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Error interno; intenta de nuevo",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code.into(),
                message: self.message,
                field: self.field,
                current: self.current,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<DomainError> for ApiError {
    fn from(e: DomainError) -> Self {
        ApiError::validation(&e.field, e.message)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            let constraint = db.constraint().unwrap_or_default().to_string();
            match db.code().as_deref() {
                // unique_violation
                Some("23505") => {
                    return ApiError {
                        field: Some(constraint),
                        ..ApiError::new(
                            StatusCode::CONFLICT,
                            "duplicate",
                            "Ya existe un registro con esos datos",
                        )
                    };
                }
                // foreign_key_violation: incluye intento de enlazar recursos de otro usuario.
                Some("23503") => {
                    return ApiError {
                        field: Some(constraint),
                        ..ApiError::new(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "invalid_reference",
                            "Referencia inexistente o no permitida",
                        )
                    };
                }
                // check_violation / not_null_violation
                Some("23514") | Some("23502") => {
                    return ApiError::validation(&constraint, "Valor no permitido");
                }
                Some("22P02") | Some("22007") | Some("22008") => {
                    return ApiError::validation("input", "Formato inválido");
                }
                _ => {}
            }
        }
        if matches!(e, sqlx::Error::RowNotFound) {
            return ApiError::not_found();
        }
        tracing::error!(error = %e, "error de base de datos");
        ApiError::internal()
    }
}

impl From<RepoError> for ApiError {
    fn from(e: RepoError) -> Self {
        match e {
            RepoError::NotFound => ApiError::not_found(),
            RepoError::Conflict(current) => ApiError::conflict(current),
            RepoError::Db(db) => db.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!(error = %e, "error interno");
        ApiError::internal()
    }
}
