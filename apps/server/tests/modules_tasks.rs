//! T24: tareas, arrastre que conserva ID e historial, vista diaria por área, recurrentes idempotentes y externas.

mod common;

use axum::http::StatusCode;
use chrono::{NaiveDate, TimeZone, Utc};
use domain::time::Clock;
use serde_json::json;
use sqlx::PgPool;

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn carry_over_keeps_id_and_history_without_duplicates(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    let (_, areas) = a.get("/areas").await;
    let trabajo = areas
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["slug"] == "trabajo")
        .unwrap()["id"]
        .clone();
    let (st, task) = a.post("/tasks", json!({ "title": "Enviar reporte", "area_id": trabajo, "planned_on": "2026-11-10", "priority": 2 })).await;
    assert_eq!(st, StatusCode::CREATED, "{task}");
    let id = task["id"].as_str().unwrap().to_string();
    let (_, done) = a
        .post(
            "/tasks",
            json!({ "title": "Ya hecha", "planned_on": "2026-11-10" }),
        )
        .await;
    a.post(
        &format!("/tasks/{}/complete", done["id"].as_str().unwrap()),
        json!({}),
    )
    .await;

    for to in ["2026-11-11", "2026-11-12", "2026-11-12"] {
        let (st, r) = a.post("/tasks/carry-over", json!({ "to_date": to })).await;
        assert_eq!(st, StatusCode::OK, "{r}");
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE title = 'Enviar reporte'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "el arrastre nunca crea tareas nuevas");
    let (_, detail) = a.get(&format!("/tasks/{id}")).await;
    assert_eq!(detail["task"]["id"], id.as_str());
    assert_eq!(detail["task"]["planned_on"], "2026-11-12");
    let carried: Vec<_> = detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "carried_over")
        .collect();
    assert_eq!(carried.len(), 2, "un evento por arrastre efectivo: {detail}");
    assert_eq!(carried[0]["from_date"], "2026-11-10");
    assert_eq!(carried[1]["to_date"], "2026-11-12");
    // La completada no se arrastra.
    let (_, d2) = a.get(&format!("/tasks/{}", done["id"].as_str().unwrap())).await;
    assert_eq!(d2["task"]["planned_on"], "2026-11-10");

    // Vista del día agrupada por área.
    let (_, day) = a.get("/tasks/day/2026-11-12").await;
    let groups = day["groups"].as_array().unwrap();
    let g = groups.iter().find(|g| g["area_name"] == "Trabajo").unwrap();
    assert_eq!(g["tasks"][0]["id"], id.as_str());
    assert_eq!(day["open_count"], 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn complete_reopen_and_version_conflict(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let (_, task) = a.post("/tasks", json!({ "title": "Pagar luz" })).await;
    let id = task["id"].as_str().unwrap();
    let (st, done) = a
        .post(&format!("/tasks/{id}/complete"), json!({ "version": 1 }))
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(done["status"], "done");
    assert!(done["done_at"].is_string());
    let (st, _) = a
        .post(&format!("/tasks/{id}/reopen"), json!({ "version": 1 }))
        .await;
    assert_eq!(st, StatusCode::CONFLICT, "versión vieja");
    let (_, reopened) = a.post(&format!("/tasks/{id}/reopen"), json!({})).await;
    assert_eq!(reopened["status"], "open");
    assert!(reopened["done_at"].is_null());
    let (_, detail) = a.get(&format!("/tasks/{id}")).await;
    let kinds: Vec<_> = detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(kinds, vec!["created", "completed", "reopened"]);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn completing_external_task_is_local_only(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    let uid: uuid::Uuid = a.user_id.parse().unwrap();
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tasks (user_id, title, external_source, external_id, external_url, external_status, external_status_category, planned_on)
         VALUES ($1, 'Config de pipeline', 'clickup', 'abc123', 'https://app.clickup.com/t/abc123', 'new', 'open', '2026-11-12') RETURNING id",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (st, done) = a.post(&format!("/tasks/{id}/complete"), json!({})).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(done["status"], "done");
    assert_eq!(
        done["external_status"], "new",
        "no se modifica el estado remoto ni se llama a ClickUp"
    );
    let (_, list) = a.get("/tasks?external=true&status=done").await;
    assert_eq!(list["total"], 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn task_people_must_belong_to_user(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let b = t.login(222).await;
    let (_, pb) = b.post("/people", json!({ "name": "Ana de B" })).await;
    let (st, body) = a
        .post("/tasks", json!({ "title": "Llamar", "person_ids": [pb["id"]] }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (_, pa) = a.post("/people", json!({ "name": "Ana" })).await;
    let (st, task) = a
        .post("/tasks", json!({ "title": "Llamar", "person_ids": [pa["id"]] }))
        .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(task["person_ids"][0], pa["id"]);
    assert_eq!(
        b.get(&format!("/tasks/{}", task["id"].as_str().unwrap())).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, lb) = b.get("/tasks").await;
    assert_eq!(lb["total"], 0);
}

async fn rule_dates(pool: &PgPool, title: &str) -> Vec<(String, NaiveDate)> {
    sqlx::query_as("SELECT recurrence_period, planned_on FROM tasks WHERE title = $1 ORDER BY planned_on")
        .bind(title)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn recurrences_short_months_last_penultimate_and_catch_up(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    // Enero de 2028 (año bisiesto).
    t.clock.set(Utc.with_ymd_and_hms(2028, 1, 15, 18, 0, 0).unwrap());
    let a = t.login(111).await;
    for (title, kind, day) in [
        ("Factura 31", "day", Some(31)),
        ("Factura 29", "day", Some(29)),
        ("Último", "last", None),
        ("Penúltimo", "penultimate", None),
    ] {
        let (st, r) = a
            .post(
                "/task-recurrences",
                json!({ "title": title, "day_kind": kind, "day": day, "starts_on": "2028-01-01" }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{r}");
    }
    let (st, _) = a
        .post(
            "/task-recurrences",
            json!({ "title": "Mal", "day_kind": "day", "day": 32 }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        rule_dates(&pool, "Factura 31").await.is_empty(),
        "aún no llega el 31"
    );

    // Febrero bisiesto: 29 existe; penúltimo = 28; día 31 → 29.
    t.clock.set(Utc.with_ymd_and_hms(2028, 2, 29, 18, 0, 0).unwrap());
    // La sesión Bearer (12 h) venció al avanzar el reloj: se vuelve a iniciar sesión.
    let a = t.login(111).await;
    assert_eq!(
        a.post("/task-recurrences/materialize", json!({})).await.0,
        StatusCode::OK
    );
    assert_eq!(
        rule_dates(&pool, "Factura 31").await,
        vec![
            ("2028-01".into(), d("2028-01-31")),
            ("2028-02".into(), d("2028-02-29"))
        ]
    );
    assert_eq!(
        rule_dates(&pool, "Factura 29").await.last().unwrap().1,
        d("2028-02-29")
    );
    assert_eq!(
        rule_dates(&pool, "Penúltimo").await.last().unwrap().1,
        d("2028-02-28")
    );
    assert_eq!(
        rule_dates(&pool, "Último").await.last().unwrap().1,
        d("2028-02-29")
    );

    // Febrero no bisiesto (2029): día 29/31 → 28, penúltimo → 27. Servidor apagado de feb a marzo: catch-up.
    t.clock.set(Utc.with_ymd_and_hms(2029, 3, 3, 18, 0, 0).unwrap());
    let a = t.login(111).await;
    assert_eq!(
        a.post("/task-recurrences/materialize", json!({})).await.0,
        StatusCode::OK
    );
    assert!(
        rule_dates(&pool, "Factura 29")
            .await
            .contains(&("2029-02".into(), d("2029-02-28")))
    );
    assert!(
        rule_dates(&pool, "Penúltimo")
            .await
            .contains(&("2029-02".into(), d("2029-02-27")))
    );
    assert!(
        rule_dates(&pool, "Factura 31")
            .await
            .contains(&("2029-02".into(), d("2029-02-28")))
    );

    // Repetido y concurrente no duplica.
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (r1, r2) = tokio::join!(
        a.post("/task-recurrences/materialize", json!({})),
        a.post("/task-recurrences/materialize", json!({}))
    );
    assert_eq!((r1.0, r2.0), (StatusCode::OK, StatusCode::OK));
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    let dups: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT recurrence_rule_id, recurrence_period FROM tasks WHERE recurrence_rule_id IS NOT NULL
         GROUP BY 1, 2 HAVING count(*) > 1) x",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dups, 0);
    assert!(t.clock.now() > Utc.with_ymd_and_hms(2029, 1, 1, 0, 0, 0).unwrap());
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn new_rule_does_not_backfill_and_editing_keeps_generated(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    // Hoy es 2026-11-12: una regla del día 5 creada hoy no genera la del día 5 pasado.
    let (_, r) = a
        .post(
            "/task-recurrences",
            json!({ "title": "Invoice", "day_kind": "day", "day": 5 }),
        )
        .await;
    assert!(rule_dates(&pool, "Invoice").await.is_empty());
    let (_, r12) = a
        .post(
            "/task-recurrences",
            json!({ "title": "Hoy", "day_kind": "day", "day": 12 }),
        )
        .await;
    assert_eq!(rule_dates(&pool, "Hoy").await.len(), 1, "la de hoy sí aparece");
    let (st, upd) = a
        .put(
            &format!("/task-recurrences/{}", r12["id"].as_str().unwrap()),
            json!({ "title": "Hoy editada", "day_kind": "last", "active": false, "version": 1 }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{upd}");
    assert_eq!(
        rule_dates(&pool, "Hoy").await.len(),
        1,
        "editar la regla no reescribe tareas generadas"
    );
    let _ = r;
}
