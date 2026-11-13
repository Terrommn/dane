//! A-core: revisión semanal (T26) y dashboard general (T23). Reloj: 2026-11-12 12:00 America/Monterrey.

mod common;

use axum::http::StatusCode;
use common::{Client, TestApp};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn fixed(a: &Client) -> Value {
    let (st, f) = a
        .post(
            "/habits/fixed",
            json!({
                "devotional": { "enabled": true, "frequency": "daily" },
                "exercise": { "enabled": true, "frequency": "times_per_week", "times_per_week": 4 },
                "reading": { "enabled": true, "frequency": "daily" },
                "starts_on": "2026-11-01"
            }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{f}");
    f
}

fn id_of(f: &Value, key: &str) -> String {
    f.as_array()
        .unwrap()
        .iter()
        .find(|h| h["fixed_key"] == key)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn weekly_summary_counts_sources_and_review_is_editable(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let f = fixed(&a).await;
    let dev = id_of(&f, "devotional");
    let ex = id_of(&f, "exercise");
    for d in ["2026-11-09", "2026-11-10", "2026-11-11"] {
        a.put(&format!("/habits/{dev}/entries/{d}"), json!({ "status": "done" }))
            .await;
    }
    for d in ["2026-11-09", "2026-11-11"] {
        a.put(&format!("/habits/{ex}/entries/{d}"), json!({ "status": "done" }))
            .await;
    }
    a.put(
        "/checkins/2026-11-10",
        json!({ "energy": 3, "went_well": ["Orar temprano"], "went_badly": ["Dormí tarde"] }),
    )
    .await;
    a.put(
        "/checkins/2026-11-11",
        json!({ "energy": 5, "went_badly": ["Dormí tarde otra vez"] }),
    )
    .await;
    let (_, g) = a
        .post(
            "/goals",
            json!({ "kind": "week", "period_key": "2026-W46", "title": "Dormir antes de 11" }),
        )
        .await;
    sqlx::query("INSERT INTO content_pieces (user_id, title, status, published_on) VALUES ($1::uuid, 'Reel 1', 'published', '2026-11-10')")
        .bind(&a.user_id)
        .execute(&pool)
        .await
        .unwrap();

    let (st, s) = a.get("/weekly-reviews/2026-W46/summary").await;
    assert_eq!(st, StatusCode::OK, "{s}");
    assert_eq!(s["is_open"], true);
    let fh = |k: &str| {
        s["fixed_habits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["key"] == k)
            .unwrap()
            .clone()
    };
    assert_eq!(fh("devotional")["done"], 3);
    assert_eq!(fh("devotional")["target"], 7);
    assert_eq!(fh("exercise")["done"], 2);
    assert_eq!(fh("exercise")["target"], 4, "Ejercicio: /4 (meta)");
    assert_eq!(s["went_badly"].as_array().unwrap().len(), 2);
    assert_eq!(s["went_well"][0]["text"], "Orar temprano");
    assert_eq!(s["goals"][0]["goal"]["title"], "Dormir antes de 11");
    assert_eq!(s["content_published"][0]["title"], "Reel 1");
    assert_eq!(s["energy_avg"], 4.0);
    let ev = s["evidence"].as_array().unwrap();
    assert!(
        ev.iter()
            .any(|e| e["entity_type"] == "goal" && e["entity_id"] == g["goal"]["id"])
    );
    assert!(ev.iter().any(|e| e["route"] == "/hoy?date=2026-11-10"));

    // Edición: paz 1–5 y máximo 3 prioridades.
    let (_, empty) = a.get("/weekly-reviews/2026-W46").await;
    assert!(empty["review"].is_null());
    assert_eq!(empty["next_week"], "2026-W47");
    assert_eq!(empty["planning_route"], "/objetivos/planificar?week=2026-W47");
    assert_eq!(
        a.put("/weekly-reviews/2026-W46", json!({ "peace": 6 })).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let four = json!([{ "text": "a" }, { "text": "b" }, { "text": "c" }, { "text": "d" }]);
    assert_eq!(
        a.put(
            "/weekly-reviews/2026-W46",
            json!({ "peace": 4, "priorities": four })
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (st, r) = a
        .put("/weekly-reviews/2026-W46", json!({ "peace": 4, "what_happened": "Semana productiva", "priorities": [{ "text": "Dormir temprano", "reason": "patrón de la semana" }] }))
        .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r["peace"], 4);
    assert!(r["summary"]["fixed_habits"].is_array(), "snapshot del resumen");
    // Versión vieja → 409; cerrar el objetivo es otra acción (el estado del objetivo no cambió).
    assert_eq!(
        a.put("/weekly-reviews/2026-W46", json!({ "peace": 2 })).await.0,
        StatusCode::CONFLICT
    );
    let (_, goal) = a
        .get(&format!("/goals/{}", g["goal"]["id"].as_str().unwrap()))
        .await;
    assert_eq!(goal["goal"]["status"], "active");
    let (_, list) = a.get("/weekly-reviews").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    // Aislamiento.
    let b = t.login(222).await;
    let (_, sb) = b.get("/weekly-reviews/2026-W46/summary").await;
    assert_eq!(sb["went_badly"].as_array().unwrap().len(), 0);
    let (_, rb) = b.get("/weekly-reviews/2026-W46").await;
    assert!(rb["review"].is_null());
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn dashboard_devotional_failures_weight_and_comparisons(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let f = fixed(&a).await;
    let dev = id_of(&f, "devotional");
    // Del 1 al 11 (días cerrados): 7 hechos, 2 "no" explícitos, 2 sin registro. Hoy (12) abierto no cuenta.
    for d in 1..=7 {
        a.put(
            &format!("/habits/{dev}/entries/2026-11-{d:02}"),
            json!({ "status": "done" }),
        )
        .await;
    }
    for d in [8, 9] {
        a.put(
            &format!("/habits/{dev}/entries/2026-11-{d:02}"),
            json!({ "status": "not_done" }),
        )
        .await;
    }
    let (_, s) = a.get("/settings").await;
    a.put("/settings", json!({ "version": s["version"], "max_devotional_failures": 3, "weight_goal_kg": "75", "weight_direction": "decrease" })).await;
    a.put("/checkins/2026-11-02", json!({ "weight_kg": "80", "energy": 3 }))
        .await;
    a.put(
        "/checkins/2026-11-10",
        json!({ "weight_kg": "78.5", "energy": 4 }),
    )
    .await;
    a.put("/checkins/2026-10-05", json!({ "weight_kg": "82" })).await;

    let (st, d) = a.get("/dashboard").await;
    assert_eq!(st, StatusCode::OK, "{d}");
    let dv = &d["devotional"];
    assert_eq!(dv["explicit_failures"], 2);
    assert_eq!(dv["unrecorded"], 2);
    assert_eq!(dv["failures_total"], 4);
    assert_eq!(dv["max_allowed"], 3);
    assert_eq!(dv["over_limit"], true);
    assert_eq!(dv["closed_days"], 11);
    // Peso: último real y cambio del mes sin interpolar.
    assert_eq!(d["weight"]["last"]["kg"], 78.5);
    assert_eq!(d["weight"]["change_month"], -1.5);
    assert!(
        d["weight"]["change_week"].is_null(),
        "una sola medición en la semana: sin cambio"
    );
    assert_eq!(d["weight"]["remaining_kg"], 3.5);
    // Heatmap de 30 días: hoy sin ratio (abierto) y días sin registro marcados.
    let heat = d["last_30_days"].as_array().unwrap();
    assert_eq!(heat.len(), 30);
    let today = heat.iter().find(|h| h["date"] == "2026-11-12").unwrap();
    assert!(today["ratio"].is_null());
    let day10 = heat.iter().find(|h| h["date"] == "2026-11-10").unwrap();
    assert!(day10["unrecorded"].as_u64().unwrap() >= 1);
    assert_eq!(day10["route"], "/hoy?date=2026-11-10");
    // Hábitos separados por tipo.
    assert_eq!(d["habits"]["fixed"].as_array().unwrap().len(), 3);
    // Comparación por mismo tramo (1–12 nov vs 1–12 oct) y bloque financiero.
    let mc = &d["month_comparison"];
    assert_eq!(mc["mode"], "same_span");
    assert_eq!(mc["previous_start"], "2026-10-01");
    assert_eq!(mc["previous_end"], "2026-10-12");
    let w = mc["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["key"] == "weight_last")
        .unwrap();
    assert_eq!(w["current"], 78.5);
    assert_eq!(w["previous"], 82.0);
    assert_eq!(w["better"], true, "bajar es mejor con dirección decrease");
    assert!(d["finance"]["month"]["consumption_cents"].is_number());
    let (_, full) = a.get("/dashboard?compare=full").await;
    assert_eq!(full["month_comparison"]["previous_end"], "2026-10-31");
    assert!(
        full["month_comparison"]["label"]
            .as_str()
            .unwrap()
            .starts_with("Periodos completos")
    );
    assert_eq!(
        a.get("/dashboard?compare=raro").await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn dashboard_without_configured_max_and_without_data(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (st, d) = a.get("/dashboard").await;
    assert_eq!(st, StatusCode::OK, "{d}");
    assert!(
        d["devotional"].is_null(),
        "sin hábito devocional no se inventa la métrica"
    );
    assert!(d["weight"]["last"].is_null());
    let f = fixed(&a).await;
    let _ = f;
    let (_, d) = a.get("/dashboard").await;
    assert!(d["devotional"]["max_allowed"].is_null());
    assert!(d["devotional"]["over_limit"].is_null());
}
