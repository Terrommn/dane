//! Long polling (desarrollo). El `offset` solo avanza después de que el server confirma cada update de forma durable;
//! si la entrega falla, el update se vuelve a pedir a Telegram en la siguiente vuelta (no se pierde).

use std::time::Duration;

use serde_json::Value;

use crate::server_api::ServerApi;

pub struct Poller {
    http: reqwest::Client,
    url: url::Url,
    pub offset: i64,
}

impl Poller {
    pub fn new(telegram_api_url: &url::Url, token: &str) -> anyhow::Result<Self> {
        // Concatenar (no `join`): `bot<id>:<secreto>` se interpretaría como esquema de URL.
        let url: url::Url = format!(
            "{}/bot{token}/getUpdates",
            telegram_api_url.as_str().trim_end_matches('/')
        )
        .parse()?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(40))
            .build()?;
        Ok(Self { http, url, offset: 0 })
    }

    /// Una llamada a `getUpdates` (long poll `timeout` s). Devuelve cuántos updates entregó al server.
    pub async fn poll_once(&mut self, api: &ServerApi, timeout_secs: u64) -> anyhow::Result<usize> {
        let resp = self
            .http
            .post(self.url.clone())
            .json(&serde_json::json!({ "offset": self.offset, "timeout": timeout_secs, "allowed_updates": ["message"] }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("getUpdates: {}", e.without_url()))?;
        let body: Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("getUpdates json: {}", e.without_url()))?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            let desc = body.get("description").and_then(Value::as_str).unwrap_or("error");
            anyhow::bail!("getUpdates rechazado: {desc}");
        }
        let mut delivered = 0;
        for u in body
            .get("result")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let Some(id) = u.get("update_id").and_then(Value::as_i64) else {
                continue;
            };
            if id < self.offset {
                continue;
            }
            match api.ingest(id, u).await {
                Ok(_) => {
                    self.offset = id + 1;
                    delivered += 1;
                }
                Err(e) => {
                    // Sin avanzar el offset: Telegram lo reenviará.
                    tracing::warn!(update_id = id, error = %e, "server no aceptó el update; se reintentará");
                    break;
                }
            }
        }
        Ok(delivered)
    }
}
