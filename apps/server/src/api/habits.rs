//! T19 Hábitos: identidad estable, versiones de frecuencia, pausas, entradas por día, hábitos fijos (onboarding) y
//! vínculos explícitos. T22: cumplimiento y rachas calculados con `domain::metrics`. Propietario: agente A-core.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{Duration, NaiveDate};
use chrono_tz::Tz;
use domain::habits::{EntryStatus, Frequency, HabitSpec, HabitVersion, Pause, validate_entry};
use domain::metrics::{self, Compliance, Entries, Streaks, TodayProgress};
use domain::text::required;
use domain::time::{Period, local_date, month_period, parse_tz};
use persistence::repos::habits::{
    self as repo, Habit, HabitEntry, HabitLink, HabitPause, HabitUpdate, HabitVersionRow, NewHabit,
    NewVersion,
};
use persistence::repos::identity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_habits, create_habit))
        .routes(routes!(habit_history))
        .routes(routes!(setup_fixed_habits))
        .routes(routes!(get_habit, update_habit))
        .routes(routes!(add_version))
        .routes(routes!(add_pause))
        .routes(routes!(delete_pause))
        .routes(routes!(put_entry, delete_entry))
        .routes(routes!(list_links))
        .routes(routes!(set_link, delete_link))
}

// ---------- Helpers compartidos por los módulos de A-core ----------

/// Zona IANA y moneda del usuario.
pub async fn user_ctx(
    pool: &PgPool,
    user_id: Uuid,
) -> ApiResult<(Tz, persistence::repos::identity::UserSettings)> {
    let s = identity::get_settings(pool, user_id).await?;
    let tz = parse_tz(&s.timezone)?;
    Ok((tz, s))
}

/// "Hoy" en la zona del usuario.
pub fn today(state: &AppState, tz: Tz) -> NaiveDate {
    local_date(state.now(), tz)
}

#[derive(Debug, Clone)]
pub struct LoadedHabit {
    pub habit: Habit,
    pub versions: Vec<HabitVersionRow>,
    pub pauses: Vec<HabitPause>,
    pub spec: HabitSpec,
    pub entries: Entries,
    pub entry_rows: Vec<HabitEntry>,
}

impl LoadedHabit {
    pub fn version_row_at(&self, d: NaiveDate) -> Option<&HabitVersionRow> {
        self.versions
            .iter()
            .filter(|v| v.valid_from <= d && v.valid_to.is_none_or(|t| d <= t))
            .max_by_key(|v| v.valid_from)
    }
}

pub fn version_from_row(v: &HabitVersionRow) -> Option<HabitVersion> {
    Some(HabitVersion {
        valid_from: v.valid_from,
        valid_to: v.valid_to,
        frequency: Frequency::from_parts(&v.frequency, v.weekdays, v.times_per_week).ok()?,
    })
}

/// Carga hábitos con versiones, pausas y entradas en `[from, to]`.
pub async fn load_habits(
    pool: &PgPool,
    user_id: Uuid,
    tz: Tz,
    from: NaiveDate,
    to: NaiveDate,
    include_archived: bool,
) -> ApiResult<Vec<LoadedHabit>> {
    let habits = repo::list(pool, user_id, include_archived).await?;
    let mut versions: HashMap<Uuid, Vec<HabitVersionRow>> = HashMap::new();
    for v in repo::versions(pool, user_id).await? {
        versions.entry(v.habit_id).or_default().push(v);
    }
    let mut pauses: HashMap<Uuid, Vec<HabitPause>> = HashMap::new();
    for p in repo::pauses(pool, user_id).await? {
        pauses.entry(p.habit_id).or_default().push(p);
    }
    let mut entries: HashMap<Uuid, Vec<HabitEntry>> = HashMap::new();
    for e in repo::entries(pool, user_id, from, to, None).await? {
        entries.entry(e.habit_id).or_default().push(e);
    }
    Ok(habits
        .into_iter()
        .map(|h| {
            let vs = versions.remove(&h.id).unwrap_or_default();
            let ps = pauses.remove(&h.id).unwrap_or_default();
            let rows = entries.remove(&h.id).unwrap_or_default();
            let spec = HabitSpec {
                created_on: h.created_on,
                archived_on: h.archived_at.map(|a| local_date(a, tz)),
                versions: vs.iter().filter_map(version_from_row).collect(),
                pauses: ps
                    .iter()
                    .map(|p| Pause {
                        starts_on: p.starts_on,
                        ends_on: p.ends_on,
                    })
                    .collect(),
            };
            let map: Entries = rows
                .iter()
                .filter_map(|e| Some((e.on_date, EntryStatus::parse(&e.status).ok()?)))
                .collect();
            LoadedHabit {
                habit: h,
                versions: vs,
                pauses: ps,
                spec,
                entries: map,
                entry_rows: rows,
            }
        })
        .collect())
}

// ---------- Tipos de respuesta ----------

#[derive(Serialize, ToSchema)]
pub struct HabitSummary {
    pub habit: Habit,
    pub current_version: Option<HabitVersionRow>,
    pub goal_title: Option<String>,
    /// `month | week` si el hábito pertenece a un objetivo.
    pub goal_kind: Option<String>,
    #[schema(value_type = Object)]
    pub today: TodayProgress,
    #[schema(value_type = Object)]
    pub streaks: Streaks,
    /// Cumplimiento cerrado de los últimos 30 días (sin hoy).
    #[schema(value_type = Object)]
    pub last_30_days: Compliance,
}

#[derive(Serialize, ToSchema)]
pub struct HabitDetail {
    #[serde(flatten)]
    pub summary: HabitSummary,
    pub versions: Vec<HabitVersionRow>,
    pub pauses: Vec<HabitPause>,
    #[schema(value_type = Object)]
    pub this_month: Compliance,
    pub entries: Vec<HabitEntry>,
}

async fn goal_titles(pool: &PgPool, user_id: Uuid) -> ApiResult<HashMap<Uuid, (String, String)>> {
    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, title, kind FROM goals WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id, t, k)| (id, (t, k))).collect())
}

fn summarize(h: &LoadedHabit, today: NaiveDate, goals: &HashMap<Uuid, (String, String)>) -> HabitSummary {
    let last30 = Period {
        start: today - Duration::days(30),
        end: today - Duration::days(1),
    };
    let g = h.habit.goal_id.and_then(|id| goals.get(&id));
    HabitSummary {
        habit: h.habit.clone(),
        current_version: h
            .version_row_at(today)
            .cloned()
            .or_else(|| h.versions.last().cloned()),
        goal_title: g.map(|g| g.0.clone()),
        goal_kind: g.map(|g| g.1.clone()),
        today: metrics::today_progress(&h.spec, &h.entries, today),
        streaks: metrics::streaks(&h.spec, &h.entries, today),
        last_30_days: metrics::compliance(&h.spec, &h.entries, last30, today),
    }
}

// ---------- Hábitos ----------

#[derive(Deserialize, IntoParams)]
pub struct HabitListQuery {
    /// Fecha de referencia (por defecto hoy en la zona del usuario).
    pub date: Option<NaiveDate>,
    pub archived: Option<bool>,
}

/// Hábitos con estado de hoy, rachas y cumplimiento de 30 días.
#[utoipa::path(get, path = "/habits", tag = "habits", params(HabitListQuery), responses((status = 200, body = Vec<HabitSummary>)))]
async fn list_habits(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<HabitListQuery>,
) -> ApiResult<Json<Vec<HabitSummary>>> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let day = q.date.unwrap_or_else(|| today(&s, tz));
    let loaded = load_habits(
        &s.pool,
        auth.user_id,
        tz,
        NaiveDate::MIN.max(day - Duration::days(400)),
        day,
        q.archived.unwrap_or(false),
    )
    .await?;
    let goals = goal_titles(&s.pool, auth.user_id).await?;
    Ok(Json(loaded.iter().map(|h| summarize(h, day, &goals)).collect()))
}

#[derive(Deserialize, ToSchema)]
pub struct FrequencyBody {
    /// `daily | weekdays | times_per_week`
    pub frequency: String,
    /// Bitmask ISO (lunes = 1 … domingo = 64) para `weekdays`.
    pub weekdays: Option<i16>,
    /// 1–7 para `times_per_week`.
    pub times_per_week: Option<i16>,
    /// Meta por ocurrencia (ej. 20 minutos), opcional.
    #[schema(value_type = Option<String>)]
    pub target_quantity: Option<Decimal>,
}

impl FrequencyBody {
    fn to_new_version(&self, valid_from: NaiveDate, valid_to: Option<NaiveDate>) -> ApiResult<NewVersion> {
        let f = Frequency::from_parts(&self.frequency, self.weekdays, self.times_per_week)?;
        if self.target_quantity.is_some_and(|q| q <= Decimal::ZERO) {
            return Err(ApiError::validation("target_quantity", "debe ser positiva"));
        }
        let (frequency, weekdays, times_per_week) = f.parts();
        Ok(NewVersion {
            valid_from,
            valid_to,
            frequency,
            weekdays,
            times_per_week,
            target_quantity: self.target_quantity,
        })
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateHabit {
    pub name: String,
    /// `goal` (requiere `goal_id`) o `custom`. Los fijos se crean con `POST /habits/fixed`.
    #[serde(default = "custom")]
    pub kind: String,
    pub goal_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    /// Unidad de la cantidad registrada (ej. `min`). Independiente de la unidad de cualquier objetivo.
    pub unit: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub position: i32,
    /// Desde cuándo se exige (por defecto hoy o el inicio del objetivo si es posterior).
    pub starts_on: Option<NaiveDate>,
    #[serde(flatten)]
    pub frequency: FrequencyBody,
}

fn custom() -> String {
    "custom".into()
}

/// Crea un hábito con su primera versión. Un hábito de objetivo rige solo dentro del periodo del objetivo.
#[utoipa::path(post, path = "/habits", tag = "habits", request_body = CreateHabit, responses((status = 201, body = HabitDetail)))]
async fn create_habit(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<CreateHabit>,
) -> ApiResult<(StatusCode, Json<HabitDetail>)> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let name = required("name", &b.name, 160)?;
    let t = today(&s, tz);
    let (kind, starts, ends) = match b.kind.as_str() {
        "goal" => {
            let gid = b.goal_id.ok_or_else(|| {
                ApiError::validation("goal_id", "un hábito de objetivo necesita su objetivo")
            })?;
            let g = persistence::repos::goals::get(&s.pool, auth.user_id, gid).await?;
            let start = b.starts_on.unwrap_or(t).max(g.starts_on);
            if start > g.ends_on {
                return Err(ApiError::validation("starts_on", "el objetivo ya terminó"));
            }
            ("goal", start, Some(g.ends_on))
        }
        "custom" => ("custom", b.starts_on.unwrap_or(t), None),
        _ => {
            return Err(ApiError::validation(
                "kind",
                "usa goal o custom; los fijos se configuran en /habits/fixed",
            ));
        }
    };
    let v = b.frequency.to_new_version(starts, ends)?;
    let mut tx = s.pool.begin().await?;
    let h = repo::create(
        &mut tx,
        auth.user_id,
        &NewHabit {
            name: &name,
            kind,
            fixed_key: None,
            goal_id: if kind == "goal" { b.goal_id } else { None },
            area_id: b.area_id,
            unit: b.unit.as_deref().map(str::trim).filter(|u| !u.is_empty()),
            description: &b.description,
            position: b.position,
            created_on: starts,
        },
        &v,
    )
    .await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "habit",
        Some(h.id),
        "create",
        serde_json::json!({ "name": name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(detail(&s, auth.user_id, h.id).await?)))
}

async fn detail(s: &AppState, user_id: Uuid, id: Uuid) -> ApiResult<HabitDetail> {
    let (tz, _) = user_ctx(&s.pool, user_id).await?;
    let t = today(s, tz);
    repo::get(&s.pool, user_id, id).await?;
    let loaded = load_habits(
        &s.pool,
        user_id,
        tz,
        t - Duration::days(400),
        t + Duration::days(1),
        true,
    )
    .await?;
    let h = loaded
        .into_iter()
        .find(|h| h.habit.id == id)
        .ok_or_else(ApiError::not_found)?;
    let goals = goal_titles(&s.pool, user_id).await?;
    let month = month_period(t);
    Ok(HabitDetail {
        summary: summarize(&h, t, &goals),
        this_month: metrics::compliance(&h.spec, &h.entries, month, t),
        versions: h.versions.clone(),
        pauses: h.pauses.clone(),
        entries: h
            .entry_rows
            .iter()
            .filter(|e| e.on_date >= t - Duration::days(60))
            .cloned()
            .collect(),
    })
}

#[utoipa::path(get, path = "/habits/{id}", tag = "habits", responses((status = 200, body = HabitDetail), (status = 404, body = crate::error::ErrorBody)))]
async fn get_habit(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<HabitDetail>> {
    Ok(Json(detail(&s, auth.user_id, id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateHabit {
    pub name: String,
    pub area_id: Option<Uuid>,
    pub unit: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub archived: bool,
    pub version: i32,
}

/// Edita la identidad (nombre, área, unidad, archivo). La frecuencia se cambia con una versión nueva.
#[utoipa::path(put, path = "/habits/{id}", tag = "habits", request_body = UpdateHabit, responses((status = 200, body = Habit), (status = 409, body = crate::error::ErrorBody)))]
async fn update_habit(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<UpdateHabit>,
) -> ApiResult<Json<Habit>> {
    let name = required("name", &b.name, 160)?;
    let h = repo::update(
        &s.pool,
        auth.user_id,
        id,
        b.version,
        &HabitUpdate {
            name: &name,
            area_id: b.area_id,
            unit: b.unit.as_deref(),
            description: &b.description,
            position: b.position,
            archived: b.archived,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "habit",
        Some(id),
        "update",
        serde_json::json!({ "name": name, "archived": b.archived }),
    )
    .await;
    Ok(Json(h))
}

#[derive(Deserialize, ToSchema)]
pub struct NewVersionBody {
    /// Fecha desde la que rige la nueva frecuencia (el pasado conserva la anterior).
    pub valid_from: NaiveDate,
    #[serde(flatten)]
    pub frequency: FrequencyBody,
}

/// Cambia la frecuencia/meta desde una fecha sin alterar el historial.
#[utoipa::path(post, path = "/habits/{id}/versions", tag = "habits", request_body = NewVersionBody, responses((status = 201, body = HabitVersionRow)))]
async fn add_version(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<NewVersionBody>,
) -> ApiResult<(StatusCode, Json<HabitVersionRow>)> {
    let h = repo::get(&s.pool, auth.user_id, id).await?;
    if b.valid_from < h.created_on {
        return Err(ApiError::validation(
            "valid_from",
            "no puede ser anterior a la creación del hábito",
        ));
    }
    // Un hábito de objetivo no rige más allá del objetivo.
    let valid_to = match h.goal_id {
        Some(g) => Some(
            persistence::repos::goals::get(&s.pool, auth.user_id, g)
                .await?
                .ends_on,
        ),
        None => None,
    };
    let v = b.frequency.to_new_version(b.valid_from, valid_to)?;
    let mut tx = s.pool.begin().await?;
    let row = repo::add_version(&mut tx, auth.user_id, id, &v).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "habit",
        Some(id),
        "new_version",
        serde_json::json!({ "valid_from": b.valid_from, "frequency": b.frequency.frequency }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize, ToSchema)]
pub struct PauseBody {
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    #[serde(default)]
    pub reason: String,
}

/// Pausa: los días cubiertos salen del denominador.
#[utoipa::path(post, path = "/habits/{id}/pauses", tag = "habits", request_body = PauseBody, responses((status = 201, body = HabitPause)))]
async fn add_pause(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<PauseBody>,
) -> ApiResult<(StatusCode, Json<HabitPause>)> {
    if b.ends_on.is_some_and(|e| e < b.starts_on) {
        return Err(ApiError::validation(
            "ends_on",
            "la pausa termina antes de empezar",
        ));
    }
    repo::get(&s.pool, auth.user_id, id).await?;
    let p = repo::add_pause(&s.pool, auth.user_id, id, b.starts_on, b.ends_on, b.reason.trim()).await?;
    Ok((StatusCode::CREATED, Json(p)))
}

#[utoipa::path(delete, path = "/habits/{id}/pauses/{pause_id}", tag = "habits", responses((status = 204)))]
async fn delete_pause(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((id, pause_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    repo::delete_pause(&s.pool, auth.user_id, id, pause_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct EntryBody {
    /// `done | not_done | not_applicable`
    pub status: String,
    #[schema(value_type = Option<String>)]
    pub quantity: Option<Decimal>,
    /// Obligatorio para `not_applicable`.
    pub reason: Option<String>,
    /// Versión esperada si la entrada ya existe (control de conflictos).
    pub version: Option<i32>,
}

/// Valida que el hábito exista y rija ese día, y que la fecha no sea futura.
pub fn check_entry_date(h: &LoadedHabit, on: NaiveDate, today: NaiveDate) -> ApiResult<()> {
    if on > today {
        return Err(ApiError::validation("date", "no se registran días futuros"));
    }
    if h.spec.version_at(on).is_none() {
        return Err(ApiError::validation(
            "date",
            "el hábito no estaba vigente ese día",
        ));
    }
    Ok(())
}

/// Registra el estado de un día (una sola ocurrencia por hábito y fecha).
#[utoipa::path(put, path = "/habits/{id}/entries/{date}", tag = "habits", request_body = EntryBody,
    responses((status = 200, body = HabitEntry), (status = 409, body = crate::error::ErrorBody)))]
async fn put_entry(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((id, date)): Path<(Uuid, NaiveDate)>,
    Json(b): Json<EntryBody>,
) -> ApiResult<Json<HabitEntry>> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let t = today(&s, tz);
    let status = EntryStatus::parse(&b.status)?;
    validate_entry(status, b.reason.as_deref(), b.quantity)?;
    let loaded = load_habits(&s.pool, auth.user_id, tz, date, date, true).await?;
    let h = loaded
        .iter()
        .find(|h| h.habit.id == id)
        .ok_or_else(ApiError::not_found)?;
    check_entry_date(h, date, t)?;
    let mut tx = s.pool.begin().await?;
    let reason = b.reason.as_deref().map(str::trim).filter(|r| !r.is_empty());
    let e = repo::upsert_entry(
        &mut tx,
        auth.user_id,
        id,
        date,
        status.as_str(),
        b.quantity,
        reason,
        "manual",
        b.version,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(e))
}

/// Vuelve el día a "sin registrar".
#[utoipa::path(delete, path = "/habits/{id}/entries/{date}", tag = "habits", responses((status = 204)))]
async fn delete_entry(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((id, date)): Path<(Uuid, NaiveDate)>,
) -> ApiResult<StatusCode> {
    repo::get(&s.pool, auth.user_id, id).await?;
    repo::delete_entry(&s.pool, auth.user_id, id, date).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- Vista de 30 días (Hábitos.base) ----------

#[derive(Deserialize, IntoParams)]
pub struct HistoryQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

#[derive(Serialize, ToSchema)]
pub struct HistoryCell {
    pub habit_id: Uuid,
    /// `done | not_done | not_applicable | unrecorded | not_required | paused | future | open`
    pub state: String,
}

#[derive(Serialize, ToSchema)]
pub struct HistoryDay {
    pub date: NaiveDate,
    pub energy: Option<i16>,
    #[schema(value_type = Option<String>)]
    pub weight_kg: Option<Decimal>,
    pub cells: Vec<HistoryCell>,
    /// Cumplidas / exigibles del día (para el heatmap). `None` si nada era exigible.
    pub done: u32,
    pub required: u32,
}

#[derive(Serialize, ToSchema)]
pub struct HabitHistory {
    pub habits: Vec<Habit>,
    pub days: Vec<HistoryDay>,
}

/// Estado de un hábito en un día para calendarios/heatmaps.
pub fn cell_state(h: &LoadedHabit, d: NaiveDate, today: NaiveDate) -> &'static str {
    if d > today {
        return "future";
    }
    let Some(v) = h.spec.version_at(d) else {
        return "not_required";
    };
    if h.spec.is_paused(d) {
        return "paused";
    }
    match h.entries.get(&d) {
        Some(EntryStatus::Done) => "done",
        Some(EntryStatus::NotDone) => "not_done",
        Some(EntryStatus::NotApplicable) => "not_applicable",
        None if d == today => "open",
        None if v.frequency.requires_day(d) => "unrecorded",
        None => "not_required",
    }
}

/// Historial por día (por defecto últimos 30 días) con energía, peso y estado de cada hábito.
#[utoipa::path(get, path = "/habits/history", tag = "habits", params(HistoryQuery), responses((status = 200, body = HabitHistory)))]
async fn habit_history(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<HistoryQuery>,
) -> ApiResult<Json<HabitHistory>> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let t = today(&s, tz);
    let to = q.to.unwrap_or(t);
    let from = q.from.unwrap_or(to - Duration::days(29));
    if from > to || (to - from).num_days() > 400 {
        return Err(ApiError::validation("from", "rango inválido (máximo 400 días)"));
    }
    let loaded = load_habits(&s.pool, auth.user_id, tz, from, to, true).await?;
    let checkins = persistence::repos::checkins::list(&s.pool, auth.user_id, from, to).await?;
    let by_day: HashMap<NaiveDate, _> = checkins.into_iter().map(|c| (c.on_date, c)).collect();
    let mut days = Vec::new();
    for d in (Period { start: from, end: to }).iter_days() {
        let mut cells = Vec::new();
        let (mut done, mut required) = (0, 0);
        for h in &loaded {
            let st = cell_state(h, d, t);
            if st == "not_required" && h.spec.version_at(d).is_none() {
                continue;
            }
            if h.spec.version_at(d).is_some_and(|v| v.frequency.requires_day(d))
                && !matches!(st, "paused" | "not_applicable" | "future" | "open")
            {
                required += 1;
                if st == "done" {
                    done += 1;
                }
            }
            cells.push(HistoryCell {
                habit_id: h.habit.id,
                state: st.into(),
            });
        }
        let c = by_day.get(&d);
        days.push(HistoryDay {
            date: d,
            energy: c.and_then(|c| c.energy),
            weight_kg: c.and_then(|c| c.weight_kg),
            cells,
            done,
            required,
        });
    }
    days.reverse();
    Ok(Json(HabitHistory {
        habits: loaded.into_iter().map(|h| h.habit).collect(),
        days,
    }))
}

// ---------- Hábitos fijos (onboarding) ----------

#[derive(Deserialize, ToSchema)]
pub struct FixedHabitConfig {
    pub enabled: bool,
    /// Nombre visible (ej. "Meditación" si sustituye al devocional).
    pub name: Option<String>,
    #[serde(flatten)]
    pub frequency: FrequencyBody,
}

#[derive(Deserialize, ToSchema)]
pub struct FixedHabitsBody {
    pub devotional: Option<FixedHabitConfig>,
    pub exercise: Option<FixedHabitConfig>,
    pub reading: Option<FixedHabitConfig>,
    /// Desde cuándo rigen (por defecto hoy).
    pub starts_on: Option<NaiveDate>,
}

/// Confirma los hábitos fijos propuestos en onboarding (devocional, ejercicio, lectura). Nada se crea sin esta
/// confirmación. Idempotente: si ya existe, actualiza su frecuencia desde `starts_on` o lo archiva si `enabled=false`.
#[utoipa::path(post, path = "/habits/fixed", tag = "habits", request_body = FixedHabitsBody, responses((status = 200, body = Vec<Habit>)))]
async fn setup_fixed_habits(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<FixedHabitsBody>,
) -> ApiResult<Json<Vec<Habit>>> {
    let (tz, settings) = user_ctx(&s.pool, auth.user_id).await?;
    let start = b.starts_on.unwrap_or_else(|| today(&s, tz));
    let mut tx = s.pool.begin().await?;
    let mut out = Vec::new();
    for (key, default_name, cfg) in [
        (
            "devotional",
            if settings.devotional_mode == "meditation" {
                "Meditación"
            } else {
                "Devocional"
            },
            &b.devotional,
        ),
        ("exercise", "Ejercicio", &b.exercise),
        ("reading", "Lectura", &b.reading),
    ] {
        let Some(cfg) = cfg else { continue };
        let existing = repo::fixed_habit(&mut *tx, auth.user_id, key).await?;
        let name = cfg
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(default_name)
            .to_string();
        match (existing, cfg.enabled) {
            (None, false) => {}
            (None, true) => {
                let v = cfg.frequency.to_new_version(start, None)?;
                let h = repo::create(
                    &mut tx,
                    auth.user_id,
                    &NewHabit {
                        name: &name,
                        kind: "fixed",
                        fixed_key: Some(key),
                        goal_id: None,
                        area_id: None,
                        unit: None,
                        description: "",
                        position: 0,
                        created_on: start,
                    },
                    &v,
                )
                .await?;
                out.push(h);
            }
            (Some(h), enabled) => {
                if enabled {
                    let v = cfg.frequency.to_new_version(start.max(h.created_on), None)?;
                    repo::add_version(&mut tx, auth.user_id, h.id, &v).await?;
                }
                sqlx::query(
                    "UPDATE habits SET name = $3, archived_at = CASE WHEN $4 THEN NULL ELSE coalesce(archived_at, now()) END,
                     version = version + 1 WHERE user_id = $1 AND id = $2",
                )
                .bind(auth.user_id)
                .bind(h.id)
                .bind(&name)
                .bind(enabled)
                .execute(&mut *tx)
                .await?;
                out.push(repo::get(&mut *tx, auth.user_id, h.id).await?);
            }
        }
    }
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "habit",
        None,
        "setup_fixed",
        serde_json::json!({ "count": out.len() }),
    )
    .await;
    Ok(Json(out))
}

// ---------- Vínculos explícitos ----------

const TRIGGERS: [&str; 4] = [
    "weight_logged",
    "expense_logged",
    "positive_reflection",
    "devotional_content",
];

/// Vínculos configurados (registrar peso → "Pesarme", gasto → "Registrar gastos", etc.).
#[utoipa::path(get, path = "/habit-links", tag = "habits", responses((status = 200, body = Vec<HabitLink>)))]
async fn list_links(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<HabitLink>>> {
    Ok(Json(repo::links(&s.pool, auth.user_id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct LinkBody {
    pub habit_id: Uuid,
}

#[utoipa::path(put, path = "/habit-links/{trigger}", tag = "habits", request_body = LinkBody, responses((status = 200, body = Vec<HabitLink>)))]
async fn set_link(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(trigger): Path<String>,
    Json(b): Json<LinkBody>,
) -> ApiResult<Json<Vec<HabitLink>>> {
    if !TRIGGERS.contains(&trigger.as_str()) {
        return Err(ApiError::validation("trigger", "disparador inválido"));
    }
    repo::get(&s.pool, auth.user_id, b.habit_id).await?;
    repo::set_link(&s.pool, auth.user_id, &trigger, b.habit_id).await?;
    Ok(Json(repo::links(&s.pool, auth.user_id).await?))
}

#[utoipa::path(delete, path = "/habit-links/{trigger}", tag = "habits", responses((status = 204)))]
async fn delete_link(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(trigger): Path<String>,
) -> ApiResult<StatusCode> {
    repo::delete_link(&s.pool, auth.user_id, &trigger).await?;
    Ok(StatusCode::NO_CONTENT)
}
