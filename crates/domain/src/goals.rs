//! Objetivos: estados, cierre pendiente, progreso cuantitativo, ritmo y avisos de carga (T17, T18, T22, C12).
//!
//! ## Progreso
//! * **Acumulativo** (`cumulative`): `total = baseline + Σ amount`. `ratio = total / target`. Ritmo esperado lineal sobre el
//!   calendario del periodo: `esperado = target × días_transcurridos / días_totales` (días transcurridos incluye hoy y se
//!   limita al periodo). **Proyección** (estimación, etiquetada así): `total / transcurridos × totales`, solo si hay datos.
//! * **Valor actual** (`current_value`): última medición (o baseline si no hay). Avance `(actual − baseline) / (target − baseline)`
//!   si baseline existe y difiere del target. **Sin proyección lineal** (no tiene sentido para peso o saldos).
//! * **Hito** (`milestone`): logrado si algún evento marca `completed`.
//! * Target 0: `ratio = None` y `target_zero = true` (no se divide entre cero). Sobrecumplimiento: `overachieved`.
//!   Sin eventos: `has_data = false` (no es 0 % "real").
//!
//! El progreso del resultado es independiente de la adherencia a hábitos: registrar un hábito no suma unidades al
//! objetivo salvo que su fuente esté configurada explícitamente con la **misma unidad**.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::DomainError;
use crate::time::Period;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressType {
    None,
    Cumulative,
    CurrentValue,
    Milestone,
}

impl ProgressType {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        Ok(match s {
            "none" => Self::None,
            "cumulative" => Self::Cumulative,
            "current_value" => Self::CurrentValue,
            "milestone" => Self::Milestone,
            _ => return Err(DomainError::invalid("progress_type", "tipo de progreso inválido")),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Increase,
    Decrease,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgressEvent {
    pub on: NaiveDate,
    pub amount: Option<Decimal>,
    pub value: Option<Decimal>,
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalProgressInput {
    pub progress_type: ProgressType,
    pub baseline: Option<Decimal>,
    pub target: Option<Decimal>,
    pub direction: Option<Direction>,
    pub period: Period,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GoalProgress {
    pub has_data: bool,
    /// Acumulado (acumulativo) o último valor (valor actual).
    pub current: Option<f64>,
    pub target: Option<f64>,
    pub baseline: Option<f64>,
    /// Avance 0..∞ (1 = meta). `None` sin datos o target 0.
    pub ratio: Option<f64>,
    pub remaining: Option<f64>,
    pub achieved: bool,
    pub overachieved: bool,
    pub target_zero: bool,
    /// Solo acumulativos: lo que "debería" llevarse hoy según el calendario.
    pub expected_by_today: Option<f64>,
    /// Solo acumulativos: `current − expected_by_today` (positivo = adelantado).
    pub pace_delta: Option<f64>,
    /// Solo acumulativos con datos: estimación lineal al cierre del periodo. Es una **estimación**.
    pub projection_estimate: Option<f64>,
    pub last_measured_on: Option<NaiveDate>,
    pub elapsed_days: i64,
    pub total_days: i64,
}

fn f(d: Decimal) -> f64 {
    d.to_f64().unwrap_or(0.0)
}

pub fn progress(input: &GoalProgressInput, events: &[ProgressEvent], today: NaiveDate) -> GoalProgress {
    let total_days = input.period.days();
    let elapsed_days = if today < input.period.start {
        0
    } else {
        ((today.min(input.period.end) - input.period.start).num_days() + 1).min(total_days)
    };
    let mut p = GoalProgress {
        target: input.target.map(f),
        baseline: input.baseline.map(f),
        target_zero: input.target.is_some_and(|t| t.is_zero()),
        elapsed_days,
        total_days,
        ..Default::default()
    };
    match input.progress_type {
        ProgressType::None => {}
        ProgressType::Milestone => {
            let done = events
                .iter()
                .filter(|e| e.completed == Some(true))
                .max_by_key(|e| e.on);
            p.has_data = !events.is_empty();
            p.achieved = done.is_some();
            p.ratio = Some(if p.achieved { 1.0 } else { 0.0 });
            p.last_measured_on = done.map(|e| e.on);
        }
        ProgressType::Cumulative => {
            let sum: Decimal = events.iter().filter_map(|e| e.amount).sum();
            let total = input.baseline.unwrap_or_default() + sum;
            p.has_data = events.iter().any(|e| e.amount.is_some());
            p.current = Some(f(total));
            p.last_measured_on = events.iter().filter(|e| e.amount.is_some()).map(|e| e.on).max();
            if let Some(t) = input.target {
                if !t.is_zero() {
                    p.ratio = Some(f(total) / f(t));
                    p.expected_by_today = Some(f(t) * elapsed_days as f64 / total_days as f64);
                    p.pace_delta = p.expected_by_today.map(|e| f(total) - e);
                }
                p.remaining = Some(f((t - total).max(Decimal::ZERO)));
                p.achieved = total >= t;
                p.overachieved = total > t;
                if p.has_data && elapsed_days > 0 {
                    p.projection_estimate = Some(
                        f(sum) / elapsed_days as f64 * total_days as f64
                            + f(input.baseline.unwrap_or_default()),
                    );
                }
            }
        }
        ProgressType::CurrentValue => {
            let last = events.iter().filter(|e| e.value.is_some()).max_by_key(|e| e.on);
            p.has_data = last.is_some();
            let current = last.and_then(|e| e.value).or(input.baseline);
            p.current = current.map(f);
            p.last_measured_on = last.map(|e| e.on);
            if let (Some(c), Some(t)) = (current, input.target) {
                let dir = input
                    .direction
                    .unwrap_or(if input.baseline.is_some_and(|b| b > t) {
                        Direction::Decrease
                    } else {
                        Direction::Increase
                    });
                p.achieved = match dir {
                    Direction::Increase => c >= t,
                    Direction::Decrease => c <= t,
                };
                p.overachieved = match dir {
                    Direction::Increase => c > t,
                    Direction::Decrease => c < t,
                };
                p.remaining = Some(if p.achieved { 0.0 } else { f((t - c).abs()) });
                if let Some(b) = input.baseline.filter(|b| *b != t) {
                    p.ratio = Some(f(c - b) / f(t - b));
                }
            }
        }
    }
    p
}

/// "Cierre pendiente": el periodo terminó (antes de hoy) y el objetivo sigue activo. Es distinto de archivar.
pub fn closure_pending(status: &str, ends_on: NaiveDate, today: NaiveDate) -> bool {
    status == "active" && ends_on < today
}

/// Avisos de carga de hábitos (referencias del vault: ~6 mensuales, ~8 combinados, 1–3 por objetivo). Son avisos
/// editables, no restricciones.
pub fn habit_load_warnings(
    month_habits: u32,
    week_habits: u32,
    per_goal: &[(String, u32)],
    warn_month: u32,
    warn_total: u32,
) -> Vec<String> {
    let mut w = Vec::new();
    if month_habits > warn_month {
        w.push(format!(
            "Tienes {month_habits} hábitos diarios del mes; la referencia realista es ~{warn_month}."
        ));
    }
    if month_habits + week_habits > warn_total {
        w.push(format!(
            "Mes + semana suman {} hábitos al día; la referencia es ~{warn_total}.",
            month_habits + week_habits
        ));
    }
    for (goal, n) in per_goal {
        if *n > 3 {
            w.push(format!(
                "“{goal}” tiene {n} hábitos; se recomiendan de 1 a 3 por objetivo."
            ));
        }
        if *n == 0 {
            w.push(format!("“{goal}” no tiene hábitos diarios todavía."));
        }
    }
    w
}

/// Valida estado y coherencia cuantitativa al crear/editar.
pub fn validate_goal(
    status: &str,
    ptype: ProgressType,
    unit: Option<&str>,
    target: Option<Decimal>,
) -> Result<(), DomainError> {
    if !["active", "achieved", "not_achieved"].contains(&status) {
        return Err(DomainError::invalid(
            "status",
            "estado inválido (active, achieved, not_achieved)",
        ));
    }
    if matches!(ptype, ProgressType::Cumulative | ProgressType::CurrentValue) {
        if unit.is_none_or(|u| u.trim().is_empty()) {
            return Err(DomainError::invalid(
                "unit",
                "indica la unidad (libros, sesiones, kg, MXN…)",
            ));
        }
        if target.is_none() {
            return Err(DomainError::invalid("target", "indica la meta numérica"));
        }
    }
    if target.is_some_and(|t| t.is_sign_negative()) {
        return Err(DomainError::invalid("target", "no puede ser negativa"));
    }
    Ok(())
}

/// Unidades compatibles (comparación normalizada). Un hábito en "min" no alimenta un objetivo en "libros".
pub fn units_compatible(a: &str, b: &str) -> bool {
    crate::text::fold(a.trim()) == crate::text::fold(b.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }
    fn nov() -> Period {
        Period {
            start: d(2026, 11, 1),
            end: d(2026, 11, 30),
        }
    }
    fn amt(day: u32, a: &str) -> ProgressEvent {
        ProgressEvent {
            on: d(2026, 11, day),
            amount: Some(dec(a)),
            value: None,
            completed: None,
        }
    }

    #[test]
    fn cumulative_pace_and_projection() {
        let i = GoalProgressInput {
            progress_type: ProgressType::Cumulative,
            baseline: None,
            target: Some(dec("4")),
            direction: None,
            period: nov(),
        };
        let p = progress(&i, &[amt(5, "1"), amt(12, "1")], d(2026, 11, 15));
        assert_eq!(p.current, Some(2.0));
        assert_eq!(p.ratio, Some(0.5));
        assert_eq!(p.remaining, Some(2.0));
        assert!((p.expected_by_today.unwrap() - 2.0).abs() < 1e-9);
        assert!((p.projection_estimate.unwrap() - 4.0).abs() < 1e-9);
        assert!(!p.achieved);
    }

    #[test]
    fn cumulative_without_data_and_overachievement() {
        let i = GoalProgressInput {
            progress_type: ProgressType::Cumulative,
            baseline: None,
            target: Some(dec("2")),
            direction: None,
            period: nov(),
        };
        let p = progress(&i, &[], d(2026, 11, 10));
        assert!(!p.has_data);
        assert_eq!(p.projection_estimate, None);
        let p = progress(&i, &[amt(1, "1"), amt(2, "1"), amt(3, "1")], d(2026, 11, 10));
        assert!(p.achieved && p.overachieved);
        assert_eq!(p.remaining, Some(0.0));
    }

    #[test]
    fn target_zero_is_explicit() {
        let i = GoalProgressInput {
            progress_type: ProgressType::Cumulative,
            baseline: None,
            target: Some(Decimal::ZERO),
            direction: None,
            period: nov(),
        };
        let p = progress(&i, &[], d(2026, 11, 10));
        assert!(p.target_zero);
        assert_eq!(p.ratio, None);
        assert!(p.achieved);
    }

    #[test]
    fn current_value_weight_has_no_linear_projection() {
        let i = GoalProgressInput {
            progress_type: ProgressType::CurrentValue,
            baseline: Some(dec("80")),
            target: Some(dec("75")),
            direction: Some(Direction::Decrease),
            period: nov(),
        };
        let ev = vec![
            ProgressEvent {
                on: d(2026, 11, 3),
                amount: None,
                value: Some(dec("79")),
                completed: None,
            },
            ProgressEvent {
                on: d(2026, 11, 10),
                amount: None,
                value: Some(dec("78")),
                completed: None,
            },
        ];
        let p = progress(&i, &ev, d(2026, 11, 12));
        assert_eq!(p.current, Some(78.0));
        assert_eq!(p.ratio, Some(0.4));
        assert_eq!(p.remaining, Some(3.0));
        assert_eq!(p.projection_estimate, None);
        assert_eq!(p.expected_by_today, None);
        assert_eq!(p.last_measured_on, Some(d(2026, 11, 10)));
    }

    #[test]
    fn milestone() {
        let i = GoalProgressInput {
            progress_type: ProgressType::Milestone,
            baseline: None,
            target: None,
            direction: None,
            period: nov(),
        };
        assert!(!progress(&i, &[], d(2026, 11, 10)).achieved);
        let p = progress(
            &i,
            &[ProgressEvent {
                on: d(2026, 11, 9),
                amount: None,
                value: None,
                completed: Some(true),
            }],
            d(2026, 11, 10),
        );
        assert!(p.achieved);
    }

    #[test]
    fn closure_and_warnings_and_units() {
        assert!(closure_pending("active", d(2026, 11, 8), d(2026, 11, 9)));
        assert!(!closure_pending("active", d(2026, 11, 9), d(2026, 11, 9)));
        assert!(!closure_pending("achieved", d(2026, 11, 1), d(2026, 11, 9)));
        let w = habit_load_warnings(7, 2, &[("Leer".into(), 4)], 6, 8);
        assert_eq!(w.len(), 3);
        assert!(habit_load_warnings(3, 2, &[("A".into(), 2)], 6, 8).is_empty());
        assert!(units_compatible("Minutos", "minutos"));
        assert!(!units_compatible("min", "libros"));
        assert!(validate_goal("active", ProgressType::Cumulative, None, Some(dec("4"))).is_err());
        assert!(validate_goal("done", ProgressType::None, None, None).is_err());
    }
}
