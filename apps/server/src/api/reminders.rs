//! T48 Estado de recordatorios para el usuario: próximas ocurrencias, entregas recientes y pausa (propietario: D-telegram).

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use domain::reminders::{ReminderPrefs, upcoming};
use persistence::repos::jobs::{self, Delivery};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(pause))
        .routes(routes!(resume))
}

#[derive(Serialize, ToSchema)]
pub struct UpcomingReminder {
    /// `morning | daily | weekly | monthly`
    pub kind: String,
    pub occurrence_key: String,
    pub local_date: NaiveDate,
    pub run_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct ReminderStatus {
    pub paused: bool,
    pub paused_until: Option<DateTime<Utc>>,
    /// El bot conoce tu chat privado (enviaste /start).
    pub chat_known: bool,
    /// Telegram acepta mensajes del bot (no bloqueado).
    pub can_message: bool,
    pub last_error: Option<String>,
    pub timezone: String,
    pub upcoming: Vec<UpcomingReminder>,
    /// Últimas entregas: `sent | failed | cancelled | expired | pending | leased`.
    pub recent: Vec<Delivery>,
}

#[derive(sqlx::FromRow)]
struct PrefsRow {
    timezone: String,
    morning_enabled: bool,
    morning_time: chrono::NaiveTime,
    daily_enabled: bool,
    daily_time: chrono::NaiveTime,
    weekly_enabled: bool,
    weekly_weekday: i16,
    weekly_time: chrono::NaiveTime,
    monthly_enabled: bool,
    monthly_time: chrono::NaiveTime,
    reminders_paused: bool,
    paused_until: Option<DateTime<Utc>>,
}

async fn build_status(s: &AppState, auth: &AuthUser) -> ApiResult<ReminderStatus> {
    let p = sqlx::query_as::<_, PrefsRow>(
        "SELECT timezone, morning_enabled, morning_time, daily_enabled, daily_time, weekly_enabled, weekly_weekday, weekly_time, monthly_enabled,
                monthly_time, reminders_paused, paused_until FROM user_settings WHERE user_id = $1",
    )
    .bind(auth.user_id)
    .fetch_one(&s.pool)
    .await?;
    let chat: Option<(bool, Option<String>)> =
        sqlx::query_as("SELECT can_message, last_error FROM telegram_chats WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_optional(&s.pool)
            .await?;
    let prefs = ReminderPrefs {
        morning_enabled: p.morning_enabled,
        morning_time: p.morning_time,
        daily_enabled: p.daily_enabled,
        daily_time: p.daily_time,
        weekly_enabled: p.weekly_enabled,
        weekly_weekday: p.weekly_weekday as u32,
        weekly_time: p.weekly_time,
        monthly_enabled: p.monthly_enabled,
        monthly_time: p.monthly_time,
        paused: p.reminders_paused,
        paused_until: p.paused_until,
    };
    let now = s.now();
    let tz = domain::time::parse_tz(&p.timezone)?;
    let kind = |k: domain::reminders::ReminderKind| match k {
        domain::reminders::ReminderKind::Morning => "morning",
        domain::reminders::ReminderKind::Daily => "daily",
        domain::reminders::ReminderKind::Weekly => "weekly",
        domain::reminders::ReminderKind::Monthly => "monthly",
    };
    Ok(ReminderStatus {
        paused: prefs.is_paused(now),
        paused_until: p.paused_until.filter(|u| *u > now),
        chat_known: chat.is_some(),
        can_message: chat.as_ref().is_some_and(|c| c.0),
        last_error: chat.and_then(|c| c.1),
        timezone: p.timezone.clone(),
        upcoming: upcoming(&prefs, tz, now, 8)
            .into_iter()
            .map(|o| UpcomingReminder {
                kind: kind(o.kind).into(),
                occurrence_key: o.key,
                local_date: o.local_date,
                run_at: o.run_at,
            })
            .collect(),
        recent: jobs::recent_deliveries(&s.pool, auth.user_id, 20).await?,
    })
}

/// Próximos recordatorios, estado del chat y entregas recientes.
#[utoipa::path(get, path = "/reminders", tag = "reminders", responses((status = 200, body = ReminderStatus)))]
async fn status(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<ReminderStatus>> {
    Ok(Json(build_status(&s, &auth).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct ReminderPauseBody {
    /// Días de pausa (1–365). Sin valor: pausa indefinida hasta reanudar.
    pub days: Option<i64>,
}

/// Pausa los recordatorios y cancela los pendientes.
#[utoipa::path(post, path = "/reminders/pause", tag = "reminders", request_body = ReminderPauseBody, responses((status = 200, body = ReminderStatus)))]
async fn pause(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<ReminderPauseBody>,
) -> ApiResult<Json<ReminderStatus>> {
    let mut tx = s.pool.begin().await?;
    match b.days {
        Some(d) if (1..=365).contains(&d) => {
            sqlx::query(
                "UPDATE user_settings SET paused_until = $2, version = version + 1 WHERE user_id = $1",
            )
            .bind(auth.user_id)
            .bind(s.now() + Duration::days(d))
            .execute(&mut *tx)
            .await?;
        }
        Some(_) => return Err(ApiError::validation("days", "entre 1 y 365")),
        None => {
            sqlx::query(
                "UPDATE user_settings SET reminders_paused = true, version = version + 1 WHERE user_id = $1",
            )
            .bind(auth.user_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    jobs::cancel_pending_reminders(&mut *tx, auth.user_id, "recordatorios pausados").await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "reminders",
        None,
        "pause",
        serde_json::json!({ "days": b.days }),
    )
    .await;
    Ok(Json(build_status(&s, &auth).await?))
}

/// Reanuda los recordatorios (el scheduler programa la próxima ocurrencia vigente; no envía atrasados).
#[utoipa::path(post, path = "/reminders/resume", tag = "reminders", responses((status = 200, body = ReminderStatus)))]
async fn resume(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<ReminderStatus>> {
    sqlx::query("UPDATE user_settings SET reminders_paused = false, paused_until = NULL, version = version + 1 WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&s.pool)
        .await?;
    audit(
        &s,
        auth.user_id,
        "reminders",
        None,
        "resume",
        serde_json::json!({}),
    )
    .await;
    Ok(Json(build_status(&s, &auth).await?))
}
