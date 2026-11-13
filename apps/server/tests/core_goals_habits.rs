//! A-core: objetivos (T17/T18), hábitos (T19), métricas (T22) y lecturas (T29) contra PostgreSQL real.
//! Reloj de pruebas: 2026-11-12 12:00 America/Monterrey (jueves, semana 2026-W46).

mod common;

use axum::http::StatusCode;
use common::{Client, TestApp};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn month_goal(a: &Client, title: &str, extra: Value) -> Value {
    let mut body = json!({ "kind": "month", "period_key": "2026-11", "title": title });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let (st, g) = a.post("/goals", body).await;
    assert_eq!(st, StatusCode::CREATED, "{g}");
    g
}

async fn week_goal(a: &Client, title: &str, parent: Option<&str>, extra: Value) -> Value {
    let mut body =
        json!({ "kind": "week", "period_key": "2026-W46", "title": title, "parent_goal_id": parent });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let (st, g) = a.post("/goals", body).await;
    assert_eq!(st, StatusCode::CREATED, "{g}");
    g
}

fn id(v: &Value) -> String {
    v["goal"]["id"]
        .as_str()
        .or(v["id"].as_str())
        .or(v["habit"]["id"].as_str())
        .unwrap()
        .to_string()
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn goals_week_supports_month_and_closing_is_separate(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let m = month_goal(
        &a,
        "Bajar a 75 kg",
        json!({ "motivation": "salud", "success_criteria": "pesar 75" }),
    )
    .await;
    assert_eq!(m["goal"]["starts_on"], "2026-11-01");
    assert_eq!(m["goal"]["ends_on"], "2026-11-30");
    assert_eq!(m["closure_pending"], false);
    let mid = id(&m);
    let w = week_goal(&a, "Gym 4 veces", Some(&mid), json!({})).await;
    assert_eq!(w["goal"]["parent_goal_id"], mid.as_str());

    // El padre debe ser mensual.
    let (st, e) = a
        .post(
            "/goals",
            json!({ "kind": "week", "period_key": "2026-W46", "title": "x", "parent_goal_id": id(&w) }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{e}");
    // Un mensual no puede tener padre.
    let (st, _) = a
        .post(
            "/goals",
            json!({ "kind": "month", "period_key": "2026-11", "title": "x", "parent_goal_id": mid }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    // Semana fuera del mes.
    let (st, _) = a
        .post(
            "/goals",
            json!({ "kind": "week", "period_key": "2026-W50", "title": "x", "parent_goal_id": mid }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    // Periodo inválido.
    let (st, _) = a
        .post(
            "/goals",
            json!({ "kind": "week", "period_key": "2026-W54", "title": "x" }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // Semana anterior terminada y activa → cierre pendiente (distinto de archivar).
    let (_, old) = a
        .post(
            "/goals",
            json!({ "kind": "week", "period_key": "2026-W45", "title": "Leer 3 días" }),
        )
        .await;
    assert_eq!(old["closure_pending"], true);
    let oid = id(&old);
    // Archivar no cierra.
    let (st, arch) = a
        .put(
            &format!("/goals/{oid}"),
            json!({ "title": "Leer 3 días", "archived": true, "version": 1 }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{arch}");
    assert_eq!(arch["goal"]["status"], "active");
    assert_eq!(arch["closure_pending"], true);
    // Cierre explícito con reflexión.
    let (st, _) = a
        .post(
            &format!("/goals/{oid}/close"),
            json!({ "status": "active", "version": 2 }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, closed) = a
        .post(&format!("/goals/{oid}/close"), json!({ "status": "not_achieved", "achievement": "2 de 3", "learning": "leer de noche", "difficulties": "viaje", "adjustments": "audiolibro", "version": 2 }))
        .await;
    assert_eq!(st, StatusCode::OK, "{closed}");
    assert_eq!(closed["goal"]["status"], "not_achieved");
    assert_eq!(closed["goal"]["closing"]["learning"], "leer de noche");
    assert_eq!(closed["closure_pending"], false);
    // Versión vieja → 409.
    let (st, c) = a
        .post(
            &format!("/goals/{oid}/close"),
            json!({ "status": "achieved", "version": 2 }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "{c}");

    let (_, detail) = a.get(&format!("/goals/{mid}")).await;
    assert_eq!(detail["children"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn planning_asks_month_first_and_warns_on_load(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (st, p) = a.get("/goals/planning").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(p["week"], "2026-W46");
    assert_eq!(p["month"], "2026-11");
    assert_eq!(p["define_month_first"], true);
    let m = month_goal(&a, "Proyecto X", json!({})).await;
    let mid = id(&m);
    for i in 0..7 {
        let (st, h) = a.post("/habits", json!({ "name": format!("Hábito {i}"), "kind": "goal", "goal_id": mid, "frequency": "daily" })).await;
        assert_eq!(st, StatusCode::CREATED, "{h}");
        // Hábito de objetivo: rige desde hoy (o el inicio) hasta el fin del objetivo.
        assert_eq!(h["current_version"]["valid_to"], "2026-11-30");
    }
    let (_, p) = a.get("/goals/planning?week=2026-W46").await;
    assert_eq!(p["define_month_first"], false);
    assert_eq!(p["month_habit_count"], 7);
    let w = p["warnings"].as_array().unwrap();
    assert!(w.iter().any(|x| x.as_str().unwrap().contains("~6")), "{w:?}");
    assert!(
        w.iter().any(|x| x.as_str().unwrap().contains("de 1 a 3")),
        "{w:?}"
    );
    // Avisos, no bloqueos: se pudieron crear los 7.
    // Sábado/domingo → semana siguiente por defecto.
    use chrono::TimeZone;
    t.clock
        .set(chrono::Utc.with_ymd_and_hms(2026, 11, 14, 18, 0, 0).unwrap());
    let a = t.login(111).await; // la sesión Bearer (12 h) venció al adelantar el reloj
    let (_, p) = a.get("/goals/planning").await;
    assert_eq!(p["week"], "2026-W47");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn habit_compliance_distinguishes_missing_and_open_day(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (st, h) = a
        .post(
            "/habits",
            json!({ "name": "Orar", "frequency": "daily", "starts_on": "2026-11-09" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{h}");
    let hid = id(&h);
    assert_eq!(
        a.put(
            &format!("/habits/{hid}/entries/2026-11-09"),
            json!({ "status": "done" })
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        a.put(
            &format!("/habits/{hid}/entries/2026-11-10"),
            json!({ "status": "not_done" })
        )
        .await
        .0,
        StatusCode::OK
    );
    // 11: sin registro. 12: hoy abierto.
    let (_, d) = a.get(&format!("/habits/{hid}")).await;
    let c = &d["last_30_days"];
    assert_eq!(
        (
            c["required"].as_u64(),
            c["done"].as_u64(),
            c["missed"].as_u64(),
            c["unrecorded"].as_u64()
        ),
        (Some(3), Some(1), Some(1), Some(1)),
        "{c}"
    );
    assert_eq!(d["today"]["required_today"], true);
    assert!(d["today"]["status"].is_null());

    // not_applicable exige motivo y sale del denominador.
    let (st, _) = a
        .put(
            &format!("/habits/{hid}/entries/2026-11-11"),
            json!({ "status": "not_applicable" }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = a
        .put(
            &format!("/habits/{hid}/entries/2026-11-11"),
            json!({ "status": "not_applicable", "reason": "enfermo" }),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let (_, d) = a.get(&format!("/habits/{hid}")).await;
    assert_eq!(d["last_30_days"]["required"], 2);
    assert_eq!(d["last_30_days"]["excluded_not_applicable"], 1);
    // Futuro y antes de existir → 422.
    assert_eq!(
        a.put(
            &format!("/habits/{hid}/entries/2026-11-13"),
            json!({ "status": "done" })
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        a.put(
            &format!("/habits/{hid}/entries/2026-11-01"),
            json!({ "status": "done" })
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Volver a "sin registrar".
    assert_eq!(
        a.delete(&format!("/habits/{hid}/entries/2026-11-10")).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, d) = a.get(&format!("/habits/{hid}")).await;
    assert_eq!(d["last_30_days"]["unrecorded"], 1);
    // Conflicto de versión en una entrada.
    let (_, e) = a
        .put(
            &format!("/habits/{hid}/entries/2026-11-09"),
            json!({ "status": "done", "version": 1 }),
        )
        .await;
    assert_eq!(e["version"], 2);
    let (st, _) = a
        .put(
            &format!("/habits/{hid}/entries/2026-11-09"),
            json!({ "status": "not_done", "version": 1 }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Historial de 30 días distingue estados.
    let (_, hist) = a.get("/habits/history?from=2026-11-09&to=2026-11-12").await;
    let days = hist["days"].as_array().unwrap();
    let state = |date: &str| {
        days.iter().find(|d| d["date"] == date).unwrap()["cells"][0]["state"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(state("2026-11-09"), "done");
    assert_eq!(state("2026-11-10"), "unrecorded");
    assert_eq!(state("2026-11-11"), "not_applicable");
    assert_eq!(state("2026-11-12"), "open");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn times_per_week_and_frequency_change_keep_history(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (_, h) = a.post("/habits", json!({ "name": "Gym", "frequency": "times_per_week", "times_per_week": 4, "starts_on": "2026-11-02" })).await;
    let hid = id(&h);
    for d in [
        "2026-11-02",
        "2026-11-04",
        "2026-11-06",
        "2026-11-07",
        "2026-11-10",
    ] {
        assert_eq!(
            a.put(&format!("/habits/{hid}/entries/{d}"), json!({ "status": "done" }))
                .await
                .0,
            StatusCode::OK
        );
    }
    let (_, d) = a.get(&format!("/habits/{hid}")).await;
    // Hoy martes sin gym no es falla: semana en curso 1/4.
    assert_eq!(d["today"]["required_today"], false);
    assert_eq!(d["today"]["week_done"], 1);
    assert_eq!(d["today"]["week_target"], 4);
    assert_eq!(d["streaks"]["unit"], "weeks");
    assert_eq!(d["streaks"]["current"], 1);
    let before = d["this_month"].clone();
    assert_eq!(
        (before["required"].as_u64(), before["done"].as_u64()),
        (Some(4), Some(4))
    );
    // Cambiar a diario desde el 16: el pasado sigue evaluándose como 4/semana.
    let (st, v) = a
        .post(
            &format!("/habits/{hid}/versions"),
            json!({ "valid_from": "2026-11-16", "frequency": "daily" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let (_, d) = a.get(&format!("/habits/{hid}")).await;
    assert_eq!(d["this_month"], before);
    assert_eq!(d["versions"].as_array().unwrap().len(), 2);
    assert_eq!(d["versions"][0]["valid_to"], "2026-11-15");
    // Pausa: fuera del denominador.
    let (st, _) = a
        .post(
            &format!("/habits/{hid}/pauses"),
            json!({ "starts_on": "2026-11-02", "ends_on": "2026-11-08", "reason": "viaje" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let (_, d) = a.get(&format!("/habits/{hid}")).await;
    assert_eq!(d["this_month"]["required"], 0);
    assert_eq!(d["this_month"]["paused_days"], 7);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn fixed_habits_are_created_only_on_confirmation_and_idempotent(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (_, list) = a.get("/habits").await;
    assert_eq!(
        list.as_array().unwrap().len(),
        0,
        "no se siembran hábitos sin confirmación"
    );
    let body = json!({
        "devotional": { "enabled": true, "frequency": "daily" },
        "exercise": { "enabled": true, "frequency": "times_per_week", "times_per_week": 4 },
        "reading": { "enabled": false, "frequency": "daily" }
    });
    let (st, r) = a.post("/habits/fixed", body.clone()).await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(r.as_array().unwrap().len(), 2);
    let (_, r2) = a.post("/habits/fixed", body).await;
    assert_eq!(r2.as_array().unwrap().len(), 2);
    let (_, list) = a.get("/habits").await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    let ex = list
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["habit"]["fixed_key"] == "exercise")
        .unwrap();
    assert_eq!(ex["current_version"]["times_per_week"], 4);
    // Vínculos explícitos.
    let ex_id = ex["habit"]["id"].as_str().unwrap();
    assert_eq!(
        a.put("/habit-links/weight_logged", json!({ "habit_id": ex_id }))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        a.put("/habit-links/por_texto", json!({ "habit_id": ex_id }))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (_, links) = a.get("/habit-links").await;
    assert_eq!(links[0]["trigger"], "weight_logged");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn reading_linked_to_week_and_month_counts_once_each(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let m = month_goal(&a, "Leer 4 libros", json!({ "progress_type": "cumulative", "unit": "libros", "target": "4", "progress_source": "readings" })).await;
    let w = week_goal(&a, "Terminar un libro", Some(&id(&m)), json!({ "progress_type": "cumulative", "unit": "libros", "target": "1", "progress_source": "readings" })).await;
    let (st, r) = a.post("/readings", json!({ "title": "Hábitos atómicos", "author": "Clear", "status": "reading", "goal_ids": [id(&m), id(&w)] })).await;
    assert_eq!(st, StatusCode::CREATED, "{r}");
    let rid = r["id"].as_str().unwrap().to_string();
    let progress = |g: &Value| g["progress"]["current"].as_f64().unwrap();
    let (_, gm) = a.get(&format!("/goals/{}", id(&m))).await;
    assert_eq!(progress(&gm), 0.0);
    // Terminar → +1 a cada objetivo, una sola vez aunque se guarde varias veces.
    let mut v = 1;
    for _ in 0..3 {
        let (st, r) = a.put(&format!("/readings/{rid}"), json!({ "title": "Hábitos atómicos", "status": "finished", "finished_on": "2026-11-11", "rating": 5, "version": v })).await;
        assert_eq!(st, StatusCode::OK, "{r}");
        v = r["version"].as_i64().unwrap();
    }
    let (_, gm) = a.get(&format!("/goals/{}", id(&m))).await;
    let (_, gw) = a.get(&format!("/goals/{}", id(&w))).await;
    assert_eq!(progress(&gm), 1.0);
    assert_eq!(progress(&gw), 1.0);
    assert_eq!(gw["progress"]["achieved"], true);
    assert_eq!(gm["progress"]["remaining"], 3.0);
    // Ritmo esperado al día 12 de 30: 4×12/30 = 1.6; proyección marcada como estimación.
    assert!((gm["progress"]["expected_by_today"].as_f64().unwrap() - 1.6).abs() < 1e-9);
    assert!(gm["progress"]["projection_estimate"].is_number());
    // Deja de estar terminada → se retira el aporte.
    let (_, _) = a.put(&format!("/readings/{rid}"), json!({ "title": "Hábitos atómicos", "status": "reading", "goal_ids": [id(&m), id(&w)], "version": v })).await;
    let (_, gm) = a.get(&format!("/goals/{}", id(&m))).await;
    assert_eq!(progress(&gm), 0.0);
    // Lista por estado.
    let (_, reading) = a.get("/readings?status=reading").await;
    assert_eq!(reading.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn books_goal_and_minutes_habit_keep_distinct_units(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (_, h) = a.post("/habits", json!({ "name": "Leer 20 min", "unit": "min", "frequency": "daily", "target_quantity": "20", "starts_on": "2026-11-01" })).await;
    let hid = id(&h);
    // Un objetivo de libros no puede alimentarse del hábito en minutos.
    let (st, e) = a.post("/goals", json!({ "kind": "month", "period_key": "2026-11", "title": "4 libros", "progress_type": "cumulative", "unit": "libros", "target": "4", "progress_source": "habit", "source_ref": hid })).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{e}");
    assert_eq!(e["error"]["field"], "unit");
    // Un objetivo de minutos sí, sumando solo cantidades registradas.
    let minutes = month_goal(&a, "600 minutos", json!({ "progress_type": "cumulative", "unit": "min", "target": "600", "progress_source": "habit", "source_ref": hid })).await;
    let books = month_goal(
        &a,
        "4 libros",
        json!({ "progress_type": "cumulative", "unit": "libros", "target": "4" }),
    )
    .await;
    a.put(
        &format!("/habits/{hid}/entries/2026-11-10"),
        json!({ "status": "done", "quantity": "25" }),
    )
    .await;
    a.put(
        &format!("/habits/{hid}/entries/2026-11-11"),
        json!({ "status": "done", "quantity": "15" }),
    )
    .await;
    let (_, gm) = a.get(&format!("/goals/{}", id(&minutes))).await;
    assert_eq!(gm["progress"]["current"], 40.0);
    let (_, gb) = a.get(&format!("/goals/{}", id(&books))).await;
    assert_eq!(gb["progress"]["current"], 0.0, "registrar minutos no suma libros");
    // Evento manual de libro solo en el objetivo de libros; tipo incorrecto → 422.
    let (st, _) = a
        .post(
            &format!("/goals/{}/progress", id(&books)),
            json!({ "occurred_on": "2026-11-11", "amount": "1" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let (st, _) = a
        .post(
            &format!("/goals/{}/progress", id(&books)),
            json!({ "occurred_on": "2026-11-11", "value": "1" }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = a
        .post(
            &format!("/goals/{}/progress", id(&minutes)),
            json!({ "occurred_on": "2026-11-11", "amount": "5" }),
        )
        .await;
    assert_eq!(
        st,
        StatusCode::UNPROCESSABLE_ENTITY,
        "la fuente hábito no admite eventos manuales"
    );
    let (_, gb) = a.get(&format!("/goals/{}", id(&books))).await;
    assert_eq!(gb["progress"]["current"], 1.0);
    let (_, gm) = a.get(&format!("/goals/{}", id(&minutes))).await;
    assert_eq!(gm["progress"]["current"], 40.0);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn weight_goal_uses_measurements_without_projection(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let g = month_goal(&a, "Llegar a 75 kg", json!({ "progress_type": "current_value", "unit": "kg", "baseline": "80", "target": "75", "direction": "decrease", "progress_source": "weight" })).await;
    a.put("/checkins/2026-11-03", json!({ "weight_kg": "79.5" }))
        .await;
    a.put("/checkins/2026-11-10", json!({ "weight_kg": "78" })).await;
    let (_, d) = a.get(&format!("/goals/{}", id(&g))).await;
    assert_eq!(d["progress"]["current"], 78.0);
    assert_eq!(d["progress"]["remaining"], 3.0);
    assert!(d["progress"]["projection_estimate"].is_null());
    assert!(d["progress"]["expected_by_today"].is_null());
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn goals_and_habits_are_isolated_between_users(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let b = t.login(222).await;
    let g = month_goal(&a, "Privado", json!({})).await;
    let gid = id(&g);
    let (_, h) = a
        .post(
            "/habits",
            json!({ "name": "Privado", "frequency": "daily", "starts_on": "2026-11-01" }),
        )
        .await;
    let hid = id(&h);
    assert_eq!(b.get(&format!("/goals/{gid}")).await.0, StatusCode::NOT_FOUND);
    assert_eq!(b.get(&format!("/habits/{hid}")).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        b.put(
            &format!("/habits/{hid}/entries/2026-11-10"),
            json!({ "status": "done" })
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // B no puede colgar un hábito ni un objetivo semanal del objetivo de A.
    assert_ne!(
        b.post(
            "/habits",
            json!({ "name": "x", "kind": "goal", "goal_id": gid, "frequency": "daily" })
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        b.post(
            "/goals",
            json!({ "kind": "week", "period_key": "2026-W46", "title": "x", "parent_goal_id": gid })
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_ne!(
        b.put("/habit-links/weight_logged", json!({ "habit_id": hid }))
            .await
            .0,
        StatusCode::OK
    );
    let (_, rb) = b.post("/readings", json!({ "title": "Libro B" })).await;
    let (st, _) = a
        .post(
            &format!("/goals/{gid}/links"),
            json!({ "src_type": "reading", "src_id": rb["id"] }),
        )
        .await;
    assert_eq!(
        st,
        StatusCode::UNPROCESSABLE_ENTITY,
        "A no puede enlazar una lectura de B"
    );
    let (_, lb) = b.get("/goals").await;
    assert_eq!(lb.as_array().unwrap().len(), 0);
}
