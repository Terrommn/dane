//! `vida-server [serve|migrate|openapi]`

use std::sync::Arc;

use anyhow::Context;
use domain::time::SystemClock;
use server::config::Config;
use server::state::AppState;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    if cmd == "openapi" {
        // No requiere configuración ni base de datos.
        println!("{}", server::api::openapi().to_pretty_json()?);
        return Ok(());
    }
    let config = Config::from_env().context("configuración inválida")?;
    init_tracing(config.log_json);
    tracing::info!(config = ?config, "iniciando vida-server");

    let pool = persistence::connect(&config.database_url, config.database_max_connections)
        .await
        .context("no se pudo conectar a PostgreSQL")?;
    persistence::migrate(&pool)
        .await
        .context("migraciones fallidas")?;
    if cmd == "migrate" {
        tracing::info!("migraciones aplicadas");
        return Ok(());
    }

    let state = AppState::new(pool, config.clone(), Arc::new(SystemClock));
    let shutdown = tokio_util_shutdown();
    let workers = server_workers(state.clone(), shutdown.clone());

    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .context("bind")?;
    tracing::info!(addr = %config.bind_addr, "escuchando");
    let sd = shutdown.clone();
    axum::serve(listener, server::app(state))
        .with_graceful_shutdown(async move {
            sd.cancelled().await;
        })
        .await?;
    for w in workers {
        let _ = w.await;
    }
    tracing::info!("apagado ordenado completo");
    Ok(())
}

fn init_tracing(json: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));
    if json {
        tracing_subscriber::fmt().with_env_filter(filter).json().init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}

/// Señal de apagado compartida (SIGINT/SIGTERM).
fn tokio_util_shutdown() -> server::workers::Shutdown {
    let s = server::workers::Shutdown::new();
    let s2 = s.clone();
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM");
        #[cfg(unix)]
        tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
        #[cfg(not(unix))]
        let _ = ctrl_c.await;
        tracing::info!("señal de apagado recibida");
        s2.cancel();
    });
    s
}

fn server_workers(state: AppState, shutdown: server::workers::Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    server::workers::spawn_all(state, shutdown)
}
