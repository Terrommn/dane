//! T49/T50: Google OAuth (PKCE, state de un solo uso, refresh, revocación), Calendar y Gmail con servidor simulado.

mod common;

use axum::http::StatusCode;
use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use server::config::OAuthApp;
use server::integrations::{Endpoints, google};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn app(pool: PgPool) -> common::TestApp {
    common::TestApp::with_config(pool, |c| {
        c.google = Some(OAuthApp {
            client_id: "cid".into(),
            client_secret: "csecret".into(),
        })
    })
}

async fn connect(t: &common::TestApp, mock: &MockServer, user: Uuid) {
    let ep = Endpoints::mock(&mock.uri());
    let url = google::authorize_url(&t.state, user, &ep, "/ajustes")
        .await
        .unwrap();
    let st = url::Url::parse(&url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code_verifier="))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "at-1", "refresh_token": "rt-secret-1", "expires_in": 3600,
            "scope": "https://www.googleapis.com/auth/calendar.readonly https://www.googleapis.com/auth/gmail.readonly"
        })))
        .mount(mock)
        .await;
    let (u, redirect) = google::complete(&t.state, &ep, &st, "code-1").await.unwrap();
    assert_eq!(u, user);
    assert_eq!(redirect, "/ajustes");
    // El state es de un solo uso.
    assert!(google::complete(&t.state, &ep, &st, "code-1").await.is_err());
    sqlx::query("UPDATE integration_connections SET config = config || jsonb_build_object('endpoints_override', $2::text) WHERE user_id = $1 AND provider = 'google'")
        .bind(user)
        .bind(mock.uri())
        .execute(&t.state.pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn connect_url_requests_minimal_readonly_scopes_with_pkce(pool: PgPool) {
    let t = app(pool);
    let a = t.login(111).await;
    let (st, body) = a.get("/integrations/google/connect?redirect_to=/hoy").await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let url = url::Url::parse(body["url"].as_str().unwrap()).unwrap();
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["access_type"], "offline");
    assert!(q["scope"].contains("calendar.readonly") && q["scope"].contains("gmail.readonly"));
    assert!(!q["scope"].contains("drive"), "Drive solo como enlaces");
    assert!(
        !q["scope"].contains("calendar ") && !q["scope"].ends_with("/calendar"),
        "sin escritura de calendario"
    );
    assert_eq!(
        q["redirect_uri"],
        "http://localhost:5173/api/v1/integrations/google/callback"
    );
    // Callback con state inválido no conecta.
    let (st, _) = t
        .raw(
            axum::http::Method::GET,
            "/api/v1/integrations/google/callback?code=x&state=forged",
            None,
            &[],
        )
        .await;
    assert_eq!(st, StatusCode::SEE_OTHER);
    let (_, ov) = a.get("/integrations").await;
    assert_eq!(ov["google"]["status"], "disconnected");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn calendar_and_gmail_sync_without_duplicates_or_silent_people(pool: PgPool) {
    let t = app(pool.clone());
    let a = t.login(111).await;
    let user: Uuid = a.user_id.parse().unwrap();
    let mock = MockServer::start().await;
    connect(&t, &mock, user).await;
    // Los tokens se guardan cifrados.
    let enc: Vec<u8> =
        sqlx::query_scalar("SELECT credentials_enc FROM integration_connections WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!String::from_utf8_lossy(&enc).contains("rt-secret-1"));

    sqlx::query("INSERT INTO people (user_id, name, email) VALUES ($1, 'Ana López', 'ana@x.com')")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();

    Mock::given(method("GET"))
        .and(path("/calendar/v3/users/me/calendarList"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [
            { "id": "neil@gmail.com", "summary": "Neil", "primary": true },
            { "id": "en.mexican#holiday@group.v.calendar.google.com", "summary": "Festivos" }
        ]})))
        .mount(&mock)
        .await;
    let events = |start: &str| {
        json!({ "items": [
            { "id": "ev1", "status": "confirmed", "summary": "Comida con Ana", "start": { "dateTime": start }, "end": { "dateTime": "2026-11-12T17:00:00Z" },
              "attendees": [ { "email": "neil@gmail.com", "self": true }, { "email": "ana@x.com" }, { "email": "beto@y.com", "displayName": "Beto" }, { "email": "sala@resource.calendar.google.com", "resource": true } ] },
            { "id": "ev2", "status": "confirmed", "summary": "Viaje", "start": { "date": "2026-11-13" }, "end": { "date": "2026-11-14" } },
            { "id": "ev3", "status": "cancelled" }
        ]})
    };
    Mock::given(method("GET"))
        .and(path("/calendar/v3/calendars/neil%40gmail.com/events"))
        .and(query_param("singleEvents", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(events("2026-11-12T16:00:00Z")))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/gmail/v1/users/me/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "messages": [ { "id": "m1" }, { "id": "m2" } ] })),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/gmail/v1/users/me/messages/m1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "m1", "internalDate": "1794506400000",
            "payload": { "headers": [ { "name": "To", "value": "Ana <ana@x.com>, no-reply@bank.com" }, { "name": "Subject", "value": "Propuesta" } ] } })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/gmail/v1/users/me/messages/m2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "m2", "internalDate": "1794506400000",
            "payload": { "headers": [ { "name": "To", "value": "lista@news.com" }, { "name": "List-Unsubscribe", "value": "<mailto:u@news.com>" } ] } })))
        .mount(&mock)
        .await;

    for _ in 0..2 {
        let (st, reps) = a.post("/integrations/google/sync", json!({})).await;
        assert_eq!(st, StatusCode::OK, "{reps}");
        assert!(
            reps.as_array().unwrap().iter().all(|r| r["error"].is_null()),
            "{reps}"
        );
    }
    let events_n: i64 = sqlx::query_scalar("SELECT count(*) FROM calendar_events WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        events_n, 1,
        "solo el evento con hora; festivos/día completo/cancelados fuera; sin duplicados al resincronizar"
    );
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM people WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(people, 1, "nunca crea personas");
    let (_, sug) = a.get("/integrations/suggestions").await;
    let ints = sug["interactions"].as_array().unwrap();
    assert_eq!(ints.len(), 2, "Ana desde Calendar y Gmail: {sug}");
    let ppl = sug["people"].as_array().unwrap();
    assert_eq!(
        ppl.len(),
        1,
        "Beto sugerido; no-reply, newsletter y sala ignorados: {sug}"
    );
    assert_eq!(ppl[0]["name"], "Beto");
    let interactions: i64 = sqlx::query_scalar("SELECT count(*) FROM interactions WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(interactions, 0, "un evento no prueba una conversación");

    // Aceptar una sugerencia registra la interacción y actualiza último contacto.
    let id = ints[0]["id"].as_str().unwrap();
    assert_eq!(
        a.post(
            &format!("/integrations/suggestions/interactions/{id}/accept"),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    let last: Option<chrono::NaiveDate> =
        sqlx::query_scalar("SELECT last_contact_on FROM people WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(last.unwrap().to_string(), "2026-11-12");
    // Otro usuario no puede aceptar sugerencias ajenas.
    let b = t.login(222).await;
    assert_eq!(
        b.post(
            &format!(
                "/integrations/suggestions/interactions/{}/accept",
                ints[1]["id"].as_str().unwrap()
            ),
            json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // Cambio de hora: se actualiza el mismo evento. Desaparición: se quita.
    mock.reset().await;
    Mock::given(method("GET"))
        .and(path("/calendar/v3/calendars/neil%40gmail.com/events"))
        .respond_with(ResponseTemplate::new(200).set_body_json(events("2026-11-12T15:00:00Z")))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/calendar/v3/users/me/calendarList"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "items": [ { "id": "neil@gmail.com", "primary": true } ] })),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/gmail/v1/users/me/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&mock)
        .await;
    a.post("/integrations/google/sync", json!({})).await;
    let start: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT starts_at FROM calendar_events WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(start, Utc.with_ymd_and_hms(2026, 11, 12, 15, 0, 0).unwrap());
    mock.reset().await;
    Mock::given(method("GET"))
        .and(path("/calendar/v3/users/me/calendarList"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "items": [ { "id": "neil@gmail.com", "primary": true } ] })),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/calendar/v3/calendars/neil%40gmail.com/events"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/gmail/v1/users/me/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&mock)
        .await;
    a.post("/integrations/google/sync", json!({})).await;
    let events_n: i64 = sqlx::query_scalar("SELECT count(*) FROM calendar_events WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(events_n, 0);
    // Solo lectura: nunca se envió POST/PUT/PATCH/DELETE a las APIs de Calendar/Gmail.
    for r in mock.received_requests().await.unwrap() {
        assert_eq!(r.method.as_str(), "GET", "{}", r.url);
    }
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn expired_refresh_marks_error_and_disconnect_revokes(pool: PgPool) {
    let t = app(pool.clone());
    let a = t.login(111).await;
    let user: Uuid = a.user_id.parse().unwrap();
    let mock = MockServer::start().await;
    connect(&t, &mock, user).await;
    // Token vencido + refresh rechazado (invalid_grant) → estado error que pide reconectar.
    t.clock
        .set(Utc.with_ymd_and_hms(2026, 11, 12, 18, 0, 0).unwrap() + Duration::hours(3));
    mock.reset().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })))
        .expect(1)
        .mount(&mock)
        .await;
    let (_, reps) = a.post("/integrations/google/sync", json!({})).await;
    assert!(
        reps[0]["error"].as_str().unwrap().contains("vuelve a conectar"),
        "{reps}"
    );
    let (_, ov) = a.get("/integrations").await;
    assert_eq!(ov["google"]["status"], "error");
    assert!(ov["google"]["last_error"].as_str().is_some());
    assert!(
        ov["google"]["config"].get("endpoints_override").is_none(),
        "no se exponen detalles internos"
    );
    // Desconectar revoca en el proveedor y borra credenciales.
    Mock::given(method("POST"))
        .and(path("/revoke"))
        .and(body_string_contains("token=rt-secret-1"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock)
        .await;
    assert_eq!(a.delete("/integrations/google").await.0, StatusCode::NO_CONTENT);
    let (status, creds): (String, Option<Vec<u8>>) =
        sqlx::query_as("SELECT status, credentials_enc FROM integration_connections WHERE user_id = $1")
            .bind(user)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "revoked");
    assert!(creds.is_none());
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn refresh_success_rotates_access_token(pool: PgPool) {
    let t = app(pool.clone());
    let a = t.login(111).await;
    let user: Uuid = a.user_id.parse().unwrap();
    let mock = MockServer::start().await;
    connect(&t, &mock, user).await;
    t.clock.set(Utc.with_ymd_and_hms(2026, 11, 12, 21, 0, 0).unwrap());
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "access_token": "at-2", "expires_in": 3600 })),
        )
        .expect(1)
        .mount(&mock)
        .await;
    let tok = google::access_token(&t.state, user, &Endpoints::mock(&mock.uri()))
        .await
        .unwrap();
    assert_eq!(tok, "at-2");
    // Segunda llamada usa el token vigente (no vuelve a refrescar).
    assert_eq!(
        google::access_token(&t.state, user, &Endpoints::mock(&mock.uri()))
            .await
            .unwrap(),
        "at-2"
    );
}
