//! T12: Telegram Login OIDC contra un proveedor simulado (discovery + JWKS + token endpoint, ID token RS256).
//! Casos: flujo válido con PKCE y vinculación a la misma identidad de la Mini App; state, nonce, audiencia, emisor,
//! expiración, firma y claim `id` inválidos; ID fuera de la allowlist.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use server::config::OidcConfig;
use sqlx::PgPool;
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Construye un ID token a partir del nonce del flujo.
type TokenFactory<'a> = Box<dyn Fn(&str) -> String + 'a>;

const CLIENT_ID: &str = "8000000001";
const CLIENT_SECRET: &str = "test-secret";
const KEY: &str = include_str!("fixtures/oidc_test_key.pem");
const OTHER_KEY: &str = include_str!("fixtures/oidc_other_key.pem");
const JWKS: &str = include_str!("fixtures/oidc_jwks.json");

/// IP simulada distinta por petición: el límite de intentos (10/min por IP) se prueba aparte.
fn next_ip() -> String {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    format!(
        "10.0.0.{}",
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 250
    )
}

async fn idp() -> MockServer {
    let server = MockServer::start().await;
    let iss = server.uri();
    Mock::given(method("GET"))
        .and(path("/.well-known/openid-configuration"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issuer": iss,
            "authorization_endpoint": format!("{iss}/auth"),
            "token_endpoint": format!("{iss}/token"),
            "jwks_uri": format!("{iss}/.well-known/jwks.json"),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
            "scopes_supported": ["openid", "profile"],
            "code_challenge_methods_supported": ["S256"]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(JWKS, "application/json"))
        .mount(&server)
        .await;
    server
}

fn app(pool: PgPool, idp: &MockServer) -> TestApp {
    let issuer = idp.uri();
    TestApp::with_config(pool, move |c| {
        c.oidc = Some(OidcConfig {
            issuer,
            client_id: CLIENT_ID.into(),
            client_secret: CLIENT_SECRET.into(),
        });
    })
}

/// Inicia el flujo y devuelve (state, nonce, code_challenge) del redirect al proveedor.
async fn start(t: &TestApp) -> (String, String, String) {
    let req = axum::http::Request::builder()
        .uri("/api/v1/auth/oidc/start?redirect_to=/hoy")
        .header("x-forwarded-for", next_ip())
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = t.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let loc = url::Url::parse(resp.headers()["location"].to_str().unwrap()).unwrap();
    let q: std::collections::HashMap<_, _> = loc.query_pairs().into_owned().collect();
    assert_eq!(q["client_id"], CLIENT_ID);
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["response_type"], "code");
    assert!(q["scope"].contains("openid"));
    (
        q["state"].clone(),
        q["nonce"].clone(),
        q["code_challenge"].clone(),
    )
}

fn id_token(iss: &str, claims_patch: Value, key: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    let mut claims = json!({
        "iss": iss, "aud": CLIENT_ID, "sub": "1234123412341234123", "id": 111,
        "iat": now, "exp": now + 300, "nonce": "", "preferred_username": "neil", "name": "Neil"
    });
    for (k, v) in claims_patch.as_object().unwrap() {
        if v.is_null() {
            claims.as_object_mut().unwrap().remove(k);
        } else {
            claims[k] = v.clone();
        }
    }
    let mut h = Header::new(Algorithm::RS256);
    h.kid = Some("test-key".into());
    jsonwebtoken::encode(&h, &claims, &EncodingKey::from_rsa_pem(key.as_bytes()).unwrap()).unwrap()
}

async fn mock_token(idp: &MockServer, token: String) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(header_exists("authorization")) // Basic client_id:client_secret
        .and(body_string_contains("code_verifier="))
        .and(body_string_contains("grant_type=authorization_code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "at", "token_type": "Bearer", "expires_in": 3600, "id_token": token
        })))
        .up_to_n_times(1)
        .mount(idp)
        .await;
}

/// Ejecuta el callback y devuelve (Location, Set-Cookie[]).
async fn callback(t: &TestApp, state: &str) -> (String, Vec<String>) {
    let req = axum::http::Request::builder()
        .uri(format!("/api/v1/auth/oidc/callback?code=abc&state={state}"))
        .header("x-forwarded-for", next_ip())
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = t.app.clone().oneshot(req).await.unwrap();
    let loc = resp
        .headers()
        .get("location")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let cookies = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    (loc, cookies)
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn valid_flow_creates_cookie_session_linked_to_miniapp_identity(pool: PgPool) {
    let idp = idp().await;
    let t = app(pool.clone(), &idp);
    let miniapp = t.login(111).await; // misma persona entró antes por la Mini App
    let (state, nonce, _) = start(&t).await;
    mock_token(&idp, id_token(&idp.uri(), json!({ "nonce": nonce }), KEY)).await;
    let (loc, cookies) = callback(&t, &state).await;
    assert_eq!(loc, "/hoy");
    let session = cookies
        .iter()
        .find(|c| c.starts_with("vida_session="))
        .expect("cookie de sesión");
    assert!(session.contains("HttpOnly"));
    let cookie = session.split(';').next().unwrap().to_string();
    let (st, me) = t
        .raw(Method::GET, "/api/v1/me", None, &[("cookie", &cookie)])
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        me["user"]["id"],
        miniapp.user_id.as_str(),
        "OIDC y Mini App son el mismo usuario"
    );
    let providers: Vec<&str> = me["identities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["provider"].as_str().unwrap())
        .collect();
    assert!(providers.contains(&"telegram") && providers.contains(&"telegram_oidc"));
    let oidc = me["identities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["provider"] == "telegram_oidc")
        .unwrap();
    assert_eq!(oidc["subject"], "1234123412341234123");
    assert_eq!(oidc["telegram_user_id"], 111);
    // El state es de un solo uso.
    let (loc, _) = callback(&t, &state).await;
    assert_eq!(loc, "/login?error=invalid_state");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn invalid_tokens_are_rejected(pool: PgPool) {
    let idp = idp().await;
    let t = app(pool.clone(), &idp);
    let iss = idp.uri();
    let cases: Vec<(&str, TokenFactory)> = vec![
        (
            "nonce",
            Box::new(|_n| id_token(&iss, json!({ "nonce": "otro" }), KEY)),
        ),
        (
            "audience",
            Box::new(|n| id_token(&iss, json!({ "nonce": n, "aud": "999" }), KEY)),
        ),
        (
            "issuer",
            Box::new(|n| id_token(&iss, json!({ "nonce": n, "iss": "https://evil.example" }), KEY)),
        ),
        (
            "expired",
            Box::new(|n| {
                let past = chrono::Utc::now().timestamp() - 7200;
                id_token(&iss, json!({ "nonce": n, "iat": past - 300, "exp": past }), KEY)
            }),
        ),
        (
            "signature",
            Box::new(|n| id_token(&iss, json!({ "nonce": n }), OTHER_KEY)),
        ),
        (
            "missing id claim",
            Box::new(|n| id_token(&iss, json!({ "nonce": n, "id": null }), KEY)),
        ),
    ];
    for (name, make) in cases {
        let (state, nonce, _) = start(&t).await;
        mock_token(&idp, make(&nonce)).await;
        let (loc, cookies) = callback(&t, &state).await;
        assert_eq!(loc, "/login?error=invalid_token", "caso {name}");
        assert!(
            cookies.iter().all(|c| !c.starts_with("vida_session=")),
            "caso {name} no debe crear sesión"
        );
    }
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 0);
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn unknown_state_and_non_allowlisted_id_are_rejected(pool: PgPool) {
    let idp = idp().await;
    let t = app(pool.clone(), &idp);
    let (loc, _) = callback(&t, "forged-state").await;
    assert_eq!(loc, "/login?error=invalid_state");
    let (state, nonce, _) = start(&t).await;
    mock_token(
        &idp,
        id_token(&idp.uri(), json!({ "nonce": nonce, "id": 999 }), KEY),
    )
    .await;
    let (loc, _) = callback(&t, &state).await;
    assert_eq!(loc, "/login?error=forbidden");
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(users, 0, "no se crean usuarios fuera de la allowlist");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn open_redirects_are_neutralized(pool: PgPool) {
    let idp = idp().await;
    let t = app(pool, &idp);
    let req = axum::http::Request::builder()
        .uri("/api/v1/auth/oidc/start?redirect_to=//evil.com")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = t.app.clone().oneshot(req).await.unwrap();
    let loc = url::Url::parse(resp.headers()["location"].to_str().unwrap()).unwrap();
    let state = loc
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    let nonce = loc
        .query_pairs()
        .find(|(k, _)| k == "nonce")
        .unwrap()
        .1
        .to_string();
    mock_token(&idp, id_token(&idp.uri(), json!({ "nonce": nonce }), KEY)).await;
    let (loc, _) = callback(&t, &state).await;
    assert_eq!(loc, "/");
}

#[sqlx::test(migrator = "persistence::MIGRATOR")]
async fn auth_attempts_are_rate_limited_per_client(pool: PgPool) {
    let idp = idp().await;
    let t = app(pool, &idp);
    let mut statuses = Vec::new();
    for _ in 0..12 {
        let req = axum::http::Request::builder()
            .uri("/api/v1/auth/oidc/start")
            .header("x-forwarded-for", "203.0.113.7")
            .body(axum::body::Body::empty())
            .unwrap();
        statuses.push(t.app.clone().oneshot(req).await.unwrap().status());
    }
    assert!(statuses[..10].iter().all(|s| *s == StatusCode::SEE_OTHER));
    assert_eq!(statuses[11], StatusCode::TOO_MANY_REQUESTS);
}
