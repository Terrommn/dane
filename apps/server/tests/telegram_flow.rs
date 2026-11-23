//! T45–T48: ingestión durable de updates, procesamiento transaccional, comandos, outbox con leases y recordatorios.

mod common;

use axum::http::{Method, StatusCode};
use chrono::{Duration, TimeZone, Utc};
use common::{TestApp, send};
use domain::time::Clock;
use serde_json::{Value, json};
use server::telegram::{processor, scheduler};
use sqlx::PgPool;
use uuid::Uuid;

const INTERNAL: &str = "Bearer internal-test-token-0123456789abcdef";

fn upd(id: i64, from: i64, chat_type: &str, text: &str) -> Value {
    json!({"update_id": id, "message": {"message_id": id, "date": 0,
        "chat": {"id": from, "type": chat_type}, "from": {"id": from, "is_bot": false, "first_name": "Usuario"}, "text": text}})
}

async fn internal(t: &TestApp, path: &str, body: Value) -> (StatusCode, Value) {
    send(
        &t.app,
        Method::POST,
        &format!("/internal/v1{path}"),
        Some(body),
        &[("authorization", INTERNAL)],
    )
    .await
}

async fn ingest(t: &TestApp, u: Value) -> (StatusCode, Value) {
    internal(
        t,
        "/telegram/updates",
        json!({"update_id": u["update_id"], "payload": u}),
    )
    .await
}

async fn claim(t: &TestApp, limit: u32) -> Vec<Value> {
    let (st, body) = internal(
        t,
        "/jobs/claim",
        json!({"worker_id": "w1", "limit": limit, "lease_secs": 60}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    body["jobs"].as_array().unwrap().clone()
}

async fn ack(t: &TestApp, job: &Value, outcome: Value) -> (StatusCode, Value) {
    internal(
        t,
        &format!("/jobs/{}/ack", job["id"].as_str().unwrap()),
        json!({"lease_token": job["lease_token"], "outcome": outcome}),
    )
    .await
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_string()))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn internal_api_requires_token(pool: PgPool) {
    let t = TestApp::new(pool);
    let (st, _) = send(
        &t.app,
        Method::POST,
        "/internal/v1/jobs/claim",
        Some(json!({"worker_id": "x", "limit": 1, "lease_secs": 30})),
        &[],
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _) = send(
        &t.app,
        Method::POST,
        "/internal/v1/jobs/claim",
        Some(json!({"worker_id": "x", "limit": 1, "lease_secs": 30})),
        &[("authorization", "Bearer wrong")],
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn duplicate_updates_are_accepted_once_and_never_lost(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let u = upd(1000, 111, "private", "/start");
    let (st, b) = ingest(&t, u.clone()).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(b["duplicate"], false);
    // Repetición antes de procesar: sigue pendiente (no se pierde por dedupe prematuro).
    let (_, b) = ingest(&t, u.clone()).await;
    assert_eq!(b["duplicate"], true);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM telegram_updates WHERE status = 'pending'"
        )
        .await,
        1
    );
    assert_eq!(processor::drain(&t.state).await.unwrap(), 1);
    // Repetición después de procesar: no genera otra respuesta.
    let (_, b) = ingest(&t, u).await;
    assert_eq!(b["duplicate"], true);
    assert_eq!(processor::drain(&t.state).await.unwrap(), 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM jobs WHERE kind = 'reply'").await,
        1
    );
    // update_id inconsistente con el payload se rechaza.
    let (st, _) = internal(
        &t,
        "/telegram/updates",
        json!({"update_id": 5, "payload": upd(6, 111, "private", "x")}),
    )
    .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn start_links_private_chat_and_replies_with_checkin_button(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    ingest(&t, upd(1, 111, "private", "/start")).await;
    processor::drain(&t.state).await.unwrap();
    let chat: (i64, bool) = sqlx::query_as("SELECT c.chat_id, c.can_message FROM telegram_chats c JOIN identities i ON i.user_id = c.user_id WHERE i.telegram_user_id = 111")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(chat, (111, true));
    let jobs = claim(&t, 10).await;
    assert_eq!(jobs.len(), 1);
    let msg = &jobs[0]["message"];
    assert_eq!(msg["chat_id"], 111);
    // Origen local (http) → botón de enlace normal; con HTTPS sería web_app.
    assert_eq!(msg["buttons"][0][0]["type"], "url");
    assert_eq!(
        msg["buttons"][0][0]["url"],
        "http://localhost:5173/hoy?date=2026-11-12&guiado=manana"
    );
    // Mismo usuario que la Mini App: dev-login con el mismo ID ve el chat conectado.
    let a = t.login(111).await;
    let (_, me) = a.get("/me").await;
    assert_eq!(me["telegram"]["chat_known"], true);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn web_app_buttons_on_https_origin(pool: PgPool) {
    let t = TestApp::with_config(pool, |c| {
        c.public_base_url = "https://dane.example.com".parse().unwrap()
    });
    ingest(&t, upd(1, 111, "private", "/checkin ayer")).await;
    processor::drain(&t.state).await.unwrap();
    let jobs = claim(&t, 10).await;
    let b = &jobs[0]["message"]["buttons"][0][0];
    assert_eq!(b["type"], "web_app");
    assert_eq!(
        b["url"], "https://dane.example.com/hoy?date=2026-11-11",
        "conserva la fecha objetivo"
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn non_allowlisted_and_group_messages_do_not_create_users(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    ingest(&t, upd(1, 999, "private", "/start")).await;
    ingest(&t, upd(2, 999, "private", "/gasto 100 tacos")).await;
    ingest(&t, upd(3, 111, "group", "/start")).await;
    assert_eq!(processor::drain(&t.state).await.unwrap(), 3);
    assert_eq!(count(&pool, "SELECT count(*) FROM users").await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM capture_proposals").await, 0);
    let jobs = claim(&t, 10).await;
    assert_eq!(
        jobs.len(),
        1,
        "solo /start de un desconocido recibe un aviso genérico"
    );
    assert_eq!(jobs[0]["message"]["text"], "Este bot es privado.");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn processing_failure_rolls_back_effects_and_retries(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    sqlx::query("CREATE FUNCTION boom() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'falla simulada'; END $$")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER boom BEFORE INSERT ON capture_proposals FOR EACH ROW EXECUTE FUNCTION boom()",
    )
    .execute(&pool)
    .await
    .unwrap();
    ingest(&t, upd(7, 111, "private", "/gasto 250 gasolina con débito")).await;
    assert!(processor::process_next(&t.state).await.unwrap());
    let (status, attempts): (String, i32) =
        sqlx::query_as("SELECT status, attempts FROM telegram_updates WHERE update_id = 7")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((status.as_str(), attempts), ("pending", 1));
    // Nada de sus efectos quedó: ni usuario, ni chat, ni respuesta.
    assert_eq!(count(&pool, "SELECT count(*) FROM telegram_chats").await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM jobs").await, 0);
    // En backoff no se reintenta de inmediato.
    assert!(!processor::process_next(&t.state).await.unwrap());
    sqlx::query("DROP TRIGGER boom ON capture_proposals")
        .execute(&pool)
        .await
        .unwrap();
    t.clock.advance(Duration::minutes(5));
    assert!(processor::process_next(&t.state).await.unwrap());
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM telegram_updates WHERE status = 'processed'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM capture_proposals WHERE intent = 'gasto' AND source = 'telegram'"
        )
        .await,
        1
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM jobs WHERE kind = 'reply'").await,
        1
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn capture_commands_create_reviewable_proposals(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    ingest(&t, upd(1, 111, "private", "/gasto ayer 250 gasolina")).await;
    ingest(&t, upd(2, 111, "private", "/weekly-review")).await;
    ingest(&t, upd(3, 111, "private", "comprar pilas")).await;
    ingest(&t, upd(4, 111, "private", "/log_session dane main: bot listo")).await;
    processor::drain(&t.state).await.unwrap();
    let (id, text, date): (Uuid, String, chrono::NaiveDate) =
        sqlx::query_as("SELECT id, input_text, reference_date FROM capture_proposals WHERE intent = 'gasto'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(text, "ayer 250 gasolina");
    assert_eq!(
        date.to_string(),
        "2026-11-12",
        "la fecha de referencia es la local de hoy; 'ayer' lo resuelve la captura"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM capture_proposals WHERE intent = 'log_session' AND status = 'pending'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM inbox_items WHERE text = 'comprar pilas' AND source = 'telegram'"
        )
        .await,
        1
    );
    let jobs = claim(&t, 10).await;
    let urls: Vec<String> = jobs
        .iter()
        .filter_map(|j| j["message"]["buttons"][0][0]["url"].as_str().map(str::to_string))
        .collect();
    assert!(urls.contains(&format!("http://localhost:5173/captura/{id}")));
    assert!(urls.contains(&"http://localhost:5173/semanal?week=2026-W46".to_string()));
    assert!(urls.contains(&"http://localhost:5173/inbox".to_string()));
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn concurrent_claims_never_share_a_job(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let now = t.clock.now();
    for i in 0..30 {
        let payload = json!({"chat_id": 1, "text": format!("m{i}")});
        persistence::repos::jobs::enqueue(&pool, None, "reply", &format!("k{i}"), &payload, now, None, 5)
            .await
            .unwrap();
    }
    let (a, b, c) = tokio::join!(claim(&t, 15), claim(&t, 15), claim(&t, 15));
    let mut ids: Vec<String> = a
        .iter()
        .chain(&b)
        .chain(&c)
        .map(|j| j["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 30);
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 30, "ningún job reclamado dos veces");
    assert!(
        claim(&t, 10).await.is_empty(),
        "leases vigentes no se re-reclaman"
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn expired_lease_is_reclaimable_and_stale_ack_is_rejected(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    persistence::repos::jobs::enqueue(
        &pool,
        None,
        "reply",
        "k",
        &json!({"chat_id": 1, "text": "hola"}),
        t.clock.now(),
        None,
        5,
    )
    .await
    .unwrap();
    let first = claim(&t, 1).await.remove(0);
    t.clock.advance(Duration::seconds(61));
    let second = claim(&t, 1).await.remove(0);
    assert_eq!(first["id"], second["id"]);
    assert_ne!(first["lease_token"], second["lease_token"]);
    assert_eq!(second["attempt"], 2);
    let (st, b) = ack(&t, &first, json!({"result": "sent", "telegram_message_id": 1})).await;
    assert_eq!(st, StatusCode::CONFLICT, "{b}");
    assert_eq!(b["error"]["code"], "stale_lease");
    let (st, b) = ack(&t, &second, json!({"result": "sent", "telegram_message_id": 2})).await;
    assert_eq!(
        (st, b["status"].as_str(), b["replayed"].as_bool()),
        (StatusCode::OK, Some("sent"), Some(false))
    );
    let (st, b) = ack(&t, &second, json!({"result": "sent", "telegram_message_id": 2})).await;
    assert_eq!(
        (st, b["replayed"].as_bool()),
        (StatusCode::OK, Some(true)),
        "ack repetido es idempotente"
    );
    assert!(claim(&t, 5).await.is_empty(), "un job enviado no se reenvía");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn rate_limit_reschedules_and_attempts_are_bounded(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    persistence::repos::jobs::enqueue(
        &pool,
        None,
        "reply",
        "k",
        &json!({"chat_id": 1, "text": "hola"}),
        t.clock.now(),
        None,
        2,
    )
    .await
    .unwrap();
    let j = claim(&t, 1).await.remove(0);
    let (_, b) = ack(
        &t,
        &j,
        json!({"result": "retry", "retry_after_secs": 5, "error": "429 Too Many Requests"}),
    )
    .await;
    assert_eq!(b["status"], "pending");
    let next = chrono::DateTime::parse_from_rfc3339(b["next_attempt_at"].as_str().unwrap()).unwrap();
    assert_eq!(next.with_timezone(&Utc), t.clock.now() + Duration::seconds(5));
    assert!(claim(&t, 1).await.is_empty(), "respeta retry_after");
    t.clock.advance(Duration::seconds(6));
    let j2 = claim(&t, 1).await.remove(0);
    assert_eq!(j2["attempt"], 2);
    let (_, b) = ack(
        &t,
        &j2,
        json!({"result": "retry", "retry_after_secs": null, "error": "timeout"}),
    )
    .await;
    assert_eq!(b["status"], "failed", "max_attempts=2 agotado");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn permanent_block_marks_chat_and_stops_reminders(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    ingest(&t, upd(1, 111, "private", "/start")).await;
    processor::drain(&t.state).await.unwrap();
    let reply = claim(&t, 1).await.remove(0);
    let (_, b) = ack(
        &t,
        &reply,
        json!({"result": "failed", "error": "Forbidden: bot was blocked by the user", "chat_blocked": true}),
    )
    .await;
    assert_eq!(b["status"], "failed");
    assert_eq!(
        count(&pool, "SELECT count(*) FROM telegram_chats WHERE NOT can_message").await,
        1
    );
    assert_eq!(
        scheduler::run_once(&t.state).await.unwrap(),
        0,
        "no se programan recordatorios a un chat bloqueado"
    );
    // Si el usuario vuelve a escribir, el chat se reactiva.
    ingest(&t, upd(2, 111, "private", "/start")).await;
    processor::drain(&t.state).await.unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM telegram_chats WHERE can_message").await,
        1
    );
}

async fn user_with_chat(t: &TestApp) {
    ingest(t, upd(1, 111, "private", "/start")).await;
    processor::drain(&t.state).await.unwrap();
    for j in claim(t, 10).await {
        ack(t, &j, json!({"result": "sent", "telegram_message_id": 1})).await;
    }
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn daily_reminder_is_scheduled_once_at_local_time(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    user_with_chat(&t).await;
    let (a, b) = tokio::join!(scheduler::run_once(&t.state), scheduler::run_once(&t.state));
    assert_eq!(a.unwrap() + b.unwrap(), 1, "dos schedulers no duplican");
    assert_eq!(scheduler::run_once(&t.state).await.unwrap(), 0);
    let run_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT run_at FROM jobs WHERE kind = 'reminder_daily'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        run_at,
        Utc.with_ymd_and_hms(2026, 11, 13, 3, 0, 0).unwrap(),
        "21:00 en Monterrey"
    );
    assert!(claim(&t, 5).await.is_empty(), "aún no es la hora");
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 13, 3, 1, 0).unwrap());
    let jobs = claim(&t, 5).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["kind"], "reminder_daily");
    assert_eq!(
        jobs[0]["message"]["buttons"][0][0]["url"],
        "http://localhost:5173/hoy?date=2026-11-12&guiado=noche"
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn reminder_is_cancelled_when_checkin_already_finished(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    user_with_chat(&t).await;
    scheduler::run_once(&t.state).await.unwrap();
    sqlx::query("INSERT INTO daily_checkins (user_id, on_date, evening_done_at) SELECT user_id, '2026-11-12', now() FROM telegram_chats")
        .execute(&pool)
        .await
        .unwrap();
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 13, 3, 1, 0).unwrap());
    assert!(claim(&t, 5).await.is_empty());
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM jobs WHERE kind = 'reminder_daily' AND status = 'cancelled'"
        )
        .await,
        1
    );
    // Con el check-in terminado, el scheduler tampoco lo vuelve a crear.
    assert_eq!(scheduler::run_once(&t.state).await.unwrap(), 0);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn downtime_does_not_flood_historic_reminders(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    user_with_chat(&t).await;
    scheduler::run_once(&t.state).await.unwrap();
    // El bot estuvo caído 3 días: el recordatorio del 12 caduca; al volver solo se programa el de hoy.
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 15, 18, 0, 0).unwrap());
    assert!(claim(&t, 10).await.is_empty());
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM jobs WHERE kind = 'reminder_daily' AND status = 'expired'"
        )
        .await,
        1
    );
    scheduler::run_once(&t.state).await.unwrap();
    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT occurrence_key FROM jobs WHERE kind LIKE 'reminder%' AND status = 'pending' ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        keys,
        vec!["2026-11-15", "2026-W46"],
        "solo hoy (domingo: diario + semanal), sin 13 ni 14"
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn timezone_change_recalculates_pending_reminder(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    user_with_chat(&t).await;
    scheduler::run_once(&t.state).await.unwrap();
    sqlx::query("UPDATE user_settings SET timezone = 'Europe/Madrid', daily_time = '22:00'")
        .execute(&pool)
        .await
        .unwrap();
    scheduler::run_once(&t.state).await.unwrap();
    let run_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT run_at FROM jobs WHERE kind = 'reminder_daily'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        run_at,
        Utc.with_ymd_and_hms(2026, 11, 12, 21, 0, 0).unwrap(),
        "22:00 en Madrid (UTC+1)"
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn pause_via_bot_and_api_cancels_and_resume_reschedules(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    user_with_chat(&t).await;
    scheduler::run_once(&t.state).await.unwrap();
    ingest(&t, upd(2, 111, "private", "/pause")).await;
    processor::drain(&t.state).await.unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM jobs WHERE kind = 'reminder_daily' AND status = 'cancelled'"
        )
        .await,
        1
    );
    assert_eq!(scheduler::run_once(&t.state).await.unwrap(), 0);
    let a = t.login(111).await;
    let (st, s) = a.get("/reminders").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["paused"], true);
    assert!(s["upcoming"].as_array().unwrap().is_empty());
    let (_, s) = a.post("/reminders/resume", json!({})).await;
    assert_eq!(s["paused"], false);
    assert!(!s["upcoming"].as_array().unwrap().is_empty());
    assert!(s["recent"][0]["status"].as_str().is_some());
    // La ocurrencia cancelada no se reprograma (UNIQUE), y no se duplica.
    assert_eq!(scheduler::run_once(&t.state).await.unwrap(), 0);
    let (_, s) = a.post("/reminders/pause", json!({"days": 2})).await;
    assert_eq!(s["paused"], true);
    assert!(s["paused_until"].is_string());
    let (st, _) = a.post("/reminders/pause", json!({"days": 0})).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
}

// ---------- Asistente diario: saludo, mañana y avisos de eventos ----------

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn greeting_opens_menu_instead_of_inbox(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    user_with_chat(&t).await;
    ingest(&t, upd(2, 111, "private", "Hola!")).await;
    processor::drain(&t.state).await.unwrap();
    let jobs = claim(&t, 5).await;
    assert_eq!(jobs.len(), 1);
    let msg = &jobs[0]["message"];
    assert!(msg["text"].as_str().unwrap().starts_with("¡Hola, Usuario!"));
    assert_eq!(
        msg["buttons"][0][0]["url"],
        "http://localhost:5173/hoy?date=2026-11-12&guiado=manana"
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM inbox_items").await,
        0,
        "un saludo no es una captura"
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn morning_message_summarizes_tasks_and_agenda(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    // 05:00 en Monterrey: la mañana (07:00) aún no llega.
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 12, 11, 0, 0).unwrap());
    user_with_chat(&t).await;
    sqlx::query("INSERT INTO tasks (user_id, title, planned_on) SELECT user_id, 'Pagar renta', '2026-11-12' FROM telegram_chats")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO calendar_events (user_id, on_date, starts_at, ends_at, title)
         SELECT user_id, '2026-11-12', '2026-11-12T15:00:00Z', '2026-11-12T16:00:00Z', 'Junta con cliente' FROM telegram_chats",
    )
    .execute(&pool)
    .await
    .unwrap();
    scheduler::run_once(&t.state).await.unwrap();
    let run_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT run_at FROM jobs WHERE kind = 'reminder_morning'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        run_at,
        Utc.with_ymd_and_hms(2026, 11, 12, 13, 0, 0).unwrap(),
        "07:00 en Monterrey"
    );
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 12, 13, 1, 0).unwrap());
    let jobs = claim(&t, 5).await;
    let m = jobs
        .iter()
        .find(|j| j["kind"] == "reminder_morning")
        .expect("mensaje de la mañana");
    let text = m["message"]["text"].as_str().unwrap();
    assert!(text.contains("Tienes 1 tarea pendiente."), "{text}");
    assert!(text.contains("• 09:00 Junta con cliente"), "{text}");
    assert_eq!(
        m["message"]["buttons"][0][0]["url"],
        "http://localhost:5173/hoy?date=2026-11-12&guiado=manana"
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn morning_is_skipped_when_morning_checkin_done(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 12, 11, 0, 0).unwrap());
    user_with_chat(&t).await;
    scheduler::run_once(&t.state).await.unwrap();
    sqlx::query("INSERT INTO daily_checkins (user_id, on_date, morning_done_at) SELECT user_id, '2026-11-12', now() FROM telegram_chats")
        .execute(&pool)
        .await
        .unwrap();
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 12, 13, 1, 0).unwrap());
    assert!(claim(&t, 5).await.iter().all(|j| j["kind"] != "reminder_morning"));
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn event_reminder_fires_before_the_event_once(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    user_with_chat(&t).await;
    sqlx::query(
        "INSERT INTO calendar_events (user_id, on_date, starts_at, ends_at, title, location)
         SELECT user_id, '2026-11-12', '2026-11-12T19:00:00Z', '2026-11-12T20:00:00Z', 'Dentista', 'Av. Juárez 10' FROM telegram_chats",
    )
    .execute(&pool)
    .await
    .unwrap();
    // 18:00 UTC: faltan 60 min, fuera de la ventana (15 min + 5).
    scheduler::run_once(&t.state).await.unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM jobs WHERE kind = 'reminder_event'").await,
        0
    );
    t.clock
        .set(Utc.with_ymd_and_hms(2026, 11, 12, 18, 41, 0).unwrap());
    scheduler::run_once(&t.state).await.unwrap();
    scheduler::run_once(&t.state).await.unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM jobs WHERE kind = 'reminder_event'").await,
        1,
        "sin duplicados"
    );
    t.clock
        .set(Utc.with_ymd_and_hms(2026, 11, 12, 18, 46, 0).unwrap());
    let jobs = claim(&t, 5).await;
    let e = jobs
        .iter()
        .find(|j| j["kind"] == "reminder_event")
        .expect("aviso del evento");
    let text = e["message"]["text"].as_str().unwrap();
    assert_eq!(text, "⏰ A las 13:00: Dentista\n📍 Av. Juárez 10");
}
