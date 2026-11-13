//! Configuración del bot desde el entorno, validada al arrancar. El bot **no** recibe `DATABASE_URL`.
//!
//! | Variable | Obligatoria | Descripción |
//! |---|---|---|
//! | `TELEGRAM_BOT_TOKEN` | sí | Token de @BotFather |
//! | `SERVER_INTERNAL_URL` | sí | URL interna del server (ej. `http://server:8080`), nunca pública |
//! | `INTERNAL_API_TOKEN` | sí | ≥ 32 caracteres; el mismo que el server |
//! | `TELEGRAM_MODE` | no | `polling` (por defecto, desarrollo) o `webhook` (producción). Exclusivos |
//! | `WEBHOOK_PUBLIC_URL` | webhook | URL HTTPS pública que Telegram llamará, ej. `https://vida.ejemplo.com/telegram/webhook` |
//! | `WEBHOOK_SECRET` | webhook | 16–256 caracteres `[A-Za-z0-9_-]`; Telegram lo manda en `X-Telegram-Bot-Api-Secret-Token` |
//! | `BOT_BIND_ADDR` | no | Escucha HTTP del bot (webhook + `/healthz`), por defecto `0.0.0.0:8081` |
//! | `TELEGRAM_API_URL` | no | Base de la Bot API (por defecto `https://api.telegram.org`; útil para pruebas) |
//! | `BOT_WORKER_ID` | no | Identificador del proceso para los leases |

use std::net::SocketAddr;

use anyhow::{Context, bail};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Polling,
    Webhook { public_url: url::Url, secret: String },
}

#[derive(Clone)]
pub struct BotConfig {
    pub token: String,
    pub server_url: url::Url,
    pub internal_token: String,
    pub mode: Mode,
    pub bind_addr: SocketAddr,
    pub telegram_api_url: url::Url,
    pub worker_id: String,
}

impl std::fmt::Debug for BotConfig {
    // Sin secretos en logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BotConfig")
            .field("server_url", &self.server_url.as_str())
            .field(
                "mode",
                &match &self.mode {
                    Mode::Polling => "polling".to_string(),
                    Mode::Webhook { public_url, .. } => format!("webhook {public_url}"),
                },
            )
            .field("bind_addr", &self.bind_addr)
            .field("worker_id", &self.worker_id)
            .finish_non_exhaustive()
    }
}

impl BotConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let var = |k: &str| get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        if var("DATABASE_URL").is_some() {
            bail!("El bot no debe recibir DATABASE_URL: solo habla con el server por la API interna");
        }
        let token = var("TELEGRAM_BOT_TOKEN").context("TELEGRAM_BOT_TOKEN es obligatorio")?;
        if !token.contains(':') {
            bail!("TELEGRAM_BOT_TOKEN no tiene el formato <id>:<secreto>");
        }
        let server_url: url::Url = var("SERVER_INTERNAL_URL")
            .context("SERVER_INTERNAL_URL es obligatorio")?
            .parse()
            .context("SERVER_INTERNAL_URL inválido")?;
        let internal_token = var("INTERNAL_API_TOKEN").context("INTERNAL_API_TOKEN es obligatorio")?;
        if internal_token.len() < 32 {
            bail!("INTERNAL_API_TOKEN debe tener al menos 32 caracteres");
        }
        let mode = match var("TELEGRAM_MODE").as_deref().unwrap_or("polling") {
            "polling" => {
                if var("WEBHOOK_PUBLIC_URL").is_some() {
                    bail!("TELEGRAM_MODE=polling con WEBHOOK_PUBLIC_URL definido: elige un solo modo");
                }
                Mode::Polling
            }
            "webhook" => {
                let public_url: url::Url = var("WEBHOOK_PUBLIC_URL")
                    .context("WEBHOOK_PUBLIC_URL es obligatorio en modo webhook")?
                    .parse()
                    .context("WEBHOOK_PUBLIC_URL inválido")?;
                if public_url.scheme() != "https" {
                    bail!("WEBHOOK_PUBLIC_URL debe ser https");
                }
                let secret =
                    var("WEBHOOK_SECRET").context("WEBHOOK_SECRET es obligatorio en modo webhook")?;
                if !(16..=256).contains(&secret.len())
                    || !secret
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    bail!("WEBHOOK_SECRET debe tener 16–256 caracteres [A-Za-z0-9_-]");
                }
                Mode::Webhook { public_url, secret }
            }
            other => bail!("TELEGRAM_MODE inválido: {other} (polling|webhook)"),
        };
        Ok(BotConfig {
            token,
            server_url,
            internal_token,
            mode,
            bind_addr: var("BOT_BIND_ADDR")
                .unwrap_or_else(|| "0.0.0.0:8081".into())
                .parse()
                .context("BOT_BIND_ADDR inválido")?,
            telegram_api_url: var("TELEGRAM_API_URL")
                .unwrap_or_else(|| "https://api.telegram.org".into())
                .parse()
                .context("TELEGRAM_API_URL inválido")?,
            worker_id: var("BOT_WORKER_ID")
                .unwrap_or_else(|| format!("bot-{}", uuid::Uuid::new_v4().simple())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(extra: &[(&str, &str)]) -> anyhow::Result<BotConfig> {
        let mut m: HashMap<String, String> = [
            ("TELEGRAM_BOT_TOKEN", "123:ABC"),
            ("SERVER_INTERNAL_URL", "http://server:8080"),
            ("INTERNAL_API_TOKEN", "x".repeat(40).as_str()),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        for (k, v) in extra {
            m.insert(k.to_string(), v.to_string());
        }
        BotConfig::from_lookup(|k| m.get(k).cloned())
    }

    #[test]
    fn defaults_to_polling() {
        assert_eq!(cfg(&[]).unwrap().mode, Mode::Polling);
    }

    #[test]
    fn refuses_database_url_and_weak_tokens() {
        assert!(cfg(&[("DATABASE_URL", "postgres://x")]).is_err());
        assert!(cfg(&[("INTERNAL_API_TOKEN", "short")]).is_err());
        assert!(cfg(&[("TELEGRAM_BOT_TOKEN", "nocolon")]).is_err());
    }

    #[test]
    fn webhook_requires_https_and_valid_secret_and_is_exclusive() {
        let ok = cfg(&[
            ("TELEGRAM_MODE", "webhook"),
            ("WEBHOOK_PUBLIC_URL", "https://v.example.com/telegram/webhook"),
            ("WEBHOOK_SECRET", "abcdefghijklmnop_123"),
        ]);
        assert!(matches!(ok.unwrap().mode, Mode::Webhook { .. }));
        assert!(
            cfg(&[
                ("TELEGRAM_MODE", "webhook"),
                ("WEBHOOK_PUBLIC_URL", "http://v.example.com/x"),
                ("WEBHOOK_SECRET", "abcdefghijklmnop")
            ])
            .is_err()
        );
        assert!(
            cfg(&[
                ("TELEGRAM_MODE", "webhook"),
                ("WEBHOOK_PUBLIC_URL", "https://v.example.com/x"),
                ("WEBHOOK_SECRET", "corto")
            ])
            .is_err()
        );
        assert!(
            cfg(&[
                ("TELEGRAM_MODE", "webhook"),
                ("WEBHOOK_PUBLIC_URL", "https://v.example.com/x"),
                ("WEBHOOK_SECRET", "con espacios no válido!!")
            ])
            .is_err()
        );
        assert!(
            cfg(&[("WEBHOOK_PUBLIC_URL", "https://v.example.com/x")]).is_err(),
            "polling + URL de webhook es ambiguo"
        );
        assert!(cfg(&[("TELEGRAM_MODE", "ambos")]).is_err());
    }

    #[test]
    fn debug_hides_secrets() {
        let c = cfg(&[]).unwrap();
        let s = format!("{c:?}");
        assert!(!s.contains("123:ABC") && !s.contains(&"x".repeat(40)));
    }
}
