//! T58: datos demo opt-in, aislados y prohibidos cuando DEMO_ENABLED=false.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn demo_space_is_isolated_and_populated(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let real = t.login(111).await;
    let (st, body) = t
        .raw(
            Method::POST,
            "/api/v1/demo/session",
            Some(json!({"transport": "bearer"})),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_ne!(body["user_id"], real.user_id.as_str());
    let demo = common::Client {
        app: t.app.clone(),
        token: body["token"].as_str().unwrap().into(),
        user_id: body["user_id"].as_str().unwrap().into(),
    };
    let (_, me) = demo.get("/me").await;
    assert_eq!(me["user"]["is_demo"], true);
    let (st, dash) = demo.get("/dashboard?date=2026-11-12").await;
    assert_eq!(st, StatusCode::OK, "{dash}");
    // Ejercicio y lectura (el demo ya no crea "Devocional", D-039).
    assert_eq!(dash["habits"]["fixed"].as_array().unwrap().len(), 2);
    assert!(dash["finance"]["month"]["consumption_cents"].as_i64().unwrap() > 0);
    // El usuario real no recibe nada de la demo.
    let (_, goals) = real.get("/goals").await;
    assert!(goals.as_array().unwrap().is_empty());
    let demo_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM transactions t JOIN users u ON u.id = t.user_id WHERE NOT u.is_demo",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(demo_rows, 0);
    // Reset recrea sin duplicar usuarios demo.
    let (st, again) = t
        .raw(
            Method::POST,
            "/api/v1/demo/session",
            Some(json!({"reset": true, "transport": "bearer"})),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_ne!(again["user_id"], body["user_id"]);
    let demos: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE is_demo")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(demos, 1);
    // La función de siembra se niega con un usuario real.
    let real_id: uuid::Uuid = real.user_id.parse().unwrap();
    assert!(
        sqlx::query("SELECT seed_demo($1, '2026-11-12')")
            .bind(real_id)
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn demo_disabled_returns_404(pool: PgPool) {
    let t = TestApp::with_config(pool, |c| c.demo_enabled = false);
    let (st, _) = t
        .raw(Method::POST, "/api/v1/demo/session", Some(json!({})), &[])
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}
