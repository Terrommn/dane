//! Configuración desde variables de entorno, validada al arrancar. Ver `.env.example`.

use std::collections::HashSet;
use std::net::SocketAddr;

use anyhow::{Context, bail};
use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnv {
    Development,
    Test,
    Production,
}

#[derive(Clone)]
pub struct Config {
    pub env: AppEnv,
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub database_max_connections: u32,
    /// Origen público único de web + API (ej. `https://dane.ejemplo.com`).
    pub public_base_url: url::Url,
    pub telegram_bot_token: Option<String>,
    pub telegram_bot_username: Option<String>,
    pub miniapp_max_age_secs: i64,
    pub allowed_telegram_ids: HashSet<i64>,
    pub cookie_secure: bool,
    pub cookie_session_ttl_hours: i64,
    pub bearer_session_ttl_hours: i64,
    pub oidc: Option<OidcConfig>,
    pub internal_api_token: String,
    pub encryption_key: [u8; 32],
    pub dev_login_enabled: bool,
    pub demo_enabled: bool,
    pub google: Option<OAuthApp>,
    pub anthropic_api_key: Option<String>,
    pub llm_model: String,
    pub anthropic_base_url: String,
    pub scheduler_enabled: bool,
    pub web_dist_dir: Option<std::path::PathBuf>,
    pub log_json: bool,
    pub import_max_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone, Debug)]
pub struct OAuthApp {
    pub client_id: String,
    pub client_secret: String,
}

impl std::fmt::Debug for Config {
    // Nunca imprimir secretos.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("env", &self.env)
            .field("bind_addr", &self.bind_addr)
            .field("public_base_url", &self.public_base_url.as_str())
            .field("telegram_bot", &self.telegram_bot_token.is_some())
            .field("allowed_telegram_ids", &self.allowed_telegram_ids.len())
            .field("oidc", &self.oidc.is_some())
            .field("google", &self.google.is_some())
            .field("llm", &self.anthropic_api_key.is_some())
            .field("dev_login_enabled", &self.dev_login_enabled)
            .finish_non_exhaustive()
    }
}

fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn flag(name: &str, default: bool) -> anyhow::Result<bool> {
    match var(name).as_deref() {
        None => Ok(default),
        Some("1" | "true" | "yes") => Ok(true),
        Some("0" | "false" | "no") => Ok(false),
        Some(v) => bail!("{name} debe ser true/false, no '{v}'"),
    }
}

fn num<T: std::str::FromStr>(name: &str, default: T) -> anyhow::Result<T> {
    match var(name) {
        None => Ok(default),
        Some(v) => v
            .parse()
            .map_err(|_| anyhow::anyhow!("{name} no es un número válido")),
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let env = match var("APP_ENV").as_deref().unwrap_or("development") {
            "development" => AppEnv::Development,
            "test" => AppEnv::Test,
            "production" => AppEnv::Production,
            other => bail!("APP_ENV inválido: {other} (development|test|production)"),
        };
        let bind_addr: SocketAddr = var("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:8080".into())
            .parse()
            .context("BIND_ADDR inválido")?;
        let database_url = var("DATABASE_URL").context("DATABASE_URL es obligatorio")?;
        let public_base_url: url::Url = var("PUBLIC_BASE_URL")
            .unwrap_or_else(|| "http://localhost:5180".into())
            .parse()
            .context("PUBLIC_BASE_URL inválido")?;
        let allowed_telegram_ids = var("ALLOWED_TELEGRAM_IDS")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                s.trim()
                    .parse::<i64>()
                    .context("ALLOWED_TELEGRAM_IDS debe ser una lista de IDs numéricos")
            })
            .collect::<anyhow::Result<HashSet<_>>>()?;
        let internal_api_token = var("INTERNAL_API_TOKEN").context("INTERNAL_API_TOKEN es obligatorio")?;
        if internal_api_token.len() < 32 {
            bail!("INTERNAL_API_TOKEN debe tener al menos 32 caracteres");
        }
        let key_b64 = var("ENCRYPTION_KEY").context("ENCRYPTION_KEY es obligatorio (32 bytes en base64)")?;
        let key = base64::engine::general_purpose::STANDARD
            .decode(key_b64)
            .context("ENCRYPTION_KEY no es base64")?;
        let encryption_key: [u8; 32] = key
            .try_into()
            .map_err(|_| anyhow::anyhow!("ENCRYPTION_KEY debe decodificar a 32 bytes"))?;
        let oidc = match (var("OIDC_CLIENT_ID"), var("OIDC_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret)) => Some(OidcConfig {
                issuer: var("OIDC_ISSUER").unwrap_or_else(|| "https://oauth.telegram.org".into()),
                client_id,
                client_secret,
            }),
            (None, None) => None,
            _ => bail!("OIDC_CLIENT_ID y OIDC_CLIENT_SECRET van juntos"),
        };
        let google = match (var("GOOGLE_CLIENT_ID"), var("GOOGLE_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret)) => Some(OAuthApp {
                client_id,
                client_secret,
            }),
            (None, None) => None,
            _ => bail!("GOOGLE_CLIENT_ID y GOOGLE_CLIENT_SECRET van juntos"),
        };
        let cfg = Config {
            env,
            bind_addr,
            database_url,
            database_max_connections: num("DATABASE_MAX_CONNECTIONS", 20)?,
            public_base_url,
            telegram_bot_token: var("TELEGRAM_BOT_TOKEN"),
            telegram_bot_username: var("TELEGRAM_BOT_USERNAME"),
            miniapp_max_age_secs: num("MINIAPP_MAX_AGE_SECS", 6 * 3600)?,
            allowed_telegram_ids,
            cookie_secure: flag("COOKIE_SECURE", true)?,
            cookie_session_ttl_hours: num("SESSION_TTL_HOURS", 24 * 30)?,
            bearer_session_ttl_hours: num("BEARER_SESSION_TTL_HOURS", 12)?,
            oidc,
            internal_api_token,
            encryption_key,
            dev_login_enabled: flag("DEV_LOGIN_ENABLED", false)?,
            demo_enabled: flag("DEMO_ENABLED", false)?,
            google,
            anthropic_api_key: var("ANTHROPIC_API_KEY"),
            llm_model: var("LLM_MODEL").unwrap_or_else(|| "claude-haiku-4-5-20251001".into()),
            anthropic_base_url: var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|| "https://api.anthropic.com".into()),
            scheduler_enabled: flag("SCHEDULER_ENABLED", true)?,
            web_dist_dir: var("WEB_DIST_DIR").map(Into::into),
            log_json: var("LOG_FORMAT").as_deref() == Some("json"),
            import_max_bytes: num("IMPORT_MAX_BYTES", 50 * 1024 * 1024)?,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Reglas de seguridad: el modo de pruebas/desarrollo no puede quedar activo en producción.
    pub fn validate(&self) -> anyhow::Result<()> {
        let host_is_local = matches!(
            self.public_base_url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]")
        );
        if self.dev_login_enabled {
            if self.env == AppEnv::Production {
                bail!("DEV_LOGIN_ENABLED está prohibido con APP_ENV=production");
            }
            if !host_is_local || !self.bind_addr.ip().is_loopback() {
                bail!("DEV_LOGIN_ENABLED solo se permite con PUBLIC_BASE_URL local y BIND_ADDR en loopback");
            }
        }
        if self.env == AppEnv::Production {
            if self.public_base_url.scheme() != "https" {
                bail!("En producción PUBLIC_BASE_URL debe ser https");
            }
            if !self.cookie_secure {
                bail!("En producción COOKIE_SECURE debe ser true");
            }
            if self.allowed_telegram_ids.is_empty() {
                bail!("En producción ALLOWED_TELEGRAM_IDS no puede estar vacío (evita registro abierto)");
            }
            if self.demo_enabled {
                bail!("DEMO_ENABLED está prohibido en producción");
            }
            if self.telegram_bot_token.is_none() {
                bail!("En producción TELEGRAM_BOT_TOKEN es obligatorio");
            }
        } else if !self.cookie_secure && !host_is_local {
            bail!("COOKIE_SECURE=false solo con PUBLIC_BASE_URL local");
        }
        Ok(())
    }

    pub fn is_allowed(&self, telegram_id: i64) -> bool {
        self.allowed_telegram_ids.contains(&telegram_id)
    }

    pub fn oidc_redirect_uri(&self) -> String {
        self.public_base_url
            .join("/api/v1/auth/oidc/callback")
            .expect("url")
            .to_string()
    }

    pub fn google_redirect_uri(&self) -> String {
        self.public_base_url
            .join("/api/v1/integrations/google/callback")
            .expect("url")
            .to_string()
    }

    /// Configuración mínima para pruebas de integración (no lee el entorno).
    pub fn for_tests(database_url: &str) -> Self {
        Config {
            env: AppEnv::Test,
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            database_url: database_url.into(),
            database_max_connections: 5,
            public_base_url: "http://localhost:5173".parse().unwrap(),
            telegram_bot_token: Some("123456:TEST-TOKEN".into()),
            telegram_bot_username: Some("dane_test_bot".into()),
            miniapp_max_age_secs: 3600,
            allowed_telegram_ids: [111, 222].into_iter().collect(),
            cookie_secure: false,
            cookie_session_ttl_hours: 24,
            bearer_session_ttl_hours: 12,
            oidc: None,
            internal_api_token: "internal-test-token-0123456789abcdef".into(),
            encryption_key: [7u8; 32],
            dev_login_enabled: true,
            demo_enabled: true,
            google: None,
            anthropic_api_key: None,
            llm_model: "claude-haiku-4-5-20251001".into(),
            anthropic_base_url: "https://api.anthropic.com".into(),
            scheduler_enabled: false,
            web_dist_dir: None,
            log_json: false,
            import_max_bytes: 10 * 1024 * 1024,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_login_is_impossible_in_production() {
        let mut c = Config::for_tests("postgres://x");
        c.env = AppEnv::Production;
        c.public_base_url = "https://dane.example.com".parse().unwrap();
        c.cookie_secure = true;
        c.demo_enabled = false;
        assert!(c.validate().is_err(), "dev login en producción debe fallar");
        c.dev_login_enabled = false;
        assert!(c.validate().is_ok());
        c.allowed_telegram_ids.clear();
        assert!(c.validate().is_err(), "allowlist vacía en producción debe fallar");
    }

    #[test]
    fn dev_login_requires_local_origin() {
        let mut c = Config::for_tests("postgres://x");
        c.public_base_url = "https://dane.example.com".parse().unwrap();
        assert!(c.validate().is_err());
    }
}
