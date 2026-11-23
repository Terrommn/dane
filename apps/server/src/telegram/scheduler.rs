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
    pub display_name: String,
    pub morning_enabled: bool,
    pub morning_time: NaiveTime,
    pub event_reminder_minutes: i16,
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
            morning_enabled: self.morning_enabled,
            morning_time: self.morning_time,
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

/// Resumen del día para el mensaje de la mañana: lo que hace útil abrir el check-in (y no solo "recuérdalo").
#[derive(Debug, Default)]
pub(crate) struct MorningSummary {
    pub open_tasks: i64,
    /// Eventos con hora de hoy (hora local "HH:MM", título), en orden.
    pub events: Vec<(String, String)>,
}

/// Texto del mensaje de la mañana. Separado del envío para poder probarlo sin base de datos.
pub(crate) fn morning_text(name: &str, date: chrono::NaiveDate, s: &MorningSummary) -> String {
    let hello = if name.trim().is_empty() {
        "☀️ Buenos días.".to_string()
    } else {
        format!("☀️ Buenos días, {}.", name.trim())
    };
    let mut t = format!("{hello} Hoy es {}.\n", human_date(date));
    match s.open_tasks {
        0 => t.push_str("No tienes tareas pendientes para hoy."),
        1 => t.push_str("Tienes 1 tarea pendiente."),
        n => t.push_str(&format!("Tienes {n} tareas pendientes.")),
    }
    if !s.events.is_empty() {
        t.push_str("\n\n📅 Agenda:");
        for (hhmm, title) in s.events.iter().take(5) {
            t.push_str(&format!("\n• {hhmm} {title}"));
        }
        if s.events.len() > 5 {
            t.push_str(&format!("\n• y {} más", s.events.len() - 5));
        }
    }
    t.push_str("\n\n¿Hacemos el check-in de la mañana? Son 2 minutos.");
    t
}

fn payload(
    state: &AppState,
    r: &UserReminderRow,
    o: &Occurrence,
    summary: Option<&MorningSummary>,
) -> serde_json::Value {
    let chat_id = r.chat_id;
    let msg = match o.kind {
        ReminderKind::Morning => message(
            chat_id,
            morning_text(
                &r.display_name,
                o.local_date,
                summary.unwrap_or(&MorningSummary::default()),
            ),
            vec![
                vec![app_button(
                    state,
                    "Empezar check-in",
                    &format!("/hoy?date={}&guiado=manana", o.local_date),
                )],
                vec![app_button(state, "Ver mi día", "/")],
            ],
        ),
        ReminderKind::Daily => message(
            chat_id,
            format!(
                "🌙 ¿Cómo te fue hoy? Cierra tu día del {} en 2 minutos.",
                human_date(o.local_date)
            ),
            vec![vec![app_button(
                state,
                "Cerrar el día",
                &format!("/hoy?date={}&guiado=noche", o.local_date),
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
        "SELECT s.user_id, c.chat_id, s.timezone, u.display_name, s.morning_enabled, s.morning_time,
                s.event_reminder_minutes, s.daily_enabled, s.daily_time, s.weekly_enabled, s.weekly_weekday,
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
            // Mañana y noche se omiten si esa parte del check-in ya se hizo (no tiene sentido recordarla).
            let done_sql = match o.kind {
                ReminderKind::Morning => Some(
                    "SELECT EXISTS (SELECT 1 FROM daily_checkins WHERE user_id = $1 AND on_date = $2 AND morning_done_at IS NOT NULL)",
                ),
                ReminderKind::Daily => Some(
                    "SELECT EXISTS (SELECT 1 FROM daily_checkins WHERE user_id = $1 AND on_date = $2 AND evening_done_at IS NOT NULL)",
                ),
                _ => None,
            };
            if let Some(sql) = done_sql {
                let done: bool = sqlx::query_scalar(sql)
                    .bind(r.user_id)
                    .bind(o.local_date)
                    .fetch_one(&state.pool)
                    .await?;
                if done {
                    continue;
                }
            }
            let summary = if o.kind == ReminderKind::Morning {
                Some(morning_summary(state, r.user_id, o.local_date, tz).await?)
            } else {
                None
            };
            if jobs::upsert_reminder(
                &state.pool,
                r.user_id,
                o.kind.job_kind(),
                &o.key,
                &payload(state, &r, &o, summary.as_ref()),
                o.run_at,
                o.expires_at,
            )
            .await?
            {
                created += 1;
            }
        }
        created += event_reminders(state, &r, tz, now).await?;
    }
    Ok(created)
}

async fn morning_summary(
    state: &AppState,
    user_id: uuid::Uuid,
    date: chrono::NaiveDate,
    tz: chrono_tz::Tz,
) -> anyhow::Result<MorningSummary> {
    // Misma regla que "Tareas por área" en Hoy: abiertas planeadas o con vencimiento hasta hoy (incluye arrastradas).
    let open_tasks: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM tasks WHERE user_id = $1 AND status = 'open' AND archived_at IS NULL
           AND COALESCE(planned_on, due_on) <= $2",
    )
    .bind(user_id)
    .bind(date)
    .fetch_one(&state.pool)
    .await?;
    let rows: Vec<(DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT starts_at, title FROM calendar_events
         WHERE user_id = $1 AND on_date = $2 AND NOT all_day AND starts_at IS NOT NULL AND status <> 'cancelled'
         ORDER BY starts_at",
    )
    .bind(user_id)
    .bind(date)
    .fetch_all(&state.pool)
    .await?;
    let events = rows
        .into_iter()
        .map(|(at, title)| (at.with_timezone(&tz).format("%H:%M").to_string(), title))
        .collect();
    Ok(MorningSummary { open_tasks, events })
}

/// Avisos "en N minutos tienes…" para eventos con hora (Google Calendar o manuales). Reemplaza la app de
/// Recordatorios. Idempotente: la clave del job incluye el evento y su hora, así que mover el evento genera un aviso
/// nuevo y el viejo caduca solo. Respeta la pausa global de recordatorios.
async fn event_reminders(
    state: &AppState,
    r: &UserReminderRow,
    tz: chrono_tz::Tz,
    now: DateTime<Utc>,
) -> anyhow::Result<usize> {
    let lead = chrono::Duration::minutes(i64::from(r.event_reminder_minutes));
    if r.event_reminder_minutes <= 0 || r.prefs().is_paused(now) {
        return Ok(0);
    }
    // Solo eventos que empiezan dentro de la ventana [now, now + aviso + 1 pasada del scheduler].
    let rows: Vec<(uuid::Uuid, DateTime<Utc>, String, Option<String>)> = sqlx::query_as(
        "SELECT id, starts_at, title, location FROM calendar_events
         WHERE user_id = $1 AND NOT all_day AND status <> 'cancelled'
           AND starts_at > $2 AND starts_at <= $3",
    )
    .bind(r.user_id)
    .bind(now)
    .bind(now + lead + chrono::Duration::minutes(5))
    .fetch_all(&state.pool)
    .await?;
    let mut created = 0;
    for (id, starts_at, title, location) in rows {
        let local = starts_at.with_timezone(&tz);
        let mut text = format!("⏰ A las {}: {}", local.format("%H:%M"), title);
        if let Some(l) = location.filter(|l| !l.trim().is_empty()) {
            text.push_str(&format!("\n📍 {l}"));
        }
        let msg = message(
            r.chat_id,
            text,
            vec![vec![app_button(state, "Ver agenda", "/agenda")]],
        );
        let key = format!("{id}@{}", starts_at.timestamp());
        let run_at = (starts_at - lead).max(now);
        if jobs::upsert_reminder(
            &state.pool,
            r.user_id,
            "reminder_event",
            &key,
            &serde_json::to_value(msg)?,
            run_at,
            starts_at,
        )
        .await?
        {
            created += 1;
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morning_text_lists_tasks_and_events() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let s = MorningSummary {
            open_tasks: 3,
            events: vec![("09:00".into(), "Junta con cliente".into())],
        };
        let t = morning_text("David", d, &s);
        assert!(t.starts_with("☀️ Buenos días, David. Hoy es lunes 23 de noviembre."));
        assert!(t.contains("Tienes 3 tareas pendientes."));
        assert!(t.contains("• 09:00 Junta con cliente"));
        let empty = morning_text("", d, &MorningSummary::default());
        assert!(empty.starts_with("☀️ Buenos días. Hoy"));
        assert!(empty.contains("No tienes tareas pendientes para hoy."));
    }
}
