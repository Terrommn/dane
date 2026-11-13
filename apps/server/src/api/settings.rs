//! T15 Preferencias (zona, moneda, módulos, recordatorios, metas). Merge parcial con `version`.

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, NaiveTime, Utc};
use persistence::repos::identity::{self, UserSettings};
use rust_decimal::Decimal;
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_settings, update_settings))
}

#[utoipa::path(get, path = "/settings", tag = "settings", responses((status = 200, body = UserSettings)))]
async fn get_settings(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<UserSettings>> {
    Ok(Json(identity::get_settings(&s.pool, auth.user_id).await?))
}

/// Campos ausentes no cambian. Para borrar un opcional envía `null` explícito en los campos `Option<Option<_>>`.
#[derive(Deserialize, ToSchema, Default)]
pub struct SettingsPatch {
    pub version: i32,
    pub locale: Option<String>,
    pub timezone: Option<String>,
    pub currency: Option<String>,
    pub week_start: Option<i16>,
    #[schema(value_type = Option<Object>)]
    pub modules: Option<serde_json::Value>,
    pub devotional_mode: Option<String>,
    #[schema(value_type = Option<Vec<Object>>)]
    pub devotional_template: Option<serde_json::Value>,
    #[serde(default, with = "double_option")]
    #[schema(value_type = Option<i32>)]
    pub max_devotional_failures: Option<Option<i32>>,
    #[serde(default, with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub weight_goal_kg: Option<Option<Decimal>>,
    #[serde(default, with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub weight_direction: Option<Option<String>>,
    pub habit_warn_month: Option<i32>,
    pub habit_warn_total: Option<i32>,
    pub track_family_loans: Option<bool>,
    pub daily_enabled: Option<bool>,
    #[schema(value_type = Option<String>, example = "21:00")]
    pub daily_time: Option<NaiveTime>,
    pub weekly_enabled: Option<bool>,
    pub weekly_weekday: Option<i16>,
    #[schema(value_type = Option<String>)]
    pub weekly_time: Option<NaiveTime>,
    pub monthly_enabled: Option<bool>,
    #[schema(value_type = Option<String>)]
    pub monthly_time: Option<NaiveTime>,
    pub reminders_paused: Option<bool>,
    #[serde(default, with = "double_option")]
    #[schema(value_type = Option<DateTime<Utc>>)]
    pub paused_until: Option<Option<DateTime<Utc>>>,
    pub onboarding_completed: Option<bool>,
}

/// Distingue "campo ausente" (None) de "null explícito" (Some(None)).
pub mod double_option {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Option<T>>, D::Error> {
        Ok(Some(Option::deserialize(d)?))
    }
}

pub fn apply_patch(cur: &mut UserSettings, p: SettingsPatch, now: DateTime<Utc>) -> ApiResult<()> {
    cur.version = p.version;
    if let Some(v) = p.locale {
        cur.locale = v;
    }
    if let Some(tz) = p.timezone {
        domain::time::parse_tz(&tz)?;
        cur.timezone = tz;
    }
    if let Some(c) = p.currency {
        cur.currency = domain::money::Currency::parse(&c)?.to_string();
    }
    if let Some(w) = p.week_start {
        if !(1..=7).contains(&w) {
            return Err(ApiError::validation("week_start", "1 (lunes) a 7 (domingo)"));
        }
        cur.week_start = w;
    }
    if let Some(m) = p.modules {
        if !m.is_object() {
            return Err(ApiError::validation("modules", "debe ser un objeto"));
        }
        cur.modules = m;
    }
    if let Some(m) = p.devotional_mode {
        if !["devotional", "meditation"].contains(&m.as_str()) {
            return Err(ApiError::validation("devotional_mode", "devotional o meditation"));
        }
        cur.devotional_mode = m;
    }
    if let Some(t) = p.devotional_template {
        if !t.is_array() {
            return Err(ApiError::validation(
                "devotional_template",
                "debe ser una lista de secciones",
            ));
        }
        cur.devotional_template = t;
    }
    if let Some(v) = p.max_devotional_failures {
        cur.max_devotional_failures = v;
    }
    if let Some(v) = p.weight_goal_kg {
        cur.weight_goal_kg = v;
    }
    if let Some(v) = p.weight_direction {
        if let Some(d) = &v {
            if !["decrease", "increase", "maintain"].contains(&d.as_str()) {
                return Err(ApiError::validation(
                    "weight_direction",
                    "decrease, increase o maintain",
                ));
            }
        }
        cur.weight_direction = v;
    }
    if let Some(v) = p.habit_warn_month {
        cur.habit_warn_month = v;
    }
    if let Some(v) = p.habit_warn_total {
        cur.habit_warn_total = v;
    }
    if let Some(v) = p.track_family_loans {
        cur.track_family_loans = v;
    }
    if let Some(v) = p.daily_enabled {
        cur.daily_enabled = v;
    }
    if let Some(v) = p.daily_time {
        cur.daily_time = v;
    }
    if let Some(v) = p.weekly_enabled {
        cur.weekly_enabled = v;
    }
    if let Some(v) = p.weekly_weekday {
        if !(1..=7).contains(&v) {
            return Err(ApiError::validation("weekly_weekday", "1 (lunes) a 7 (domingo)"));
        }
        cur.weekly_weekday = v;
    }
    if let Some(v) = p.weekly_time {
        cur.weekly_time = v;
    }
    if let Some(v) = p.monthly_enabled {
        cur.monthly_enabled = v;
    }
    if let Some(v) = p.monthly_time {
        cur.monthly_time = v;
    }
    if let Some(v) = p.reminders_paused {
        cur.reminders_paused = v;
    }
    if let Some(v) = p.paused_until {
        cur.paused_until = v;
    }
    if let Some(done) = p.onboarding_completed {
        cur.onboarding_completed_at = if done {
            cur.onboarding_completed_at.or(Some(now))
        } else {
            None
        };
    }
    Ok(())
}

/// Actualiza preferencias. Cambiar la zona afecta cálculos futuros; las fechas guardadas no se reclasifican.
#[utoipa::path(put, path = "/settings", tag = "settings", request_body = SettingsPatch,
    responses((status = 200, body = UserSettings), (status = 409, body = crate::error::ErrorBody)))]
async fn update_settings(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(p): Json<SettingsPatch>,
) -> ApiResult<Json<UserSettings>> {
    let mut cur = identity::get_settings(&s.pool, auth.user_id).await?;
    apply_patch(&mut cur, p, s.now())?;
    let saved = identity::save_settings(&s.pool, &cur).await?;
    audit(
        &s,
        auth.user_id,
        "settings",
        None,
        "update",
        serde_json::json!({ "timezone": saved.timezone }),
    )
    .await;
    Ok(Json(saved))
}
