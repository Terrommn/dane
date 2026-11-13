//! T20 Check-in diario: un registro por usuario y fecha local; mañana y noche actualizan el mismo registro; guardado
//! parcial (campo ausente = sin cambio, `null` = borrar), control de versión (409) y efectos en la misma transacción:
//! entradas de hábitos y vínculos explícitos (peso → hábito, reflexión positiva → hábito). Propietario: agente A-core.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, NaiveDate, Utc};
use domain::checkin::{validate_energy, validate_weight};
use domain::habits::{EntryStatus, validate_entry};
use domain::metrics::{self, TodayProgress};
use domain::time::{iso_week_key, month_key};
use persistence::RepoError;
use persistence::hooks::{self, HabitTrigger};
use persistence::repos::checkins::{self as repo, Checkin, LogEntry, Reflection};
use persistence::repos::dashboard::{self as dash, AgendaItem, DevlogBrief, InteractionBrief, TaskBrief};
use persistence::repos::devotionals::Devotional;
use persistence::repos::goals::{Goal, GoalFilter};
use persistence::repos::habits as habit_repo;
use persistence::repos::habits::{Habit, HabitEntry};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::api::habits::{LoadedHabit, check_entry_date, load_habits, today, user_ctx};
use crate::api::settings::double_option;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_checkins))
        .routes(routes!(get_checkin, put_checkin))
        .routes(routes!(day_view))
}

#[derive(Serialize, ToSchema)]
pub struct CheckinFull {
    /// `null` si aún no hay registro ese día (todo "sin registrar").
    pub checkin: Option<Checkin>,
    /// Versión a enviar en el siguiente guardado (0 si no existe).
    pub version: i32,
    pub log: Vec<LogEntry>,
    pub went_well: Vec<Reflection>,
    pub went_badly: Vec<Reflection>,
    pub habit_entries: Vec<HabitEntry>,
}

async fn full(s: &AppState, user_id: Uuid, on: NaiveDate) -> ApiResult<CheckinFull> {
    let checkin = repo::get(&s.pool, user_id, on).await?;
    let refl = repo::reflections(&s.pool, user_id, on, on).await?;
    Ok(CheckinFull {
        version: checkin.as_ref().map(|c| c.version).unwrap_or(0),
        checkin,
        log: repo::log_entries(&s.pool, user_id, on).await?,
        went_well: refl.iter().filter(|r| r.polarity == "good").cloned().collect(),
        went_badly: refl.into_iter().filter(|r| r.polarity == "bad").collect(),
        habit_entries: habit_repo::entries(&s.pool, user_id, on, on, None).await?,
    })
}

#[derive(Deserialize, IntoParams)]
pub struct RangeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// Check-ins de un rango (historial; no incluye días sin registro).
#[utoipa::path(get, path = "/checkins", tag = "checkins", params(RangeQuery), responses((status = 200, body = Vec<Checkin>)))]
async fn list_checkins(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<Checkin>>> {
    if q.to < q.from || (q.to - q.from).num_days() > 400 {
        return Err(ApiError::validation("from", "rango inválido (máximo 400 días)"));
    }
    Ok(Json(repo::list(&s.pool, auth.user_id, q.from, q.to).await?))
}

#[utoipa::path(get, path = "/checkins/{date}", tag = "checkins", responses((status = 200, body = CheckinFull)))]
async fn get_checkin(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
) -> ApiResult<Json<CheckinFull>> {
    Ok(Json(full(&s, auth.user_id, date).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct HabitEntryPatch {
    pub habit_id: Uuid,
    /// `done | not_done | not_applicable`; `null` vuelve a "sin registrar".
    pub status: Option<String>,
    #[schema(value_type = Option<String>)]
    pub quantity: Option<Decimal>,
    pub reason: Option<String>,
}

/// Guardado parcial: los campos ausentes no cambian; `null` explícito borra (vuelve a "sin dato", que no es cero).
#[derive(Deserialize, ToSchema, Default)]
pub struct CheckinPatch {
    /// Versión conocida por el cliente (0 o ausente si el día no tenía registro). Si no coincide → 409 con el actual.
    pub version: Option<i32>,
    #[serde(default, with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub weight_kg: Option<Option<Decimal>>,
    #[serde(default, with = "double_option")]
    #[schema(value_type = Option<i16>)]
    pub energy: Option<Option<i16>>,
    #[serde(default, with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub planned_spend_text: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    #[schema(value_type = Option<i64>)]
    pub planned_spend_cents: Option<Option<i64>>,
    /// "No planeo gastar nada" (distinto de no haber respondido).
    pub planned_spend_none: Option<bool>,
    pub notes_md: Option<String>,
    pub morning_done: Option<bool>,
    /// Marca el cierre nocturno (cancela el recordatorio del día).
    pub evening_done: Option<bool>,
    /// Lista completa del log del día (reemplaza).
    pub log: Option<Vec<String>>,
    /// "Lo que hice bien hoy" (reemplaza). Solo lo que el usuario escribió.
    pub went_well: Option<Vec<String>>,
    /// "Lo que hice mal hoy" (reemplaza).
    pub went_badly: Option<Vec<String>>,
    /// Entradas de hábitos del día (fijos y de objetivos).
    pub habits: Option<Vec<HabitEntryPatch>>,
}

fn clean_list(field: &str, items: Vec<String>) -> ApiResult<Vec<String>> {
    if items.len() > 100 {
        return Err(ApiError::validation(field, "máximo 100 elementos"));
    }
    let out: Vec<String> = items
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if out.iter().any(|t| t.chars().count() > 2000) {
        return Err(ApiError::validation(field, "máximo 2000 caracteres por elemento"));
    }
    Ok(out)
}

/// Aplica el check-in (mañana o noche) sobre el registro único del día.
#[utoipa::path(put, path = "/checkins/{date}", tag = "checkins", request_body = CheckinPatch,
    responses((status = 200, body = CheckinFull), (status = 409, body = crate::error::ErrorBody), (status = 422, body = crate::error::ErrorBody)))]
async fn put_checkin(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
    Json(p): Json<CheckinPatch>,
) -> ApiResult<Json<CheckinFull>> {
    let (tz, _) = user_ctx(&s.pool, auth.user_id).await?;
    let t = today(&s, tz);
    if date > t {
        return Err(ApiError::validation("date", "no se registran días futuros"));
    }
    // Validaciones previas (sin efectos).
    if let Some(Some(e)) = p.energy {
        validate_energy(e)?;
    }
    if let Some(Some(w)) = p.weight_kg {
        validate_weight(w)?;
    }
    if let Some(Some(c)) = p.planned_spend_cents
        && c < 0
    {
        return Err(ApiError::validation(
            "planned_spend_cents",
            "no puede ser negativo",
        ));
    }
    let log = p.log.map(|l| clean_list("log", l)).transpose()?;
    let good = p.went_well.map(|l| clean_list("went_well", l)).transpose()?;
    let bad = p.went_badly.map(|l| clean_list("went_badly", l)).transpose()?;
    let habits: Vec<LoadedHabit> = if p.habits.is_some() {
        load_habits(&s.pool, auth.user_id, tz, date, date, true).await?
    } else {
        vec![]
    };
    let mut entry_ops = Vec::new();
    for e in p.habits.unwrap_or_default() {
        let h = habits
            .iter()
            .find(|h| h.habit.id == e.habit_id)
            .ok_or_else(|| ApiError::validation("habits", "hábito inexistente"))?;
        check_entry_date(h, date, t)?;
        let status = e.status.as_deref().map(EntryStatus::parse).transpose()?;
        if let Some(st) = status {
            validate_entry(st, e.reason.as_deref(), e.quantity)?;
        }
        entry_ops.push((e.habit_id, status, e.quantity, e.reason));
    }

    let mut tx = s.pool.begin().await?;
    let (mut c, inserted) = repo::lock_or_create(&mut tx, auth.user_id, date).await?;
    if !inserted && p.version.unwrap_or(0) != c.version {
        let current = serde_json::to_value(full_from_tx(&mut tx, auth.user_id, date, c.clone()).await?)
            .unwrap_or_default();
        return Err(RepoError::Conflict(current).into());
    }
    let now = s.now();
    if let Some(w) = p.weight_kg {
        c.weight_kg = w;
    }
    if let Some(e) = p.energy {
        c.energy = e;
    }
    if let Some(t) = p.planned_spend_text {
        c.planned_spend_text = t.map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    }
    if let Some(v) = p.planned_spend_cents {
        c.planned_spend_cents = v;
    }
    if let Some(v) = p.planned_spend_none {
        c.planned_spend_none = v;
    }
    if let Some(n) = p.notes_md {
        c.notes_md = n;
    }
    if let Some(m) = p.morning_done {
        c.morning_done_at = if m { c.morning_done_at.or(Some(now)) } else { None };
    }
    if let Some(e) = p.evening_done {
        c.evening_done_at = if e { c.evening_done_at.or(Some(now)) } else { None };
    }
    let saved = repo::save(&mut tx, auth.user_id, &c).await?;
    if let Some(l) = &log {
        repo::replace_log(&mut tx, auth.user_id, date, l, "manual").await?;
    }
    if let Some(g) = &good {
        repo::replace_reflections(&mut tx, auth.user_id, date, "good", g).await?;
        if g.is_empty() {
            hooks::unmark_linked_habit(&mut tx, auth.user_id, HabitTrigger::PositiveReflection, date).await?;
        } else {
            hooks::mark_linked_habit(&mut tx, auth.user_id, HabitTrigger::PositiveReflection, date).await?;
        }
    }
    if let Some(b) = &bad {
        repo::replace_reflections(&mut tx, auth.user_id, date, "bad", b).await?;
    }
    if let Some(w) = p.weight_kg {
        match w {
            Some(_) => {
                hooks::mark_linked_habit(&mut tx, auth.user_id, HabitTrigger::WeightLogged, date).await?;
            }
            None => {
                hooks::unmark_linked_habit(&mut tx, auth.user_id, HabitTrigger::WeightLogged, date).await?
            }
        }
    }
    for (habit_id, status, qty, reason) in entry_ops {
        match status {
            Some(st) => {
                let reason = reason.as_deref().map(str::trim).filter(|r| !r.is_empty());
                habit_repo::upsert_entry(
                    &mut tx,
                    auth.user_id,
                    habit_id,
                    date,
                    st.as_str(),
                    qty,
                    reason,
                    "checkin",
                    None,
                )
                .await?;
            }
            None => {
                sqlx::query(
                    "DELETE FROM habit_entries WHERE user_id = $1 AND habit_id = $2 AND on_date = $3",
                )
                .bind(auth.user_id)
                .bind(habit_id)
                .bind(date)
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "checkin",
        Some(saved.id),
        "save",
        serde_json::json!({ "date": date, "version": saved.version }),
    )
    .await;
    Ok(Json(full(&s, auth.user_id, date).await?))
}

async fn full_from_tx(
    tx: &mut persistence::Tx<'_>,
    user_id: Uuid,
    on: NaiveDate,
    c: Checkin,
) -> ApiResult<CheckinFull> {
    let refl = repo::reflections(&mut **tx, user_id, on, on).await?;
    Ok(CheckinFull {
        version: c.version,
        checkin: Some(c),
        log: repo::log_entries(&mut **tx, user_id, on).await?,
        went_well: refl.iter().filter(|r| r.polarity == "good").cloned().collect(),
        went_badly: refl.into_iter().filter(|r| r.polarity == "bad").collect(),
        habit_entries: habit_repo::entries(&mut **tx, user_id, on, on, None).await?,
    })
}

// ---------- Vista de Hoy ----------

#[derive(Serialize, ToSchema)]
pub struct HabitDay {
    pub habit: Habit,
    pub goal_id: Option<Uuid>,
    pub goal_title: Option<String>,
    pub entry: Option<HabitEntry>,
    #[schema(value_type = Object)]
    pub progress: TodayProgress,
    #[schema(value_type = Option<String>)]
    pub target_quantity: Option<Decimal>,
}

#[derive(Serialize, ToSchema)]
pub struct HabitGroups {
    /// Devocional/meditación, ejercicio, lectura.
    pub fixed: Vec<HabitDay>,
    pub month: Vec<HabitDay>,
    pub week: Vec<HabitDay>,
    pub other: Vec<HabitDay>,
}

#[derive(Serialize, ToSchema)]
pub struct AreaTasks {
    pub area_id: Option<Uuid>,
    pub area_name: String,
    pub tasks: Vec<TaskBrief>,
}

#[derive(Serialize, ToSchema)]
pub struct DerivedLog {
    /// Pendientes cerrados ese día (derivado, no se guarda en el log).
    pub closed_tasks: Vec<TaskBrief>,
    /// Eventos de agenda ya terminados.
    pub past_events: Vec<AgendaItem>,
    pub devlog_sessions: Vec<DevlogBrief>,
    /// Gasto confirmado del día en moneda base (centavos).
    pub expenses_total_cents: i64,
    pub expenses_count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct GoalBrief {
    pub id: Uuid,
    pub title: String,
    pub kind: String,
    pub period_key: String,
    pub status: String,
}

#[derive(Serialize, ToSchema)]
pub struct DayView {
    pub date: NaiveDate,
    pub today: NaiveDate,
    pub timezone: String,
    pub iso_week: String,
    pub month: String,
    /// El día sigue abierto (es hoy): sus faltantes no penalizan todavía.
    pub is_open: bool,
    #[serde(flatten)]
    pub checkin: CheckinFull,
    pub habits: HabitGroups,
    pub month_goals: Vec<GoalBrief>,
    pub week_goals: Vec<GoalBrief>,
    /// Módulo devocional activo y modo (`devotional | meditation`).
    pub devotional_enabled: bool,
    pub devotional_mode: String,
    pub devotional: Option<Devotional>,
    /// Agenda del día (propuesta: un evento no prueba asistencia).
    pub agenda: Vec<AgendaItem>,
    pub tasks_by_area: Vec<AreaTasks>,
    pub people: Vec<InteractionBrief>,
    pub derived: DerivedLog,
    pub reminder_done: bool,
    pub currency: String,
    pub generated_at: DateTime<Utc>,
}

fn brief(g: &Goal) -> GoalBrief {
    GoalBrief {
        id: g.id,
        title: g.title.clone(),
        kind: g.kind.clone(),
        period_key: g.period_key.clone(),
        status: g.status.clone(),
    }
}

/// Todo lo necesario para la pantalla Hoy de una fecha (también días anteriores).
#[utoipa::path(get, path = "/checkins/{date}/view", tag = "checkins", responses((status = 200, body = DayView)))]
async fn day_view(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
) -> ApiResult<Json<DayView>> {
    let (tz, st) = user_ctx(&s.pool, auth.user_id).await?;
    let t = today(&s, tz);
    let checkin = full(&s, auth.user_id, date).await?;
    let week = domain::time::iso_week_period(date);
    let loaded = load_habits(
        &s.pool,
        auth.user_id,
        tz,
        week.start.min(date),
        week.end.max(date),
        false,
    )
    .await?;
    let goals = persistence::repos::goals::list(
        &s.pool,
        auth.user_id,
        &GoalFilter {
            kind: None,
            period_key: None,
            status: None,
            from: Some(date),
            to: Some(date),
            include_archived: false,
        },
    )
    .await?;
    let goal_map: HashMap<Uuid, &Goal> = goals.iter().map(|g| (g.id, g)).collect();
    let entries_today: HashMap<Uuid, HabitEntry> = checkin
        .habit_entries
        .iter()
        .map(|e| (e.habit_id, e.clone()))
        .collect();
    let mut groups = HabitGroups {
        fixed: vec![],
        month: vec![],
        week: vec![],
        other: vec![],
    };
    for h in &loaded {
        if !h.spec.shown_on(date) {
            continue;
        }
        let goal = h.habit.goal_id.and_then(|g| goal_map.get(&g));
        // Hábitos de objetivos solo mientras el objetivo esté activo.
        if h.habit.kind == "goal" && goal.is_none_or(|g| g.status != "active") {
            continue;
        }
        let day = HabitDay {
            habit: h.habit.clone(),
            goal_id: h.habit.goal_id,
            goal_title: goal.map(|g| g.title.clone()),
            entry: entries_today.get(&h.habit.id).cloned(),
            progress: metrics::today_progress(&h.spec, &h.entries, date),
            target_quantity: h.version_row_at(date).and_then(|v| v.target_quantity),
        };
        match (h.habit.kind.as_str(), goal.map(|g| g.kind.as_str())) {
            ("fixed", _) => groups.fixed.push(day),
            (_, Some("month")) => groups.month.push(day),
            (_, Some("week")) => groups.week.push(day),
            _ => groups.other.push(day),
        }
    }
    let devotional_enabled = st
        .modules
        .get("devotional")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let devotional = persistence::repos::devotionals::get(&s.pool, auth.user_id, date).await?;
    let agenda = dash::agenda(&s.pool, auth.user_id, date).await?;
    let tasks = dash::tasks_for_day(&s.pool, auth.user_id, &st.timezone, date).await?;
    let mut by_area: Vec<AreaTasks> = Vec::new();
    for task in tasks
        .iter()
        .filter(|t| t.status == "open" || t.done_on == Some(date))
    {
        match by_area.iter_mut().find(|a| a.area_id == task.area_id) {
            Some(a) => a.tasks.push(task.clone()),
            None => by_area.push(AreaTasks {
                area_id: task.area_id,
                area_name: task.area_name.clone().unwrap_or_else(|| "Sin área".into()),
                tasks: vec![task.clone()],
            }),
        }
    }
    let (total, count) = dash::expenses_between(&s.pool, auth.user_id, &st.currency, date, date).await?;
    let now = s.now();
    let derived = DerivedLog {
        closed_tasks: tasks
            .iter()
            .filter(|t| t.status == "done" && t.done_on == Some(date))
            .cloned()
            .collect(),
        past_events: agenda
            .iter()
            .filter(|e| e.ends_at.is_some_and(|end| end < now))
            .cloned()
            .collect(),
        devlog_sessions: dash::devlog_between(&s.pool, auth.user_id, date, date).await?,
        expenses_total_cents: total,
        expenses_count: count,
    };
    Ok(Json(DayView {
        date,
        today: t,
        timezone: st.timezone.clone(),
        iso_week: iso_week_key(date),
        month: month_key(date),
        is_open: date == t,
        reminder_done: checkin
            .checkin
            .as_ref()
            .is_some_and(|c| c.evening_done_at.is_some()),
        checkin,
        habits: groups,
        month_goals: goals.iter().filter(|g| g.kind == "month").map(brief).collect(),
        week_goals: goals.iter().filter(|g| g.kind == "week").map(brief).collect(),
        devotional_enabled,
        devotional_mode: st.devotional_mode.clone(),
        devotional,
        agenda,
        tasks_by_area: by_area,
        people: dash::interactions_between(&s.pool, auth.user_id, date, date).await?,
        derived,
        currency: st.currency,
        generated_at: now,
    }))
}
