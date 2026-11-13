//! Scheduler de recordatorios. Seguro con varias instancias: el UNIQUE de `jobs` evita duplicados y solo se asegura la
//! ocurrencia local vigente (sin acumular históricos). Recalcula por fecha local en la zona del usuario.

use chrono::{DateTime, NaiveTime, Utc};
use domain::reminders::{Occurrence, ReminderKind, ReminderPrefs, due_occurrences};
use persistence::repos::jobs;
use sqlx::FromRow;
use uuid::Uuid;

use super::messages::{app_button, human_date, message, month_name};
use crate::state::AppState;

#[derive(Debug, FromRow)]
pub(crate) struct UserReminderRow {
    pub user_id: Uuid,
    pub chat_id: i64,
    pub timezone: String,
    pub daily_enabled: bool,
    pub daily_time: NaiveTime,
    pub weekly_enabled: bool,
    pub weekly_weekday: i16,
    pub weekly_time: NaiveTime,
    pub monthly_enabled: bool,
    pub monthly_time: NaiveTime,
    pub reminders_paused: bool,
    pub paused_until: Option<DateTime<Utc>>,
}

impl UserReminderRow {
    pub fn prefs(&self) -> ReminderPrefs {
        ReminderPrefs {
            daily_enabled: self.daily_enabled,
            daily_time: self.daily_time,
            weekly_enabled: self.weekly_enabled,
            weekly_weekday: self.weekly_weekday as u32,
            weekly_time: self.weekly_time,
            monthly_enabled: self.monthly_enabled,
            monthly_time: self.monthly_time,
            paused: self.reminders_paused,
            paused_until: self.paused_until,
        }
    }
}

fn payload(state: &AppState, chat_id: i64, o: &Occurrence) -> serde_json::Value {
    let msg = match o.kind {
        ReminderKind::Daily => message(
            chat_id,
            format!(
                "🌙 ¿Cómo te fue hoy? Registra tu check-in del {}.",
                human_date(o.local_date)
            ),
            vec![vec![app_button(
                state,
                "Abrir check-in",
                &format!("/hoy?date={}", o.local_date),
            )]],
        ),
        ReminderKind::Weekly => message(
            chat_id,
            format!("🗓️ Revisión semanal {}: ¿qué pasó esta semana?", o.key),
            vec![vec![app_button(
                state,
                "Abrir revisión",
                &format!("/semanal?week={}", o.key),
            )]],
        ),
        ReminderKind::Monthly => message(
            chat_id,
            format!(
                "📅 Empieza {}: define tus objetivos del mes.",
                month_name(o.local_date)
            ),
            vec![vec![app_button(state, "Planificar", "/objetivos/planificar")]],
        ),
    };
    let mut v = serde_json::to_value(msg).expect("mensaje serializable");
    v["date"] = serde_json::Value::String(o.local_date.to_string());
    v
}

/// Una pasada del scheduler. Devuelve cuántos recordatorios nuevos creó.
pub async fn run_once(state: &AppState) -> anyhow::Result<usize> {
    let now = state.now();
    let rows = sqlx::query_as::<_, UserReminderRow>(
        "SELECT s.user_id, c.chat_id, s.timezone, s.daily_enabled, s.daily_time, s.weekly_enabled, s.weekly_weekday,
                s.weekly_time, s.monthly_enabled, s.monthly_time, s.reminders_paused, s.paused_until
         FROM user_settings s
         JOIN telegram_chats c ON c.user_id = s.user_id AND c.can_message
         JOIN users u ON u.id = s.user_id AND u.disabled_at IS NULL AND NOT u.is_demo",
    )
    .fetch_all(&state.pool)
    .await?;
    let mut created = 0;
    for r in rows {
        let Ok(tz) = domain::time::parse_tz(&r.timezone) else {
            tracing::warn!(user_id = %r.user_id, "zona horaria inválida; se omite");
            continue;
        };
        for o in due_occurrences(&r.prefs(), tz, now) {
            if o.kind == ReminderKind::Daily {
                let done: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM daily_checkins WHERE user_id = $1 AND on_date = $2 AND evening_done_at IS NOT NULL)",
                )
                .bind(r.user_id)
                .bind(o.local_date)
                .fetch_one(&state.pool)
                .await?;
                if done {
                    continue;
                }
            }
            if jobs::upsert_reminder(
                &state.pool,
                r.user_id,
                o.kind.job_kind(),
                &o.key,
                &payload(state, r.chat_id, &o),
                o.run_at,
                o.expires_at,
            )
            .await?
            {
                created += 1;
            }
        }
    }
    Ok(created)
}
