//! Recordatorios: cálculo puro de ocurrencias en la zona del usuario (propietario: D-telegram).
//!
//! Reglas:
//! * La ocurrencia se calcula por **fecha local** y hora local → UTC con la regla DST de `time::local_to_utc`
//!   (nunca sumando 24 h UTC).
//! * Diario: cada día a `daily_time` (clave `YYYY-MM-DD`). Semanal: el día ISO `weekly_weekday` a `weekly_time`
//!   (clave `GGGG-Www`). Mensual: el día 1 a `monthly_time` (clave `YYYY-MM`).
//! * No se generan ocurrencias vencidas: si `now` ya pasó `run_at + gracia`, se omite (sin recordatorios históricos
//!   acumulados). La gracia también es la caducidad del job.
//! * Pausa indefinida (`paused`) o temporal (`paused_until > now`) no genera nada.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use serde::Serialize;

use crate::time::{iso_week_key, local_date, local_to_utc, month_key};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    Daily,
    Weekly,
    Monthly,
}

impl ReminderKind {
    /// Valor de `jobs.kind`.
    pub fn job_kind(&self) -> &'static str {
        match self {
            Self::Daily => "reminder_daily",
            Self::Weekly => "reminder_weekly",
            Self::Monthly => "reminder_monthly",
        }
    }
    /// Ventana durante la cual aún tiene sentido enviar el recordatorio.
    pub fn grace(&self) -> Duration {
        match self {
            Self::Daily => Duration::hours(3),
            Self::Weekly | Self::Monthly => Duration::hours(12),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderPrefs {
    pub daily_enabled: bool,
    pub daily_time: NaiveTime,
    pub weekly_enabled: bool,
    /// 1 = lunes … 7 = domingo.
    pub weekly_weekday: u32,
    pub weekly_time: NaiveTime,
    pub monthly_enabled: bool,
    pub monthly_time: NaiveTime,
    pub paused: bool,
    pub paused_until: Option<DateTime<Utc>>,
}

impl ReminderPrefs {
    pub fn is_paused(&self, now: DateTime<Utc>) -> bool {
        self.paused || self.paused_until.is_some_and(|u| u > now)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Occurrence {
    pub kind: ReminderKind,
    /// `occurrence_key` del job (fecha, semana ISO o mes).
    pub key: String,
    pub local_date: NaiveDate,
    pub run_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

fn occurrence_on(prefs: &ReminderPrefs, kind: ReminderKind, date: NaiveDate, tz: Tz) -> Option<Occurrence> {
    let (enabled, time, applies, key) = match kind {
        ReminderKind::Daily => (prefs.daily_enabled, prefs.daily_time, true, date.to_string()),
        ReminderKind::Weekly => (
            prefs.weekly_enabled,
            prefs.weekly_time,
            date.weekday().number_from_monday() == prefs.weekly_weekday,
            iso_week_key(date),
        ),
        ReminderKind::Monthly => (
            prefs.monthly_enabled,
            prefs.monthly_time,
            date.day() == 1,
            month_key(date),
        ),
    };
    if !enabled || !applies {
        return None;
    }
    let run_at = local_to_utc(date, time, tz);
    Some(Occurrence {
        kind,
        key,
        local_date: date,
        run_at,
        expires_at: run_at + kind.grace(),
    })
}

const KINDS: [ReminderKind; 3] = [ReminderKind::Daily, ReminderKind::Weekly, ReminderKind::Monthly];

/// Ocurrencias que el scheduler debe asegurar ahora: las de la fecha local de hoy que no han caducado.
/// Incluye las futuras de hoy (el bot solo las reclama cuando llega `run_at`).
pub fn due_occurrences(prefs: &ReminderPrefs, tz: Tz, now: DateTime<Utc>) -> Vec<Occurrence> {
    if prefs.is_paused(now) {
        return Vec::new();
    }
    let today = local_date(now, tz);
    KINDS
        .iter()
        .filter_map(|k| occurrence_on(prefs, *k, today, tz))
        .filter(|o| o.expires_at > now)
        .collect()
}

/// Próximas ocurrencias (para mostrar en la app) en los siguientes `days` días locales, ordenadas.
pub fn upcoming(prefs: &ReminderPrefs, tz: Tz, now: DateTime<Utc>, days: u32) -> Vec<Occurrence> {
    let today = local_date(now, tz);
    let mut out: Vec<Occurrence> = (0..days as i64)
        .flat_map(|i| {
            let d = today + Duration::days(i);
            KINDS.iter().filter_map(move |k| occurrence_on(prefs, *k, d, tz))
        })
        .filter(|o| o.run_at > now && !prefs.paused && prefs.paused_until.is_none_or(|u| o.run_at >= u))
        .collect();
    out.sort_by_key(|o| o.run_at);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::parse_tz;
    use chrono::TimeZone;

    fn prefs() -> ReminderPrefs {
        ReminderPrefs {
            daily_enabled: true,
            daily_time: NaiveTime::from_hms_opt(21, 0, 0).unwrap(),
            weekly_enabled: true,
            weekly_weekday: 7,
            weekly_time: NaiveTime::from_hms_opt(19, 0, 0).unwrap(),
            monthly_enabled: true,
            monthly_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            paused: false,
            paused_until: None,
        }
    }

    fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap()
    }

    #[test]
    fn daily_at_local_21_in_monterrey() {
        let tz = parse_tz("America/Monterrey").unwrap();
        // 2026-11-12 12:00 local (18:00 UTC), jueves.
        let occ = due_occurrences(&prefs(), tz, utc(2026, 11, 12, 18, 0));
        assert_eq!(occ.len(), 1);
        assert_eq!(occ[0].key, "2026-11-12");
        assert_eq!(occ[0].run_at, utc(2026, 11, 13, 3, 0)); // 21:00 −06:00
    }

    #[test]
    fn local_evening_after_utc_midnight_keeps_local_date() {
        let tz = parse_tz("America/Monterrey").unwrap();
        // 03:30 UTC del 13 = 21:30 local del 12: aún dentro de la gracia del 12.
        let occ = due_occurrences(&prefs(), tz, utc(2026, 11, 13, 3, 30));
        assert_eq!(occ[0].key, "2026-11-12");
    }

    #[test]
    fn expired_occurrences_are_not_generated() {
        let tz = parse_tz("America/Monterrey").unwrap();
        // 01:30 local del 13 → la del 12 caducó a las 00:00 del 13 (21:00 + 3 h); la del 13 aún no toca pero se asegura.
        let occ = due_occurrences(&prefs(), tz, utc(2026, 11, 13, 7, 30));
        assert_eq!(
            occ.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
            vec!["2026-11-13"]
        );
        // Un servidor apagado varios días no produce recordatorios atrasados: solo el de hoy.
        assert!(
            occ.iter()
                .all(|o| o.local_date == NaiveDate::from_ymd_opt(2026, 11, 13).unwrap())
        );
    }

    #[test]
    fn dst_change_uses_local_time_not_24h_utc() {
        let tz = parse_tz("America/New_York").unwrap();
        let a = due_occurrences(&prefs(), tz, utc(2026, 3, 7, 15, 0))[0].run_at;
        let b = due_occurrences(&prefs(), tz, utc(2026, 3, 8, 15, 0))[0].run_at;
        assert_eq!(a, utc(2026, 3, 8, 2, 0)); // 21:00 EST
        assert_eq!(b, utc(2026, 3, 9, 1, 0)); // 21:00 EDT
        assert_eq!((b - a).num_hours(), 23);
    }

    #[test]
    fn nonexistent_local_time_moves_forward() {
        let tz = parse_tz("America/New_York").unwrap();
        let mut p = prefs();
        p.daily_time = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        let o = due_occurrences(&p, tz, utc(2026, 3, 8, 5, 0));
        assert_eq!(o[0].run_at, utc(2026, 3, 8, 7, 0)); // 03:00 EDT
    }

    #[test]
    fn timezone_change_moves_future_occurrences() {
        let now = utc(2026, 11, 12, 18, 0);
        let mty = due_occurrences(&prefs(), parse_tz("America/Monterrey").unwrap(), now)[0].clone();
        let mad = due_occurrences(&prefs(), parse_tz("Europe/Madrid").unwrap(), now)[0].clone();
        assert_eq!(mad.key, "2026-11-12");
        assert_eq!(mad.run_at, utc(2026, 11, 12, 20, 0));
        assert_ne!(mty.run_at, mad.run_at);
    }

    #[test]
    fn weekly_and_monthly_only_on_their_days() {
        let tz = parse_tz("America/Monterrey").unwrap();
        // Domingo 2026-11-15.
        let sunday = due_occurrences(&prefs(), tz, utc(2026, 11, 15, 18, 0));
        assert!(
            sunday
                .iter()
                .any(|o| o.kind == ReminderKind::Weekly && o.key == "2026-W46")
        );
        // 2026-12-01 martes.
        let first = due_occurrences(&prefs(), tz, utc(2026, 12, 1, 14, 0));
        assert!(
            first
                .iter()
                .any(|o| o.kind == ReminderKind::Monthly && o.key == "2026-12")
        );
        assert!(!first.iter().any(|o| o.kind == ReminderKind::Weekly));
    }

    #[test]
    fn pause_and_disabled() {
        let tz = parse_tz("America/Monterrey").unwrap();
        let now = utc(2026, 11, 12, 18, 0);
        let mut p = prefs();
        p.paused = true;
        assert!(due_occurrences(&p, tz, now).is_empty());
        p.paused = false;
        p.paused_until = Some(now + Duration::days(2));
        assert!(due_occurrences(&p, tz, now).is_empty());
        assert!(
            upcoming(&p, tz, now, 7)
                .iter()
                .all(|o| o.run_at >= now + Duration::days(2))
        );
        p.paused_until = Some(now - Duration::hours(1));
        assert_eq!(due_occurrences(&p, tz, now).len(), 1);
        p.daily_enabled = false;
        assert!(due_occurrences(&p, tz, now).is_empty());
    }

    #[test]
    fn upcoming_is_sorted_and_future() {
        let tz = parse_tz("America/Monterrey").unwrap();
        let now = utc(2026, 11, 12, 18, 0);
        let u = upcoming(&prefs(), tz, now, 7);
        assert!(u.windows(2).all(|w| w[0].run_at <= w[1].run_at));
        assert_eq!(u.iter().filter(|o| o.kind == ReminderKind::Daily).count(), 7);
        assert_eq!(u.iter().filter(|o| o.kind == ReminderKind::Weekly).count(), 1);
    }
}
