//! T17 Objetivos mes/semana y planificación; T18 progreso cuantitativo (eventos, fuentes, enlaces que aportan).
//! Propietario: agente A-core.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use domain::goals::{self as dg, Direction, GoalProgress, GoalProgressInput, ProgressEvent, ProgressType};
use domain::metrics::{self, Compliance};
use domain::text::required;
use domain::time::{Period, iso_week_key, iso_week_period, month_key, parse_iso_week, parse_month};
use persistence::hooks::{self, ProgressValue};
use persistence::repos::goals::{self as repo, Goal, GoalFilter, GoalInput, GoalLink, ProgressEventRow};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::api::habits::{LoadedHabit, load_habits, today, user_ctx};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_goals, create_goal))
        .routes(routes!(planning))
        .routes(routes!(get_goal, update_goal))
        .routes(routes!(close_goal))
        .routes(routes!(add_progress))
        .routes(routes!(delete_progress))
        .routes(routes!(add_link))
        .routes(routes!(delete_link))
}

#[derive(Serialize, ToSchema, Clone)]
pub struct HabitBrief {
    pub id: Uuid,
    pub name: String,
    pub unit: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct GoalView {
    pub goal: Goal,
    /// Progreso del resultado (independiente de la adherencia a hábitos).
    #[schema(value_type = Object)]
    pub progress: GoalProgress,
    /// Periodo terminado y estado aún activo: falta cerrarlo (distinto de archivar).
    pub closure_pending: bool,
    pub habits: Vec<HabitBrief>,
    /// Adherencia cerrada de los hábitos del objetivo en su periodo.
    #[schema(value_type = Object)]
    pub habit_adherence: Compliance,
}

#[derive(Serialize, ToSchema)]
pub struct GoalDetail {
    #[serde(flatten)]
    pub view: GoalView,
    pub events: Vec<ProgressEventRow>,
    pub links: Vec<GoalLink>,
    /// Objetivos semanales que apoyan a este (si es mensual).
    pub children: Vec<Goal>,
    pub parent: Option<Goal>,
}

fn period_of(g: &Goal) -> Period {
    Period {
        start: g.starts_on,
        end: g.ends_on,
    }
}

/// Progreso de un objetivo según su fuente. Hábito: suma de cantidades `done` en la misma unidad; peso: mediciones del
/// check-in; resto: eventos registrados.
pub async fn compute_progress(
    s: &AppState,
    user_id: Uuid,
    g: &Goal,
    events: &[ProgressEventRow],
    today: NaiveDate,
) -> ApiResult<GoalProgress> {
    let ptype = ProgressType::parse(&g.progress_type)?;
    let evs: Vec<ProgressEvent> = match g.progress_source.as_str() {
        "habit" => match g.source_ref {
            Some(h) => repo::habit_quantities(&s.pool, user_id, h, g.starts_on, g.ends_on)
                .await?
                .into_iter()
                .filter_map(|(on, q)| {
                    Some(ProgressEvent {
                        on,
                        amount: Some(q?),
                        value: None,
                        completed: None,
                    })
                })
                .collect(),
            None => vec![],
        },
        "weight" => repo::weight_points(&s.pool, user_id, g.starts_on, g.ends_on)
            .await?
            .into_iter()
            .map(|(on, v)| ProgressEvent {
                on,
                amount: None,
                value: Some(v),
                completed: None,
            })
            .collect(),
        _ => events
            .iter()
            .filter(|e| e.goal_id == g.id)
            .map(|e| ProgressEvent {
                on: e.occurred_on,
                amount: e.amount,
                value: e.value,
                completed: e.completed,
            })
            .collect(),
    };
    let direction = match g.direction.as_deref() {
        Some("increase") => Some(Direction::Increase),
        Some("decrease") => Some(Direction::Decrease),
        _ => None,
    };
    Ok(dg::progress(
        &GoalProgressInput {
            progress_type: ptype,
            baseline: g.baseline,
            target: g.target,
            direction,
            period: period_of(g),
        },
        &evs,
        today,
    ))
}

pub fn adherence(habits: &[LoadedHabit], goal: &Goal, today: NaiveDate) -> Compliance {
    let items: Vec<Compliance> = habits
        .iter()
        .filter(|h| h.habit.goal_id == Some(goal.id))
        .map(|h| metrics::compliance(&h.spec, &h.entries, period_of(goal), today))
        .collect();
    Compliance::merge(&items)
}

/// Construye vistas para una lista de objetivos (carga eventos y hábitos una vez).
pub async fn views(
    s: &AppState,
    user_id: Uuid,
    goals: Vec<Goal>,
    today: NaiveDate,
) -> ApiResult<Vec<GoalView>> {
    if goals.is_empty() {
        return Ok(vec![]);
    }
    let (tz, _) = user_ctx(&s.pool, user_id).await?;
    let ids: Vec<Uuid> = goals.iter().map(|g| g.id).collect();
    let events = repo::events(&s.pool, user_id, &ids).await?;
    let from = goals.iter().map(|g| g.starts_on).min().unwrap_or(today);
    let to = goals.iter().map(|g| g.ends_on).max().unwrap_or(today).max(today);
    let habits = load_habits(&s.pool, user_id, tz, from, to, true).await?;
    let mut out = Vec::new();
    for g in goals {
        let progress = compute_progress(s, user_id, &g, &events, today).await?;
        out.push(GoalView {
            closure_pending: dg::closure_pending(&g.status, g.ends_on, today),
            habits: habits
                .iter()
                .filter(|h| h.habit.goal_id == Some(g.id) && h.habit.archived_at.is_none())
                .map(|h| HabitBrief {
                    id: h.habit.id,
                    name: h.habit.name.clone(),
                    unit: h.habit.unit.clone(),
                })
                .collect(),
            habit_adherence: adherence(&habits, &g, today),
            progress,
            goal: g,
        });
    }
    Ok(out)
}

#[derive(Deserialize, IntoParams)]
pub struct GoalQuery {
    /// `month | week`
    pub kind: Option<String>,
    pub period_key: Option<String>,
    pub status: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub archived: Option<bool>,
}

/// Objetivos (vista Objetivos.base: agrupables por tipo, con periodo, inicio, fin y estado).
#[utoipa::path(get, path = "/goals", tag = "goals", params(GoalQuery), responses((status = 200, body = Vec<GoalView>)))]
async fn list_goals(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<GoalQuery>,
) -> ApiResult<Json<Vec<GoalView>>> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let goals = repo::list(
        &s.pool,
        auth.user_id,
        &GoalFilter {
            kind: q.kind,
            period_key: q.period_key,
            status: q.status,
            from: q.from,
            to: q.to,
            include_archived: q.archived.unwrap_or(false),
        },
    )
    .await?;
    Ok(Json(views(&s, auth.user_id, goals, today(&s, tz)).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct GoalBody {
    /// `month | week` (solo al crear).
    pub kind: Option<String>,
    /// `YYYY-MM` o `GGGG-Www` (solo al crear).
    pub period_key: Option<String>,
    pub title: String,
    pub area_id: Option<Uuid>,
    #[serde(default)]
    pub motivation: String,
    #[serde(default)]
    pub success_criteria: String,
    /// `active | achieved | not_achieved` (para cerrar usa `POST /goals/{id}/close`).
    #[serde(default = "active")]
    pub status: String,
    /// Objetivo mensual al que apoya (solo semanales).
    pub parent_goal_id: Option<Uuid>,
    #[serde(default = "none")]
    pub progress_type: String,
    pub unit: Option<String>,
    #[schema(value_type = Option<String>)]
    pub baseline: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub target: Option<Decimal>,
    /// `increase | decrease`
    pub direction: Option<String>,
    /// `manual | readings | habit | weight | debt | finance`
    #[serde(default = "manual")]
    pub progress_source: String,
    /// Hábito (fuente `habit`) o deuda (fuente `debt`).
    pub source_ref: Option<Uuid>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

fn active() -> String {
    "active".into()
}
fn none() -> String {
    "none".into()
}
fn manual() -> String {
    "manual".into()
}

/// Validaciones de coherencia (dueño, padre mensual, fuente y unidad).
async fn validate_body(
    s: &AppState,
    user_id: Uuid,
    kind: &str,
    period: Period,
    b: &GoalBody,
) -> ApiResult<()> {
    let ptype = ProgressType::parse(&b.progress_type)?;
    dg::validate_goal(&b.status, ptype, b.unit.as_deref(), b.target)?;
    if let Some(d) = &b.direction
        && !["increase", "decrease"].contains(&d.as_str())
    {
        return Err(ApiError::validation("direction", "increase o decrease"));
    }
    if let Some(pid) = b.parent_goal_id {
        if kind != "week" {
            return Err(ApiError::validation(
                "parent_goal_id",
                "solo un objetivo semanal apoya a uno mensual",
            ));
        }
        let parent = repo::get(&s.pool, user_id, pid)
            .await
            .map_err(|_| ApiError::validation("parent_goal_id", "objetivo mensual inexistente"))?;
        if parent.kind != "month" {
            return Err(ApiError::validation(
                "parent_goal_id",
                "el objetivo padre debe ser mensual",
            ));
        }
        if period.intersect(&period_of(&parent)).is_none() {
            return Err(ApiError::validation(
                "parent_goal_id",
                "la semana no cae dentro de ese mes",
            ));
        }
    }
    match b.progress_source.as_str() {
        "manual" | "readings" | "finance" => {}
        "habit" => {
            if ptype != ProgressType::Cumulative {
                return Err(ApiError::validation(
                    "progress_type",
                    "un hábito solo alimenta objetivos acumulativos",
                ));
            }
            let hid = b
                .source_ref
                .ok_or_else(|| ApiError::validation("source_ref", "elige el hábito fuente"))?;
            let h = persistence::repos::habits::get(&s.pool, user_id, hid)
                .await
                .map_err(|_| ApiError::validation("source_ref", "hábito inexistente"))?;
            let ok = matches!((&h.unit, &b.unit), (Some(hu), Some(gu)) if dg::units_compatible(hu, gu));
            if !ok {
                return Err(ApiError::validation(
                    "unit",
                    "el hábito y el objetivo usan unidades distintas; registrar el hábito no suma unidades incompatibles",
                ));
            }
        }
        "weight" => {
            if ptype != ProgressType::CurrentValue {
                return Err(ApiError::validation(
                    "progress_type",
                    "el peso es un valor actual",
                ));
            }
        }
        "debt" => {
            let did = b
                .source_ref
                .ok_or_else(|| ApiError::validation("source_ref", "elige la deuda"))?;
            let exists: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM debts WHERE user_id = $1 AND id = $2")
                    .bind(user_id)
                    .bind(did)
                    .fetch_optional(&s.pool)
                    .await?;
            if exists.is_none() {
                return Err(ApiError::validation("source_ref", "deuda inexistente"));
            }
        }
        _ => return Err(ApiError::validation("progress_source", "fuente inválida")),
    }
    Ok(())
}

fn resolve_period(kind: &str, key: &str) -> ApiResult<Period> {
    match kind {
        "month" => Ok(parse_month(key)?),
        "week" => Ok(parse_iso_week(key)?),
        _ => Err(ApiError::validation("kind", "month o week")),
    }
}

fn input<'a>(kind: &'a str, key: &'a str, p: Period, title: &'a str, b: &'a GoalBody) -> GoalInput<'a> {
    GoalInput {
        kind,
        period_key: key,
        starts_on: p.start,
        ends_on: p.end,
        title,
        area_id: b.area_id,
        motivation: &b.motivation,
        success_criteria: &b.success_criteria,
        status: &b.status,
        parent_goal_id: b.parent_goal_id,
        progress_type: &b.progress_type,
        unit: b.unit.as_deref().map(str::trim).filter(|u| !u.is_empty()),
        baseline: b.baseline,
        target: b.target,
        direction: b.direction.as_deref(),
        progress_source: &b.progress_source,
        source_ref: b.source_ref,
        position: b.position,
        archived: b.archived,
    }
}

/// Crea un objetivo. No se crean objetivos sin decisión explícita del usuario (esta llamada es esa decisión).
#[utoipa::path(post, path = "/goals", tag = "goals", request_body = GoalBody, responses((status = 201, body = GoalView)))]
async fn create_goal(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<GoalBody>,
) -> ApiResult<(StatusCode, Json<GoalView>)> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let kind = b
        .kind
        .clone()
        .ok_or_else(|| ApiError::validation("kind", "month o week"))?;
    let key = b
        .period_key
        .clone()
        .ok_or_else(|| ApiError::validation("period_key", "indica el periodo (YYYY-MM o GGGG-Www)"))?;
    let period = resolve_period(&kind, &key)?;
    let title = required("title", &b.title, 200)?;
    validate_body(&s, auth.user_id, &kind, period, &b).await?;
    let g = repo::create(&s.pool, auth.user_id, &input(&kind, &key, period, &title, &b)).await?;
    audit(
        &s,
        auth.user_id,
        "goal",
        Some(g.id),
        "create",
        serde_json::json!({ "title": title, "period": key }),
    )
    .await;
    let v = views(&s, auth.user_id, vec![g], today(&s, tz)).await?.remove(0);
    Ok((StatusCode::CREATED, Json(v)))
}

#[utoipa::path(get, path = "/goals/{id}", tag = "goals", responses((status = 200, body = GoalDetail), (status = 404, body = crate::error::ErrorBody)))]
async fn get_goal(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<GoalDetail>> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let g = repo::get(&s.pool, auth.user_id, id).await?;
    let events = repo::events(&s.pool, auth.user_id, &[id]).await?;
    let links = repo::links(&s.pool, auth.user_id, id).await?;
    let children: Vec<Goal> = repo::list(
        &s.pool,
        auth.user_id,
        &GoalFilter {
            kind: Some("week".into()),
            period_key: None,
            status: None,
            from: Some(g.starts_on),
            to: Some(g.ends_on),
            include_archived: true,
        },
    )
    .await?
    .into_iter()
    .filter(|c| c.parent_goal_id == Some(id))
    .collect();
    let parent = match g.parent_goal_id {
        Some(p) => Some(repo::get(&s.pool, auth.user_id, p).await?),
        None => None,
    };
    let view = views(&s, auth.user_id, vec![g], today(&s, tz)).await?.remove(0);
    Ok(Json(GoalDetail {
        view,
        events,
        links,
        children,
        parent,
    }))
}

/// Edita el objetivo (no cambia tipo ni periodo). Logros subjetivos se confirman con el cierre explícito.
#[utoipa::path(put, path = "/goals/{id}", tag = "goals", request_body = GoalBody,
    responses((status = 200, body = GoalView), (status = 409, body = crate::error::ErrorBody)))]
async fn update_goal(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<GoalBody>,
) -> ApiResult<Json<GoalView>> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    let cur = repo::get(&s.pool, auth.user_id, id).await?;
    let title = required("title", &b.title, 200)?;
    if b.parent_goal_id == Some(id) {
        return Err(ApiError::validation(
            "parent_goal_id",
            "un objetivo no puede apoyarse a sí mismo",
        ));
    }
    validate_body(&s, auth.user_id, &cur.kind, period_of(&cur), &b).await?;
    let g = repo::update(
        &s.pool,
        auth.user_id,
        id,
        version,
        &input(&cur.kind, &cur.period_key, period_of(&cur), &title, &b),
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "goal",
        Some(id),
        "update",
        serde_json::json!({ "title": title, "status": b.status }),
    )
    .await;
    Ok(Json(
        views(&s, auth.user_id, vec![g], today(&s, tz)).await?.remove(0),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct CloseBody {
    /// `achieved | not_achieved` — decisión del usuario.
    pub status: String,
    #[serde(default)]
    pub achievement: String,
    #[serde(default)]
    pub learning: String,
    #[serde(default)]
    pub difficulties: String,
    #[serde(default)]
    pub adjustments: String,
    pub version: i32,
}

/// Cierre del objetivo (acción diferenciada de editar o archivar): estado final + logro, aprendizaje, dificultades y ajustes.
#[utoipa::path(post, path = "/goals/{id}/close", tag = "goals", request_body = CloseBody,
    responses((status = 200, body = GoalView), (status = 409, body = crate::error::ErrorBody)))]
async fn close_goal(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<CloseBody>,
) -> ApiResult<Json<GoalView>> {
    if !["achieved", "not_achieved"].contains(&b.status.as_str()) {
        return Err(ApiError::validation(
            "status",
            "al cerrar elige achieved o not_achieved",
        ));
    }
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let closing = serde_json::json!({
        "achievement": b.achievement.trim(), "learning": b.learning.trim(),
        "difficulties": b.difficulties.trim(), "adjustments": b.adjustments.trim(),
    });
    let g = repo::close(&s.pool, auth.user_id, id, b.version, &b.status, &closing).await?;
    audit(
        &s,
        auth.user_id,
        "goal",
        Some(id),
        "close",
        serde_json::json!({ "status": b.status }),
    )
    .await;
    Ok(Json(
        views(&s, auth.user_id, vec![g], today(&s, tz)).await?.remove(0),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct ProgressBody {
    pub occurred_on: NaiveDate,
    /// Incremento (acumulativos).
    #[schema(value_type = Option<String>)]
    pub amount: Option<Decimal>,
    /// Medición (valor actual).
    #[schema(value_type = Option<String>)]
    pub value: Option<Decimal>,
    /// Hito completado.
    pub completed: Option<bool>,
    #[serde(default)]
    pub note: String,
}

/// Evento manual de progreso (debe coincidir con el tipo de progreso del objetivo).
#[utoipa::path(post, path = "/goals/{id}/progress", tag = "goals", request_body = ProgressBody, responses((status = 201, body = GoalDetail)))]
async fn add_progress(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ProgressBody>,
) -> ApiResult<(StatusCode, Json<GoalDetail>)> {
    let g = repo::get(&s.pool, auth.user_id, id).await?;
    if g.progress_source != "manual" && g.progress_source != "readings" {
        return Err(ApiError::validation(
            "progress_source",
            "este objetivo se alimenta de su fuente; no admite eventos manuales",
        ));
    }
    let value = match (
        ProgressType::parse(&g.progress_type)?,
        b.amount,
        b.value,
        b.completed,
    ) {
        (ProgressType::Cumulative, Some(a), None, None) if a > Decimal::ZERO => ProgressValue::Amount(a),
        (ProgressType::CurrentValue, None, Some(v), None) => ProgressValue::Value(v),
        (ProgressType::Milestone, None, None, Some(true)) => ProgressValue::Completed,
        _ => {
            return Err(ApiError::validation(
                "amount",
                "el evento no corresponde al tipo de progreso del objetivo",
            ));
        }
    };
    let mut tx = s.pool.begin().await?;
    hooks::record_goal_progress(
        &mut tx,
        auth.user_id,
        id,
        b.occurred_on,
        value,
        "manual",
        Uuid::new_v4(),
        b.note.trim(),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, get_goal(State(s), auth, Path(id)).await?))
}

#[utoipa::path(delete, path = "/goals/{id}/progress/{event_id}", tag = "goals", responses((status = 204)))]
async fn delete_progress(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((id, event_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    repo::delete_manual_event(&s.pool, auth.user_id, id, event_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct LinkBody {
    /// `reading | content | task | debt`
    pub src_type: String,
    pub src_id: Uuid,
}

/// Aplica el aporte actual de una fuente enlazada (si ya cumple su condición). Idempotente.
pub async fn sync_source_contribution(
    tx: &mut persistence::Tx<'_>,
    user_id: Uuid,
    tz: &str,
    src_type: &str,
    src_id: Uuid,
    goal_ids: &[Uuid],
) -> ApiResult<()> {
    let done_on: Option<NaiveDate> = match src_type {
        "reading" => sqlx::query_scalar("SELECT finished_on FROM readings WHERE user_id = $1 AND id = $2 AND status = 'finished'")
            .bind(user_id)
            .bind(src_id)
            .fetch_optional(&mut **tx)
            .await?
            .flatten(),
        "content" => sqlx::query_scalar("SELECT coalesce(published_on, publish_on) FROM content_pieces WHERE user_id = $1 AND id = $2 AND status = 'published'")
            .bind(user_id)
            .bind(src_id)
            .fetch_optional(&mut **tx)
            .await?
            .flatten(),
        "task" => sqlx::query_scalar("SELECT (done_at AT TIME ZONE $3)::date FROM tasks WHERE user_id = $1 AND id = $2 AND status = 'done'")
            .bind(user_id)
            .bind(src_id)
            .bind(tz)
            .fetch_optional(&mut **tx)
            .await?
            .flatten(),
        "debt" => sqlx::query_scalar("SELECT paid_on FROM debts WHERE user_id = $1 AND id = $2 AND status = 'paid'")
            .bind(user_id)
            .bind(src_id)
            .fetch_optional(&mut **tx)
            .await?
            .flatten(),
        _ => None,
    };
    for gid in goal_ids {
        match done_on {
            Some(on) => {
                let pt: String =
                    sqlx::query_scalar("SELECT progress_type FROM goals WHERE user_id = $1 AND id = $2")
                        .bind(user_id)
                        .bind(gid)
                        .fetch_one(&mut **tx)
                        .await?;
                let v = if pt == "milestone" || src_type == "debt" {
                    ProgressValue::Completed
                } else {
                    ProgressValue::Amount(Decimal::ONE)
                };
                hooks::record_goal_progress(tx, user_id, *gid, on, v, src_type, src_id, "").await?;
            }
            None => {
                hooks::remove_goal_progress(tx, user_id, src_type, src_id, Some(*gid)).await?;
            }
        }
    }
    Ok(())
}

/// Enlaza una entidad para que aporte al objetivo (una vez por objetivo; enlazarla a la semana y al mes aporta una vez a cada uno).
#[utoipa::path(post, path = "/goals/{id}/links", tag = "goals", request_body = LinkBody, responses((status = 201, body = GoalDetail)))]
async fn add_link(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<LinkBody>,
) -> ApiResult<(StatusCode, Json<GoalDetail>)> {
    let (_, settings) = user_ctx(&s.pool, auth.user_id).await?;
    let g = repo::get(&s.pool, auth.user_id, id).await?;
    if g.progress_type == "current_value" {
        return Err(ApiError::validation(
            "src_type",
            "un objetivo de valor actual no suma entidades",
        ));
    }
    let table = match b.src_type.as_str() {
        "reading" => "readings",
        "content" => "content_pieces",
        "task" => "tasks",
        "debt" => "debts",
        _ => return Err(ApiError::validation("src_type", "reading, content, task o debt")),
    };
    let exists: Option<Uuid> = sqlx::query_scalar(persistence::sqlf!(
        "SELECT id FROM {table} WHERE user_id = $1 AND id = $2"
    ))
    .bind(auth.user_id)
    .bind(b.src_id)
    .fetch_optional(&s.pool)
    .await?;
    if exists.is_none() {
        return Err(ApiError::validation("src_id", "no existe"));
    }
    let mut tx = s.pool.begin().await?;
    sqlx::query(
        "INSERT INTO entity_links (user_id, src_type, src_id, dst_type, dst_id, relation) VALUES ($1, $2, $3, 'goal', $4, 'contributes_to')
         ON CONFLICT DO NOTHING",
    )
    .bind(auth.user_id)
    .bind(&b.src_type)
    .bind(b.src_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sync_source_contribution(
        &mut tx,
        auth.user_id,
        &settings.timezone,
        &b.src_type,
        b.src_id,
        &[id],
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, get_goal(State(s), auth, Path(id)).await?))
}

#[utoipa::path(delete, path = "/goals/{id}/links/{src_type}/{src_id}", tag = "goals", responses((status = 204)))]
async fn delete_link(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((id, src_type, src_id)): Path<(Uuid, String, Uuid)>,
) -> ApiResult<StatusCode> {
    repo::get(&s.pool, auth.user_id, id).await?;
    let mut tx = s.pool.begin().await?;
    sqlx::query("DELETE FROM entity_links WHERE user_id = $1 AND src_type = $2 AND src_id = $3 AND dst_type = 'goal' AND dst_id = $4 AND relation = 'contributes_to'")
        .bind(auth.user_id)
        .bind(&src_type)
        .bind(src_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    hooks::remove_goal_progress(&mut tx, auth.user_id, &src_type, src_id, Some(id)).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Planificación ----------

#[derive(Deserialize, IntoParams)]
pub struct PlanningQuery {
    /// Semana `GGGG-Www`. Por defecto la actual; sábado y domingo, la siguiente.
    pub week: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ReflectionDigest {
    pub good: Vec<String>,
    pub bad: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Planning {
    pub week: String,
    pub week_start: NaiveDate,
    pub week_end: NaiveDate,
    /// Mes de referencia de la semana (el de su lunes) y, si la semana cruza de mes, el siguiente.
    pub month: String,
    pub next_month: Option<String>,
    pub month_goals: Vec<GoalView>,
    pub next_month_goals: Vec<GoalView>,
    pub week_goals: Vec<GoalView>,
    /// Hay que definir el mes antes que la semana (no hay objetivos mensuales activos con hábitos).
    pub define_month_first: bool,
    pub previous_week: String,
    pub previous_week_goals: Vec<GoalView>,
    /// Objetivos con periodo terminado aún sin cerrar (revisar periodo anterior antes de planear).
    pub pending_closure: Vec<GoalView>,
    pub previous_week_reflections: ReflectionDigest,
    pub month_habit_count: u32,
    pub week_habit_count: u32,
    /// Avisos editables de carga (~6 mensuales, ~8 combinados, 1–3 por objetivo). No bloquean.
    pub warnings: Vec<String>,
}

/// Contexto para planear la semana (y el mes primero cuando toca), con avisos de carga de hábitos.
#[utoipa::path(get, path = "/goals/planning", tag = "goals", params(PlanningQuery), responses((status = 200, body = Planning)))]
async fn planning(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PlanningQuery>,
) -> ApiResult<Json<Planning>> {
    let (tz, settings) = user_ctx(&s.pool, auth.user_id).await?;
    let t = today(&s, tz);
    let week = match q.week {
        Some(w) => parse_iso_week(&w)?,
        None => {
            let base = if matches!(t.weekday(), Weekday::Sat | Weekday::Sun) {
                t + Duration::days(7)
            } else {
                t
            };
            iso_week_period(base)
        }
    };
    let week_key = iso_week_key(week.start);
    let month = month_key(week.start);
    let next_month = (month_key(week.end) != month).then(|| month_key(week.end));
    let prev_week = iso_week_period(week.start - Duration::days(1));
    let load = |kind: &str, key: &str| GoalFilter {
        kind: Some(kind.into()),
        period_key: Some(key.into()),
        status: None,
        from: None,
        to: None,
        include_archived: false,
    };
    let month_goals = views(
        &s,
        auth.user_id,
        repo::list(&s.pool, auth.user_id, &load("month", &month)).await?,
        t,
    )
    .await?;
    let next_month_goals = match &next_month {
        Some(m) => {
            views(
                &s,
                auth.user_id,
                repo::list(&s.pool, auth.user_id, &load("month", m)).await?,
                t,
            )
            .await?
        }
        None => vec![],
    };
    let week_goals = views(
        &s,
        auth.user_id,
        repo::list(&s.pool, auth.user_id, &load("week", &week_key)).await?,
        t,
    )
    .await?;
    let prev_key = iso_week_key(prev_week.start);
    let previous_week_goals = views(
        &s,
        auth.user_id,
        repo::list(&s.pool, auth.user_id, &load("week", &prev_key)).await?,
        t,
    )
    .await?;
    let pending = repo::list(
        &s.pool,
        auth.user_id,
        &GoalFilter {
            kind: None,
            period_key: None,
            status: Some("active".into()),
            from: None,
            to: Some(t - Duration::days(1)),
            include_archived: false,
        },
    )
    .await?
    .into_iter()
    .filter(|g| g.ends_on < t)
    .collect();
    let pending_closure = views(&s, auth.user_id, pending, t).await?;
    let refl =
        persistence::repos::checkins::reflections(&s.pool, auth.user_id, prev_week.start, prev_week.end)
            .await?;
    let active_month: Vec<&GoalView> = month_goals.iter().filter(|g| g.goal.status == "active").collect();
    let define_month_first = !active_month.iter().any(|g| !g.habits.is_empty());
    let month_habit_count: u32 = active_month.iter().map(|g| g.habits.len() as u32).sum();
    let week_habit_count: u32 = week_goals
        .iter()
        .filter(|g| g.goal.status == "active")
        .map(|g| g.habits.len() as u32)
        .sum();
    let per_goal: Vec<(String, u32)> = active_month
        .iter()
        .copied()
        .chain(week_goals.iter().filter(|g| g.goal.status == "active"))
        .map(|g| (g.goal.title.clone(), g.habits.len() as u32))
        .collect();
    let warnings = dg::habit_load_warnings(
        month_habit_count,
        week_habit_count,
        &per_goal,
        settings.habit_warn_month as u32,
        settings.habit_warn_total as u32,
    );
    Ok(Json(Planning {
        week: week_key,
        week_start: week.start,
        week_end: week.end,
        month,
        next_month,
        month_goals,
        next_month_goals,
        week_goals,
        define_month_first,
        previous_week: prev_key,
        previous_week_goals,
        pending_closure,
        previous_week_reflections: ReflectionDigest {
            good: refl
                .iter()
                .filter(|r| r.polarity == "good")
                .map(|r| r.text.clone())
                .collect(),
            bad: refl
                .iter()
                .filter(|r| r.polarity == "bad")
                .map(|r| r.text.clone())
                .collect(),
        },
        month_habit_count,
        week_habit_count,
        warnings,
    }))
}

/// Mapa id → objetivo para otros módulos.
pub fn index(goals: &[GoalView]) -> HashMap<Uuid, &GoalView> {
    goals.iter().map(|g| (g.goal.id, g)).collect()
}
