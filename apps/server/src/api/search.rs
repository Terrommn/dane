//! Propietario: agente B-modules. T34 Búsqueda global tolerante a acentos, aislada por usuario.

use axum::Json;
use axum::extract::{Query, State};
use chrono::NaiveDate;
use domain::text::fold;
use persistence::repos::notes::{self as repo, SearchFilter, SearchHit};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(search))
}

#[derive(Deserialize, IntoParams)]
pub struct SearchQuery {
    /// Texto a buscar (sin importar acentos ni mayúsculas; admite prefijos).
    pub q: String,
    /// Módulos separados por coma (`note,person,task,…`).
    pub types: Option<String>,
    pub area_id: Option<Uuid>,
    pub status: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Incluir archivados (por defecto no).
    pub archived: Option<bool>,
    pub limit: Option<i64>,
}

/// Normaliza la consulta a tokens alfanuméricos sin acentos (seguros para `to_tsquery`).
pub fn tokens(q: &str) -> Vec<String> {
    fold(q)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .filter(|t| t.chars().all(|c| c.is_ascii_alphanumeric()))
        .take(12)
        .map(str::to_string)
        .collect()
}

#[utoipa::path(get, path = "/search", tag = "search", params(SearchQuery), responses((status = 200, body = Vec<SearchHit>)))]
async fn search(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<Vec<SearchHit>>> {
    let raw = q.q.trim().chars().take(200).collect::<String>();
    let toks = tokens(&raw);
    if toks.is_empty() {
        return Err(ApiError::validation("q", "escribe al menos una palabra"));
    }
    let types: Vec<String> = q
        .types
        .as_deref()
        .map(|t| {
            t.split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| repo::table_of(x).is_some())
                .collect()
        })
        .unwrap_or_default();
    let f = SearchFilter {
        tokens: toks,
        raw: fold(&raw),
        types,
        area_id: q.area_id,
        status: q.status,
        from: q.from,
        to: q.to,
        include_archived: q.archived.unwrap_or(false),
        limit: q.limit.unwrap_or(30).clamp(1, 100),
    };
    Ok(Json(repo::search(&s.pool, auth.user_id, &f).await?))
}

#[cfg(test)]
mod tests {
    #[test]
    fn tokens_are_sanitized() {
        assert_eq!(
            super::tokens("Canción & oración'); DROP"),
            vec!["cancion", "oracion", "drop"]
        );
        assert!(super::tokens("!!!").is_empty());
    }
}
