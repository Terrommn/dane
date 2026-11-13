//! Fechas civiles, zonas IANA, periodos ISO y reloj inyectable.
//!
//! Reglas (D-017, D-018):
//! * Los instantes se manejan en UTC; las fechas de actividad son `NaiveDate` locales del usuario.
//! * Semana ISO lunes–domingo; el año ISO puede diferir del año civil (ej. 2026-12-31 es 2026-W53).
//! * DST: una hora local inexistente se mueve a la primera hora válida posterior; una ambigua usa la más temprana.

use chrono::{
    DateTime, Datelike, Duration, IsoWeek, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc, Weekday,
};
use chrono_tz::Tz;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use crate::DomainError;

/// Fuente de tiempo inyectable: `SystemClock` en producción, `FixedClock` en pruebas.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Reloj controlable para pruebas.
#[derive(Debug, Clone)]
pub struct FixedClock(Arc<Mutex<DateTime<Utc>>>);

impl FixedClock {
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(at)))
    }
    pub fn set(&self, at: DateTime<Utc>) {
        *self.0.lock().expect("clock lock") = at;
    }
    pub fn advance(&self, d: Duration) {
        let mut g = self.0.lock().expect("clock lock");
        *g += d;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock lock")
    }
}

pub fn parse_tz(name: &str) -> Result<Tz, DomainError> {
    Tz::from_str(name).map_err(|_| DomainError::invalid("timezone", format!("zona IANA desconocida: {name}")))
}

/// Fecha local de un instante en una zona.
pub fn local_date(at: DateTime<Utc>, tz: Tz) -> NaiveDate {
    at.with_timezone(&tz).date_naive()
}

/// Convierte fecha+hora local a UTC aplicando la regla DST (D-018).
pub fn local_to_utc(date: NaiveDate, time: NaiveTime, tz: Tz) -> DateTime<Utc> {
    let naive = date.and_time(time);
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt.with_timezone(&Utc),
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        LocalResult::None => {
            // Hueco de DST: avanzar minuto a minuto hasta la primera hora válida (máx. 3 h).
            let mut probe = naive;
            for _ in 0..180 {
                probe += Duration::minutes(1);
                if let LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) =
                    tz.from_local_datetime(&probe)
                {
                    return dt.with_timezone(&Utc);
                }
            }
            Utc.from_utc_datetime(&naive)
        }
    }
}

/// Inicio (00:00 local) de una fecha en UTC.
pub fn start_of_local_day(date: NaiveDate, tz: Tz) -> DateTime<Utc> {
    local_to_utc(date, NaiveTime::MIN, tz)
}

/// Clave ISO `GGGG-Www` de una fecha.
pub fn iso_week_key(date: NaiveDate) -> String {
    let w: IsoWeek = date.iso_week();
    format!("{:04}-W{:02}", w.year(), w.week())
}

/// Clave de mes `YYYY-MM`.
pub fn month_key(date: NaiveDate) -> String {
    format!("{:04}-{:02}", date.year(), date.month())
}

/// Periodo civil con límites inclusivos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl Period {
    pub fn new(start: NaiveDate, end: NaiveDate) -> Result<Self, DomainError> {
        if end < start {
            return Err(DomainError::invalid("period", "fin antes del inicio"));
        }
        Ok(Self { start, end })
    }
    pub fn contains(&self, d: NaiveDate) -> bool {
        d >= self.start && d <= self.end
    }
    pub fn days(&self) -> i64 {
        (self.end - self.start).num_days() + 1
    }
    pub fn iter_days(&self) -> impl Iterator<Item = NaiveDate> + '_ {
        self.start.iter_days().take_while(move |d| *d <= self.end)
    }
    /// Intersección con otro periodo, si existe.
    pub fn intersect(&self, other: &Period) -> Option<Period> {
        let s = self.start.max(other.start);
        let e = self.end.min(other.end);
        (s <= e).then_some(Period { start: s, end: e })
    }
}

pub fn iso_week_period(date: NaiveDate) -> Period {
    let start = date - Duration::days(date.weekday().num_days_from_monday() as i64);
    Period {
        start,
        end: start + Duration::days(6),
    }
}

/// Parsea `GGGG-Www` a su periodo lunes–domingo.
pub fn parse_iso_week(key: &str) -> Result<Period, DomainError> {
    let err = || DomainError::invalid("iso_week", format!("semana ISO inválida: {key}"));
    let (y, w) = key.split_once("-W").ok_or_else(err)?;
    let year: i32 = y.parse().map_err(|_| err())?;
    let week: u32 = w.parse().map_err(|_| err())?;
    let monday = NaiveDate::from_isoywd_opt(year, week, Weekday::Mon).ok_or_else(err)?;
    Ok(Period {
        start: monday,
        end: monday + Duration::days(6),
    })
}

pub fn month_period(date: NaiveDate) -> Period {
    let start = date.with_day(1).expect("day 1");
    Period {
        start,
        end: last_day_of_month(date.year(), date.month()),
    }
}

/// Parsea `YYYY-MM` a su periodo.
pub fn parse_month(key: &str) -> Result<Period, DomainError> {
    let err = || DomainError::invalid("month", format!("mes inválido: {key}"));
    let (y, m) = key.split_once('-').ok_or_else(err)?;
    let year: i32 = y.parse().map_err(|_| err())?;
    let month: u32 = m.parse().map_err(|_| err())?;
    let start = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(err)?;
    Ok(month_period(start))
}

pub fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(ny, nm, 1).expect("valid") - Duration::days(1)
}

pub fn previous_month_key(key: &str) -> Result<String, DomainError> {
    let p = parse_month(key)?;
    Ok(month_key(p.start - Duration::days(1)))
}

/// Día del mes de una regla recurrente: número, último o penúltimo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "day", rename_all = "snake_case")]
pub enum DayOfMonth {
    Day(u32),
    Last,
    Penultimate,
}

impl DayOfMonth {
    pub fn from_parts(kind: &str, day: Option<i16>) -> Result<Self, DomainError> {
        match (kind, day) {
            ("day", Some(d)) if (1..=31).contains(&d) => Ok(Self::Day(d as u32)),
            ("last", None) => Ok(Self::Last),
            ("penultimate", None) => Ok(Self::Penultimate),
            _ => Err(DomainError::invalid("day_kind", "regla de día inválida")),
        }
    }
    pub fn parts(&self) -> (&'static str, Option<i16>) {
        match self {
            Self::Day(d) => ("day", Some(*d as i16)),
            Self::Last => ("last", None),
            Self::Penultimate => ("penultimate", None),
        }
    }
    /// Fecha concreta en un mes. Un día inexistente (31 en abril, 30 en febrero) se ajusta al último día.
    pub fn resolve(&self, year: i32, month: u32) -> NaiveDate {
        let last = last_day_of_month(year, month);
        match self {
            Self::Day(d) => NaiveDate::from_ymd_opt(year, month, *d).unwrap_or(last),
            Self::Last => last,
            Self::Penultimate => last - Duration::days(1),
        }
    }
}

/// Meses `YYYY-MM` desde `from` hasta `to` inclusive (para catch-up de recurrencias).
pub fn months_between(from: NaiveDate, to: NaiveDate) -> Vec<(i32, u32)> {
    let mut out = Vec::new();
    let (mut y, mut m) = (from.year(), from.month());
    while (y, m) <= (to.year(), to.month()) {
        out.push((y, m));
        if m == 12 {
            y += 1;
            m = 1;
        } else {
            m += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn iso_week_crosses_calendar_year() {
        assert_eq!(iso_week_key(d(2026, 12, 31)), "2026-W53");
        assert_eq!(iso_week_key(d(2027, 1, 3)), "2026-W53");
        assert_eq!(iso_week_key(d(2027, 1, 4)), "2027-W01");
        assert_eq!(iso_week_key(d(2024, 12, 30)), "2025-W01");
        let p = parse_iso_week("2025-W01").unwrap();
        assert_eq!(p.start, d(2024, 12, 30));
        assert_eq!(p.end, d(2025, 1, 5));
    }

    #[test]
    fn iso_week_crossing_month_has_7_days() {
        let p = iso_week_period(d(2026, 11, 30));
        assert_eq!(p.start, d(2026, 11, 30));
        assert_eq!(p.end, d(2026, 12, 6));
        assert_eq!(p.days(), 7);
        assert!(parse_iso_week("2026-W54").is_err());
    }

    #[test]
    fn local_midnight_boundary_in_monterrey() {
        let tz = parse_tz("America/Monterrey").unwrap();
        // 05:59 UTC del 13 = 23:59 del 12 en Monterrey (UTC−6 todo el año desde 2022).
        let at = Utc.with_ymd_and_hms(2026, 11, 13, 5, 59, 0).unwrap();
        assert_eq!(local_date(at, tz), d(2026, 11, 12));
        let at2 = at + Duration::minutes(1);
        assert_eq!(local_date(at2, tz), d(2026, 11, 13));
    }

    #[test]
    fn dst_gap_moves_forward_and_ambiguous_uses_earliest() {
        let ny = parse_tz("America/New_York").unwrap();
        // 2026-03-08 02:30 no existe en Nueva York → 03:00 EDT = 07:00 UTC.
        let gap = local_to_utc(d(2026, 3, 8), NaiveTime::from_hms_opt(2, 30, 0).unwrap(), ny);
        assert_eq!(gap, Utc.with_ymd_and_hms(2026, 3, 8, 7, 0, 0).unwrap());
        // 2026-11-01 01:30 ocurre dos veces → la primera (EDT, UTC−4) = 05:30 UTC.
        let amb = local_to_utc(d(2026, 11, 1), NaiveTime::from_hms_opt(1, 30, 0).unwrap(), ny);
        assert_eq!(amb, Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0).unwrap());
    }

    #[test]
    fn same_local_time_across_dst_is_not_24h_utc() {
        let ny = parse_tz("America/New_York").unwrap();
        let t = NaiveTime::from_hms_opt(21, 0, 0).unwrap();
        let before = local_to_utc(d(2026, 3, 7), t, ny);
        let after = local_to_utc(d(2026, 3, 8), t, ny);
        assert_eq!((after - before).num_hours(), 23);
    }

    #[test]
    fn timezone_change_changes_local_date_of_same_instant() {
        let at = Utc.with_ymd_and_hms(2026, 11, 13, 3, 0, 0).unwrap();
        assert_eq!(
            local_date(at, parse_tz("America/Monterrey").unwrap()),
            d(2026, 11, 12)
        );
        assert_eq!(
            local_date(at, parse_tz("Europe/Madrid").unwrap()),
            d(2026, 11, 13)
        );
        assert!(parse_tz("Mars/Olympus").is_err());
    }

    #[test]
    fn day_of_month_rules_handle_short_months() {
        assert_eq!(DayOfMonth::Day(31).resolve(2026, 4), d(2026, 4, 30));
        assert_eq!(DayOfMonth::Day(29).resolve(2026, 2), d(2026, 2, 28));
        assert_eq!(DayOfMonth::Day(29).resolve(2028, 2), d(2028, 2, 29));
        assert_eq!(DayOfMonth::Day(30).resolve(2026, 2), d(2026, 2, 28));
        assert_eq!(DayOfMonth::Last.resolve(2026, 2), d(2026, 2, 28));
        assert_eq!(DayOfMonth::Penultimate.resolve(2026, 2), d(2026, 2, 27));
        assert_eq!(DayOfMonth::Penultimate.resolve(2028, 2), d(2028, 2, 28));
        assert_eq!(DayOfMonth::Penultimate.resolve(2026, 12), d(2026, 12, 30));
        assert!(DayOfMonth::from_parts("day", Some(32)).is_err());
        assert!(DayOfMonth::from_parts("last", Some(3)).is_err());
    }

    #[test]
    fn months_between_and_previous_month() {
        assert_eq!(
            months_between(d(2026, 11, 20), d(2027, 2, 1)),
            vec![(2026, 11), (2026, 12), (2027, 1), (2027, 2)]
        );
        assert_eq!(previous_month_key("2027-01").unwrap(), "2026-12");
        assert_eq!(parse_month("2026-02").unwrap().end, d(2026, 2, 28));
    }

    #[test]
    fn fixed_clock_is_controllable() {
        let c = FixedClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
        c.advance(Duration::hours(25));
        assert_eq!(c.now(), Utc.with_ymd_and_hms(2026, 1, 2, 1, 0, 0).unwrap());
    }
}
