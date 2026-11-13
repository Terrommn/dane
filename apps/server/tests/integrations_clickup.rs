//! T51: ClickUp por vista configurada, paginación, mapeo de estados, orden de la daily y solo lectura.

mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn task(id: &str, name: &str, status: &str, kind: &str, assignees: &[i64]) -> serde_json::Value {
    json!({ "id": id, "name": name, "url": format!("https://app.clickup.com/t/{id}"), "status": { "status": status, "type": kind },
            "priority": { "priority": "high" }, "assignees": assignees.iter().map(|a| json!({ "id": a })).collect::<Vec<_>>(), "tags": [] })
}

async fn setup(mock: &MockServer, a: &common::Client) {
    Mock::given(method("GET"))
        .and(path("/api/v2/user"))
        .and(header("Authorization", "pk_test_token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "user": { "id": 7, "username": "neil" } })),
        )
        .mount(mock)
        .await;
    let (st, body) = a.put("/integrations/clickup", json!({ "token": "pk_test_token", "view_id": "2ky4g3c0-10613", "endpoints_override": mock.uri() })).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "connected");
    assert_eq!(body["account_label"], "neil");
    assert_eq!(body["config"]["clickup_user_id"], 7);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn syncs_only_configured_view_with_ordering_and_no_duplicates(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    let user: Uuid = a.user_id.parse().unwrap();
    let mock = MockServer::start().await;
    setup(&mock, &a).await;
    let enc: Vec<u8> =
        sqlx::query_scalar("SELECT credentials_enc FROM integration_connections WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !String::from_utf8_lossy(&enc).contains("pk_test_token"),
        "token cifrado"
    );

    Mock::given(method("GET"))
        .and(path("/api/v2/view/2ky4g3c0-10613/task"))
        .and(query_param("page", "0"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "last_page": false, "tasks": [
                task("a1", "Nueva pantalla de reportes", "new", "open", &[7]),
                task("a2", "Config de webhooks", "new", "open", &[]),
                task("a3", "Cambiar texto del botón", "new", "open", &[7]),
                task("a4", "Tarea de otra persona", "new", "open", &[99]),
                task("a5", "Revisar contrato cliente", "need info", "custom", &[7]),
            ]})),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v2/view/2ky4g3c0-10613/task"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "last_page": true, "tasks": [
                task("a6", "Ajustar campos del formulario", "new", "open", &[7]),
                task("a7", "Deploy viejo", "closed", "closed", &[7]),
            ]})),
        )
        .mount(&mock)
        .await;

    for _ in 0..2 {
        let (st, rep) = a.post("/integrations/clickup/sync", json!({})).await;
        assert_eq!(st, StatusCode::OK, "{rep}");
        assert_eq!(rep["upserted"], 6);
    }
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM tasks WHERE user_id = $1 AND external_source = 'clickup'")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        n, 6,
        "IDs estables: resincronizar no duplica; la tarea de otro usuario se excluye"
    );
    let closed: String =
        sqlx::query_scalar("SELECT status FROM tasks WHERE user_id = $1 AND external_id = 'a7'")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(closed, "done");
    let area: String = sqlx::query_scalar("SELECT a.slug FROM tasks t JOIN areas a ON a.id = t.area_id WHERE t.user_id = $1 AND t.external_id = 'a1'").bind(user).fetch_one(&pool).await.unwrap();
    assert_eq!(area, "trabajo");

    let (_, ordered) = a.get("/integrations/clickup/tasks").await;
    let o = ordered.as_array().unwrap();
    let groups: Vec<&str> = o.iter().map(|x| x["group"].as_str().unwrap()).collect();
    assert_eq!(&groups[..3], &["easy", "easy", "easy"], "{ordered}");
    assert!(o[..3].iter().all(|x| x["reason"].as_str().is_some()));
    assert!(o[..3].iter().all(|x| x["title"] != "Nueva pantalla de reportes"));
    assert_eq!(o.last().unwrap()["group"], "need_info");
    assert_eq!(o.last().unwrap()["title"], "Revisar contrato cliente");

    // Si en ClickUp se cierra una tarea, se marca hecha localmente. Marcar en la app nunca escribe en ClickUp.
    mock.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/view/2ky4g3c0-10613/task"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "last_page": true, "tasks": [ task("a3", "Cambiar texto del botón", "complete", "done", &[7]) ]})))
        .mount(&mock)
        .await;
    a.post("/integrations/clickup/sync", json!({})).await;
    let st: String = sqlx::query_scalar("SELECT status FROM tasks WHERE user_id = $1 AND external_id = 'a3'")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(st, "done");
    for r in mock.received_requests().await.unwrap() {
        assert_eq!(r.method.as_str(), "GET", "nunca se modifica ClickUp: {}", r.url);
        assert!(
            r.url.path().starts_with("/api/v2/view/2ky4g3c0-10613") || r.url.path() == "/api/v2/user",
            "solo la vista configurada: {}",
            r.url
        );
    }
    // Aislamiento: el otro usuario no ve estas tareas.
    let b = t.login(222).await;
    let (_, ob) = b.get("/integrations/clickup/tasks").await;
    assert!(ob.as_array().unwrap().is_empty());
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn revoked_token_sets_error_status(pool: PgPool) {
    let t = common::TestApp::new(pool);
    let a = t.login(111).await;
    let mock = MockServer::start().await;
    setup(&mock, &a).await;
    mock.reset().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "err": "Token invalid" })))
        .mount(&mock)
        .await;
    let (st, body) = a.post("/integrations/clickup/sync", json!({})).await;
    assert_eq!(st, StatusCode::BAD_GATEWAY, "{body}");
    let (_, ov) = a.get("/integrations").await;
    assert_eq!(ov["clickup"]["status"], "error");
    assert!(ov["recent_runs"][0]["ok"] == false);
    // Token inválido al conectar: no se guarda la conexión.
    let b = t.login(222).await;
    let (st, _) = b
        .put(
            "/integrations/clickup",
            json!({ "token": "pk_bad", "view_id": "v", "endpoints_override": mock.uri() }),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_GATEWAY);
    let (_, ovb) = b.get("/integrations").await;
    assert_eq!(ovb["clickup"]["status"], "disconnected");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn status_map_is_configurable_and_validated(pool: PgPool) {
    let t = common::TestApp::new(pool.clone());
    let a = t.login(111).await;
    let mock = MockServer::start().await;
    setup(&mock, &a).await;
    let (_, ov) = a.get("/integrations").await;
    let v = ov["clickup"]["version"].as_i64().unwrap();
    let (st, _) = a
        .put(
            "/integrations/clickup",
            json!({ "version": v, "status_map": { "en espera": "bloqueada" } }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, body) = a
        .put(
            "/integrations/clickup",
            json!({ "version": v, "status_map": { "en espera": "need_info", "new": "open" } }),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    Mock::given(method("GET"))
        .and(path("/api/v2/view/2ky4g3c0-10613/task"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "last_page": true, "tasks": [ task("b1", "Algo", "en espera", "custom", &[7]) ]}),
        ))
        .mount(&mock)
        .await;
    a.post("/integrations/clickup/sync", json!({})).await;
    let cat: String =
        sqlx::query_scalar("SELECT external_status_category FROM tasks WHERE external_id = 'b1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cat, "need_info");
    // Versión vieja → conflicto.
    let (st, _) = a
        .put(
            "/integrations/clickup",
            json!({ "version": v, "view_id": "otra" }),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
}
