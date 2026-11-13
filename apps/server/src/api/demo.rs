//! Datos demo opt-in (D-024). Solo con `DEMO_ENABLED=true` (prohibido en producción por configuración).
//! Crea un usuario **separado** (`users.is_demo = true`, identidad `dev/demo`), lo puebla con datos ficticios marcados
//! `[DEMO]` y abre una sesión con él. Nunca toca los datos del usuario real.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::session::{self, Transport};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(demo_session))
}

#[derive(Deserialize, ToSchema, Default)]
pub struct DemoRequest {
    /// Borra el usuario demo y lo vuelve a crear con datos frescos.
    #[serde(default)]
    pub reset: bool,
    /// `cookie` (por defecto) o `bearer` (Mini App).
    pub transport: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct DemoSession {
    pub user_id: Uuid,
    pub is_demo: bool,
    pub token: Option<String>,
    pub csrf: Option<String>,
}

/// Entra a un espacio demo aislado (solo desarrollo).
#[utoipa::path(post, path = "/demo/session", tag = "demo", request_body = DemoRequest,
    responses((status = 200, body = DemoSession), (status = 404, body = crate::error::ErrorBody)))]
async fn demo_session(
    State(s): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<DemoRequest>,
) -> ApiResult<Response> {
    if !s.config.demo_enabled {
        return Err(ApiError::not_found());
    }
    let mut tx = s.pool.begin().await?;
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM identities WHERE provider = 'dev' AND subject = 'demo'")
            .fetch_optional(&mut *tx)
            .await?;
    if let (Some(u), true) = (existing, req.reset) {
        sqlx::query("DELETE FROM users WHERE id = $1 AND is_demo")
            .bind(u)
            .execute(&mut *tx)
            .await?;
    }
    let user_id = match existing.filter(|_| !req.reset) {
        Some(u) => u,
        None => {
            let u: Uuid = sqlx::query_scalar(
                "INSERT INTO users (display_name, is_demo) VALUES ('Usuario demo', true) RETURNING id",
            )
            .fetch_one(&mut *tx)
            .await?;
            persistence::repos::identity::bootstrap_user(&mut tx, u).await?;
            sqlx::query("INSERT INTO identities (user_id, provider, subject) VALUES ($1, 'dev', 'demo')")
                .bind(u)
                .execute(&mut *tx)
                .await?;
            let settings = persistence::repos::identity::get_settings(&mut *tx, u).await?;
            let tz = domain::time::parse_tz(&settings.timezone)?;
            let today = domain::time::local_date(s.now(), tz);
            debug_assert!(today.year() > 2000);
            sqlx::query("SELECT seed_demo($1, $2)")
                .bind(u)
                .bind(today)
                .execute(&mut *tx)
                .await?;
            u
        }
    };
    tx.commit().await?;
    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    if req.transport.as_deref() == Some("bearer") {
        let sess = session::issue(&s, user_id, Transport::Bearer, ua).await?;
        return Ok(Json(DemoSession {
            user_id,
            is_demo: true,
            token: Some(sess.token),
            csrf: None,
        })
        .into_response());
    }
    let sess = session::issue(&s, user_id, Transport::Cookie, ua).await?;
    let [a, b] = session::session_cookies(&s, &sess);
    Ok((
        jar.add(a).add(b),
        Json(DemoSession {
            user_id,
            is_demo: true,
            token: None,
            csrf: sess.csrf.clone(),
        }),
    )
        .into_response())
}
