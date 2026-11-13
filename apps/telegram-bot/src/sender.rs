//! Envío de mensajes: reclamar → enviar a Telegram (sin locks abiertos en el server) → confirmar.
//!
//! Garantía: *al menos una vez*. Si Telegram aceptó el mensaje pero la respuesta o el ack se pierden (timeout), el lease
//! vence y el job puede reenviarse: un duplicado excepcional es posible y aceptado; nunca se promete exactamente una vez.

use contracts::{AckOutcome, AckRequest, Button, ClaimedJob, OutboundMessage};
use teloxide::prelude::*;
use teloxide::types::{ChatId, InlineKeyboardButton, InlineKeyboardMarkup, WebAppInfo};
use teloxide::{ApiError, RequestError};

use crate::server_api::{self, ServerApi};

pub const LEASE_SECS: u32 = 60;
pub const BATCH: u32 = 10;

fn keyboard(rows: &[Vec<Button>]) -> Option<InlineKeyboardMarkup> {
    let rows: Vec<Vec<InlineKeyboardButton>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .filter_map(|b| match b {
                    Button::WebApp { text, url } => url
                        .parse()
                        .ok()
                        .map(|u| InlineKeyboardButton::web_app(text.clone(), WebAppInfo { url: u })),
                    Button::Url { text, url } => url
                        .parse()
                        .ok()
                        .map(|u| InlineKeyboardButton::url(text.clone(), u)),
                })
                .collect::<Vec<_>>()
        })
        .filter(|r: &Vec<InlineKeyboardButton>| !r.is_empty())
        .collect();
    (!rows.is_empty()).then(|| InlineKeyboardMarkup::new(rows))
}

/// Clasifica el resultado de Telegram en el outcome del ack.
pub fn classify(result: Result<i64, RequestError>) -> AckOutcome {
    match result {
        Ok(id) => AckOutcome::Sent {
            telegram_message_id: Some(id),
        },
        Err(RequestError::RetryAfter(s)) => AckOutcome::Retry {
            retry_after_secs: Some(s.chrono_duration().num_seconds().max(1) as u64),
            error: "429 Too Many Requests".into(),
        },
        Err(RequestError::Api(e)) => match e {
            ApiError::BotBlocked
            | ApiError::UserDeactivated
            | ApiError::ChatNotFound
            | ApiError::BotKicked
            | ApiError::BotKickedFromSupergroup
            | ApiError::CantInitiateConversation
            | ApiError::CantTalkWithBots => AckOutcome::Failed {
                error: e.to_string(),
                chat_blocked: true,
            },
            ApiError::Unknown(ref d) if !(d.starts_with("Bad Request") || d.starts_with("Forbidden")) => {
                // 5xx u otros errores no clasificados de Telegram: transitorios.
                AckOutcome::Retry {
                    retry_after_secs: None,
                    error: d.chars().take(300).collect(),
                }
            }
            other => AckOutcome::Failed {
                error: other.to_string(),
                chat_blocked: false,
            },
        },
        Err(RequestError::Network(e)) => {
            // La URL de la Bot API contiene el token: nunca se registra el error completo.
            let kind = if e.is_timeout() {
                "timeout (el mensaje pudo haberse entregado)"
            } else if e.is_connect() {
                "conexión"
            } else {
                "transporte"
            };
            AckOutcome::Retry {
                retry_after_secs: None,
                error: format!("red: {kind}"),
            }
        }
        Err(other) => AckOutcome::Retry {
            retry_after_secs: None,
            error: other.to_string().chars().take(300).collect(),
        },
    }
}

pub async fn deliver(bot: &Bot, msg: &OutboundMessage) -> AckOutcome {
    let mut req = bot.send_message(ChatId(msg.chat_id), &msg.text);
    if let Some(kb) = keyboard(&msg.buttons) {
        req = req.reply_markup(kb);
    }
    classify(req.await.map(|m| m.id.0 as i64))
}

/// Un ciclo: reclama un lote, envía y confirma. Devuelve cuántos jobs procesó.
pub async fn send_batch(bot: &Bot, api: &ServerApi) -> anyhow::Result<usize> {
    let jobs: Vec<ClaimedJob> = api.claim(BATCH, LEASE_SECS).await?;
    let n = jobs.len();
    for job in jobs {
        let outcome = deliver(bot, &job.message).await;
        match &outcome {
            AckOutcome::Sent { .. } => tracing::info!(job_id = %job.id, kind = %job.kind, "mensaje enviado"),
            AckOutcome::Retry {
                retry_after_secs,
                error,
            } => {
                tracing::warn!(job_id = %job.id, ?retry_after_secs, error = %error, "envío reintentable")
            }
            AckOutcome::Failed { error, chat_blocked } => {
                tracing::warn!(job_id = %job.id, chat_blocked, error = %error, "envío rechazado permanentemente")
            }
        }
        match api
            .ack(
                job.id,
                &AckRequest {
                    lease_token: job.lease_token,
                    outcome,
                },
            )
            .await
        {
            Ok(_) => {}
            Err(server_api::ApiError::StaleLease) => {
                tracing::warn!(job_id = %job.id, "ack con lease obsoleto: otro worker lo tomó (posible duplicado)")
            }
            Err(e) => {
                tracing::error!(job_id = %job.id, error = %e, "no se pudo confirmar; el lease vencerá y se reintentará")
            }
        }
    }
    Ok(n)
}
