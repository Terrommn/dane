//! Flujo punta a punta del requisito §11 (lado server, con bot simulado vía API interna):
//! crear meta y hábito → /start en Telegram → recordatorio a las 21:00 locales con enlace al check-in de la fecha →
//! guardar check-in (también reintento offline con la misma Idempotency-Key) → el dashboard muestra el mismo
//! progreso y el gasto del día → al día siguiente el recordatorio se cancela si el check-in ya se cerró.

mod common;

use axum::http::{Method, StatusCode};
use chrono::{TimeZone, Utc};
use common::{TestApp, send};
use serde_json::{Value, json};
use server::telegram::{processor, scheduler};
use sqlx::PgPool;

const INTERNAL: &str = "Bearer internal-test-token-0123456789abcdef";

async fn internal(t: &TestApp, path: &str, body: Value) -> Value {
    let (st, b) = send(
        &t.app,
        Method::POST,
        &format!("/internal/v1{path}"),
        Some(body),
        &[("authorization", INTERNAL)],
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{path}: {b}");
    b
}

async fn claim_all(t: &TestApp) -> Vec<Value> {
    let jobs = internal(
        t,
        "/jobs/claim",
        json!({"worker_id": "e2e", "limit": 20, "lease_secs": 60}),
    )
    .await["jobs"]
        .as_array()
        .unwrap()
        .clone();
    for j in &jobs {
        internal(
            t,
            &format!("/jobs/{}/ack", j["id"].as_str().unwrap()),
            json!({"lease_token": j["lease_token"], "outcome": {"result": "sent", "telegram_message_id": 7}}),
        )
        .await;
    }
    jobs
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn goal_reminder_checkin_dashboard_round_trip(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;

    // 1. Meta del mes con hábito diario.
    let (st, goal) = a.post("/goals", json!({"kind": "month", "period_key": "2026-11", "title": "Correr un 10K", "motivation": "Salud", "success_criteria": "Terminar el 10K"})).await;
    assert_eq!(st, StatusCode::CREATED, "{goal}");
    let goal_id = goal["goal"]["id"]
        .as_str()
        .or_else(|| goal["id"].as_str())
        .unwrap()
        .to_string();
    let (st, habit) = a.post("/habits", json!({"name": "Correr 20 min", "kind": "goal", "goal_id": goal_id, "starts_on": "2026-11-01", "frequency": "daily"})).await;
    assert_eq!(st, StatusCode::CREATED, "{habit}");
    let habit_id = habit["habit"]["id"]
        .as_str()
        .or_else(|| habit["id"].as_str())
        .unwrap()
        .to_string();

    // 2. El usuario abre el bot (/start) y conoce el chat privado.
    internal(&t, "/telegram/updates", json!({"update_id": 1, "payload": {"update_id": 1, "message": {"message_id": 1, "date": 0,
        "chat": {"id": 111, "type": "private"}, "from": {"id": 111, "is_bot": false, "first_name": "Neil"}, "text": "/start"}}})).await;
    processor::drain(&t.state).await.unwrap();
    assert_eq!(claim_all(&t).await.len(), 1, "respuesta de bienvenida");

    // 3. Recordatorio nocturno a las 21:00 de Monterrey con enlace al check-in de esa fecha.
    scheduler::run_once(&t.state).await.unwrap();
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 13, 3, 1, 0).unwrap());
    let jobs = claim_all(&t).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["kind"], "reminder_daily");
    assert_eq!(
        jobs[0]["message"]["buttons"][0][0]["url"],
        "http://localhost:5173/hoy?date=2026-11-12"
    );

    // 4. Abre el check-in de esa fecha (aunque ya sea otro día en UTC) y guarda; el reintento offline no duplica.
    let body = json!({"energy": 4, "evening_done": true, "went_well": ["Corrí temprano"], "habits": [{"habit_id": habit_id, "status": "done"}]});
    let k = [("idempotency-key", "offline-checkin-2026-11-12")];
    let (st, saved) = a
        .req(Method::PUT, "/checkins/2026-11-12", Some(body.clone()), &k)
        .await;
    assert_eq!(st, StatusCode::OK, "{saved}");
    let (st2, replay) = a.req(Method::PUT, "/checkins/2026-11-12", Some(body), &k).await;
    assert_eq!(st2, StatusCode::OK);
    assert_eq!(
        saved["version"], replay["version"],
        "el reintento devuelve la misma respuesta"
    );
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM habit_entries WHERE habit_id = $1::uuid")
        .bind(&habit_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(entries, 1);

    // Gasto del día para el bloque financiero.
    let (_, cats) = a.get("/finance/categories").await;
    let comida = cats
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Comida")
        .unwrap()["id"]
        .clone();
    let (st, tx) = a.post("/finance/transactions", json!({"kind": "expense", "occurred_on": "2026-11-12", "concept": "Comida", "amount": "180.50", "category_id": comida})).await;
    assert_eq!(st, StatusCode::CREATED, "{tx}");

    // 5. El dashboard muestra el mismo progreso que se guardó.
    let (st, dash) = a.get("/dashboard?date=2026-11-12").await;
    assert_eq!(st, StatusCode::OK, "{dash}");
    let stat = dash["habits"]["month"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["habit"]["id"] == habit_id.as_str())
        .expect("hábito del mes en el dashboard");
    assert_eq!(stat["today"]["status"], "done");
    // El 12 sigue abierto (21:01 en Monterrey): no entra al cumplimiento cerrado (D-019), sí al avance de hoy.
    assert_eq!(stat["month"]["done"], 0);
    assert_eq!(dash["finance"]["month"]["consumption_cents"], 18050);
    assert!(
        dash["month_goals"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g.to_string().contains(&goal_id))
    );

    // 6. Día siguiente: check-in cerrado antes de las 21:00 → el recordatorio se cancela, no se envía.
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 13, 18, 0, 0).unwrap());
    scheduler::run_once(&t.state).await.unwrap();
    // La sesión Bearer (12 h) venció: la Mini App vuelve a autenticarse al abrirse.
    assert_eq!(a.get("/me").await.0, StatusCode::UNAUTHORIZED);
    let a = t.login(111).await;
    let (st, _) = a.put("/checkins/2026-11-13", json!({"evening_done": true})).await;
    assert_eq!(st, StatusCode::OK);
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 14, 3, 5, 0).unwrap());
    assert!(
        claim_all(&t).await.is_empty(),
        "no se envía recordatorio de un check-in terminado"
    );
    let cancelled: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'reminder_daily' AND occurrence_key LIKE '%2026-11-13%' AND status = 'cancelled'").fetch_one(&pool).await.unwrap();
    assert_eq!(cancelled, 1);

    // Ya cerrado el 12, el cumplimiento del mes lo incluye.
    let (_, dash) = a.get("/dashboard?date=2026-11-13").await;
    let stat = dash["habits"]["month"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["habit"]["id"] == habit_id.as_str())
        .unwrap();
    assert_eq!(stat["month"]["done"], 1);
}
