//! Punta a punta: Telegram simulado → webhook del bot → server real (PostgreSQL) → procesador → outbox →
//! envío del bot → ack. Verifica que los contratos de `crates/contracts` coinciden en el cable.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{TimeZone, Utc};
use domain::time::FixedClock;
use serde_json::{Value, json};
use server::config::Config;
use server::state::AppState;
use sqlx::PgPool;
use telegram_bot::config::{BotConfig, Mode};
use telegram_bot::server_api::ServerApi;
use telegram_bot::{sender, webhook};
use tower::ServiceExt;
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn start_command_round_trip(pool: PgPool) {
    let cfg = Config::for_tests("postgres://unused");
    let internal = cfg.internal_api_token.clone();
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 11, 12, 18, 0, 0).unwrap());
    let state = AppState::new(pool.clone(), cfg, Arc::new(clock));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, server::app(state.clone())).into_future());

    let tg = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"(?i)/sendmessage$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true, "result": {
            "message_id": 77, "date": 1800000000, "chat": {"id": 111, "type": "private", "first_name": "N"}, "text": "x"}})))
        .mount(&tg)
        .await;
    let bot_cfg = BotConfig {
        token: "123:TEST".into(),
        server_url: format!("http://{addr}").parse().unwrap(),
        internal_token: internal.clone(),
        mode: Mode::Polling,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        telegram_api_url: tg.uri().parse().unwrap(),
        worker_id: "e2e".into(),
    };
    let api = ServerApi::new(bot_cfg.server_url.clone(), internal, "e2e".into());

    // Telegram → webhook del bot → server.
    let update = json!({"update_id": 5000, "message": {"message_id": 1, "date": 0, "chat": {"id": 111, "type": "private"},
        "from": {"id": 111, "is_bot": false, "first_name": "Neil"}, "text": "/start"}});
    let hook = webhook::router(webhook::WebhookState {
        api: api.clone(),
        secret: Some("abcdefghijklmnop_123".into()),
    });
    let resp = hook
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(webhook::WEBHOOK_PATH)
                .header("content-type", "application/json")
                .header(webhook::SECRET_HEADER, "abcdefghijklmnop_123")
                .body(Body::from(update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM telegram_updates WHERE status = 'pending'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pending, 1, "aceptado de forma durable antes del 200");

    server::telegram::processor::drain(&state).await.unwrap();
    assert_eq!(
        sender::send_batch(&telegram_bot::bot(&bot_cfg), &api)
            .await
            .unwrap(),
        1
    );

    let sent: Vec<Value> = tg
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_id"], 111);
    assert_eq!(
        sent[0]["reply_markup"]["inline_keyboard"][0][0]["url"],
        "http://localhost:5173/hoy?date=2026-11-12&guiado=manana"
    );
    let (status, msg_id): (String, Option<i64>) =
        sqlx::query_as("SELECT status, telegram_message_id FROM jobs WHERE kind = 'reply'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((status.as_str(), msg_id), ("sent", Some(77)));
    // Nada más pendiente.
    assert_eq!(
        sender::send_batch(&telegram_bot::bot(&bot_cfg), &api)
            .await
            .unwrap(),
        0
    );
}
