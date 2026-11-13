//! Telegram Login por OpenID Connect: Authorization Code + PKCE (S256), validado en el servidor con `openidconnect`.
//!
//! * `state`, `nonce` y `code_verifier` se guardan en `oauth_flows` (un solo uso, 10 min).
//! * El ID token se valida (firma JWKS, `iss`, `aud`, `exp`, `nonce`) con el verificador de la librería.
//! * El ID de Telegram viene en el claim `id`, distinto de `sub`; se lee del mismo token ya verificado.

use std::time::{Duration as StdDuration, Instant};

use base64::Engine;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use tokio::sync::Mutex;

use crate::config::OidcConfig;

#[derive(Default)]
pub struct MetadataCache(Mutex<Option<(Instant, CoreProviderMetadata)>>);

const METADATA_TTL: StdDuration = StdDuration::from_secs(3600);

fn http_client() -> anyhow::Result<openidconnect::reqwest::Client> {
    Ok(openidconnect::reqwest::ClientBuilder::new()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .timeout(StdDuration::from_secs(15))
        .build()?)
}

impl MetadataCache {
    /// Metadatos de descubrimiento + JWKS, cacheados una hora. `force` los recarga (rotación de llaves).
    pub async fn get(&self, cfg: &OidcConfig, force: bool) -> anyhow::Result<CoreProviderMetadata> {
        let mut g = self.0.lock().await;
        if !force {
            if let Some((at, m)) = g.as_ref() {
                if at.elapsed() < METADATA_TTL {
                    return Ok(m.clone());
                }
            }
        }
        let m = CoreProviderMetadata::discover_async(IssuerUrl::new(cfg.issuer.clone())?, &http_client()?)
            .await?;
        *g = Some((Instant::now(), m.clone()));
        Ok(m)
    }
}

pub struct AuthStart {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

pub async fn start(cache: &MetadataCache, cfg: &OidcConfig, redirect_uri: &str) -> anyhow::Result<AuthStart> {
    let meta = cache.get(cfg, false).await?;
    let client = CoreClient::from_provider_metadata(
        meta,
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string())?);
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, state, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("profile".into()))
        .set_pkce_challenge(challenge)
        .url();
    Ok(AuthStart {
        url: url.to_string(),
        state: state.secret().clone(),
        nonce: nonce.secret().clone(),
        pkce_verifier: verifier.secret().clone(),
    })
}

#[derive(Debug, Clone)]
pub struct OidcIdentity {
    pub subject: String,
    pub telegram_user_id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    #[error("intercambio de código fallido: {0}")]
    Exchange(String),
    #[error("ID token ausente")]
    MissingIdToken,
    #[error("ID token inválido: {0}")]
    InvalidToken(String),
    #[error("ID token sin claim id de Telegram")]
    MissingTelegramId,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub async fn finish(
    cache: &MetadataCache,
    cfg: &OidcConfig,
    redirect_uri: &str,
    code: &str,
    pkce_verifier: &str,
    nonce: &str,
) -> Result<OidcIdentity, OidcError> {
    let meta = cache.get(cfg, false).await?;
    let client = CoreClient::from_provider_metadata(
        meta,
        ClientId::new(cfg.client_id.clone()),
        Some(ClientSecret::new(cfg.client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string()).map_err(anyhow::Error::from)?);
    let http = http_client()?;
    let token = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .map_err(|e| OidcError::Exchange(e.to_string()))?
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier.to_string()))
        .request_async(&http)
        .await
        .map_err(|e| OidcError::Exchange(e.to_string()))?;
    let id_token = token.id_token().ok_or(OidcError::MissingIdToken)?;
    let verifier = client.id_token_verifier();
    let claims = match id_token.claims(&verifier, &Nonce::new(nonce.to_string())) {
        Ok(c) => c.clone(),
        Err(e) => {
            // Posible rotación de llaves: recargar JWKS una vez y reintentar la verificación.
            let meta = cache.get(cfg, true).await?;
            let client2 = CoreClient::from_provider_metadata(
                meta,
                ClientId::new(cfg.client_id.clone()),
                Some(ClientSecret::new(cfg.client_secret.clone())),
            );
            id_token
                .claims(&client2.id_token_verifier(), &Nonce::new(nonce.to_string()))
                .map_err(|_| OidcError::InvalidToken(e.to_string()))?
                .clone()
        }
    };
    // El token ya fue verificado: se leen claims adicionales (`id`, `preferred_username`) de su payload.
    let raw = id_token.to_string();
    let payload = raw
        .split('.')
        .nth(1)
        .ok_or_else(|| OidcError::InvalidToken("formato".into()))?;
    let json: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload.trim_end_matches('='))
            .map_err(|_| OidcError::InvalidToken("base64".into()))?,
    )
    .map_err(|_| OidcError::InvalidToken("json".into()))?;
    let telegram_user_id = json
        .get("id")
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .ok_or(OidcError::MissingTelegramId)?;
    Ok(OidcIdentity {
        subject: claims.subject().as_str().to_string(),
        telegram_user_id,
        username: json
            .get("preferred_username")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        first_name: json
            .get("name")
            .or_else(|| json.get("given_name"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}
