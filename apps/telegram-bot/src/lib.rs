//! Bot de Telegram: transporte entre Telegram y el server. No accede a la base de datos ni contiene reglas de negocio.

pub mod config;
pub mod polling;
pub mod sender;
pub mod server_api;
pub mod webhook;

use teloxide::prelude::*;
use teloxide::types::{AllowedUpdate, BotCommand};

/// Construye el cliente de la Bot API (URL configurable para pruebas o un Bot API server local).
pub fn bot(cfg: &config::BotConfig) -> Bot {
    Bot::new(cfg.token.clone()).set_api_url(cfg.telegram_api_url.clone())
}

pub fn commands() -> Vec<BotCommand> {
    contracts::COMMANDS
        .iter()
        .map(|(c, d, _)| BotCommand::new(*c, *d))
        .collect()
}

/// Configura el modo exclusivo: webhook registrado **o** sin webhook (polling). Nunca ambos.
pub async fn configure_mode(bot: &Bot, mode: &config::Mode) -> anyhow::Result<()> {
    match mode {
        config::Mode::Polling => {
            bot.delete_webhook().await?;
            tracing::info!("modo polling: webhook eliminado");
        }
        config::Mode::Webhook { public_url, secret } => {
            bot.set_webhook(public_url.clone())
                .secret_token(secret.clone())
                .allowed_updates(vec![AllowedUpdate::Message])
                .await?;
            tracing::info!(url = %public_url, "modo webhook registrado");
        }
    }
    Ok(())
}
