//! T52 Captura libre: texto → propuesta tipada (LLM o manual) → vista previa/edición → confirmación → aplicación.
//! Nada se aplica sin confirmación; sin credencial de IA la propuesta se llena a mano (modo manual explícito).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::capture::{self, Proposal, Validated};
use persistence::repos::captures::{self as repo, CaptureRow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::integrations::{apply, llm};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_captures, create_capture))
        .routes(routes!(get_capture, update_capture))
        .routes(routes!(apply_capture))
        .routes(routes!(reject_capture))
        .routes(routes!(suggest))
}

#[derive(Serialize, ToSchema)]
pub struct CaptureView {
    pub id: Uuid,
    /// `daily | dump | gasto | ingreso | objetivos | weekly_review | log_session | inbox | free`
    pub intent: String,
    pub input_text: String,
    pub source: String,
    pub reference_date: NaiveDate,
    /// `pending | needs_input | applied | rejected | failed`
    pub status: String,
    /// `llm` (propuesta extraída) o `manual` (sin IA; el usuario llena la propuesta).
    pub mode: String,
    /// Propuesta tipada: expenses, incomes, tasks, inbox, log, reflections_good/bad, interactions, checkin, goals, devlog, summary.
    #[schema(value_type = Object)]
    pub proposal: Value,
    #[schema(value_type = Vec<Object>)]
    pub questions: Value,
    #[schema(value_type = Vec<String>)]
    pub errors: Value,
    #[schema(value_type = Option<Object>)]
    pub applied: Option<Value>,
    pub llm_configured: bool,
    pub llm_model: Option<String>,
    pub allowed_fields: Vec<String>,
    pub version: i32,
}

fn view(s: &AppState, r: CaptureRow) -> CaptureView {
    CaptureView {
        allowed_fields: capture::allowed_fields(&r.intent).iter().map(|x| x.to_string()).collect(),
        id: r.id,
        intent: r.intent,
        input_text: r.input_text,
        source: r.source,
        reference_date: r.reference_date,
        status: r.status,
        mode: r.mode,
        proposal: r.proposal,
        questions: r.questions,
        errors: r.errors,
        applied: r.applied,
        llm_configured: llm::configured(s),
        llm_model: r.llm_model,
        version: r.version,
    }
}

/// Si hay IA y la propuesta está vacía, extrae; si no, deja el modo manual con su estado explicado.
pub async fn extract_if_needed(s: &AppState, row: CaptureRow) -> ApiResult<CaptureRow> {
    let empty = row.proposal.as_object().is_none_or(|o| o.is_empty());
    if row.status != "pending" || !empty {
        return Ok(row);
    }
    if !llm::configured(s) {
        let errors = json!(["La IA no está configurada: completa la propuesta manualmente."]);
        return Ok(repo::save_proposal(&s.pool, row.user_id, row.id, Some(row.version), "pending", "manual", &json!(Proposal::default()), &json!([]), &errors, None)
            .await?
            .unwrap_or(row));
    }
    let ctx = llm::context(s, row.user_id, row.reference_date).await?;
    match llm::extract(s, &row.intent, &ctx, &row.input_text).await {
        Ok(p) => {
            let v = capture::validate(&row.intent, p, &row.input_text, &ctx, true);
            save_validated(s, &row, v, "llm", Some(row.version)).await
        }
        Err(e) => {
            tracing::warn!(error = %e, "extracción LLM fallida");
            let errors = json!([format!("{e}. Puedes completar la propuesta manualmente.")]);
            Ok(repo::save_proposal(&s.pool, row.user_id, row.id, Some(row.version), "failed", "manual", &json!(Proposal::default()), &json!([]), &errors, None)
                .await?
                .unwrap_or(row))
        }
    }
}

async fn save_validated(s: &AppState, row: &CaptureRow, v: Validated, mode: &str, version: Option<i32>) -> ApiResult<CaptureRow> {
    let blocking = apply::blocking_questions(&v.proposal);
    let status = if v.questions.is_empty() && blocking.is_empty() { "pending" } else { "needs_input" };
    let model = (mode == "llm").then(|| s.config.llm_model.clone());
    match repo::save_proposal(&s.pool, row.user_id, row.id, version, status, mode, &json!(v.proposal), &json!(v.questions), &json!(v.errors), model.as_deref()).await? {
        Some(r) => Ok(r),
        None => Err(ApiError::conflict(serde_json::to_value(view(s, repo::get(&s.pool, row.user_id, row.id).await?)).unwrap_or_default())),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct NewCapture {
    pub intent: String,
    pub text: String,
    /// Fecha de referencia ("hoy" para el usuario si se omite).
    pub reference_date: Option<NaiveDate>,
}

/// Crea una captura desde la app y, si hay IA, propone su interpretación.
#[utoipa::path(post, path = "/captures", tag = "captures", request_body = NewCapture, responses((status = 201, body = CaptureView)))]
async fn create_capture(State(s): State<AppState>, auth: AuthUser, Json(b): Json<NewCapture>) -> ApiResult<(StatusCode, Json<CaptureView>)> {
    if !capture::valid_intent(&b.intent) {
        return Err(ApiError::validation("intent", "comando desconocido"));
    }
    let text = domain::text::required("text", &b.text, 8000)?;
    let date = match b.reference_date {
        Some(d) => d,
        None => crate::integrations::today(&s, auth.user_id).await,
    };
    let row = repo::create(&s.pool, auth.user_id, &text, "app", &b.intent, date, &json!({})).await?;
    let row = extract_if_needed(&s, row).await?;
    Ok((StatusCode::CREATED, Json(view(&s, row))))
}

#[derive(Deserialize, IntoParams)]
pub struct CaptureFilter {
    pub status: Option<String>,
}

#[utoipa::path(get, path = "/captures", tag = "captures", params(CaptureFilter), responses((status = 200, body = Vec<CaptureView>)))]
async fn list_captures(State(s): State<AppState>, auth: AuthUser, Query(f): Query<CaptureFilter>) -> ApiResult<Json<Vec<CaptureView>>> {
    let rows = repo::list(&s.pool, auth.user_id, f.status.as_deref(), 100).await?;
    Ok(Json(rows.into_iter().map(|r| view(&s, r)).collect()))
}

/// Obtiene la captura; si viene del bot y aún no tiene propuesta, la extrae (o explica el modo manual).
#[utoipa::path(get, path = "/captures/{id}", tag = "captures", responses((status = 200, body = CaptureView), (status = 404, body = crate::error::ErrorBody)))]
async fn get_capture(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<Json<CaptureView>> {
    let row = repo::get(&s.pool, auth.user_id, id).await?;
    let row = extract_if_needed(&s, row).await?;
    Ok(Json(view(&s, row)))
}

#[derive(Deserialize, ToSchema)]
pub struct EditCapture {
    pub version: i32,
    #[schema(value_type = Object)]
    pub proposal: Value,
}

/// Edita la propuesta (vista previa). Se valida formato y catálogos; el autor es el usuario.
#[utoipa::path(put, path = "/captures/{id}", tag = "captures", request_body = EditCapture, responses((status = 200, body = CaptureView), (status = 409, body = crate::error::ErrorBody)))]
async fn update_capture(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>, Json(b): Json<EditCapture>) -> ApiResult<Json<CaptureView>> {
    let row = repo::get(&s.pool, auth.user_id, id).await?;
    let p: Proposal = serde_json::from_value(b.proposal).map_err(|e| ApiError::validation("proposal", format!("propuesta inválida: {e}")))?;
    let ctx = llm::context(&s, auth.user_id, row.reference_date).await?;
    let v = capture::validate(&row.intent, p, &row.input_text, &ctx, false);
    let mode = row.mode.clone();
    let saved = save_validated(&s, &row, v, &mode, Some(b.version)).await?;
    Ok(Json(view(&s, saved)))
}

#[derive(Deserialize, ToSchema)]
pub struct ApplyBody {
    pub version: i32,
}

/// Confirma y aplica la propuesta en una sola transacción. Repetir no duplica (claves de idempotencia por elemento).
#[utoipa::path(post, path = "/captures/{id}/apply", tag = "captures", request_body = ApplyBody, responses((status = 200, body = CaptureView), (status = 409, body = crate::error::ErrorBody), (status = 422, body = crate::error::ErrorBody)))]
async fn apply_capture(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>, Json(b): Json<ApplyBody>) -> ApiResult<Json<CaptureView>> {
    let row = repo::get(&s.pool, auth.user_id, id).await?;
    if row.status == "applied" {
        return Ok(Json(view(&s, row)));
    }
    if row.status == "rejected" {
        return Err(ApiError::new(StatusCode::CONFLICT, "rejected", "La captura fue descartada"));
    }
    if row.version != b.version {
        return Err(ApiError::conflict(serde_json::to_value(view(&s, row)).unwrap_or_default()));
    }
    let p: Proposal = serde_json::from_value(row.proposal.clone()).map_err(|_| ApiError::validation("proposal", "propuesta inválida"))?;
    let blocking = apply::blocking_questions(&p);
    if !blocking.is_empty() {
        return Err(ApiError {
            current: Some(json!(blocking)),
            ..ApiError::validation("proposal", "Faltan datos por confirmar antes de aplicar")
        });
    }
    if p.is_empty() && row.meta.get("habits").is_none() {
        return Err(ApiError::validation("proposal", "La propuesta está vacía"));
    }
    let origin = if row.source == "telegram" { "bot" } else if row.mode == "llm" { "llm" } else { "manual" };
    let mut tx = s.pool.begin().await?;
    // Bloqueo de la fila: dos confirmaciones simultáneas no aplican dos veces.
    let locked: Option<String> = sqlx::query_scalar("SELECT status FROM capture_proposals WHERE user_id = $1 AND id = $2 FOR UPDATE").bind(auth.user_id).bind(id).fetch_optional(&mut *tx).await?;
    if locked.as_deref() == Some("applied") {
        tx.rollback().await?;
        return Ok(Json(view(&s, repo::get(&s.pool, auth.user_id, id).await?)));
    }
    let applied = apply::apply(&mut tx, auth.user_id, &p, row.reference_date, origin, &format!("capture:{id}"), &row.meta)
        .await
        .map_err(|e| ApiError::validation("proposal", e))?;
    let done = repo::finish(&mut *tx, auth.user_id, id, "applied", Some(&applied)).await?.ok_or_else(ApiError::not_found)?;
    tx.commit().await?;
    audit(&s, auth.user_id, "capture", Some(id), "apply", json!({ "intent": done.intent })).await;
    Ok(Json(view(&s, done)))
}

/// Descarta la captura sin aplicar nada.
#[utoipa::path(post, path = "/captures/{id}/reject", tag = "captures", responses((status = 200, body = CaptureView)))]
async fn reject_capture(State(s): State<AppState>, auth: AuthUser, Path(id): Path<Uuid>) -> ApiResult<Json<CaptureView>> {
    let r = repo::finish(&s.pool, auth.user_id, id, "rejected", None).await?;
    match r {
        Some(r) => Ok(Json(view(&s, r))),
        None => Err(ApiError::new(StatusCode::CONFLICT, "not_pending", "La captura ya no está pendiente")),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct SuggestBody {
    /// `weekly_summary | inbox_classification | goal_habits`
    pub kind: String,
    pub iso_week: Option<String>,
    pub goal_id: Option<Uuid>,
}

/// Sugerencia asistida por IA sobre datos propios (resumen semanal, clasificación de Inbox, hábitos para un objetivo).
/// Se crea como propuesta que requiere confirmación; los datos se envían como texto delimitado y mínimo.
#[utoipa::path(post, path = "/captures/suggest", tag = "captures", request_body = SuggestBody, responses((status = 201, body = CaptureView), (status = 409, body = crate::error::ErrorBody)))]
async fn suggest(State(s): State<AppState>, auth: AuthUser, Json(b): Json<SuggestBody>) -> ApiResult<(StatusCode, Json<CaptureView>)> {
    if !llm::configured(&s) {
        return Err(ApiError::new(StatusCode::CONFLICT, "llm_not_configured", "La IA no está configurada; esta sugerencia requiere ANTHROPIC_API_KEY"));
    }
    let u = auth.user_id;
    let today = crate::integrations::today(&s, u).await;
    let (intent, text, meta) = match b.kind.as_str() {
        "weekly_summary" => {
            let week = b.iso_week.clone().unwrap_or_else(|| domain::time::iso_week_key(today));
            let p = domain::time::parse_iso_week(&week)?;
            let logs: Vec<(NaiveDate, String)> = sqlx::query_as("SELECT on_date, text FROM daily_log_entries WHERE user_id = $1 AND on_date BETWEEN $2 AND $3 ORDER BY on_date, position LIMIT 200")
                .bind(u)
                .bind(p.start)
                .bind(p.end)
                .fetch_all(&s.pool)
                .await?;
            let refl: Vec<(NaiveDate, String, String)> = sqlx::query_as("SELECT on_date, polarity, text FROM reflection_items WHERE user_id = $1 AND on_date BETWEEN $2 AND $3 ORDER BY on_date LIMIT 100")
                .bind(u)
                .bind(p.start)
                .bind(p.end)
                .fetch_all(&s.pool)
                .await?;
            let done: Vec<String> = sqlx::query_scalar("SELECT title FROM tasks WHERE user_id = $1 AND status = 'done' AND done_at::date BETWEEN $2 AND $3 LIMIT 100")
                .bind(u)
                .bind(p.start)
                .bind(p.end)
                .fetch_all(&s.pool)
                .await?;
            let mut t = format!("Semana {week}. Resume en 5 líneas qué pasó, sin inventar nada.\nBitácora:\n");
            for (d, l) in logs {
                t.push_str(&format!("- {d}: {l}\n"));
            }
            t.push_str("Reflexiones:\n");
            for (d, pol, l) in refl {
                t.push_str(&format!("- {d} ({}): {l}\n", if pol == "good" { "bien" } else { "mal" }));
            }
            t.push_str("Pendientes cerrados:\n");
            for x in done {
                t.push_str(&format!("- {x}\n"));
            }
            ("weekly_review", t, json!({ "iso_week": week, "kind": "weekly_summary" }))
        }
        "inbox_classification" => {
            let items: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, text FROM inbox_items WHERE user_id = $1 AND status = 'pending' ORDER BY captured_at LIMIT 50")
                .bind(u)
                .fetch_all(&s.pool)
                .await?;
            if items.is_empty() {
                return Err(ApiError::validation("kind", "El Inbox está vacío"));
            }
            let mut t = String::from("Convierte en pendientes (tasks) los elementos del Inbox que sean acciones; clasifica cada uno en un área.\n");
            for (_, x) in &items {
                t.push_str(&format!("- {x}\n"));
            }
            ("inbox", t, json!({ "inbox_ids": items.iter().map(|i| i.0).collect::<Vec<_>>(), "kind": "inbox_classification" }))
        }
        "goal_habits" => {
            let gid = b.goal_id.ok_or_else(|| ApiError::validation("goal_id", "obligatorio"))?;
            let g: (String, String, String, String) = sqlx::query_as("SELECT title, kind, motivation, success_criteria FROM goals WHERE user_id = $1 AND id = $2")
                .bind(u)
                .bind(gid)
                .fetch_optional(&s.pool)
                .await?
                .ok_or_else(ApiError::not_found)?;
            let t = format!(
                "Propón de 1 a 3 hábitos diarios (acciones marcables en una línea) para el objetivo: {}. Por qué importa: {}. Cómo sabré que lo logré: {}. Devuélvelos en goals[0].habits con el mismo título y kind {}.",
                g.0, g.2, g.3, g.1
            );
            ("objetivos", t, json!({ "goal_id": gid, "kind": "goal_habits" }))
        }
        _ => return Err(ApiError::validation("kind", "tipo de sugerencia desconocido")),
    };
    let row = repo::create(&s.pool, u, &text, "app", intent, today, &meta).await?;
    let row = extract_if_needed(&s, row).await?;
    // Hábitos sugeridos para un objetivo existente: se aplican al objetivo, no como objetivo nuevo.
    let row = if b.kind == "goal_habits" {
        let p: Proposal = serde_json::from_value(row.proposal.clone()).unwrap_or_default();
        let habits: Vec<String> = p.goals.iter().flat_map(|g| g.habits.clone()).take(3).collect();
        let mut meta = row.meta.clone();
        meta["habits"] = json!(habits);
        sqlx::query("UPDATE capture_proposals SET meta = $3, proposal = $4 WHERE user_id = $1 AND id = $2")
            .bind(u)
            .bind(row.id)
            .bind(&meta)
            .bind(json!(Proposal::default()))
            .execute(&s.pool)
            .await?;
        repo::get(&s.pool, u, row.id).await?
    } else {
        row
    };
    Ok((StatusCode::CREATED, Json(view(&s, row))))
}
