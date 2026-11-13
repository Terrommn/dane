//! Composición de la API `/api/v1`, OpenAPI, health/readiness y middleware.
//!
//! Para agregar un módulo: crear `api/<modulo>.rs` con `pub fn router() -> OpenApiRouter<AppState>` y añadir una línea
//! `.merge(<modulo>::router())` en `v1()`. Cada handler declara `#[utoipa::path]` con `tag` del módulo.

pub mod agenda;
pub mod areas;
pub mod captures;
pub mod checkins;
pub mod content;
pub mod dashboard;
pub mod dev;
pub mod devotionals;
pub mod drawings;
pub mod exports;
pub mod finance;
pub mod goals;
pub mod habits;
pub mod imports;
pub mod inbox;
pub mod integrations;
pub mod notes;
pub mod people;
pub mod readings;
pub mod reminders;
pub mod school;
pub mod search;
pub mod settings;
pub mod tasks;
pub mod weekly;

use std::time::Duration;

use axum::Router;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::routing::get;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;

use crate::auth;
use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    info(title = "Vida API", version = "1.0.0", description = "API de la app personal de hábitos, objetivos y organización de vida."),
    servers((url = "/api/v1")),
    modifiers(&SecurityAddon),
    components(schemas(crate::error::ErrorBody))
)]
pub struct ApiDoc;

struct SecurityAddon;
impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let c = openapi.components.get_or_insert_with(Default::default);
        c.add_security_scheme(
            "bearer",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
        c.add_security_scheme(
            "cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(auth::session::SESSION_COOKIE))),
        );
    }
}

/// Rutas de negocio (relativas a `/api/v1`).
fn v1() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(auth::routes::router())
        .merge(settings::router())
        .merge(areas::router())
        .merge(goals::router())
        .merge(habits::router())
        .merge(checkins::router())
        .merge(dashboard::router())
        .merge(weekly::router())
        .merge(devotionals::router())
        .merge(readings::router())
        .merge(tasks::router())
        .merge(inbox::router())
        .merge(people::router())
        .merge(content::router())
        .merge(school::router())
        .merge(dev::router())
        .merge(notes::router())
        .merge(drawings::router())
        .merge(agenda::router())
        .merge(search::router())
        .merge(finance::router())
        .merge(reminders::router())
        .merge(integrations::router())
        .merge(captures::router())
        .merge(imports::router())
        .merge(exports::router())
}

/// OpenAPI completo (para `vida-server openapi` y `/api/v1/openapi.json`).
pub fn openapi() -> utoipa::openapi::OpenApi {
    let (_, api) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .merge(v1())
        .split_for_parts();
    api
}

pub fn app(state: AppState) -> Router {
    let (api_router, api_doc) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(v1())
        .split_for_parts();
    let api_doc = std::sync::Arc::new(api_doc);
    let api = api_router
        .route(
            "/openapi.json",
            get(move || {
                let d = api_doc.clone();
                async move { axum::Json((*d).clone()) }
            }),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::http::idempotency::layer,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::session::resolve,
        ))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024));

    let mut router = Router::new()
        .nest("/api/v1", api)
        .nest("/internal/v1", crate::internal::router(state.clone()))
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz));

    if let Some(dir) = state.config.web_dist_dir.clone() {
        let index = dir.join("index.html");
        router = router.fallback_service(
            tower_http::services::ServeDir::new(dir).fallback(tower_http::services::ServeFile::new(index)),
        );
    }

    router
        .with_state(state)
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &axum::http::Request<_>| {
                let id = req.headers().get("x-request-id").and_then(|v| v.to_str().ok()).unwrap_or("-").to_string();
                // Solo método y ruta: nunca query (puede llevar `code`/`state`) ni headers con credenciales.
                tracing::info_span!("http", method = %req.method(), path = %req.uri().path(), request_id = %id)
            }),
        )
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(60)))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

async fn readyz(State(s): State<AppState>) -> (StatusCode, &'static str) {
    match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&s.pool).await {
        Ok(_) => (StatusCode::OK, "ready"),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "database unavailable"),
    }
}

/// Registro de auditoría de cambios (no bloquea la operación si falla).
pub async fn audit(
    s: &AppState,
    user_id: Uuid,
    entity_type: &str,
    entity_id: Option<Uuid>,
    action: &str,
    changes: serde_json::Value,
) {
    if let Err(e) = sqlx::query("INSERT INTO audit_log (user_id, entity_type, entity_id, action, changes) VALUES ($1, $2, $3, $4, $5)")
        .bind(user_id)
        .bind(entity_type)
        .bind(entity_id)
        .bind(action)
        .bind(changes)
        .execute(&s.pool)
        .await
    {
        tracing::warn!(error = %e, "no se pudo registrar auditoría");
    }
}
