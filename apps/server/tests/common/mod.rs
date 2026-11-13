//! Arnés de pruebas de integración: app real + PostgreSQL real (BD efímera por prueba con `#[sqlx::test]`).
//! Uso:
//! ```ignore
//! #[sqlx::test(migrator = "persistence::MIGRATOR")]
//! async fn algo(pool: PgPool) {
//!     let t = TestApp::new(pool);
//!     let a = t.login(111).await;              // usuario A (allowlist: 111, 222)
//!     let (st, body) = a.post("/areas", json!({"name": "X"})).await;
//! }
//! ```
#![allow(dead_code)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::{TimeZone, Utc};
use domain::time::FixedClock;
use serde_json::Value;
use server::config::Config;
use server::state::AppState;
use sqlx::PgPool;
use tower::ServiceExt;

pub struct TestApp {
    pub app: Router,
    pub state: AppState,
    pub clock: FixedClock,
}

impl TestApp {
    pub fn new(pool: PgPool) -> Self {
        Self::with_config(pool, |_| {})
    }

    pub fn with_config(pool: PgPool, f: impl FnOnce(&mut Config)) -> Self {
        let mut cfg = Config::for_tests("postgres://unused");
        f(&mut cfg);
        // 2026-11-12 18:00 UTC = 12:00 en Monterrey.
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 11, 12, 18, 0, 0).unwrap());
        let state = AppState::new(pool, cfg, Arc::new(clock.clone()));
        Self {
            app: server::app(state.clone()),
            state,
            clock,
        }
    }

    /// Sesión Bearer por dev-login (equivalente a Mini App) para el ID de Telegram dado.
    pub async fn login(&self, telegram_id: i64) -> Client {
        let (st, body) = self
            .raw(
                Method::POST,
                "/api/v1/auth/dev-login",
                Some(serde_json::json!({ "telegram_id": telegram_id, "transport": "bearer" })),
                &[],
            )
            .await;
        assert_eq!(st, StatusCode::OK, "login falló: {body}");
        Client {
            app: self.app.clone(),
            token: body["token"].as_str().unwrap().to_string(),
            user_id: body["user_id"].as_str().unwrap().to_string(),
        }
    }

    pub async fn raw(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        send(&self.app, method, path, body, headers).await
    }
}

pub async fn send(
    app: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 50 * 1024 * 1024)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, json)
}

#[derive(Clone)]
pub struct Client {
    pub app: Router,
    pub token: String,
    pub user_id: String,
}

impl Client {
    pub async fn req(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        extra: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let auth = format!("Bearer {}", self.token);
        let mut headers: Vec<(&str, &str)> = vec![("authorization", auth.as_str())];
        headers.extend_from_slice(extra);
        send(&self.app, method, &format!("/api/v1{path}"), body, &headers).await
    }
    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.req(Method::GET, path, None, &[]).await
    }
    pub async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.req(Method::POST, path, Some(body), &[]).await
    }
    pub async fn put(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.req(Method::PUT, path, Some(body), &[]).await
    }
    pub async fn patch(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.req(Method::PATCH, path, Some(body), &[]).await
    }
    pub async fn delete(&self, path: &str) -> (StatusCode, Value) {
        self.req(Method::DELETE, path, None, &[]).await
    }
}
