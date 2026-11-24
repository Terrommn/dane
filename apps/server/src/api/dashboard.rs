//! T23 Dashboard general: objetivos vs avance/ritmo, hábitos fijos/mensuales/semanales,
//! 30 días + heatmap + rachas, peso sin interpolar, energía, paz semanal, finanzas y comparación con el mismo tramo
//! anterior (o periodos completos, etiquetado). Cada bloque incluye rutas a sus fuentes. Propietario: agente A-core.

use axum::Json;
use axum::extract::{Query, State};
use chrono::{Duration, NaiveDate};
use domain::metrics::{self, CompareMode, Compliance, PeriodKind, Streaks, TodayProgress};
use domain::time::{Period, iso_week_key, iso_week_period, month_key, month_period};
use persistence::repos::finance::{self as fin, FinanceSummary};
use persistence::repos::goals::GoalFilter;
use persistence::repos::habits::Habit;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::goals::{GoalView, views};
use crate::api::habits::{LoadedHabit, cell_state, load_habits, today, user_ctx};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(dashboard))
}

#[derive(Deserialize, IntoParams)]
pub struct DashboardQuery {
    /// Fecha de referencia (por defecto hoy).
    pub date: Option<NaiveDate>,
    /// `same_span` (por defecto: mismo tramo del periodo anterior) o `full` (periodos completos).
    pub compare: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct HabitStat {
    pub habit: Habit,
    pub goal_title: Option<String>,
    #[schema(value_type = Object)]
    pub today: TodayProgress,
    #[schema(value_type = Object)]
    pub streaks: Streaks,
    /// Adherencia cerrada (sin el día abierto) de la semana y el mes en curso y de los últimos 30 días.
    #[schema(value_type = Object)]
    pub week: Compliance,
    #[schema(value_type = Object)]
    pub month: Compliance,
    #[schema(value_type = Object)]
    pub last_30_days: Compliance,
    pub route: String,
}

#[derive(Serialize, ToSchema)]
pub struct HabitBlock {
    pub fixed: Vec<HabitStat>,
    pub month: Vec<HabitStat>,
    pub week: Vec<HabitStat>,
    pub other: Vec<HabitStat>,
}

#[derive(Serialize, ToSchema)]
pub struct HeatDay {
    pub date: NaiveDate,
    pub done: u32,
    pub required: u32,
    /// `done/required`; `None` si nada era exigible o el día está abierto.
    pub ratio: Option<f64>,
    pub unrecorded: u32,
    pub energy: Option<i16>,
    pub route: String,
}

#[derive(Serialize, ToSchema)]
pub struct WeightPoint {
    pub date: NaiveDate,
    pub kg: f64,
}

#[derive(Serialize, ToSchema)]
pub struct WeightBlock {
    /// Mediciones reales (sin interpolar días sin dato).
    pub points: Vec<WeightPoint>,
    pub last: Option<WeightPoint>,
    /// Cambio primero→último dentro de la semana/mes en curso (solo con ≥ 2 mediciones reales).
    pub change_week: Option<f64>,
    pub change_month: Option<f64>,
    pub goal_kg: Option<f64>,
    /// `decrease | increase | maintain`
    pub direction: Option<String>,
    pub remaining_kg: Option<f64>,
}

#[derive(Serialize, ToSchema)]
pub struct PeacePoint {
    pub iso_week: String,
    pub peace: i16,
    pub route: String,
}

#[derive(Serialize, ToSchema)]
pub struct EnergyPoint {
    pub date: NaiveDate,
    pub energy: i16,
}

#[derive(Serialize, ToSchema)]
pub struct MetricDelta {
    pub key: String,
    pub label: String,
    pub current: Option<f64>,
    pub previous: Option<f64>,
    pub delta: Option<f64>,
    /// `true` si el cambio es favorable; `None` si no aplica o falta dato.
    pub better: Option<bool>,
    /// `count | pct | kg | cents | score`
    pub unit: String,
    pub route: String,
}

#[derive(Serialize, ToSchema)]
pub struct Comparison {
    /// `same_span | full`
    pub mode: String,
    /// Texto para la UI (ej. "del 1 al 12 vs mismo tramo de octubre").
    pub label: String,
    pub current_start: NaiveDate,
    pub current_end: NaiveDate,
    pub previous_start: NaiveDate,
    pub previous_end: NaiveDate,
    pub metrics: Vec<MetricDelta>,
}

#[derive(Serialize, ToSchema)]
pub struct FinanceBlock {
    pub month: FinanceSummary,
    pub week: FinanceSummary,
    pub route: String,
}

#[derive(Serialize, ToSchema)]
pub struct Dashboard {
    pub date: NaiveDate,
    pub today: NaiveDate,
    pub iso_week: String,
    pub month: String,
    pub month_goals: Vec<GoalView>,
    pub week_goals: Vec<GoalView>,
    pub habits: HabitBlock,
    pub last_30_days: Vec<HeatDay>,
    pub weight: WeightBlock,
    pub energy: Vec<EnergyPoint>,
    pub peace: Vec<PeacePoint>,
    pub finance: FinanceBlock,
    pub week_comparison: Comparison,
    pub month_comparison: Comparison,
}

fn stat(h: &LoadedHabit, d: NaiveDate, goal_title: Option<String>) -> HabitStat {
    HabitStat {
        habit: h.habit.clone(),
        goal_title,
        today: metrics::today_progress(&h.spec, &h.entries, d),
        streaks: metrics::streaks(&h.spec, &h.entries, d),
        week: metrics::compliance(&h.spec, &h.entries, iso_week_period(d), d),
        month: metrics::compliance(&h.spec, &h.entries, month_period(d), d),
        last_30_days: metrics::compliance(
            &h.spec,
            &h.entries,
            Period {
                start: d - Duration::days(30),
                end: d - Duration::days(1),
            },
            d,
        ),
        route: format!("/habitos?id={}", h.habit.id),
    }
}

fn f(d: Decimal) -> f64 {
    d.to_f64().unwrap_or(0.0)
}

fn delta(
    key: &str,
    label: &str,
    cur: Option<f64>,
    prev: Option<f64>,
    higher_is_better: Option<bool>,
    unit: &str,
    route: String,
) -> MetricDelta {
    let delta = match (cur, prev) {
        (Some(c), Some(p)) => Some(c - p),
        _ => None,
    };
    let better = match (delta, higher_is_better) {
        (Some(d), Some(h)) if d != 0.0 => Some((d > 0.0) == h),
        _ => None,
    };
    MetricDelta {
        key: key.into(),
        label: label.into(),
        current: cur,
        previous: prev,
        delta,
        better,
        unit: unit.into(),
        route,
    }
}

struct PeriodNumbers {
    fixed_done: Vec<(String, f64)>,
    month_rate: Option<f64>,
    week_rate: Option<f64>,
    weight_last: Option<f64>,
    energy_avg: Option<f64>,
    finance: FinanceSummary,
}

/// Métricas de un periodo para comparar. Los días abiertos/futuros no cuentan como fallas.
async fn numbers(
    s: &AppState,
    user_id: uuid::Uuid,
    habits: &[LoadedHabit],
    goal_kind: &std::collections::HashMap<uuid::Uuid, String>,
    p: Period,
    today: NaiveDate,
) -> ApiResult<PeriodNumbers> {
    let mut fixed_done = Vec::new();
    for h in habits.iter().filter(|h| h.habit.kind == "fixed") {
        let done = p
            .iter_days()
            .filter(|d| *d <= today && h.entries.get(d) == Some(&domain::habits::EntryStatus::Done))
            .count();
        fixed_done.push((h.habit.fixed_key.clone().unwrap_or_default(), done as f64));
    }
    let rate = |kind: &str| {
        let items: Vec<Compliance> = habits
            .iter()
            .filter(|h| {
                h.habit
                    .goal_id
                    .and_then(|g| goal_kind.get(&g))
                    .is_some_and(|k| k == kind)
            })
            .map(|h| metrics::compliance(&h.spec, &h.entries, p, today))
            .collect();
        Compliance::merge(&items).rate.map(|r| r * 100.0)
    };
    let checkins = persistence::repos::checkins::list(&s.pool, user_id, p.start, p.end).await?;
    let weight_last = checkins.iter().rev().find_map(|c| c.weight_kg).map(f);
    let energies: Vec<f64> = checkins.iter().filter_map(|c| c.energy).map(f64::from).collect();
    Ok(PeriodNumbers {
        fixed_done,
        month_rate: rate("month"),
        week_rate: rate("week"),
        weight_last,
        energy_avg: (!energies.is_empty()).then(|| energies.iter().sum::<f64>() / energies.len() as f64),
        finance: fin::period_summary(&s.pool, user_id, p.start, p.end).await?,
    })
}

#[allow(clippy::too_many_arguments)]
async fn comparison(
    s: &AppState,
    user_id: uuid::Uuid,
    habits: &[LoadedHabit],
    goal_kind: &std::collections::HashMap<uuid::Uuid, String>,
    kind: PeriodKind,
    date: NaiveDate,
    mode: CompareMode,
    today: NaiveDate,
    weight_lower_is_better: Option<bool>,
) -> ApiResult<Comparison> {
    let (cur, prev) = metrics::comparison(kind, date, mode);
    let a = numbers(s, user_id, habits, goal_kind, cur, today).await?;
    let b = numbers(s, user_id, habits, goal_kind, prev, today).await?;
    let mut m = Vec::new();
    for (key, cur_v) in &a.fixed_done {
        let prev_v = b.fixed_done.iter().find(|(k, _)| k == key).map(|x| x.1);
        let label = match key.as_str() {
            "exercise" => "Ejercicio (sesiones)",
            "reading" => "Lectura (días)",
            _ => "Hábito fijo",
        };
        m.push(delta(
            &format!("fixed_{key}"),
            label,
            Some(*cur_v),
            prev_v,
            Some(true),
            "count",
            "/habitos".into(),
        ));
    }
    m.push(delta(
        "month_habits",
        "Hábitos del mes (%)",
        a.month_rate,
        b.month_rate,
        Some(true),
        "pct",
        "/habitos".into(),
    ));
    m.push(delta(
        "week_habits",
        "Hábitos de la semana (%)",
        a.week_rate,
        b.week_rate,
        Some(true),
        "pct",
        "/habitos".into(),
    ));
    m.push(delta(
        "weight_last",
        "Peso (último)",
        a.weight_last,
        b.weight_last,
        weight_lower_is_better.map(|l| !l),
        "kg",
        format!("/hoy?date={date}"),
    ));
    m.push(delta(
        "energy_avg",
        "Energía promedio",
        a.energy_avg,
        b.energy_avg,
        Some(true),
        "score",
        format!("/hoy?date={date}"),
    ));
    m.push(delta(
        "consumption",
        "Gasto de consumo",
        Some(a.finance.consumption_cents as f64),
        Some(b.finance.consumption_cents as f64),
        Some(false),
        "cents",
        format!("/finanzas/movimientos?from={}&to={}", cur.start, cur.end),
    ));
    m.push(delta(
        "debt_payments",
        "Pagado a deudas",
        Some(a.finance.debt_payments_cents as f64),
        Some(b.finance.debt_payments_cents as f64),
        Some(true),
        "cents",
        format!("/finanzas/movimientos?from={}&to={}", cur.start, cur.end),
    ));
    let label = match mode {
        CompareMode::SameSpan => format!(
            "{} a {} vs mismo tramo anterior ({} a {})",
            cur.start, cur.end, prev.start, prev.end
        ),
        CompareMode::Full => format!(
            "Periodos completos: {} a {} vs {} a {}",
            cur.start, cur.end, prev.start, prev.end
        ),
    };
    Ok(Comparison {
        mode: if mode == CompareMode::Full {
            "full".into()
        } else {
            "same_span".into()
        },
        label,
        current_start: cur.start,
        current_end: cur.end,
        previous_start: prev.start,
        previous_end: prev.end,
        metrics: m,
    })
}

/// Dashboard general.
#[utoipa::path(get, path = "/dashboard", tag = "dashboard", params(DashboardQuery), responses((status = 200, body = Dashboard)))]
async fn dashboard(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<DashboardQuery>,
) -> ApiResult<Json<Dashboard>> {
    let (tz, st) = user_ctx(&s.pool, auth.user_id).await?;
    let t = today(&s, tz);
    let date = q.date.unwrap_or(t);
    let mode = match q.compare.as_deref() {
        None | Some("same_span") => CompareMode::SameSpan,
        Some("full") => CompareMode::Full,
        _ => return Err(ApiError::validation("compare", "same_span o full")),
    };
    // El cálculo usa `date` como "hoy" de referencia (no penaliza días posteriores a la fecha consultada).
    let ref_today = date.min(t);
    let month = month_period(date);
    let week = iso_week_period(date);
    let from = (month.start - Duration::days(70)).min(date - Duration::days(400));
    let habits = load_habits(&s.pool, auth.user_id, tz, from, date, true).await?;
    let all_goals = persistence::repos::goals::list(
        &s.pool,
        auth.user_id,
        &GoalFilter {
            kind: None,
            period_key: None,
            status: None,
            from: Some(from),
            to: None,
            include_archived: true,
        },
    )
    .await?;
    let goal_kind: std::collections::HashMap<uuid::Uuid, String> =
        all_goals.iter().map(|g| (g.id, g.kind.clone())).collect();
    let goal_title: std::collections::HashMap<uuid::Uuid, String> =
        all_goals.iter().map(|g| (g.id, g.title.clone())).collect();
    let month_goals = views(
        &s,
        auth.user_id,
        all_goals
            .iter()
            .filter(|g| g.kind == "month" && g.period_key == month_key(date) && g.archived_at.is_none())
            .cloned()
            .collect(),
        ref_today,
    )
    .await?;
    let week_goals = views(
        &s,
        auth.user_id,
        all_goals
            .iter()
            .filter(|g| g.kind == "week" && g.period_key == iso_week_key(date) && g.archived_at.is_none())
            .cloned()
            .collect(),
        ref_today,
    )
    .await?;

    let mut block = HabitBlock {
        fixed: vec![],
        month: vec![],
        week: vec![],
        other: vec![],
    };
    for h in habits.iter().filter(|h| {
        h.spec.active_on(date) || h.habit.archived_at.is_none() && h.spec.version_at(date).is_some()
    }) {
        let title = h.habit.goal_id.and_then(|g| goal_title.get(&g).cloned());
        let stat = stat(h, ref_today, title);
        match (
            h.habit.kind.as_str(),
            h.habit
                .goal_id
                .and_then(|g| goal_kind.get(&g))
                .map(String::as_str),
        ) {
            ("fixed", _) => block.fixed.push(stat),
            (_, Some("month")) => block.month.push(stat),
            (_, Some("week")) => block.week.push(stat),
            _ => block.other.push(stat),
        }
    }

    let checkins =
        persistence::repos::checkins::list(&s.pool, auth.user_id, date - Duration::days(120), date).await?;
    let energy_by_day: std::collections::HashMap<NaiveDate, i16> = checkins
        .iter()
        .filter_map(|c| Some((c.on_date, c.energy?)))
        .collect();
    let mut heat = Vec::new();
    for d in (Period {
        start: date - Duration::days(29),
        end: date,
    })
    .iter_days()
    {
        let (mut done, mut required, mut unrecorded) = (0, 0, 0);
        for h in &habits {
            match cell_state(h, d, ref_today) {
                "done" => {
                    done += 1;
                    required += 1
                }
                "not_done" => required += 1,
                "unrecorded" => {
                    unrecorded += 1;
                    required += 1
                }
                _ => {}
            }
        }
        heat.push(HeatDay {
            date: d,
            done,
            required,
            ratio: (required > 0 && d < ref_today).then(|| done as f64 / required as f64),
            unrecorded,
            energy: energy_by_day.get(&d).copied(),
            route: format!("/hoy?date={d}"),
        });
    }

    let points: Vec<(NaiveDate, f64)> = checkins
        .iter()
        .filter_map(|c| Some((c.on_date, f(c.weight_kg?))))
        .collect();
    let change = |p: Period| {
        metrics::first_last(&points, p)
            .and_then(|(a, b)| (a.0 != b.0).then(|| ((b.1 - a.1) * 100.0).round() / 100.0))
    };
    let last = points
        .iter()
        .rev()
        .find(|(d, _)| *d <= date)
        .map(|(d, v)| WeightPoint { date: *d, kg: *v });
    let goal_kg = st.weight_goal_kg.map(f);
    let remaining_kg = match (&last, goal_kg, st.weight_direction.as_deref()) {
        (Some(l), Some(g), Some("decrease")) => Some((l.kg - g).max(0.0)),
        (Some(l), Some(g), Some("increase")) => Some((g - l.kg).max(0.0)),
        (Some(l), Some(g), _) => Some((l.kg - g).abs()),
        _ => None,
    };
    let weight = WeightBlock {
        change_week: change(week),
        change_month: change(month),
        points: points
            .iter()
            .filter(|(d, _)| *d >= date - Duration::days(90))
            .map(|(d, v)| WeightPoint { date: *d, kg: *v })
            .collect(),
        last,
        goal_kg,
        direction: st.weight_direction.clone(),
        remaining_kg,
    };
    let energy: Vec<EnergyPoint> = checkins
        .iter()
        .filter(|c| c.on_date >= date - Duration::days(29))
        .filter_map(|c| {
            Some(EnergyPoint {
                date: c.on_date,
                energy: c.energy?,
            })
        })
        .collect();
    let peace =
        persistence::repos::weekly::peace_series(&s.pool, auth.user_id, date - Duration::days(7 * 26), date)
            .await?
            .into_iter()
            .map(|(w, _, p)| PeacePoint {
                route: format!("/semanal?week={w}"),
                iso_week: w,
                peace: p,
            })
            .collect();
    let (fm, _) = metrics::comparison(PeriodKind::Month, date, CompareMode::SameSpan);
    let (fw, _) = metrics::comparison(PeriodKind::Week, date, CompareMode::SameSpan);
    let finance = FinanceBlock {
        month: fin::period_summary(&s.pool, auth.user_id, fm.start, fm.end).await?,
        week: fin::period_summary(&s.pool, auth.user_id, fw.start, fw.end).await?,
        route: "/finanzas".into(),
    };
    let lower_better = match st.weight_direction.as_deref() {
        Some("decrease") => Some(true),
        Some("increase") => Some(false),
        _ => None,
    };
    let week_comparison = comparison(
        &s,
        auth.user_id,
        &habits,
        &goal_kind,
        PeriodKind::Week,
        date,
        mode,
        ref_today,
        lower_better,
    )
    .await?;
    let month_comparison = comparison(
        &s,
        auth.user_id,
        &habits,
        &goal_kind,
        PeriodKind::Month,
        date,
        mode,
        ref_today,
        lower_better,
    )
    .await?;
    Ok(Json(Dashboard {
        date,
        today: t,
        iso_week: iso_week_key(date),
        month: month_key(date),
        month_goals,
        week_goals,
        habits: block,
        last_30_days: heat,
        weight,
        energy,
        peace,
        finance,
        week_comparison,
        month_comparison,
    }))
}
