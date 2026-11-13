//! `vida-bot`: recibe updates (polling o webhook) y entrega mensajes del outbox del server.

use std::time::Duration;

use telegram_bot::config::{BotConfig, Mode};
use telegram_bot::server_api::ServerApi;
use telegram_bot::{polling, sender, webhook};
use teloxide::prelude::*;
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = BotConfig::from_env()?;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if std::env::var("LOG_FORMAT").as_deref() == Ok("json") {
        tracing_subscriber::fmt().with_env_filter(filter).json().init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
    tracing::info!(config = ?cfg, "iniciando vida-bot");

    let bot = telegram_bot::bot(&cfg);
    let api = ServerApi::new(
        cfg.server_url.clone(),
        cfg.internal_token.clone(),
        cfg.worker_id.clone(),
    );

    // Comandos y modo exclusivo (reintenta si Telegram no responde al arrancar).
    for attempt in 1..=5 {
        let r = async {
            bot.set_my_commands(telegram_bot::commands()).await?;
            telegram_bot::configure_mode(&bot, &cfg.mode).await
        }
        .await;
        match r {
            Ok(()) => break,
            Err(e) if attempt < 5 => {
                tracing::warn!(error = %e, attempt, "configuración inicial de Telegram falló; reintentando");
                tokio::time::sleep(Duration::from_secs(2 * attempt)).await;
            }
            Err(e) => return Err(e),
        }
    }

    let (stop_tx, stop_rx) = watch::channel(false);
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
            tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
        }
        #[cfg(not(unix))]
        let _ = ctrl_c.await;
        tracing::info!("señal de apagado recibida");
        let _ = stop_tx.send(true);
    });

    // HTTP: webhook (si aplica) + /healthz.
    let secret = match &cfg.mode {
        Mode::Webhook { secret, .. } => Some(secret.clone()),
        Mode::Polling => None,
    };
    let app = webhook::router(webhook::WebhookState {
        api: api.clone(),
        secret,
    });
    let listener = tokio::net::TcpListener::bind(cfg.bind_addr).await?;
    let mut http_stop = stop_rx.clone();
    let http = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = http_stop.wait_for(|s| *s).await;
            })
            .await;
    });

    // Envío del outbox.
    let mut send_stop = stop_rx.clone();
    let (sbot, sapi) = (bot.clone(), api.clone());
    let sender_task = tokio::spawn(async move {
        loop {
            let idle = match sender::send_batch(&sbot, &sapi).await {
                Ok(n) => n == 0,
                Err(e) => {
                    tracing::warn!(error = %e, "claim falló; reintentando");
                    true
                }
            };
            if idle {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                    _ = async { let _ = send_stop.wait_for(|s| *s).await; } => break,
                }
            } else if *send_stop.borrow() {
                break;
            }
        }
    });

    // Recepción por polling (solo en ese modo).
    let poll_task = if cfg.mode == Mode::Polling {
        let mut poll_stop = stop_rx.clone();
        let mut poller = polling::Poller::new(&cfg.telegram_api_url, &cfg.token)?;
        let papi = api.clone();
        Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    r = poller.poll_once(&papi, 25) => {
                        if let Err(e) = r {
                            tracing::warn!(error = %e, "polling falló; reintentando");
                            tokio::time::sleep(Duration::from_secs(3)).await;
                        }
                    }
                    _ = async { let _ = poll_stop.wait_for(|s| *s).await; } => break,
                }
            }
        }))
    } else {
        None
    };

    let mut stop = stop_rx.clone();
    let _ = stop.wait_for(|s| *s).await;
    let _ = sender_task.await;
    if let Some(p) = poll_task {
        let _ = p.await;
    }
    let _ = http.await;
    tracing::info!("vida-bot detenido");
    Ok(())
}
