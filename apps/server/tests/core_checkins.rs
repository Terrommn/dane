//! A-core: check-in diario (T20), devocionales (T28) y vínculos explícitos con hábitos.
//! Reloj de pruebas: 2026-11-12 12:00 America/Monterrey.

mod common;

use axum::http::{Method, StatusCode};
use common::{Client, TestApp};
use serde_json::{Value, json};
use sqlx::PgPool;

async fn habit(a: &Client, name: &str) -> String {
    let (st, h) = a
        .post(
            "/habits",
            json!({ "name": name, "frequency": "daily", "starts_on": "2026-11-01" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{h}");
    h["habit"]["id"].as_str().unwrap().to_string()
}

async fn entries_count(pool: &PgPool, habit: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM habit_entries WHERE habit_id = $1::uuid")
        .bind(habit)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn partial_saves_merge_and_null_is_not_zero(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let (_, empty) = a.get("/checkins/2026-11-12").await;
    assert!(empty["checkin"].is_null());
    assert_eq!(empty["version"], 0);
    // Mañana: solo peso.
    let (st, c) = a.put("/checkins/2026-11-12", json!({ "weight_kg": "72.4", "planned_spend_text": "gasolina 500", "planned_spend_cents": 50000, "morning_done": true })).await;
    assert_eq!(st, StatusCode::OK, "{c}");
    assert_eq!(c["checkin"]["weight_kg"], "72.40");
    assert!(c["checkin"]["energy"].is_null(), "sin dato ≠ 0");
    let v = c["version"].as_i64().unwrap();
    // Noche: energía y reflexión; el peso no se toca.
    let (st, c) = a
        .put("/checkins/2026-11-12", json!({ "version": v, "energy": 4, "went_well": ["Fui al gym"], "went_badly": ["Comí tarde"], "log": ["Reunión con cliente"], "evening_done": true }))
        .await;
    assert_eq!(st, StatusCode::OK, "{c}");
    assert_eq!(c["checkin"]["weight_kg"], "72.40");
    assert_eq!(c["checkin"]["energy"], 4);
    assert!(c["checkin"]["evening_done_at"].is_string());
    assert_eq!(c["went_well"][0]["text"], "Fui al gym");
    assert_eq!(c["log"].as_array().unwrap().len(), 1);
    // `null` explícito borra el peso (vuelve a "sin dato").
    let v = c["version"].as_i64().unwrap();
    let (_, c) = a
        .put("/checkins/2026-11-12", json!({ "version": v, "weight_kg": null }))
        .await;
    assert!(c["checkin"]["weight_kg"].is_null());
    assert_eq!(c["checkin"]["energy"], 4);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM daily_checkins")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1, "mañana y noche actualizan el mismo registro");
    // Validaciones.
    assert_eq!(
        a.put(
            "/checkins/2026-11-12",
            json!({ "version": c["version"], "energy": 6 })
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        a.put("/checkins/2026-11-13", json!({ "energy": 3 })).await.0,
        StatusCode::UNPROCESSABLE_ENTITY,
        "no días futuros"
    );
    // Edición de días anteriores.
    assert_eq!(
        a.put("/checkins/2026-11-01", json!({ "energy": 2 })).await.0,
        StatusCode::OK
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn stale_device_gets_conflict_with_current_state(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (_, c) = a.put("/checkins/2026-11-12", json!({ "energy": 3 })).await;
    let v = c["version"].as_i64().unwrap();
    // Dispositivo 1 guarda.
    assert_eq!(
        a.put("/checkins/2026-11-12", json!({ "version": v, "energy": 4 }))
            .await
            .0,
        StatusCode::OK
    );
    // Dispositivo 2 con la versión vieja: 409 y estado actual, sin sobrescribir.
    let (st, e) = a
        .put("/checkins/2026-11-12", json!({ "version": v, "energy": 1 }))
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(e["error"]["current"]["checkin"]["energy"], 4);
    // Un dispositivo que cree que el día está vacío (versión 0) también recibe conflicto.
    assert_eq!(
        a.put("/checkins/2026-11-12", json!({ "energy": 2 })).await.0,
        StatusCode::CONFLICT
    );
    let (_, now) = a.get("/checkins/2026-11-12").await;
    assert_eq!(now["checkin"]["energy"], 4);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn repeated_checkin_does_not_duplicate_effects(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let h = habit(&a, "Leer").await;
    let body =
        json!({ "energy": 3, "habits": [{ "habit_id": h, "status": "done" }], "went_well": ["Leí 20 min"] });
    let key = [("idempotency-key", "offline-retry-0001")];
    let (s1, b1) = a
        .req(Method::PUT, "/checkins/2026-11-11", Some(body.clone()), &key)
        .await;
    let (s2, b2) = a
        .req(Method::PUT, "/checkins/2026-11-11", Some(body.clone()), &key)
        .await;
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
    assert_eq!(
        b1["version"], b2["version"],
        "el reintento offline devuelve la misma respuesta"
    );
    // Guardar otra vez el mismo contenido con versión correcta: entradas upsert, no duplicadas.
    let v = b1["version"].as_i64().unwrap();
    let mut again = body.clone();
    again["version"] = json!(v);
    assert_eq!(a.put("/checkins/2026-11-11", again).await.0, StatusCode::OK);
    assert_eq!(entries_count(&pool, &h).await, 1);
    let refl: i64 = sqlx::query_scalar("SELECT count(*) FROM reflection_items")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(refl, 1);
    // Status null vuelve a "sin registrar".
    let (_, c) = a.get("/checkins/2026-11-11").await;
    let (st, _) = a
        .put(
            "/checkins/2026-11-11",
            json!({ "version": c["version"], "habits": [{ "habit_id": h, "status": null }] }),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(entries_count(&pool, &h).await, 0);
    // Hábito de otro usuario → 422.
    let b = t.login(222).await;
    let (st, _) = b
        .put(
            "/checkins/2026-11-11",
            json!({ "habits": [{ "habit_id": h, "status": "done" }] }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn explicit_links_mark_a_single_occurrence(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let pesarme = habit(&a, "Pesarme").await;
    let bueno = habit(&a, "Anotar algo bueno").await;
    // Sin vínculo configurado, registrar peso no marca nada (nunca por texto del nombre).
    let (_, c) = a.put("/checkins/2026-11-12", json!({ "weight_kg": "72" })).await;
    assert_eq!(entries_count(&pool, &pesarme).await, 0);
    a.put("/habit-links/weight_logged", json!({ "habit_id": pesarme }))
        .await;
    a.put("/habit-links/positive_reflection", json!({ "habit_id": bueno }))
        .await;
    let (_, c) = a
        .put(
            "/checkins/2026-11-12",
            json!({ "version": c["version"], "weight_kg": "71.8", "went_well": ["Ayudé a mi hermana"] }),
        )
        .await;
    let (_, c) = a
        .put(
            "/checkins/2026-11-12",
            json!({ "version": c["version"], "weight_kg": "71.7" }),
        )
        .await;
    assert_eq!(entries_count(&pool, &pesarme).await, 1);
    assert_eq!(entries_count(&pool, &bueno).await, 1);
    let st: String = sqlx::query_scalar(
        "SELECT status FROM habit_entries WHERE habit_id = $1::uuid AND on_date = '2026-11-12'",
    )
    .bind(&pesarme)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(st, "done");
    // Vaciar la reflexión positiva retira la marca automática.
    a.put(
        "/checkins/2026-11-12",
        json!({ "version": c["version"], "went_well": [] }),
    )
    .await;
    assert_eq!(entries_count(&pool, &bueno).await, 0);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn devotional_with_real_content_completes_linked_habit(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let (_, fixed) = a
        .post(
            "/habits/fixed",
            json!({ "devotional": { "enabled": true, "frequency": "daily" }, "starts_on": "2026-11-01" }),
        )
        .await;
    let dev = fixed[0]["id"].as_str().unwrap().to_string();
    a.put("/habit-links/devotional_content", json!({ "habit_id": dev }))
        .await;
    // Devocional vacío: no completa.
    let (st, d) = a
        .put(
            "/devotionals/2026-11-12",
            json!({ "passage": "  ", "verse_actions": [{ "verse": "", "action": "" }] }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{d}");
    assert_eq!(d["verse_actions"].as_array().unwrap().len(), 0);
    assert_eq!(entries_count(&pool, &dev).await, 0);
    // Con contenido real: completa la ocurrencia de ese día.
    let (st, d) = a
        .put("/devotionals/2026-11-12", json!({ "version": d["version"], "passage": "Juan 15:1-8", "verse_actions": [{ "verse": "15:5", "action": "Orar antes de trabajar" }], "learned": "Permanecer" }))
        .await;
    assert_eq!(st, StatusCode::OK, "{d}");
    assert_eq!(entries_count(&pool, &dev).await, 1);
    // Versión vieja → 409.
    assert_eq!(
        a.put("/devotionals/2026-11-12", json!({ "version": 1, "passage": "x" }))
            .await
            .0,
        StatusCode::CONFLICT
    );
    // Historial y búsqueda sin acentos.
    a.put(
        "/devotionals/2026-11-10",
        json!({ "passage": "Salmo 23", "prayer": "Gracias por la canción" }),
    )
    .await;
    let (_, page) = a.get("/devotionals?q=cancion").await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["on_date"], "2026-11-10");
    let (_, all) = a.get("/devotionals").await;
    assert_eq!(all["items"][0]["on_date"], "2026-11-12", "orden por fecha desc");
    // Plantilla por defecto del vault y modo meditación desde ajustes.
    let (_, tpl) = a.get("/devotionals/template").await;
    assert_eq!(tpl["sections"].as_array().unwrap().len(), 6);
    let (_, s) = a.get("/settings").await;
    a.put(
        "/settings",
        json!({ "version": s["version"], "devotional_mode": "meditation" }),
    )
    .await;
    let (_, tpl) = a.get("/devotionals/template").await;
    assert_eq!(tpl["mode"], "meditation");
    // Borrar retira la marca automática.
    assert_eq!(
        a.delete("/devotionals/2026-11-12").await.0,
        StatusCode::NO_CONTENT
    );
    let on_12: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM habit_entries WHERE habit_id = $1::uuid AND on_date = '2026-11-12'",
    )
    .bind(&dev)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(on_12, 0);
    assert_eq!(
        entries_count(&pool, &dev).await,
        1,
        "el devocional del día 10 conserva su ocurrencia"
    );
    // Futuro → 422.
    assert_eq!(
        a.put("/devotionals/2026-11-20", json!({ "passage": "x" }))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn day_view_groups_habits_and_derives_log_without_persisting(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let user = a.user_id.clone();
    a.post("/habits/fixed", json!({ "exercise": { "enabled": true, "frequency": "times_per_week", "times_per_week": 4 }, "starts_on": "2026-11-01" })).await;
    let (_, m) = a
        .post(
            "/goals",
            json!({ "kind": "month", "period_key": "2026-11", "title": "Mes" }),
        )
        .await;
    let (_, w) = a.post("/goals", json!({ "kind": "week", "period_key": "2026-W46", "title": "Semana", "parent_goal_id": m["goal"]["id"] })).await;
    a.post("/habits", json!({ "name": "Escribir 30 min", "kind": "goal", "goal_id": m["goal"]["id"], "frequency": "daily" })).await;
    a.post("/habits", json!({ "name": "Llamar a 1 cliente", "kind": "goal", "goal_id": w["goal"]["id"], "frequency": "daily" })).await;
    // Datos de otros módulos insertados directamente (agenda, tareas, gasto, interacción).
    sqlx::query("INSERT INTO calendar_events (user_id, on_date, starts_at, ends_at, title) VALUES ($1::uuid, '2026-11-12', '2026-11-12 15:00Z', '2026-11-12 16:00Z', 'Clase'), ($1::uuid, '2026-11-12', '2026-11-12 23:00Z', '2026-11-12 23:30Z', 'Cena')")
        .bind(&user)
        .execute(&pool)
        .await
        .unwrap();
    let area: String =
        sqlx::query_scalar("SELECT id::text FROM areas WHERE user_id = $1::uuid AND slug = 'trabajo'")
            .bind(&user)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO tasks (user_id, title, area_id, planned_on) VALUES ($1::uuid, 'Enviar reporte', $2::uuid, '2026-11-11'), ($1::uuid, 'Futuro', $2::uuid, '2026-11-20')")
        .bind(&user)
        .bind(&area)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tasks (user_id, title, area_id, planned_on, status, done_at) VALUES ($1::uuid, 'Pagar luz', $2::uuid, '2026-11-12', 'done', '2026-11-12 17:00Z')")
        .bind(&user)
        .bind(&area)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO transactions (user_id, kind, occurred_on, concept, amount_cents) VALUES ($1::uuid, 'expense', '2026-11-12', 'Comida', 18050), ($1::uuid, 'expense', '2026-11-11', 'Ayer', 999)")
        .bind(&user)
        .execute(&pool)
        .await
        .unwrap();
    let pid: String = sqlx::query_scalar(
        "INSERT INTO people (user_id, name) VALUES ($1::uuid, 'Ana López') RETURNING id::text",
    )
    .bind(&user)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO interactions (user_id, person_id, on_date, topic) VALUES ($1::uuid, $2::uuid, '2026-11-12', 'comida')").bind(&user).bind(&pid).execute(&pool).await.unwrap();

    let (st, v) = a.get("/checkins/2026-11-12/view").await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["is_open"], true);
    assert_eq!(v["habits"]["fixed"].as_array().unwrap().len(), 1);
    assert_eq!(v["habits"]["fixed"][0]["progress"]["week_target"], 4);
    assert_eq!(v["habits"]["month"].as_array().unwrap().len(), 1);
    assert_eq!(v["habits"]["week"].as_array().unwrap().len(), 1);
    assert_eq!(v["habits"]["week"][0]["goal_title"], "Semana");
    assert_eq!(v["agenda"].as_array().unwrap().len(), 2);
    assert_eq!(
        v["derived"]["past_events"].as_array().unwrap().len(),
        1,
        "solo la clase ya terminó a las 12:00"
    );
    assert_eq!(v["derived"]["expenses_total_cents"], 18050);
    assert_eq!(v["derived"]["closed_tasks"][0]["title"], "Pagar luz");
    let titles: Vec<&str> = v["tasks_by_area"][0]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert!(
        titles.contains(&"Enviar reporte") && titles.contains(&"Pagar luz") && !titles.contains(&"Futuro"),
        "{titles:?}"
    );
    assert_eq!(v["people"][0]["person_name"], "Ana López");
    // Derivados no se persistieron en el log ni crearon tareas.
    let logs: i64 = sqlx::query_scalar("SELECT count(*) FROM daily_log_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(logs, 0);
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tasks, 3);
    // Otro usuario no ve nada.
    let b = t.login(222).await;
    let (_, vb) = b.get("/checkins/2026-11-12/view").await;
    assert_eq!(vb["agenda"].as_array().unwrap().len(), 0);
    assert_eq!(vb["derived"]["expenses_total_cents"], 0);
    let _: Value = vb;
}
