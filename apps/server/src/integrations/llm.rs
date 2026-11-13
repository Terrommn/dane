//! Adaptador real de la Messages API de Anthropic (D-013) por HTTP: Rust no tiene SDK oficial.
//! Salida restringida con `output_config.format` (JSON Schema estricto) y validada otra vez en Rust
//! (`domain::capture::parse_model_output` + `validate`). Sin herramientas; contexto mínimo; texto del usuario delimitado.

use domain::capture::{self, CaptureContext, Proposal};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("La IA no está configurada (ANTHROPIC_API_KEY); usa el modo manual")]
    NotConfigured,
    #[error("La credencial de IA fue rechazada")]
    Unauthorized,
    #[error("El proveedor de IA está saturado o limitó las peticiones; intenta en un momento")]
    Busy,
    #[error("El modelo rechazó la petición")]
    Refused,
    #[error("Salida inválida del modelo: {0}")]
    InvalidOutput(String),
    #[error("Error del proveedor de IA ({0})")]
    Http(u16),
    #[error("Error de red con el proveedor de IA")]
    Network,
}

pub fn configured(state: &AppState) -> bool {
    state.config.anthropic_api_key.is_some()
}

#[derive(Deserialize)]
struct MessageResponse {
    #[serde(default)]
    content: Vec<Block>,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: String,
}

/// Extrae una propuesta tipada. No aplica nada.
pub async fn extract(
    state: &AppState,
    intent: &str,
    ctx: &CaptureContext,
    text: &str,
) -> Result<Proposal, LlmError> {
    let key = state
        .config
        .anthropic_api_key
        .as_deref()
        .ok_or(LlmError::NotConfigured)?;
    let body = json!({
        "model": state.config.llm_model,
        "max_tokens": 4096,
        "system": capture::SYSTEM_PROMPT,
        "messages": [{ "role": "user", "content": capture::user_message(intent, ctx, text) }],
        "output_config": { "format": { "type": "json_schema", "schema": capture::json_schema() } }
    });
    let resp = state
        .http
        .post(format!(
            "{}/v1/messages",
            state.config.anthropic_base_url.trim_end_matches('/')
        ))
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e.without_url(), "LLM: error de red");
            LlmError::Network
        })?;
    let status = resp.status().as_u16();
    match status {
        200 => {}
        401 | 403 => return Err(LlmError::Unauthorized),
        429 | 529 | 503 => return Err(LlmError::Busy),
        s => {
            tracing::warn!(status = s, "LLM: respuesta de error");
            return Err(LlmError::Http(s));
        }
    }
    let msg: MessageResponse = resp
        .json()
        .await
        .map_err(|e| LlmError::InvalidOutput(e.to_string()))?;
    if msg.stop_reason.as_deref() == Some("refusal") {
        return Err(LlmError::Refused);
    }
    if msg.stop_reason.as_deref() == Some("max_tokens") {
        return Err(LlmError::InvalidOutput("respuesta truncada".into()));
    }
    let text: String = msg
        .content
        .iter()
        .filter(|b| b.kind == "text")
        .map(|b| b.text.as_str())
        .collect();
    capture::parse_model_output(&text).map_err(LlmError::InvalidOutput)
}

/// Contexto mínimo del usuario: catálogos y nombres, nunca registros completos.
pub async fn context(
    state: &AppState,
    user_id: Uuid,
    reference_date: chrono::NaiveDate,
) -> anyhow::Result<CaptureContext> {
    let (tz, currency): (String, String) =
        sqlx::query_as("SELECT timezone, currency::text FROM user_settings WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?
            .unwrap_or_else(|| ("America/Monterrey".into(), "MXN".into()));
    let cats: Vec<(String, String)> = sqlx::query_as(
        "SELECT kind, name FROM fin_categories WHERE user_id = $1 AND archived_at IS NULL ORDER BY position",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    let areas: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM areas WHERE user_id = $1 AND archived_at IS NULL ORDER BY position",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    let accounts: Vec<String> = sqlx::query_scalar("SELECT name FROM fin_accounts WHERE user_id = $1 AND archived_at IS NULL AND status IN ('active','to_cancel') ORDER BY name")
        .bind(user_id)
        .fetch_all(&state.pool)
        .await?;
    let people: Vec<(String, Vec<String>)> = sqlx::query_as("SELECT name, aliases FROM people WHERE user_id = $1 AND archived_at IS NULL ORDER BY last_contact_on DESC NULLS LAST LIMIT 200")
        .bind(user_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(CaptureContext {
        reference_date,
        timezone: tz,
        currency,
        expense_categories: cats
            .iter()
            .filter(|c| c.0 == "expense")
            .map(|c| c.1.clone())
            .collect(),
        income_categories: cats
            .iter()
            .filter(|c| c.0 == "income")
            .map(|c| c.1.clone())
            .collect(),
        areas,
        accounts,
        people: people
            .into_iter()
            .map(|(n, a)| {
                if a.is_empty() {
                    n
                } else {
                    format!("{n} ({})", a.join(", "))
                }
            })
            .collect(),
    })
}
