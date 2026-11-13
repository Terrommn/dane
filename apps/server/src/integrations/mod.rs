//! Propietario: agente E-integrations. Google Calendar, Gmail, ClickUp, LLM, importación/exportación.
//!
//! Todas las integraciones con terceros son de **solo lectura** en este alcance: no se crean eventos, no se envían
//! correos y no se modifican tareas remotas. Las credenciales se guardan cifradas (AES-256-GCM, D-014).

pub mod apply;
pub mod clickup;
pub mod exporter;
pub mod google;
pub mod importer;
pub mod llm;

use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use persistence::repos::integrations as repo;
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::config::AppEnv;
use crate::state::AppState;
use crate::workers::Shutdown;

/// URLs base de los proveedores. En pruebas/staging se pueden sobrescribir por conexión
/// (`config.endpoints_override`) o por variables `*_BASE_URL`; nunca en producción.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub google_auth: String,
    pub google_token: String,
    pub google_revoke: String,
    pub google_calendar: String,
    pub gmail: String,
    pub clickup: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            google_auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            google_token: "https://oauth2.googleapis.com/token".into(),
            google_revoke: "https://oauth2.googleapis.com/revoke".into(),
            google_calendar: "https://www.googleapis.com/calendar/v3".into(),
            gmail: "https://gmail.googleapis.com/gmail/v1".into(),
            clickup: "https://api.clickup.com/api/v2".into(),
        }
    }
}

impl Endpoints {
    /// Un solo servidor simulado para todo (pruebas con wiremock).
    pub fn mock(base: &str) -> Self {
        Self {
            google_auth: format!("{base}/o/oauth2/v2/auth"),
            google_token: format!("{base}/token"),
            google_revoke: format!("{base}/revoke"),
            google_calendar: format!("{base}/calendar/v3"),
            gmail: format!("{base}/gmail/v1"),
            clickup: format!("{base}/api/v2"),
        }
    }

    pub fn resolve(state: &AppState, conn_config: Option<&serde_json::Value>) -> Self {
        if state.config.env == AppEnv::Production {
            return Self::default();
        }
        if let Some(base) = conn_config
            .and_then(|c| c.get("endpoints_override"))
            .and_then(|v| v.as_str())
        {
            return Self::mock(base);
        }
        match std::env::var("INTEGRATIONS_MOCK_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
        {
            Some(base) => Self::mock(&base),
            None => Self::default(),
        }
    }
}

/// Error de proveedor con clasificación para estado/backoff.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// Credenciales inválidas/revocadas: requiere reconectar.
    #[error("autorización rechazada por {0}; vuelve a conectar")]
    Unauthorized(&'static str),
    #[error("límite de peticiones de {0}; se reintentará más tarde")]
    RateLimited(&'static str),
    #[error("{provider}: {message}")]
    Http {
        provider: &'static str,
        status: u16,
        message: String,
    },
    #[error("{0} no está conectado")]
    NotConnected(&'static str),
    #[error("{0}")]
    Config(String),
    #[error("error de red con {0}: {1}")]
    Network(&'static str, String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ProviderError {
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::Unauthorized(_) | Self::NotConnected(_) | Self::Config(_)
        )
    }
}

impl From<persistence::RepoError> for ProviderError {
    fn from(e: persistence::RepoError) -> Self {
        Self::Internal(anyhow::anyhow!("repo: {e}"))
    }
}

impl From<sqlx::Error> for ProviderError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(anyhow::anyhow!("db: {e}"))
    }
}

impl From<ProviderError> for crate::error::ApiError {
    fn from(e: ProviderError) -> Self {
        use axum::http::StatusCode;
        match &e {
            ProviderError::NotConnected(_) | ProviderError::Config(_) => {
                crate::error::ApiError::new(StatusCode::CONFLICT, "integration_not_ready", e.to_string())
            }
            ProviderError::Internal(_) => crate::error::ApiError::internal(),
            _ => crate::error::ApiError::new(StatusCode::BAD_GATEWAY, "provider_error", e.to_string()),
        }
    }
}

/// Convierte una respuesta HTTP del proveedor en JSON o en `ProviderError` clasificado.
pub async fn read_json<T: DeserializeOwned>(
    provider: &'static str,
    resp: reqwest::Response,
) -> Result<T, ProviderError> {
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 && provider == "clickup" {
        return Err(ProviderError::Unauthorized(provider));
    }
    if status.as_u16() == 429 {
        return Err(ProviderError::RateLimited(provider));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| ProviderError::Network(provider, e.to_string()))?;
    if !status.is_success() {
        // El cuerpo puede incluir detalles; nunca incluye nuestras credenciales. Se recorta.
        return Err(ProviderError::Http {
            provider,
            status: status.as_u16(),
            message: body.chars().take(200).collect(),
        });
    }
    serde_json::from_str(&body).map_err(|e| ProviderError::Http {
        provider,
        status: status.as_u16(),
        message: format!("respuesta inesperada: {e}"),
    })
}

pub fn net(provider: &'static str) -> impl Fn(reqwest::Error) -> ProviderError {
    move |e| ProviderError::Network(provider, e.without_url().to_string())
}

pub fn encrypt_json<T: Serialize>(state: &AppState, v: &T) -> anyhow::Result<Vec<u8>> {
    state.cipher.encrypt(&serde_json::to_vec(v)?)
}

pub fn decrypt_json<T: DeserializeOwned>(state: &AppState, data: &[u8]) -> anyhow::Result<T> {
    Ok(serde_json::from_slice(&state.cipher.decrypt(data)?)?)
}

/// Zona del usuario (para fechas locales de eventos/correos).
pub async fn user_tz(state: &AppState, user_id: Uuid) -> chrono_tz::Tz {
    let tz: Option<String> = sqlx::query_scalar("SELECT timezone FROM user_settings WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
    tz.and_then(|t| domain::time::parse_tz(&t).ok())
        .unwrap_or(chrono_tz::America::Monterrey)
}

pub async fn today(state: &AppState, user_id: Uuid) -> NaiveDate {
    domain::time::local_date(state.now(), user_tz(state, user_id).await)
}

/// Propone una interacción con una persona conocida o una persona nueva (nunca la crea).
/// Devuelve `true` si se insertó algo nuevo.
pub async fn suggest_contact(
    state: &AppState,
    user_id: Uuid,
    email: &str,
    display_name: Option<&str>,
    on_date: NaiveDate,
    topic: &str,
    source: &str,
    source_ref: &str,
) -> Result<bool, ProviderError> {
    let email = email.trim().to_lowercase();
    if email.is_empty() {
        return Ok(false);
    }
    let person: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM people WHERE user_id = $1 AND lower(email) = $2 AND archived_at IS NULL LIMIT 1",
    )
    .bind(user_id)
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;
    let n = match person {
        Some(pid) => sqlx::query(
            "INSERT INTO interaction_suggestions (user_id, person_id, on_date, topic, source, source_ref) VALUES ($1, $2, $3, left($4, 300), $5, $6)
             ON CONFLICT (user_id, person_id, source, source_ref) DO NOTHING",
        )
        .bind(user_id)
        .bind(pid)
        .bind(on_date)
        .bind(topic)
        .bind(source)
        .bind(source_ref)
        .execute(&state.pool)
        .await?
        .rows_affected(),
        None => sqlx::query(
            "INSERT INTO person_suggestions (user_id, name, email, source, source_ref, on_date, context) VALUES ($1, $2, $3, $4, $5, $6, left($7, 300))
             ON CONFLICT (user_id, source, source_ref) DO NOTHING",
        )
        .bind(user_id)
        .bind(display_name.filter(|n| !n.trim().is_empty()).unwrap_or(&email))
        .bind(&email)
        .bind(source)
        .bind(format!("email:{email}"))
        .bind(on_date)
        .bind(topic)
        .execute(&state.pool)
        .await?
        .rows_affected(),
    };
    Ok(n > 0)
}

/// Resultado común de una sincronización.
#[derive(Debug, Clone, Default, Serialize, utoipa::ToSchema)]
pub struct SyncReport {
    pub provider: String,
    pub kind: String,
    pub upserted: i32,
    pub removed: i32,
    pub suggestions: i32,
    pub skipped: i32,
    pub error: Option<String>,
}

/// Registra el resultado de una sincronización y actualiza estado/backoff de la conexión.
pub async fn record_outcome(
    state: &AppState,
    user_id: Uuid,
    provider: &str,
    kind: &str,
    started: DateTime<Utc>,
    r: &Result<SyncReport, ProviderError>,
) {
    let now = state.now();
    let res = match r {
        Ok(rep) => {
            let _ = repo::log_run(
                &state.pool,
                user_id,
                provider,
                kind,
                started,
                true,
                rep.upserted + rep.suggestions,
                None,
            )
            .await;
            repo::mark_sync_ok(
                &state.pool,
                user_id,
                provider,
                now,
                now + chrono::Duration::minutes(15),
            )
            .await
        }
        Err(e) => {
            tracing::warn!(provider, kind, error = %e, "sincronización fallida");
            let msg = e.to_string();
            let _ = repo::log_run(
                &state.pool,
                user_id,
                provider,
                kind,
                started,
                false,
                0,
                Some(&msg),
            )
            .await;
            repo::mark_error(&state.pool, user_id, provider, &msg, e.is_fatal(), now).await
        }
    };
    if let Err(e) = res {
        tracing::error!(error = %e, "no se pudo registrar el estado de sincronización");
    }
}

/// Sincronización periódica: cada minuto revisa conexiones vencidas (cada 15 min, backoff exponencial ante errores).
pub fn workers(state: &AppState, shutdown: &Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    let state = state.clone();
    let shutdown = shutdown.clone();
    vec![tokio::spawn(async move {
        loop {
            if !shutdown.sleep(Duration::from_secs(60)).await {
                break;
            }
            sync_due(&state).await;
        }
    })]
}

pub async fn sync_due(state: &AppState) {
    let now = state.now();
    if state.config.google.is_some()
        && let Ok(users) = repo::claim_due(&state.pool, "google", now, 20).await
    {
        for u in users {
            google::sync_all(state, u, None).await;
        }
    }
    if let Ok(users) = repo::claim_due(&state.pool, "clickup", now, 20).await {
        for u in users {
            let started = state.now();
            let r = clickup::sync(state, u).await;
            record_outcome(state, u, "clickup", "tasks", started, &r).await;
        }
    }
}

/// Verificador PKCE (43+ caracteres) y su reto S256.
pub fn pkce_pair() -> (String, String) {
    use base64::Engine;
    let verifier = crate::crypto::random_token();
    let challenge =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(crate::crypto::sha256(verifier.as_bytes()));
    (verifier, challenge)
}

#[cfg(test)]
mod tests {
    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        use base64::Engine;
        let (v, c) = super::pkce_pair();
        assert!(v.len() >= 43);
        assert_eq!(
            c,
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(crate::crypto::sha256(v.as_bytes()))
        );
    }
}
