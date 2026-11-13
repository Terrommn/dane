//! T54 Exportación de datos completos: JSON y ZIP (JSON + CSV + Markdown con formato del vault). Sin secretos.

use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::integrations::exporter;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(export_json))
        .routes(routes!(export_zip))
}

/// Todos los datos propios en JSON (excluye sesiones, identidades, credenciales y claves de idempotencia).
#[utoipa::path(get, path = "/exports/json", tag = "exports", responses((status = 200, body = Object)))]
async fn export_json(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<serde_json::Value>> {
    let v = exporter::export_json(&s, auth.user_id).await?;
    audit(&s, auth.user_id, "export", None, "json", serde_json::json!({})).await;
    Ok(Json(v))
}

/// ZIP con `vida-export.json`, CSV (transacciones, hábitos, tareas) y Markdown reimportable (formato del vault).
#[utoipa::path(get, path = "/exports/zip", tag = "exports", responses((status = 200, content_type = "application/zip", body = Vec<u8>)))]
async fn export_zip(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Response> {
    let bytes = exporter::export_zip(&s, auth.user_id).await?;
    audit(
        &s,
        auth.user_id,
        "export",
        None,
        "zip",
        serde_json::json!({ "bytes": bytes.len() }),
    )
    .await;
    let name = format!(
        "vida-export-{}.zip",
        crate::integrations::today(&s, auth.user_id).await
    );
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        bytes,
    )
        .into_response())
}
