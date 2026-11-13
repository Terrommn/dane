//! Endpoints de autenticación y perfil.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use chrono::{DateTime, Duration, Utc};
use persistence::repos::identity::{self, Identity, User, UserSettings, VerifiedIdentity};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::miniapp::{InitDataError, validate_init_data};
use super::session::{self, AuthUser, Transport};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(auth_config))
        .routes(routes!(miniapp_login))
        .routes(routes!(oidc_start))
        .routes(routes!(oidc_callback))
        .routes(routes!(dev_login))
        .routes(routes!(logout))
        .routes(routes!(logout_all))
        .routes(routes!(me))
}

fn client_key(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "local".into())
}

fn rate_limit(state: &AppState, headers: &HeaderMap, scope: &str) -> ApiResult<()> {
    state
        .auth_limiter
        .check_key(&format!("{scope}:{}", client_key(headers)))
        .map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Demasiados intentos; espera un minuto",
            )
        })
}

fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers.get(USER_AGENT).and_then(|v| v.to_str().ok())
}

#[derive(Serialize, ToSchema)]
pub struct AuthConfig {
    pub miniapp_enabled: bool,
    pub oidc_enabled: bool,
    pub dev_login_enabled: bool,
    pub bot_username: Option<String>,
}

/// Métodos de inicio de sesión disponibles (público).
#[utoipa::path(get, path = "/auth/config", tag = "auth", responses((status = 200, body = AuthConfig)))]
async fn auth_config(State(state): State<AppState>) -> Json<AuthConfig> {
    Json(AuthConfig {
        miniapp_enabled: state.config.telegram_bot_token.is_some(),
        oidc_enabled: state.config.oidc.is_some(),
        dev_login_enabled: state.config.dev_login_enabled,
        bot_username: state.config.telegram_bot_username.clone(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct MiniAppLogin {
    /// `Telegram.WebApp.initData` sin modificar.
    pub init_data: String,
}

#[derive(Serialize, ToSchema)]
pub struct BearerSession {
    /// Token Bearer: guárdalo solo en memoria.
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub user_id: uuid::Uuid,
    /// `start_param` del enlace de la Mini App (ej. `checkin_2026-11-12`).
    pub start_param: Option<String>,
}

/// Inicia sesión desde la Mini App validando `initData` (firma y antigüedad) en el servidor.
#[utoipa::path(post, path = "/auth/telegram/miniapp", tag = "auth", request_body = MiniAppLogin,
    responses((status = 200, body = BearerSession), (status = 401, body = crate::error::ErrorBody), (status = 403, body = crate::error::ErrorBody)))]
async fn miniapp_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MiniAppLogin>,
) -> ApiResult<Json<BearerSession>> {
    rate_limit(&state, &headers, "miniapp")?;
    let token = state
        .config
        .telegram_bot_token
        .as_deref()
        .ok_or_else(|| ApiError::unavailable("Mini App no configurada"))?;
    let tg = validate_init_data(
        &body.init_data,
        token,
        state.now(),
        state.config.miniapp_max_age_secs,
    )
    .map_err(|e| {
        tracing::warn!(reason = %e, "initData rechazado");
        match e {
            InitDataError::Expired => ApiError::new(
                axum::http::StatusCode::UNAUTHORIZED,
                "expired",
                "La sesión de Telegram venció; vuelve a abrir la app desde el bot",
            ),
            _ => ApiError::new(
                axum::http::StatusCode::UNAUTHORIZED,
                "invalid_init_data",
                "No se pudo verificar la sesión de Telegram",
            ),
        }
    })?;
    if !state.config.is_allowed(tg.id) {
        return Err(ApiError::forbidden("Esta cuenta de Telegram no tiene acceso"));
    }
    let user_id = identity::upsert_verified_identity(
        &state.pool,
        &VerifiedIdentity {
            provider: "telegram",
            subject: tg.id.to_string(),
            telegram_user_id: Some(tg.id),
            username: tg.username,
            first_name: tg.first_name,
            last_name: tg.last_name,
        },
    )
    .await?;
    let s = session::issue(&state, user_id, Transport::Bearer, user_agent(&headers)).await?;
    Ok(Json(BearerSession {
        token: s.token,
        expires_at: s.expires_at,
        user_id,
        start_param: tg.start_param,
    }))
}

#[derive(Deserialize, IntoParams)]
pub struct OidcStartQuery {
    /// Ruta interna a la que volver (debe empezar con `/`).
    pub redirect_to: Option<String>,
}

fn safe_redirect(path: Option<&str>) -> String {
    match path {
        Some(p) if p.starts_with('/') && !p.starts_with("//") && !p.contains('\\') && p.len() < 300 => {
            p.to_string()
        }
        _ => "/".into(),
    }
}

/// Redirige a Telegram Login (OIDC + PKCE).
#[utoipa::path(get, path = "/auth/oidc/start", tag = "auth", params(OidcStartQuery), responses((status = 302)))]
async fn oidc_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<OidcStartQuery>,
) -> ApiResult<Response> {
    rate_limit(&state, &headers, "oidc")?;
    let cfg = state
        .config
        .oidc
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("Telegram Login no está configurado"))?;
    let start = super::oidc::start(&state.oidc_cache, cfg, &state.config.oidc_redirect_uri())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "OIDC discovery falló");
            ApiError::unavailable("No se pudo contactar a Telegram Login")
        })?;
    sqlx::query(
        "INSERT INTO oauth_flows (state, purpose, pkce_verifier, nonce, redirect_to, expires_at) VALUES ($1, 'login', $2, $3, $4, $5)",
    )
    .bind(&start.state)
    .bind(&start.pkce_verifier)
    .bind(&start.nonce)
    .bind(safe_redirect(q.redirect_to.as_deref()))
    .bind(state.now() + Duration::minutes(10))
    .execute(&state.pool)
    .await?;
    // Limpieza con el reloj de la app (no `now()` de la BD) para ser coherente con `expires_at`.
    sqlx::query("DELETE FROM oauth_flows WHERE expires_at < $1")
        .bind(state.now())
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&start.url).into_response())
}

#[derive(Deserialize, IntoParams)]
pub struct OidcCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Callback OIDC: valida state, intercambia el código con PKCE, verifica el ID token y crea la sesión con cookie.
#[utoipa::path(get, path = "/auth/oidc/callback", tag = "auth", params(OidcCallbackQuery), responses((status = 302)))]
async fn oidc_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Query(q): Query<OidcCallbackQuery>,
) -> Response {
    let fail = |reason: &str| Redirect::to(&format!("/login?error={reason}")).into_response();
    if rate_limit(&state, &headers, "oidc").is_err() {
        return fail("rate_limited");
    }
    let (Some(code), Some(st)) = (q.code, q.state) else {
        return fail(if q.error.is_some() {
            "cancelled"
        } else {
            "invalid_request"
        });
    };
    let Some(cfg) = state.config.oidc.as_ref() else {
        return fail("not_configured");
    };
    // El state es de un solo uso: se borra al consumirlo.
    let flow: Option<(String, String, Option<String>)> = match sqlx::query_as(
        "DELETE FROM oauth_flows WHERE state = $1 AND purpose = 'login' AND expires_at > $2 RETURNING pkce_verifier, nonce, redirect_to",
    )
    .bind(&st)
    .bind(state.now())
    .fetch_optional(&state.pool)
    .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(error = %e, "oauth_flows");
            return fail("internal");
        }
    };
    let Some((verifier, nonce, redirect_to)) = flow else {
        return fail("invalid_state");
    };
    let ident = match super::oidc::finish(
        &state.oidc_cache,
        cfg,
        &state.config.oidc_redirect_uri(),
        &code,
        &verifier,
        &nonce,
    )
    .await
    {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(error = %e, "OIDC rechazado");
            return fail("invalid_token");
        }
    };
    if !state.config.is_allowed(ident.telegram_user_id) {
        return fail("forbidden");
    }
    let user_id = match identity::upsert_verified_identity(
        &state.pool,
        &VerifiedIdentity {
            provider: "telegram_oidc",
            subject: ident.subject,
            telegram_user_id: Some(ident.telegram_user_id),
            username: ident.username,
            first_name: ident.first_name,
            last_name: None,
        },
    )
    .await
    {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "upsert identity");
            return fail("internal");
        }
    };
    match session::issue(&state, user_id, Transport::Cookie, user_agent(&headers)).await {
        Ok(s) => {
            let [a, b] = session::session_cookies(&state, &s);
            (
                jar.add(a).add(b),
                Redirect::to(&safe_redirect(redirect_to.as_deref())),
            )
                .into_response()
        }
        Err(_) => fail("internal"),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct DevLogin {
    pub telegram_id: i64,
    pub first_name: Option<String>,
    /// `cookie` (por defecto) o `bearer`.
    pub transport: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct DevLoginResult {
    pub user_id: uuid::Uuid,
    pub token: Option<String>,
    pub csrf: Option<String>,
}

/// Solo desarrollo/pruebas (bloqueado en producción por configuración). Respeta la allowlist.
#[utoipa::path(post, path = "/auth/dev-login", tag = "auth", request_body = DevLogin, responses((status = 200, body = DevLoginResult)))]
async fn dev_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<DevLogin>,
) -> ApiResult<Response> {
    if !state.config.dev_login_enabled {
        return Err(ApiError::not_found());
    }
    if !state.config.is_allowed(body.telegram_id) {
        return Err(ApiError::forbidden("ID no incluido en ALLOWED_TELEGRAM_IDS"));
    }
    let user_id = identity::upsert_verified_identity(
        &state.pool,
        &VerifiedIdentity {
            provider: "telegram",
            subject: body.telegram_id.to_string(),
            telegram_user_id: Some(body.telegram_id),
            username: None,
            first_name: body.first_name,
            last_name: None,
        },
    )
    .await?;
    if body.transport.as_deref() == Some("bearer") {
        let s = session::issue(&state, user_id, Transport::Bearer, user_agent(&headers)).await?;
        return Ok(Json(DevLoginResult {
            user_id,
            token: Some(s.token),
            csrf: None,
        })
        .into_response());
    }
    let s = session::issue(&state, user_id, Transport::Cookie, user_agent(&headers)).await?;
    let [a, b] = session::session_cookies(&state, &s);
    Ok((
        jar.add(a).add(b),
        Json(DevLoginResult {
            user_id,
            token: None,
            csrf: s.csrf.clone(),
        }),
    )
        .into_response())
}

/// Cierra la sesión actual.
#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 204)))]
async fn logout(State(state): State<AppState>, auth: AuthUser, jar: CookieJar) -> ApiResult<Response> {
    identity::revoke_session(&state.pool, auth.user_id, auth.session_id).await?;
    let [a, b] = session::clear_cookies(&state);
    Ok((jar.add(a).add(b), axum::http::StatusCode::NO_CONTENT).into_response())
}

/// Revoca todas las sesiones del usuario (todos los dispositivos).
#[utoipa::path(post, path = "/auth/logout-all", tag = "auth", responses((status = 204)))]
async fn logout_all(State(state): State<AppState>, auth: AuthUser, jar: CookieJar) -> ApiResult<Response> {
    identity::revoke_all_sessions(&state.pool, auth.user_id).await?;
    let [a, b] = session::clear_cookies(&state);
    Ok((jar.add(a).add(b), axum::http::StatusCode::NO_CONTENT).into_response())
}

#[derive(Serialize, ToSchema)]
pub struct TelegramStatus {
    /// El bot conoce el chat privado (el usuario envió /start).
    pub chat_known: bool,
    pub can_message: bool,
    pub bot_username: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Me {
    pub user: User,
    pub identities: Vec<Identity>,
    pub settings: UserSettings,
    pub telegram: TelegramStatus,
    pub session_transport: String,
}

/// Perfil del usuario autenticado.
#[utoipa::path(get, path = "/me", tag = "auth", responses((status = 200, body = Me), (status = 401, body = crate::error::ErrorBody)))]
async fn me(State(state): State<AppState>, auth: AuthUser) -> ApiResult<Json<Me>> {
    let user = identity::get_user(&state.pool, auth.user_id).await?;
    let identities = identity::identities_of(&state.pool, auth.user_id).await?;
    let settings = identity::get_settings(&state.pool, auth.user_id).await?;
    let chat: Option<(bool, Option<String>)> =
        sqlx::query_as("SELECT can_message, last_error FROM telegram_chats WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_optional(&state.pool)
            .await?;
    Ok(Json(Me {
        user,
        identities,
        settings,
        telegram: TelegramStatus {
            chat_known: chat.is_some(),
            can_message: chat.as_ref().is_some_and(|c| c.0),
            bot_username: state.config.telegram_bot_username.clone(),
            last_error: chat.and_then(|c| c.1),
        },
        session_transport: if auth.transport == Transport::Cookie {
            "cookie".into()
        } else {
            "bearer".into()
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::safe_redirect;

    #[test]
    fn redirect_only_internal_paths() {
        assert_eq!(
            safe_redirect(Some("/hoy?date=2026-11-12")),
            "/hoy?date=2026-11-12"
        );
        assert_eq!(safe_redirect(Some("//evil.com")), "/");
        assert_eq!(safe_redirect(Some("https://evil.com")), "/");
        assert_eq!(safe_redirect(Some("/\\evil.com")), "/");
        assert_eq!(safe_redirect(None), "/");
    }
}
