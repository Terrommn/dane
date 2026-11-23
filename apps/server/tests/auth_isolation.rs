//! T11–T13, T16, T21: autenticación, allowlist, CSRF, aislamiento entre usuarios, versiones e idempotencia.

mod common;

use axum::http::{Method, StatusCode};
use common::{TestApp, send};
use serde_json::json;
use server::auth::miniapp::sign_init_data_for_tests;
use sqlx::PgPool;

fn init_data(t: &TestApp, user_id: i64, age_secs: i64, token: &str) -> String {
    let auth_date = (t.clock_now_ts() - age_secs).to_string();
    sign_init_data_for_tests(
        &[
            ("auth_date", &auth_date),
            ("user", &format!(r#"{{"id":{user_id},"first_name":"Usuario"}}"#)),
            ("start_param", "checkin_2026-11-11"),
        ],
        token,
    )
}

trait ClockTs {
    fn clock_now_ts(&self) -> i64;
}
impl ClockTs for TestApp {
    fn clock_now_ts(&self) -> i64 {
        use domain::time::Clock;
        self.clock.now().timestamp()
    }
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn miniapp_login_validates_signature_freshness_and_allowlist(pool: PgPool) {
    let t = TestApp::new(pool);
    let token = "123456:TEST-TOKEN";

    let (st, body) = t
        .raw(
            Method::POST,
            "/api/v1/auth/telegram/miniapp",
            Some(json!({ "init_data": init_data(&t, 111, 10, token) })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(body["start_param"], "checkin_2026-11-11");
    let bearer = format!("Bearer {}", body["token"].as_str().unwrap());
    let (st, me) = send(
        &t.app,
        Method::GET,
        "/api/v1/me",
        None,
        &[("authorization", &bearer)],
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(me["identities"][0]["telegram_user_id"], 111);
    assert_eq!(me["settings"]["timezone"], "America/Monterrey");
    assert_eq!(me["settings"]["currency"], "MXN");

    // Firma con otro token de bot.
    let (st, _) = t
        .raw(
            Method::POST,
            "/api/v1/auth/telegram/miniapp",
            Some(json!({ "init_data": init_data(&t, 111, 10, "999:OTHER") })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    // initData viejo (2 h > 1 h permitida en pruebas).
    let (st, body) = t
        .raw(
            Method::POST,
            "/api/v1/auth/telegram/miniapp",
            Some(json!({ "init_data": init_data(&t, 111, 7200, token) })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "expired");
    // Usuario válido pero fuera de la allowlist.
    let (st, _) = t
        .raw(
            Method::POST,
            "/api/v1/auth/telegram/miniapp",
            Some(json!({ "init_data": init_data(&t, 999, 10, token) })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    // initDataUnsafe-like: un JSON de usuario sin firma.
    let (st, _) = t
        .raw(
            Method::POST,
            "/api/v1/auth/telegram/miniapp",
            Some(json!({ "init_data": "user=%7B%22id%22%3A111%7D&auth_date=1" })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn same_telegram_identity_maps_to_one_user_across_surfaces(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let (_, body) = t
        .raw(
            Method::POST,
            "/api/v1/auth/telegram/miniapp",
            Some(json!({ "init_data": init_data(&t, 111, 5, "123456:TEST-TOKEN") })),
            &[],
        )
        .await;
    let miniapp_user = body["user_id"].as_str().unwrap().to_string();
    // Simula la vinculación OIDC: sub distinto, mismo claim `id` verificado.
    let oidc_user = persistence::repos::identity::upsert_verified_identity(
        &pool,
        &persistence::repos::identity::VerifiedIdentity {
            provider: "telegram_oidc",
            subject: "sub-1234123412341234123".into(),
            telegram_user_id: Some(111),
            username: None,
            first_name: None,
            last_name: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(oidc_user.to_string(), miniapp_user);
    let ids: i64 = sqlx::query_scalar("SELECT count(*) FROM identities WHERE user_id = $1")
        .bind(oidc_user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ids, 2);
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users, 1);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn unauthenticated_and_revoked_sessions_are_rejected(pool: PgPool) {
    let t = TestApp::new(pool);
    let (st, body) = t.raw(Method::GET, "/api/v1/areas", None, &[]).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
    let a = t.login(111).await;
    assert_eq!(a.get("/areas").await.0, StatusCode::OK);
    assert_eq!(a.post("/auth/logout", json!({})).await.0, StatusCode::NO_CONTENT);
    assert_eq!(a.get("/areas").await.0, StatusCode::UNAUTHORIZED);
    let (st, _) = t
        .raw(
            Method::GET,
            "/api/v1/areas",
            None,
            &[("authorization", "Bearer forged")],
        )
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn cookie_sessions_require_matching_csrf(pool: PgPool) {
    let t = TestApp::new(pool);
    let (st, body) = t
        .raw(
            Method::POST,
            "/api/v1/auth/dev-login",
            Some(json!({ "telegram_id": 111 })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let csrf = body["csrf"].as_str().unwrap().to_string();
    // Recuperar el token de cookie desde la BD no es posible (solo hash): se obtiene de Set-Cookie.
    let req = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/dev-login")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(json!({ "telegram_id": 111 }).to_string()))
        .unwrap();
    use tower::ServiceExt;
    let resp = t.app.clone().oneshot(req).await.unwrap();
    let cookies: Vec<String> = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("dane_session=") && c.contains("HttpOnly") && c.contains("SameSite=Lax"))
    );
    let session = cookies
        .iter()
        .find(|c| c.starts_with("dane_session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let csrf_cookie = cookies
        .iter()
        .find(|c| c.starts_with("dane_csrf="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let csrf2 = csrf_cookie.trim_start_matches("dane_csrf=").to_string();
    assert_ne!(csrf, csrf2, "cada sesión tiene su propio CSRF");
    let cookie_header = format!("{session}; {csrf_cookie}");

    // GET con cookie: OK sin CSRF.
    let (st, _) = t
        .raw(Method::GET, "/api/v1/areas", None, &[("cookie", &cookie_header)])
        .await;
    assert_eq!(st, StatusCode::OK);
    // POST sin header CSRF: 403.
    let (st, _) = t
        .raw(
            Method::POST,
            "/api/v1/areas",
            Some(json!({ "name": "Salud" })),
            &[("cookie", &cookie_header)],
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    // POST con CSRF de otra sesión: 403.
    let (st, _) = t
        .raw(
            Method::POST,
            "/api/v1/areas",
            Some(json!({ "name": "Salud" })),
            &[("cookie", &cookie_header), ("x-csrf-token", &csrf)],
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    // POST con CSRF correcto: 201.
    let (st, _) = t
        .raw(
            Method::POST,
            "/api/v1/areas",
            Some(json!({ "name": "Salud" })),
            &[("cookie", &cookie_header), ("x-csrf-token", &csrf2)],
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn users_cannot_read_or_link_each_others_data(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let b = t.login(222).await;
    assert_ne!(a.user_id, b.user_id);

    let (_, areas_a) = a.get("/areas").await;
    assert_eq!(
        areas_a.as_array().unwrap().len(),
        4,
        "áreas iniciales Personal/Trabajo/Negocio/Escuela"
    );
    let area_a = areas_a[0]["id"].as_str().unwrap().to_string();

    assert_eq!(b.get(&format!("/areas/{area_a}")).await.0, StatusCode::NOT_FOUND);
    let (st, _) = b
        .put(
            &format!("/areas/{area_a}"),
            json!({ "name": "hack", "version": 1 }),
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // B intenta crear un proyecto apuntando al área de A → rechazado por FK compuesta.
    let (st, body) = b
        .post("/projects", json!({ "name": "Robo", "area_id": area_a }))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "invalid_reference");
    let (_, list_b) = b.get("/areas").await;
    assert!(
        list_b
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["id"] != area_a.as_str())
    );
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn rename_keeps_id_and_stale_version_conflicts(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (st, area) = a.post("/areas", json!({ "name": "Fe y familia" })).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(area["slug"], "fe-y-familia");
    let id = area["id"].as_str().unwrap();
    let (st, renamed) = a
        .put(&format!("/areas/{id}"), json!({ "name": "Fe", "version": 1 }))
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(renamed["id"], id);
    assert_eq!(renamed["version"], 2);
    // Otro dispositivo con versión vieja: 409 con el estado actual, sin sobrescribir.
    let (st, body) = a
        .put(&format!("/areas/{id}"), json!({ "name": "Viejo", "version": 1 }))
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(body["error"]["current"]["name"], "Fe");
    let (_, now) = a.get(&format!("/areas/{id}")).await;
    assert_eq!(now["name"], "Fe");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn idempotency_key_replays_and_detects_mismatch(pool: PgPool) {
    let t = TestApp::new(pool.clone());
    let a = t.login(111).await;
    let k = [("idempotency-key", "retry-key-0001")];
    let (s1, b1) = a
        .req(Method::POST, "/projects", Some(json!({ "name": "Tesis" })), &k)
        .await;
    let (s2, b2) = a
        .req(Method::POST, "/projects", Some(json!({ "name": "Tesis" })), &k)
        .await;
    assert_eq!(s1, StatusCode::CREATED);
    assert_eq!(s2, StatusCode::CREATED);
    assert_eq!(b1["id"], b2["id"], "el reintento devuelve el mismo recurso");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM projects WHERE name = 'Tesis'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let (s3, b3) = a
        .req(Method::POST, "/projects", Some(json!({ "name": "Otra" })), &k)
        .await;
    assert_eq!(s3, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(b3["error"]["code"], "idempotency_mismatch");
    // La misma clave de otro usuario es independiente.
    let b = t.login(222).await;
    let (s4, b4) = b
        .req(Method::POST, "/projects", Some(json!({ "name": "Tesis" })), &k)
        .await;
    assert_eq!(s4, StatusCode::CREATED);
    assert_ne!(b4["id"], b1["id"]);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn dev_login_disabled_returns_404(pool: PgPool) {
    let t = TestApp::with_config(pool, |c| c.dev_login_enabled = false);
    let (st, _) = t
        .raw(
            Method::POST,
            "/api/v1/auth/dev-login",
            Some(json!({ "telegram_id": 111 })),
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn settings_validate_timezone_and_version(pool: PgPool) {
    let t = TestApp::new(pool);
    let a = t.login(111).await;
    let (_, s) = a.get("/settings").await;
    let (st, _) = a
        .put(
            "/settings",
            json!({ "version": s["version"], "timezone": "Mars/Base" }),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    let (st, s2) = a
        .put(
            "/settings",
            json!({ "version": s["version"], "timezone": "Europe/Madrid", "daily_time": "20:30:00" }),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s2["timezone"], "Europe/Madrid");
    let (st, _) = a
        .put("/settings", json!({ "version": s["version"], "locale": "es-ES" }))
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
}
