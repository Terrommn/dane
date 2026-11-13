//! API interna para el bot (`/internal/v1`), autenticada con `INTERNAL_API_TOKEN`. No se expone por el proxy público.

use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::{self, Next};
use axum::response::Response;

use crate::crypto::ct_eq;
use crate::error::ApiError;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        // INTERNAL: los endpoints del bot (updates, jobs) se agregan aquí.
        .layer(middleware::from_fn_with_state(state, require_internal_token))
}

async fn require_internal_token(
    State(s): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let presented = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !ct_eq(presented.as_bytes(), s.config.internal_api_token.as_bytes()) {
        return Err(ApiError::unauthorized());
    }
    Ok(next.run(req).await)
}
