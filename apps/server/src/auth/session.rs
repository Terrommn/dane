//! Sesiones propias, revocables y con expiración. Solo se guarda el hash SHA-256 del token.
//!
//! * Web/PWA: cookie `vida_session` (HttpOnly, Secure, SameSite=Lax) + cookie legible `vida_csrf` y header
//!   `X-CSRF-Token` obligatorio en métodos no seguros (double submit; el hash del CSRF va ligado a la sesión).
//! * Mini App: token Bearer en memoria del WebView (D-005), sin cookies.

use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::{AUTHORIZATION, COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method};
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::cookie::{Cookie, SameSite};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::crypto::{ct_eq, random_token, sha256};
use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "vida_session";
pub const CSRF_COOKIE: &str = "vida_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Cookie,
    Bearer,
}

/// Usuario autenticado de la petición. Toda consulta de negocio usa `user_id` de aquí, nunca del cliente.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub transport: Transport,
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .copied()
            .ok_or_else(ApiError::unauthorized)
    }
}

pub struct IssuedSession {
    pub token: String,
    pub csrf: Option<String>,
    pub expires_at: DateTime<Utc>,
}

pub async fn issue(
    state: &AppState,
    user_id: Uuid,
    transport: Transport,
    user_agent: Option<&str>,
) -> Result<IssuedSession, ApiError> {
    let token = random_token();
    let (csrf, csrf_hash) = match transport {
        Transport::Cookie => {
            let c = random_token();
            let h = sha256(c.as_bytes());
            (Some(c), Some(h))
        }
        Transport::Bearer => (None, None),
    };
    let ttl = match transport {
        Transport::Cookie => state.config.cookie_session_ttl_hours,
        Transport::Bearer => state.config.bearer_session_ttl_hours,
    };
    let expires_at = state.now() + Duration::hours(ttl);
    persistence::repos::identity::create_session(
        &state.pool,
        user_id,
        &sha256(token.as_bytes()),
        csrf_hash.as_deref(),
        if transport == Transport::Cookie {
            "cookie"
        } else {
            "bearer"
        },
        expires_at,
        user_agent.map(|u| &u[..u.len().min(300)]),
    )
    .await?;
    Ok(IssuedSession {
        token,
        csrf,
        expires_at,
    })
}

pub fn session_cookies(state: &AppState, s: &IssuedSession) -> [Cookie<'static>; 2] {
    let max_age = time_duration(state.config.cookie_session_ttl_hours);
    let session = Cookie::build((SESSION_COOKIE, s.token.clone()))
        .http_only(true)
        .secure(state.config.cookie_secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(max_age)
        .build();
    let csrf = Cookie::build((CSRF_COOKIE, s.csrf.clone().unwrap_or_default()))
        .http_only(false)
        .secure(state.config.cookie_secure)
        .same_site(SameSite::Strict)
        .path("/")
        .max_age(max_age)
        .build();
    [session, csrf]
}

pub fn clear_cookies(state: &AppState) -> [Cookie<'static>; 2] {
    let mk = |name: &'static str| {
        Cookie::build((name, ""))
            .path("/")
            .secure(state.config.cookie_secure)
            .max_age(time_duration(0))
            .build()
    };
    [mk(SESSION_COOKIE), mk(CSRF_COOKIE)]
}

fn time_duration(hours: i64) -> cookie::time::Duration {
    cookie::time::Duration::hours(hours)
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|kv| {
            let (k, v) = kv.trim().split_once('=')?;
            (k == name).then(|| v.to_string())
        })
}

/// Middleware: resuelve la sesión (Bearer o cookie), valida CSRF en métodos no seguros con cookie e inserta `AuthUser`.
/// Las rutas que lo requieren lo obtienen con el extractor `AuthUser` (401 si falta).
pub async fn resolve(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let headers = req.headers();
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let (token, transport) = match bearer {
        Some(t) => (Some(t), Transport::Bearer),
        None => (cookie_value(headers, SESSION_COOKIE), Transport::Cookie),
    };
    if let Some(token) = token {
        let row =
            persistence::repos::identity::find_session(&state.pool, &sha256(token.as_bytes()), state.now())
                .await?;
        if let Some(s) = row {
            let expected_transport = if s.transport == "cookie" {
                Transport::Cookie
            } else {
                Transport::Bearer
            };
            if expected_transport != transport {
                return Err(ApiError::unauthorized());
            }
            if transport == Transport::Cookie && !is_safe(req.method()) {
                let header = req
                    .headers()
                    .get(CSRF_HEADER)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default();
                let cookie = cookie_value(req.headers(), CSRF_COOKIE).unwrap_or_default();
                let bound = s.csrf_hash.as_deref().unwrap_or_default();
                if header.is_empty() || header != cookie || !ct_eq(&sha256(header.as_bytes()), bound) {
                    return Err(ApiError::forbidden("Token CSRF inválido; recarga la página"));
                }
            }
            let _ = persistence::repos::identity::touch_session(&state.pool, s.id).await;
            req.extensions_mut().insert(AuthUser {
                user_id: s.user_id,
                session_id: s.id,
                transport,
            });
        }
    }
    Ok(next.run(req).await)
}

fn is_safe(m: &Method) -> bool {
    matches!(*m, Method::GET | Method::HEAD | Method::OPTIONS)
}
