//! Bot contra Telegram y server simulados (wiremock): envío/ack, 429, rechazos permanentes, polling y webhook.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use telegram_bot::config::{BotConfig, Mode};
use telegram_bot::server_api::ServerApi;
use telegram_bot::{polling, sender, webhook};
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "123:TEST";
const INTERNAL: &str = "internal-test-token-0123456789abcdef";

fn cfg(telegram: &MockServer, server: &MockServer) -> BotConfig {
    BotConfig {
        token: TOKEN.into(),
        server_url: server.uri().parse().unwrap(),
        internal_token: INTERNAL.into(),
        mode: Mode::Polling,
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        telegram_api_url: telegram.uri().parse().unwrap(),
        worker_id: "test-worker".into(),
    }
}

fn job(chat_id: i64, buttons: Value) -> Value {
    json!({"id": Uuid::new_v4(), "lease_token": Uuid::new_v4(), "kind": "reply", "attempt": 1,
           "message": {"chat_id": chat_id, "text": format!("hola {chat_id}"), "buttons": buttons}})
}

fn tg_error(code: u16, desc: &str, retry_after: Option<u64>) -> ResponseTemplate {
    let mut body = json!({"ok": false, "error_code": code, "description": desc});
    if let Some(r) = retry_after {
        body["parameters"] = json!({"retry_after": r});
    }
    ResponseTemplate::new(code).set_body_json(body)
}

#[tokio::test]
async fn send_batch_acks_each_outcome() {
    let tg = MockServer::start().await;
    let srv = MockServer::start().await;
    let jobs = vec![
        job(
            1,
            json!([[{"type": "web_app", "text": "Abrir", "url": "https://vida.example.com/hoy?date=2026-11-12"}]]),
        ),
        job(2, json!([])),
        job(3, json!([])),
        job(4, json!([])),
    ];
    Mock::given(method("POST"))
        .and(path("/internal/v1/jobs/claim"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jobs": jobs})))
        .mount(&srv)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/internal/v1/jobs/.+/ack$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"status": "sent", "next_attempt_at": null, "replayed": false})),
        )
        .mount(&srv)
        .await;
    let send = || Mock::given(method("POST")).and(path_regex(r"(?i)^/bot123:TEST/sendmessage$"));
    send()
        .and(body_partial_json(json!({"chat_id": 1})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true, "result": {
            "message_id": 42, "date": 1800000000, "chat": {"id": 1, "type": "private", "first_name": "N"}, "text": "hola 1"}})))
        .mount(&tg)
        .await;
    send()
        .and(body_partial_json(json!({"chat_id": 2})))
        .respond_with(tg_error(429, "Too Many Requests: retry after 7", Some(7)))
        .mount(&tg)
        .await;
    send()
        .and(body_partial_json(json!({"chat_id": 3})))
        .respond_with(tg_error(403, "Forbidden: bot was blocked by the user", None))
        .mount(&tg)
        .await;
    send()
        .and(body_partial_json(json!({"chat_id": 4})))
        .respond_with(tg_error(400, "Bad Request: message text is empty", None))
        .mount(&tg)
        .await;

    let c = cfg(&tg, &srv);
    let bot = telegram_bot::bot(&c);
    let api = ServerApi::new(
        c.server_url.clone(),
        c.internal_token.clone(),
        c.worker_id.clone(),
    );
    assert_eq!(sender::send_batch(&bot, &api).await.unwrap(), 4);

    // Telegram recibió el teclado web_app.
    let sent: Vec<Value> = tg
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect();
    let first = sent.iter().find(|b| b["chat_id"] == 1).unwrap();
    assert_eq!(
        first["reply_markup"]["inline_keyboard"][0][0]["web_app"]["url"],
        "https://vida.example.com/hoy?date=2026-11-12"
    );

    // Un ack por job, con el lease_token correcto y el outcome clasificado.
    let reqs = srv.received_requests().await.unwrap();
    assert!(
        reqs.iter()
            .all(|r| r.headers.get("authorization").unwrap() == &format!("Bearer {INTERNAL}"))
    );
    let acks: Vec<(String, Value)> = reqs
        .iter()
        .filter(|r| r.url.path().ends_with("/ack"))
        .map(|r| (r.url.path().to_string(), serde_json::from_slice(&r.body).unwrap()))
        .collect();
    assert_eq!(acks.len(), 4);
    let outcome_for = |i: usize| {
        let id = jobs[i]["id"].as_str().unwrap();
        let (_, body) = acks.iter().find(|(p, _)| p.contains(id)).unwrap();
        assert_eq!(body["lease_token"], jobs[i]["lease_token"]);
        body["outcome"].clone()
    };
    assert_eq!(
        outcome_for(0),
        json!({"result": "sent", "telegram_message_id": 42})
    );
    assert_eq!(outcome_for(1)["result"], "retry");
    assert_eq!(outcome_for(1)["retry_after_secs"], 7);
    assert_eq!(outcome_for(2)["result"], "failed");
    assert_eq!(outcome_for(2)["chat_blocked"], true);
    assert_eq!(outcome_for(3)["result"], "failed");
    assert_eq!(outcome_for(3)["chat_blocked"], false);
}

#[tokio::test]
async fn stale_lease_on_ack_does_not_crash_the_loop() {
    let tg = MockServer::start().await;
    let srv = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/v1/jobs/claim"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"jobs": [job(1, json!([]))]})))
        .mount(&srv)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/ack$"))
        .respond_with(ResponseTemplate::new(409))
        .mount(&srv)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"(?i)/sendmessage$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true, "result": {
            "message_id": 1, "date": 1800000000, "chat": {"id": 1, "type": "private", "first_name": "N"}, "text": "x"}})))
        .mount(&tg)
        .await;
    let c = cfg(&tg, &srv);
    let api = ServerApi::new(
        c.server_url.clone(),
        c.internal_token.clone(),
        c.worker_id.clone(),
    );
    assert_eq!(sender::send_batch(&telegram_bot::bot(&c), &api).await.unwrap(), 1);
}

#[tokio::test]
async fn polling_advances_offset_only_after_durable_acceptance() {
    let tg = MockServer::start().await;
    let srv = MockServer::start().await;
    let upd = |id: i64| json!({"update_id": id, "message": {"message_id": id, "date": 0, "chat": {"id": 1, "type": "private"}, "from": {"id": 1, "is_bot": false, "first_name": "N"}, "text": "hola"}});
    Mock::given(method("POST"))
        .and(path_regex(r"(?i)/bot123:TEST/getupdates$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"ok": true, "result": [upd(10), upd(11), upd(12)]})),
        )
        .mount(&tg)
        .await;
    let ok = |id: i64| ResponseTemplate::new(200).set_body_json(json!({"update_id": id, "duplicate": false}));
    Mock::given(method("POST"))
        .and(path("/internal/v1/telegram/updates"))
        .and(body_partial_json(json!({"update_id": 10})))
        .respond_with(ok(10))
        .mount(&srv)
        .await;
    Mock::given(method("POST"))
        .and(path("/internal/v1/telegram/updates"))
        .and(body_partial_json(json!({"update_id": 11})))
        .respond_with(ResponseTemplate::new(500))
        .mount(&srv)
        .await;
    Mock::given(method("POST"))
        .and(path("/internal/v1/telegram/updates"))
        .and(body_partial_json(json!({"update_id": 12})))
        .respond_with(ok(12))
        .mount(&srv)
        .await;

    let c = cfg(&tg, &srv);
    let api = ServerApi::new(
        c.server_url.clone(),
        c.internal_token.clone(),
        c.worker_id.clone(),
    );
    let mut p = polling::Poller::new(&c.telegram_api_url, &c.token).unwrap();
    assert_eq!(p.poll_once(&api, 0).await.unwrap(), 1);
    assert_eq!(
        p.offset, 11,
        "el 11 no se confirmó: se vuelve a pedir; el 12 no se salta"
    );
    let forwarded: Vec<i64> = srv
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| {
            serde_json::from_slice::<Value>(&r.body).unwrap()["update_id"]
                .as_i64()
                .unwrap()
        })
        .collect();
    assert_eq!(forwarded, vec![10, 11]);
    let get: Value = serde_json::from_slice(&tg.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(get["offset"], 0);
    assert_eq!(get["allowed_updates"], json!(["message"]));
}

async fn webhook_call(state: webhook::WebhookState, secret: Option<&str>, body: Value) -> StatusCode {
    let mut req = Request::builder()
        .method("POST")
        .uri(webhook::WEBHOOK_PATH)
        .header("content-type", "application/json");
    if let Some(s) = secret {
        req = req.header(webhook::SECRET_HEADER, s);
    }
    webhook::router(state)
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn webhook_verifies_secret_and_answers_after_server_acceptance() {
    let srv = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/v1/telegram/updates"))
        .and(body_partial_json(json!({"update_id": 1})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"update_id": 1, "duplicate": false})))
        .mount(&srv)
        .await;
    Mock::given(method("POST"))
        .and(path("/internal/v1/telegram/updates"))
        .and(body_partial_json(json!({"update_id": 2})))
        .respond_with(ResponseTemplate::new(503))
        .mount(&srv)
        .await;
    let api = ServerApi::new(srv.uri().parse().unwrap(), INTERNAL.into(), "w".into());
    let st = webhook::WebhookState {
        api: api.clone(),
        secret: Some("abcdefghijklmnop_123".into()),
    };
    assert_eq!(
        webhook_call(st.clone(), None, json!({"update_id": 1})).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        webhook_call(st.clone(), Some("incorrecto-incorrecto"), json!({"update_id": 1})).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        webhook_call(st.clone(), Some("abcdefghijklmnop_123"), json!({"update_id": 1})).await,
        StatusCode::OK
    );
    assert_eq!(
        webhook_call(st.clone(), Some("abcdefghijklmnop_123"), json!({"update_id": 2})).await,
        StatusCode::SERVICE_UNAVAILABLE,
        "si el server no confirma, Telegram debe reintentar"
    );
    assert_eq!(
        webhook_call(st, Some("abcdefghijklmnop_123"), json!({"x": 1})).await,
        StatusCode::BAD_REQUEST
    );
    // En modo polling el endpoint no existe.
    let polling_state = webhook::WebhookState { api, secret: None };
    assert_eq!(
        webhook_call(polling_state, Some("x"), json!({"update_id": 1})).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn commands_cover_all_fourteen_with_valid_names() {
    let cmds = telegram_bot::commands();
    assert_eq!(cmds.len(), 14);
    assert!(cmds.iter().all(|c| {
        c.command
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    }));
    for name in [
        "start",
        "daily",
        "checkin",
        "objetivos",
        "dump",
        "gasto",
        "ingreso",
        "weekly_review",
        "log_session",
        "dashboard",
        "settings",
        "help",
        "pause",
        "resume",
    ] {
        assert!(cmds.iter().any(|c| c.command == name), "falta /{name}");
    }
}
