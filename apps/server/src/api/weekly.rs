//! T26 Revisión semanal: resumen calculado (con evidencias a los registros fuente), texto editable, paz 1–5 y tres
//! prioridades. El cierre de objetivos es una acción aparte (`POST /goals/{id}/close`). Propietario: agente A-core.

use axum::Json;
use axum::extract::{Path, State};
use chrono::NaiveDate;
use domain::checkin::validate_peace;
use domain::metrics::{self, Compliance};
use domain::time::{Period, iso_week_key, parse_iso_week};
use persistence::repos::dashboard::{self as dash, ContentBrief, DevlogBrief, InteractionBrief, TaskBrief};
use persistence::repos::goals::GoalFilter;
use persistence::repos::weekly::{self as repo, WeeklyInput, WeeklyReview};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::api::goals::{GoalView, views};
use crate::api::habits::{load_habits, today, user_ctx};
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_reviews))
        .routes(routes!(get_review, put_review))
        .routes(routes!(summary))
}

#[derive(Serialize, ToSchema)]
pub struct FixedHabitWeek {
    pub habit_id: Uuid,
    /// `devotional | exercise | reading`
    pub key: String,
    pub name: String,
    /// Días con `done` en la semana (hasta hoy).
    pub done: u32,
    /// Meta de la semana: días exigibles (diario) o N (N por semana).
    pub target: u32,
    #[schema(value_type = Object)]
    pub compliance: Compliance,
}

#[derive(Serialize, ToSchema)]
pub struct ReflectionItem {
    pub id: Uuid,
    pub date: NaiveDate,
    pub text: String,
}

#[derive(Serialize, ToSchema)]
pub struct AreaActivity {
    pub area_name: String,
    pub done: u32,
    pub open: u32,
}

#[derive(Serialize, ToSchema)]
pub struct DevotionalNote {
    pub date: NaiveDate,
    pub passage: String,
    pub actions: Vec<String>,
    pub learned: String,
}

#[derive(Serialize, ToSchema)]
pub struct Evidence {
    /// Tipo de entidad (ver docs/CONVENTIONS.md §6).
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub label: String,
    /// Ruta de la web para abrir la fuente.
    pub route: String,
}

#[derive(Serialize, ToSchema)]
pub struct WeeklySummary {
    pub iso_week: String,
    pub week_start: NaiveDate,
    pub week_end: NaiveDate,
    /// La semana sigue en curso (los faltantes de días abiertos/futuros no cuentan).
    pub is_open: bool,
    pub fixed_habits: Vec<FixedHabitWeek>,
    /// Objetivos de la semana con adherencia de hábitos y progreso; cerrar es aparte.
    pub goals: Vec<GoalView>,
    pub went_well: Vec<ReflectionItem>,
    pub went_badly: Vec<ReflectionItem>,
    pub tasks_done: Vec<TaskBrief>,
    /// Planeado para la semana y aún abierto ("qué no pasó").
    pub tasks_not_done: Vec<TaskBrief>,
    pub areas: Vec<AreaActivity>,
    pub people: Vec<InteractionBrief>,
    pub content_published: Vec<ContentBrief>,
    pub devlog: Vec<DevlogBrief>,
    pub devotionals: Vec<DevotionalNote>,
    pub expenses_total_cents: i64,
    pub expenses_count: i64,
    pub inbox_pending: i64,
    pub energy_avg: Option<f64>,
    pub evidence: Vec<Evidence>,
}

/// Calcula el resumen de la semana `iso_week` desde los registros fuente (no inventa contenido).
pub async fn compute_summary(s: &AppState, user_id: Uuid, iso_week: &str) -> ApiResult<WeeklySummary> {
    let week = parse_iso_week(iso_week)?;
    let (tz, st) = user_ctx(&s.pool, user_id).await?;
    let t = today(s, tz);
    let habits = load_habits(&s.pool, user_id, tz, week.start, week.end, true).await?;
    let mut fixed = Vec::new();
    let mut evidence = Vec::new();
    for h in habits.iter().filter(|h| h.habit.kind == "fixed") {
        let done = week
            .iter_days()
            .filter(|d| *d <= t && h.entries.get(d) == Some(&domain::habits::EntryStatus::Done))
            .count() as u32;
        let target = match h.spec.version_at(week.end.min(t)).map(|v| v.frequency) {
            Some(domain::habits::Frequency::TimesPerWeek(n)) => n as u32,
            Some(f) => week
                .iter_days()
                .filter(|d| h.spec.version_at(*d).is_some() && f.requires_day(*d) && !h.spec.is_paused(*d))
                .count() as u32,
            None => 0,
        };
        fixed.push(FixedHabitWeek {
            habit_id: h.habit.id,
            key: h.habit.fixed_key.clone().unwrap_or_default(),
            name: h.habit.name.clone(),
            done,
            target,
            compliance: metrics::compliance(&h.spec, &h.entries, week, t),
        });
    }
    let goals = persistence::repos::goals::list(
        &s.pool,
        user_id,
        &GoalFilter {
            kind: Some("week".into()),
            period_key: Some(iso_week.into()),
            status: None,
            from: None,
            to: None,
            include_archived: false,
        },
    )
    .await?;
    for g in &goals {
        evidence.push(Evidence {
            entity_type: "goal".into(),
            entity_id: Some(g.id),
            label: g.title.clone(),
            route: format!("/objetivos?goal={}", g.id),
        });
    }
    let goals = views(s, user_id, goals, t).await?;
    let refl = persistence::repos::checkins::reflections(&s.pool, user_id, week.start, week.end).await?;
    for d in week.iter_days().filter(|d| *d <= t) {
        evidence.push(Evidence {
            entity_type: "checkin".into(),
            entity_id: None,
            label: format!("Día {d}"),
            route: format!("/hoy?date={d}"),
        });
    }
    let tasks_done = dash::tasks_done_between(&s.pool, user_id, &st.timezone, week.start, week.end).await?;
    let tasks_not_done =
        dash::tasks_open_planned_between(&s.pool, user_id, &st.timezone, week.start, week.end).await?;
    let mut areas: Vec<AreaActivity> = Vec::new();
    for (task, is_done) in tasks_done
        .iter()
        .map(|t| (t, true))
        .chain(tasks_not_done.iter().map(|t| (t, false)))
    {
        let name = task.area_name.clone().unwrap_or_else(|| "Sin área".into());
        let a = match areas.iter_mut().position(|a| a.area_name == name) {
            Some(i) => &mut areas[i],
            None => {
                areas.push(AreaActivity {
                    area_name: name,
                    done: 0,
                    open: 0,
                });
                areas.last_mut().expect("push")
            }
        };
        if is_done { a.done += 1 } else { a.open += 1 }
    }
    let content = dash::content_published_between(&s.pool, user_id, week.start, week.end).await?;
    for c in &content {
        evidence.push(Evidence {
            entity_type: "content".into(),
            entity_id: Some(c.id),
            label: c.title.clone(),
            route: format!("/contenido?id={}", c.id),
        });
    }
    let devlog = dash::devlog_between(&s.pool, user_id, week.start, week.end).await?;
    let devs: Vec<(NaiveDate, String, serde_json::Value, String)> = sqlx::query_as(
        "SELECT on_date, passage, verse_actions, learned FROM devotionals WHERE user_id = $1 AND on_date BETWEEN $2 AND $3 ORDER BY on_date",
    )
    .bind(user_id)
    .bind(week.start)
    .bind(week.end)
    .fetch_all(&s.pool)
    .await?;
    let devotionals: Vec<DevotionalNote> = devs
        .into_iter()
        .map(|(date, passage, va, learned)| DevotionalNote {
            date,
            passage,
            actions: va
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.get("action")?.as_str().map(str::to_string))
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            learned,
        })
        .collect();
    for d in &devotionals {
        evidence.push(Evidence {
            entity_type: "devotional".into(),
            entity_id: None,
            label: format!("Devocional {}", d.date),
            route: format!("/devocionales?date={}", d.date),
        });
    }
    let (total, count) = dash::expenses_between(&s.pool, user_id, &st.currency, week.start, week.end).await?;
    evidence.push(Evidence {
        entity_type: "transaction".into(),
        entity_id: None,
        label: "Gastos de la semana".into(),
        route: format!("/finanzas/movimientos?from={}&to={}", week.start, week.end),
    });
    let checkins = persistence::repos::checkins::list(&s.pool, user_id, week.start, week.end).await?;
    let energies: Vec<f64> = checkins.iter().filter_map(|c| c.energy).map(f64::from).collect();
    Ok(WeeklySummary {
        iso_week: iso_week_key(week.start),
        week_start: week.start,
        week_end: week.end,
        is_open: week.end >= t,
        fixed_habits: fixed,
        goals,
        went_well: refl
            .iter()
            .filter(|r| r.polarity == "good")
            .map(|r| ReflectionItem {
                id: r.id,
                date: r.on_date,
                text: r.text.clone(),
            })
            .collect(),
        went_badly: refl
            .iter()
            .filter(|r| r.polarity == "bad")
            .map(|r| ReflectionItem {
                id: r.id,
                date: r.on_date,
                text: r.text.clone(),
            })
            .collect(),
        tasks_done,
        tasks_not_done,
        areas,
        people: dash::interactions_between(&s.pool, user_id, week.start, week.end).await?,
        content_published: content,
        devlog,
        devotionals,
        expenses_total_cents: total,
        expenses_count: count,
        inbox_pending: dash::inbox_pending_count(&s.pool, user_id).await?,
        energy_avg: (!energies.is_empty()).then(|| energies.iter().sum::<f64>() / energies.len() as f64),
        evidence,
    })
}

#[utoipa::path(get, path = "/weekly-reviews", tag = "weekly", responses((status = 200, body = Vec<WeeklyReview>)))]
async fn list_reviews(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<WeeklyReview>>> {
    Ok(Json(repo::list(&s.pool, auth.user_id, 104).await?))
}

#[derive(Serialize, ToSchema)]
pub struct WeeklyReviewView {
    /// `null` si aún no se ha guardado la revisión de esa semana.
    pub review: Option<WeeklyReview>,
    pub version: i32,
    /// Enlace para planear la semana siguiente.
    pub next_week: String,
    pub planning_route: String,
}

fn next_week_key(iso_week: &str) -> ApiResult<String> {
    Ok(iso_week_key(
        parse_iso_week(iso_week)?.end + chrono::Duration::days(1),
    ))
}

#[utoipa::path(get, path = "/weekly-reviews/{iso_week}", tag = "weekly", responses((status = 200, body = WeeklyReviewView)))]
async fn get_review(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(iso_week): Path<String>,
) -> ApiResult<Json<WeeklyReviewView>> {
    let next = next_week_key(&iso_week)?;
    let review = repo::get(&s.pool, auth.user_id, &iso_week).await?;
    Ok(Json(WeeklyReviewView {
        version: review.as_ref().map(|r| r.version).unwrap_or(0),
        review,
        planning_route: format!("/objetivos/planificar?week={next}"),
        next_week: next,
    }))
}

/// Resumen automático (editable en el formulario; no se guarda hasta `PUT`).
#[utoipa::path(get, path = "/weekly-reviews/{iso_week}/summary", tag = "weekly", responses((status = 200, body = WeeklySummary)))]
async fn summary(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(iso_week): Path<String>,
) -> ApiResult<Json<WeeklySummary>> {
    Ok(Json(compute_summary(&s, auth.user_id, &iso_week).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct Priority {
    pub text: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Deserialize, ToSchema)]
pub struct WeeklyBody {
    /// Paz 1–5 aportada por el usuario.
    pub peace: Option<i16>,
    #[serde(default)]
    pub what_happened: String,
    #[serde(default)]
    pub what_didnt: String,
    #[serde(default)]
    pub content_md: String,
    #[serde(default)]
    pub habits_md: String,
    #[serde(default)]
    pub goals_md: String,
    #[serde(default)]
    pub went_well: String,
    #[serde(default)]
    pub went_badly: String,
    #[serde(default)]
    pub areas_md: String,
    #[serde(default)]
    pub people_md: String,
    /// Hasta tres prioridades de la semana siguiente.
    #[serde(default)]
    pub priorities: Vec<Priority>,
    /// Marca la revisión como terminada.
    #[serde(default)]
    pub completed: bool,
    /// 0 o ausente al crear.
    pub version: Option<i32>,
}

/// Guarda la revisión (con snapshot del resumen calculado y sus evidencias).
#[utoipa::path(put, path = "/weekly-reviews/{iso_week}", tag = "weekly", request_body = WeeklyBody,
    responses((status = 200, body = WeeklyReview), (status = 409, body = crate::error::ErrorBody)))]
async fn put_review(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(iso_week): Path<String>,
    Json(b): Json<WeeklyBody>,
) -> ApiResult<Json<WeeklyReview>> {
    let week: Period = parse_iso_week(&iso_week)?;
    if let Some(p) = b.peace {
        validate_peace(p)?;
    }
    let priorities: Vec<&Priority> = b
        .priorities
        .iter()
        .filter(|p| !p.text.trim().is_empty())
        .collect();
    if priorities.len() > 3 {
        return Err(ApiError::validation("priorities", "máximo tres prioridades"));
    }
    let pr = serde_json::Value::Array(
        priorities
            .iter()
            .map(|p| serde_json::json!({ "text": p.text.trim(), "reason": p.reason.trim() }))
            .collect(),
    );
    let summary = compute_summary(&s, auth.user_id, &iso_week).await?;
    let evidence = serde_json::to_value(&summary.evidence).unwrap_or_default();
    let snapshot = serde_json::to_value(&summary).unwrap_or_default();
    let key = iso_week_key(week.start);
    let r = repo::upsert(
        &s.pool,
        auth.user_id,
        &key,
        week.start,
        b.version,
        &WeeklyInput {
            peace: b.peace,
            what_happened: &b.what_happened,
            what_didnt: &b.what_didnt,
            content_md: &b.content_md,
            habits_md: &b.habits_md,
            goals_md: &b.goals_md,
            went_well: &b.went_well,
            went_badly: &b.went_badly,
            areas_md: &b.areas_md,
            people_md: &b.people_md,
            priorities: &pr,
            summary: Some(&snapshot),
            evidence: &evidence,
            completed: b.completed,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "weekly_review",
        Some(r.id),
        "save",
        serde_json::json!({ "week": key, "peace": b.peace }),
    )
    .await;
    Ok(Json(r))
}
