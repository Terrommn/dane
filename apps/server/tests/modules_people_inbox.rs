//! T27 personas (alias, cumpleaños, interacciones, sugerencias) y T25 Inbox (confirmación explícita).

mod common;

use axum::http::{Method, StatusCode};
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn people_aliases_birthdays_and_last_contact(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let (st, juan) = a
        .post("/people", json!({ "name": "Juan Pérez", "aliases": ["Juancho"], "relationship": "amigo", "birthday": "11-20", "context_md": "Amigo de la prepa" }))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{juan}");
    let id = juan["id"].as_str().unwrap();
    assert_eq!(juan["birthday_year"], serde_json::Value::Null);
    let (st, _) = a
        .post("/people", json!({ "name": "X", "relationship": "enemigo" }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, _) = a
        .post("/people", json!({ "name": "X", "birthday": "02-30" }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, ana) = a
        .post(
            "/people",
            json!({ "name": "Ána López", "birthday": "1990-11-13" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);

    // Resolución por alias y búsqueda sin acentos.
    let (_, r) = a.get("/people/resolve?name=juancho").await;
    assert_eq!(r[0]["id"], id);
    let (_, r) = a.get("/people?q=ana").await;
    assert_eq!(r.as_array().unwrap().len(), 1);

    // Interacciones: último contacto = máximo; días sin hablar.
    for (date, topic) in [
        ("2026-11-02", "café"),
        ("2026-11-10", "llamada"),
        ("2026-11-05", "boda"),
    ] {
        let (st, i) = a
            .post(
                &format!("/people/{id}/interactions"),
                json!({ "on_date": date, "topic": topic }),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{i}");
    }
    let (_, det) = a.get(&format!("/people/{id}")).await;
    assert_eq!(det["person"]["last_contact_on"], "2026-11-10");
    assert_eq!(det["person"]["days_since_contact"], 2);
    assert_eq!(det["person"]["days_to_birthday"], 8);
    assert_eq!(det["interactions"].as_array().unwrap().len(), 3);
    // Misma referencia externa no duplica.
    for _ in 0..2 {
        a.post(
            &format!("/people/{id}/interactions"),
            json!({ "on_date": "2026-11-11", "topic": "correo", "source": "gmail", "source_ref": "msg-1" }),
        )
        .await;
    }
    let (_, day) = a.get("/interactions?date=2026-11-11").await;
    assert_eq!(day.as_array().unwrap().len(), 1);
    // Borrar la interacción más reciente recalcula el último contacto.
    let latest = day[0]["id"].as_str().unwrap();
    assert_eq!(
        a.delete(&format!("/interactions/{latest}")).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, det) = a.get(&format!("/people/{id}")).await;
    assert_eq!(det["person"]["last_contact_on"], "2026-11-10");

    // Notas cronológicas.
    let (st, _) = a
        .post(
            &format!("/people/{id}/notes"),
            json!({ "text": "Le gusta el golf" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);

    // Cumpleaños próximos: Ana (13 nov, cumple 36) antes que Juan (20 nov).
    let (_, b) = a.get("/people/birthdays?days=30").await;
    assert_eq!(b[0]["id"], ana["id"]);
    assert_eq!(b[0]["turning"], 36);
    assert_eq!(b[1]["id"], id);

    // Editar sin enviar `birthday` no lo borra.
    let (st, upd) = a
        .put(
            &format!("/people/{id}"),
            json!({ "name": "Juan Pérez", "aliases": ["Juancho", "Juan"], "version": juan["version"] }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{upd}");
    let (_, det) = a.get(&format!("/people/{id}")).await;
    assert_eq!(det["person"]["birthday_month"], 11);

    // Vista agrupada por área (Personas.base).
    let (_, grouped) = a.get("/people/by-area").await;
    assert!(
        grouped
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["area_name"] == "Sin área")
    );

    // Aislamiento.
    let b2 = t.login(222).await;
    assert_eq!(b2.get(&format!("/people/{id}")).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        b2.post(&format!("/people/{id}/interactions"), json!({ "topic": "x" }))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(
        b2.get("/people/resolve?name=juancho")
            .await
            .1
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn suggestions_never_create_people_silently(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    let body = json!({ "name": "Laura Ruiz", "email": "laura@x.com", "source": "calendar", "source_ref": "evt-1", "on_date": "2026-11-12", "context": "Junta de producto" });
    let (st, s1) = a.post("/person-suggestions", body.clone()).await;
    assert_eq!(st, StatusCode::CREATED);
    a.post("/person-suggestions", body).await;
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM people")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(people, 0, "una sugerencia no crea personas");
    let (_, pending) = a.get("/person-suggestions").await;
    assert_eq!(pending.as_array().unwrap().len(), 1, "misma fuente no duplica");

    let sid = s1["id"].as_str().unwrap();
    let (st, acc) = a
        .post(
            &format!("/person-suggestions/{sid}/accept"),
            json!({ "person": { "name": "Laura Ruiz", "email": "laura@x.com" }, "record_interaction": true }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{acc}");
    assert_eq!(acc["created"], true);
    assert_eq!(acc["person"]["last_contact_on"], "2026-11-12");
    // Aceptar dos veces no crea otra.
    let (st, _) = a
        .post(
            &format!("/person-suggestions/{sid}/accept"),
            json!({ "person": { "name": "Laura Ruiz" } }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (_, s2) = a
        .post(
            "/person-suggestions",
            json!({ "name": "Spam", "source": "gmail", "source_ref": "m-9" }),
        )
        .await;
    assert_eq!(
        a.post(
            &format!("/person-suggestions/{}/dismiss", s2["id"].as_str().unwrap()),
            json!({})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM people")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(people, 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn inbox_requires_confirmation_and_is_idempotent(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    let (st, item) = a
        .post(
            "/inbox",
            json!({ "text": "Entregar tarea de cálculo el viernes", "client_id": "dev-1-0001" }),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    let (st, again) = a
        .post(
            "/inbox",
            json!({ "text": "Entregar tarea de cálculo el viernes", "client_id": "dev-1-0001" }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "reenvío offline no duplica");
    assert_eq!(again["id"], item["id"]);
    let id = item["id"].as_str().unwrap();

    // Proponer no mueve nada.
    let (_, proposed) = a.post(&format!("/inbox/{id}/propose"), json!({})).await;
    assert_eq!(proposed["status"], "pending");
    assert_eq!(proposed["proposal"]["destination"], "task");
    assert_eq!(proposed["proposal"]["area_slug"], "escuela");
    assert!(proposed["proposal"]["area_id"].is_string());
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tasks, 0);

    // Aplicar con versión vieja → 409; con la vigente crea la tarea y cierra el elemento.
    let action = json!({ "type": "task", "title": "Entregar tarea de cálculo", "area_id": proposed["proposal"]["area_id"], "due_on": "2026-11-13" });
    let (st, _) = a
        .post(
            &format!("/inbox/{id}/apply"),
            json!({ "version": 1, "action": action }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let (st, res) = a
        .post(
            &format!("/inbox/{id}/apply"),
            json!({ "version": proposed["version"], "action": action }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{res}");
    assert_eq!(res["item"]["status"], "processed");
    assert_eq!(res["result_type"], "task");
    let (_, task) = a
        .get(&format!("/tasks/{}", res["result_id"].as_str().unwrap()))
        .await;
    assert_eq!(task["task"]["due_on"], "2026-11-13");
    // Reaplicar no crea otra tarea.
    let (st, _) = a
        .post(
            &format!("/inbox/{id}/apply"),
            json!({ "version": res["item"]["version"], "action": action }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tasks, 1);

    // Archivar y recuperar.
    let (_, n) = a
        .post("/inbox", json!({ "text": "https://youtu.be/x buen hook" }))
        .await;
    let (_, r) = a
        .post(
            &format!("/inbox/{}/apply", n["id"].as_str().unwrap()),
            json!({ "version": 1, "action": { "type": "archive" } }),
        )
        .await;
    assert_eq!(r["item"]["status"], "archived");
    let (_, pending) = a.get("/inbox").await;
    assert!(pending.as_array().unwrap().is_empty());
    let (_, back) = a
        .post(&format!("/inbox/{}/reopen", n["id"].as_str().unwrap()), json!({}))
        .await;
    assert_eq!(back["status"], "pending");

    // Aislamiento.
    let b = t.login(222).await;
    let (st, _) = b
        .req(
            Method::POST,
            &format!("/inbox/{id}/propose"),
            Some(json!({})),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert!(b.get("/inbox?status=all").await.1.as_array().unwrap().is_empty());
}
