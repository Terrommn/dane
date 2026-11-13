//! Webhook (producción) y health. Responde 200 a Telegram **solo** después de que el server aceptó el update de forma
//! durable; si el server no responde, devuelve 503 y Telegram reintenta.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::server_api::ServerApi;

pub const WEBHOOK_PATH: &str = "/telegram/webhook";
pub const SECRET_HEADER: &str = "x-telegram-bot-api-secret-token";

#[derive(Clone)]
pub struct WebhookState {
    pub api: ServerApi,
    /// `None` en modo polling: el endpoint de webhook responde 404.
    pub secret: Option<String>,
}

pub fn router(state: WebhookState) -> Router {
    Router::new()
        .route(WEBHOOK_PATH, post(receive))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state)
}

/// Comparación en tiempo constante (vía hashes de igual longitud).
fn secret_matches(presented: &str, expected: &str) -> bool {
    let (a, b) = (
        Sha256::digest(presented.as_bytes()),
        Sha256::digest(expected.as_bytes()),
    );
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn receive(State(s): State<WebhookState>, headers: HeaderMap, Json(update): Json<Value>) -> StatusCode {
    let Some(expected) = s.secret.as_deref() else {
        return StatusCode::NOT_FOUND;
    };
    let presented = headers
        .get(SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !secret_matches(presented, expected) {
        tracing::warn!("webhook con secreto inválido rechazado");
        return StatusCode::UNAUTHORIZED;
    }
    let Some(id) = update.get("update_id").and_then(Value::as_i64) else {
        return StatusCode::BAD_REQUEST;
    };
    match s.api.ingest(id, update).await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            tracing::warn!(update_id = id, error = %e, "server no aceptó el update; Telegram reintentará");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
