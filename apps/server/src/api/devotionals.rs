//! T28 Devocionales / reflexión: un registro por fecha, plantilla personalizable, modo meditación, historial con
//! búsqueda, y vínculo explícito "devocional con contenido real → hábito". Propietario: agente A-core.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::checkin::{DevotionalContent, VerseAction, devotional_has_content, normalize_verse_actions};
use persistence::hooks::{self, HabitTrigger};
use persistence::repos::devotionals::{self as repo, Devotional, DevotionalInput};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::audit;
use crate::api::habits::{today, user_ctx};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::http::{ListParams, Page};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_devotionals))
        .routes(routes!(get_template))
        .routes(routes!(get_devotional, put_devotional, delete_devotional))
}

#[derive(Serialize, ToSchema)]
pub struct DevotionalPage {
    pub items: Vec<Devotional>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Historial (Devocionales.base: fecha desc con pasaje) con búsqueda sin acentos.
#[utoipa::path(get, path = "/devotionals", tag = "devotionals", params(ListParams), responses((status = 200, body = DevotionalPage)))]
async fn list_devotionals(
    State(s): State<AppState>,
    auth: AuthUser,
    axum::extract::Query(p): Query<ListParams>,
) -> ApiResult<Json<DevotionalPage>> {
    let (items, total) = repo::list(&s.pool, auth.user_id, p.q().as_deref(), p.limit(), p.offset()).await?;
    let page = Page {
        items,
        total,
        limit: p.limit(),
        offset: p.offset(),
    };
    Ok(Json(DevotionalPage {
        items: page.items,
        total: page.total,
        limit: page.limit,
        offset: page.offset,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct TemplateSection {
    pub key: String,
    pub label: String,
    /// `text | verse_actions`
    pub kind: String,
}

#[derive(Serialize, ToSchema)]
pub struct DevotionalTemplate {
    /// `devotional | meditation`
    pub mode: String,
    pub enabled: bool,
    pub sections: Vec<TemplateSection>,
}

fn default_sections(mode: &str) -> Vec<TemplateSection> {
    let mk = |k: &str, l: &str, kind: &str| TemplateSection {
        key: k.into(),
        label: l.into(),
        kind: kind.into(),
    };
    if mode == "meditation" {
        vec![
            mk("passage", "Tema o texto", "text"),
            mk("learned", "¿Qué noté?", "text"),
            mk("prayer", "Intención", "text"),
        ]
    } else {
        vec![
            mk("passage", "Pasaje", "text"),
            mk(
                "verse_actions",
                "¿Cómo puedo agradar a Dios hoy?",
                "verse_actions",
            ),
            mk("learned", "¿Qué aprendí?", "text"),
            mk("teaching", "¿Qué enseñanza puedo compartir?", "text"),
            mk("prayer", "Oración", "text"),
            mk("song", "Canción", "text"),
        ]
    }
}

/// Plantilla vigente: la personalizada en ajustes (`devotional_template`) o la del vault según el modo.
#[utoipa::path(get, path = "/devotionals/template", tag = "devotionals", responses((status = 200, body = DevotionalTemplate)))]
async fn get_template(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<DevotionalTemplate>> {
    let (_, st) = user_ctx(&s.pool, auth.user_id).await?;
    let custom: Vec<TemplateSection> = st
        .devotional_template
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some(TemplateSection {
                        key: x.get("key")?.as_str()?.to_string(),
                        label: x
                            .get("label")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string(),
                        kind: x
                            .get("kind")
                            .and_then(|v| v.as_str())
                            .unwrap_or("text")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let enabled = st
        .modules
        .get("devotional")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    Ok(Json(DevotionalTemplate {
        sections: if custom.is_empty() {
            default_sections(&st.devotional_mode)
        } else {
            custom
        },
        mode: st.devotional_mode,
        enabled,
    }))
}

#[utoipa::path(get, path = "/devotionals/{date}", tag = "devotionals", responses((status = 200, body = Devotional), (status = 404, body = crate::error::ErrorBody)))]
async fn get_devotional(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
) -> ApiResult<Json<Devotional>> {
    repo::get(&s.pool, auth.user_id, date)
        .await?
        .map(Json)
        .ok_or_else(ApiError::not_found)
}

#[derive(Deserialize, ToSchema)]
pub struct DevotionalBody {
    /// `devotional | meditation` (por defecto, el modo de ajustes).
    pub mode: Option<String>,
    #[serde(default)]
    pub passage: String,
    #[serde(default)]
    pub verse_actions: Vec<VerseActionBody>,
    #[serde(default)]
    pub learned: String,
    #[serde(default)]
    pub teaching: String,
    #[serde(default)]
    pub prayer: String,
    #[serde(default)]
    pub song: String,
    /// Secciones personalizadas `{clave: texto}`.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub extra: serde_json::Value,
    /// 0 o ausente al crear.
    pub version: Option<i32>,
}

#[derive(Deserialize, ToSchema)]
pub struct VerseActionBody {
    #[serde(default)]
    pub verse: String,
    #[serde(default)]
    pub action: String,
}

/// Crea/actualiza el devocional del día. Con contenido real marca el hábito vinculado (`devotional_content`) de esa
/// fecha; si queda vacío, retira la marca automática.
#[utoipa::path(put, path = "/devotionals/{date}", tag = "devotionals", request_body = DevotionalBody,
    responses((status = 200, body = Devotional), (status = 409, body = crate::error::ErrorBody)))]
async fn put_devotional(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
    Json(b): Json<DevotionalBody>,
) -> ApiResult<Json<Devotional>> {
    let (tz, st) = user_ctx(&s.pool, auth.user_id).await?;
    if date > today(&s, tz) {
        return Err(ApiError::validation(
            "date",
            "no se registran devocionales de días futuros",
        ));
    }
    let mode = b.mode.clone().unwrap_or(st.devotional_mode.clone());
    if !["devotional", "meditation"].contains(&mode.as_str()) {
        return Err(ApiError::validation("mode", "devotional o meditation"));
    }
    let rows = normalize_verse_actions(
        b.verse_actions
            .into_iter()
            .map(|v| VerseAction {
                verse: v.verse,
                action: v.action,
            })
            .collect(),
    )?;
    let extra = match b.extra {
        serde_json::Value::Null => serde_json::json!({}),
        serde_json::Value::Object(m) if m.values().all(|v| v.is_string()) => serde_json::Value::Object(m),
        _ => return Err(ApiError::validation("extra", "debe ser un objeto {clave: texto}")),
    };
    let extra_texts: Vec<String> = extra
        .as_object()
        .map(|m| {
            m.values()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut sections: Vec<&str> = vec![&b.learned, &b.teaching, &b.prayer, &b.song];
    sections.extend(extra_texts.iter().map(String::as_str));
    let has_content = devotional_has_content(&DevotionalContent {
        passage: &b.passage,
        verse_actions: &rows,
        sections: &sections,
    });
    let va = serde_json::to_value(&rows).unwrap_or_default();
    let mut tx = s.pool.begin().await?;
    let d = repo::upsert(
        &mut tx,
        auth.user_id,
        date,
        b.version,
        &DevotionalInput {
            mode: &mode,
            passage: b.passage.trim(),
            verse_actions: &va,
            learned: &b.learned,
            teaching: &b.teaching,
            prayer: &b.prayer,
            song: &b.song,
            extra: &extra,
        },
    )
    .await?;
    if has_content {
        hooks::mark_linked_habit(&mut tx, auth.user_id, HabitTrigger::DevotionalContent, date).await?;
    } else {
        hooks::unmark_linked_habit(&mut tx, auth.user_id, HabitTrigger::DevotionalContent, date).await?;
    }
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "devotional",
        Some(d.id),
        "save",
        serde_json::json!({ "date": date, "has_content": has_content }),
    )
    .await;
    Ok(Json(d))
}

#[utoipa::path(delete, path = "/devotionals/{date}", tag = "devotionals", responses((status = 204)))]
async fn delete_devotional(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
) -> ApiResult<StatusCode> {
    let mut tx = s.pool.begin().await?;
    if !repo::delete(&mut tx, auth.user_id, date).await? {
        return Err(ApiError::not_found());
    }
    hooks::unmark_linked_habit(&mut tx, auth.user_id, HabitTrigger::DevotionalContent, date).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
