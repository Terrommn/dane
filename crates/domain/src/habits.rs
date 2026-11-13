//! Hábitos: identidad estable, versiones con vigencia, frecuencias y pausas (D-010, C9).
//!
//! * Una versión rige `[valid_from, valid_to]` (inclusive; `valid_to = None` = abierta). Editar la frecuencia crea una
//!   versión nueva desde una fecha: el pasado se sigue evaluando con la versión que regía entonces.
//! * Frecuencias: `Daily` (todos los días), `Weekdays(mask)` (días ISO seleccionados, lunes = bit 0) y
//!   `TimesPerWeek(n)` (n veces en la semana ISO, cualquier día; gym 4/semana NO significa gym obligatorio cada día).
//! * Una pausa saca los días del denominador.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frequency {
    Daily,
    /// Bitmask ISO: lunes = 1, martes = 2, …, domingo = 64.
    Weekdays(u8),
    TimesPerWeek(u8),
}

impl Frequency {
    pub fn from_parts(kind: &str, weekdays: Option<i16>, times: Option<i16>) -> Result<Self, DomainError> {
        match (kind, weekdays, times) {
            ("daily", None, None) => Ok(Self::Daily),
            ("weekdays", Some(m), None) if (1..=127).contains(&m) => Ok(Self::Weekdays(m as u8)),
            ("times_per_week", None, Some(n)) if (1..=7).contains(&n) => Ok(Self::TimesPerWeek(n as u8)),
            _ => Err(DomainError::invalid(
                "frequency",
                "frecuencia inválida (daily, weekdays con días, times_per_week con 1–7)",
            )),
        }
    }
    /// `(frequency, weekdays, times_per_week)` para persistir.
    pub fn parts(&self) -> (&'static str, Option<i16>, Option<i16>) {
        match self {
            Self::Daily => ("daily", None, None),
            Self::Weekdays(m) => ("weekdays", Some(*m as i16), None),
            Self::TimesPerWeek(n) => ("times_per_week", None, Some(*n as i16)),
        }
    }
    pub fn is_weekly(&self) -> bool {
        matches!(self, Self::TimesPerWeek(_))
    }
    /// ¿Es exigible ese día concreto? Para `TimesPerWeek` ningún día individual es obligatorio.
    pub fn requires_day(&self, d: NaiveDate) -> bool {
        match self {
            Self::Daily => true,
            Self::Weekdays(m) => m & weekday_bit(d) != 0,
            Self::TimesPerWeek(_) => false,
        }
    }
}

pub fn weekday_bit(d: NaiveDate) -> u8 {
    1 << d.weekday().num_days_from_monday()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HabitVersion {
    pub valid_from: NaiveDate,
    pub valid_to: Option<NaiveDate>,
    pub frequency: Frequency,
}

impl HabitVersion {
    pub fn covers(&self, d: NaiveDate) -> bool {
        d >= self.valid_from && self.valid_to.is_none_or(|t| d <= t)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pause {
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
}

impl Pause {
    pub fn covers(&self, d: NaiveDate) -> bool {
        d >= self.starts_on && self.ends_on.is_none_or(|e| d <= e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    Done,
    NotDone,
    NotApplicable,
}

impl EntryStatus {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "done" => Ok(Self::Done),
            "not_done" => Ok(Self::NotDone),
            "not_applicable" => Ok(Self::NotApplicable),
            _ => Err(DomainError::invalid(
                "status",
                "estado inválido (done, not_done, not_applicable)",
            )),
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::NotDone => "not_done",
            Self::NotApplicable => "not_applicable",
        }
    }
}

/// Lo necesario para evaluar un hábito en el tiempo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HabitSpec {
    /// Primer día en que existe el hábito (no se exige antes).
    pub created_on: NaiveDate,
    /// Día de archivo: desde el día siguiente deja de exigirse.
    pub archived_on: Option<NaiveDate>,
    pub versions: Vec<HabitVersion>,
    pub pauses: Vec<Pause>,
}

impl HabitSpec {
    pub fn version_at(&self, d: NaiveDate) -> Option<&HabitVersion> {
        if d < self.created_on || self.archived_on.is_some_and(|a| d > a) {
            return None;
        }
        self.versions
            .iter()
            .filter(|v| v.covers(d))
            .max_by_key(|v| v.valid_from)
    }
    pub fn is_paused(&self, d: NaiveDate) -> bool {
        self.pauses.iter().any(|p| p.covers(d))
    }
    /// Activo (existe, tiene versión y no está en pausa) ese día.
    pub fn active_on(&self, d: NaiveDate) -> bool {
        self.version_at(d).is_some() && !self.is_paused(d)
    }
    /// Debe aparecer en la pantalla del día: diario/días seleccionados exigibles, o semanal activo.
    pub fn shown_on(&self, d: NaiveDate) -> bool {
        match self.version_at(d) {
            Some(v) if !self.is_paused(d) => v.frequency.is_weekly() || v.frequency.requires_day(d),
            _ => false,
        }
    }
}

/// Valida una nueva versión respecto a las existentes: no puede empezar antes de la creación ni duplicar inicio.
pub fn validate_new_version(spec: &HabitSpec, valid_from: NaiveDate) -> Result<(), DomainError> {
    if valid_from < spec.created_on {
        return Err(DomainError::invalid(
            "valid_from",
            "no puede ser anterior a la creación del hábito",
        ));
    }
    Ok(())
}

/// Valida una entrada: `not_applicable` exige motivo (justifica la exclusión del denominador).
pub fn validate_entry(
    status: EntryStatus,
    reason: Option<&str>,
    quantity: Option<rust_decimal::Decimal>,
) -> Result<(), DomainError> {
    if status == EntryStatus::NotApplicable && reason.is_none_or(|r| r.trim().is_empty()) {
        return Err(DomainError::invalid("reason", "explica por qué no aplica"));
    }
    if quantity.is_some_and(|q| q.is_sign_negative()) {
        return Err(DomainError::invalid("quantity", "no puede ser negativa"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn frequency_parsing_and_requirements() {
        assert_eq!(
            Frequency::from_parts("daily", None, None).unwrap(),
            Frequency::Daily
        );
        assert!(Frequency::from_parts("weekdays", None, None).is_err());
        assert!(Frequency::from_parts("times_per_week", None, Some(8)).is_err());
        // Lunes y miércoles = 1 | 4.
        let f = Frequency::from_parts("weekdays", Some(5), None).unwrap();
        assert!(f.requires_day(d(2026, 11, 9))); // lunes
        assert!(!f.requires_day(d(2026, 11, 10))); // martes
        assert!(f.requires_day(d(2026, 11, 11))); // miércoles
        assert!(!Frequency::TimesPerWeek(4).requires_day(d(2026, 11, 9)));
    }

    #[test]
    fn versions_and_pauses_apply_by_date() {
        let spec = HabitSpec {
            created_on: d(2026, 11, 1),
            archived_on: None,
            versions: vec![
                HabitVersion {
                    valid_from: d(2026, 11, 1),
                    valid_to: Some(d(2026, 11, 15)),
                    frequency: Frequency::Daily,
                },
                HabitVersion {
                    valid_from: d(2026, 11, 16),
                    valid_to: None,
                    frequency: Frequency::TimesPerWeek(3),
                },
            ],
            pauses: vec![Pause {
                starts_on: d(2026, 11, 5),
                ends_on: Some(d(2026, 11, 6)),
            }],
        };
        assert!(spec.version_at(d(2026, 10, 31)).is_none());
        assert_eq!(
            spec.version_at(d(2026, 11, 10)).unwrap().frequency,
            Frequency::Daily
        );
        assert_eq!(
            spec.version_at(d(2026, 11, 20)).unwrap().frequency,
            Frequency::TimesPerWeek(3)
        );
        assert!(!spec.active_on(d(2026, 11, 5)));
        assert!(spec.shown_on(d(2026, 11, 7)));
        assert!(validate_new_version(&spec, d(2026, 10, 1)).is_err());
    }

    #[test]
    fn not_applicable_requires_reason() {
        assert!(validate_entry(EntryStatus::NotApplicable, None, None).is_err());
        assert!(validate_entry(EntryStatus::NotApplicable, Some("  "), None).is_err());
        assert!(validate_entry(EntryStatus::NotApplicable, Some("enfermo"), None).is_ok());
        assert!(validate_entry(EntryStatus::Done, None, Some(rust_decimal::Decimal::NEGATIVE_ONE)).is_err());
    }
}
