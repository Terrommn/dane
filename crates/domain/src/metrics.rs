//! Métricas de hábitos: cumplimiento, rachas y comparaciones temporales (T22, C1, C2, C13).
//!
//! ## Reglas de cumplimiento
//! * **Cumplimiento = ocurrencias cumplidas / ocurrencias exigibles** (solo periodos **cerrados**).
//! * Un día está **cerrado** si es anterior a `today` (fecha local del usuario). El día de hoy está **abierto**: no
//!   penaliza y se reporta aparte como "avance de hoy". Los días futuros se ignoran.
//! * Hábito diario / días seleccionados: cada día exigible (versión vigente, día programado, sin pausa) es una
//!   ocurrencia. `done` cumple; `not_done` explícito es falla (`missed`); **sin registro** también es falla exigible
//!   pero se cuenta aparte (`unrecorded`) para distinguirla visualmente; `not_applicable` (con motivo) sale del
//!   denominador (`excluded_not_applicable`). Los días en pausa salen del denominador (`paused_days`).
//! * Hábito N por semana: cada semana ISO cerrada aporta `exigidas = round(N × días_elegibles / 7)` ocurrencias
//!   (redondeo half-up; semana completa = N). Días elegibles = días de la semana dentro del periodo evaluado, con versión
//!   semanal vigente, sin pausa y sin `not_applicable`. Cumplidas = `min(done, exigidas)`; el excedente se reporta en
//!   `extra` (sobrecumplimiento) sin inflar el porcentaje. La semana en curso no cuenta; se muestra en `TodayProgress`.
//! * Cambio de frecuencia: cada día se evalúa con la versión vigente ese día (el pasado no cambia).
//! * `rate = None` cuando no hay ocurrencias exigibles (sin datos ≠ 0 %).
//!
//! ## Rachas
//! Secuencia cronológica de ocurrencias cerradas (días exigibles y semanas cerradas, según la versión de cada momento).
//! Éxito = día `done` / semana que alcanza sus exigidas. `not_applicable` y pausas no rompen ni suman. Hoy cuenta solo si
//! ya está cumplido (y la semana en curso solo si ya alcanzó N): el día abierto nunca rompe la racha.
//! Racha actual = éxitos consecutivos al final de la secuencia; máxima = la mayor corrida.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::habits::{EntryStatus, Frequency, HabitSpec};
use crate::time::{Period, iso_week_period, month_period};

pub type Entries = BTreeMap<NaiveDate, EntryStatus>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Compliance {
    pub required: u32,
    pub done: u32,
    /// `not_done` explícito.
    pub missed: u32,
    /// Exigible y sin registro (día cerrado): cuenta como no cumplido pero se distingue.
    pub unrecorded: u32,
    pub excluded_not_applicable: u32,
    pub paused_days: u32,
    /// Sobrecumplimiento en hábitos semanales (no infla `rate`).
    pub extra: u32,
    /// `done / required`; `None` si no hubo ocurrencias exigibles.
    pub rate: Option<f64>,
}

impl Compliance {
    fn finish(mut self) -> Self {
        self.rate = (self.required > 0).then(|| self.done as f64 / self.required as f64);
        self
    }
    /// Suma de varios hábitos.
    pub fn merge(items: &[Compliance]) -> Compliance {
        let mut c = Compliance::default();
        for i in items {
            c.required += i.required;
            c.done += i.done;
            c.missed += i.missed;
            c.unrecorded += i.unrecorded;
            c.excluded_not_applicable += i.excluded_not_applicable;
            c.paused_days += i.paused_days;
            c.extra += i.extra;
        }
        c.finish()
    }
}

/// Avance del día abierto (y de la semana en curso para hábitos semanales).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TodayProgress {
    pub date: NaiveDate,
    /// Aparece en la pantalla del día.
    pub shown: bool,
    /// Exigible ese día (diario/días seleccionados).
    pub required_today: bool,
    pub status: Option<EntryStatus>,
    /// Hábito semanal: cumplidas en la semana en curso hasta hoy (incluido) y meta N.
    pub week_done: Option<u32>,
    pub week_target: Option<u32>,
}

fn required_for_partial_week(n: u8, eligible_days: u32) -> u32 {
    ((n as u32 * eligible_days * 2) + 7) / 14
}

/// Cumplimiento de un hábito en `period`, considerando solo lo cerrado antes de `today`.
pub fn compliance(spec: &HabitSpec, entries: &Entries, period: Period, today: NaiveDate) -> Compliance {
    let mut c = Compliance::default();
    // Días diarios / seleccionados.
    for d in period.iter_days() {
        if d >= today {
            break;
        }
        let Some(v) = spec.version_at(d) else { continue };
        if v.frequency.is_weekly() || !v.frequency.requires_day(d) {
            continue;
        }
        if spec.is_paused(d) {
            c.paused_days += 1;
            continue;
        }
        match entries.get(&d) {
            Some(EntryStatus::NotApplicable) => c.excluded_not_applicable += 1,
            Some(EntryStatus::Done) => {
                c.required += 1;
                c.done += 1
            }
            Some(EntryStatus::NotDone) => {
                c.required += 1;
                c.missed += 1
            }
            None => {
                c.required += 1;
                c.unrecorded += 1
            }
        }
    }
    // Semanas ISO cerradas para versiones N/semana.
    let mut week_start = iso_week_period(period.start).start;
    while week_start <= period.end {
        let week = Period {
            start: week_start,
            end: week_start + Duration::days(6),
        };
        week_start += Duration::days(7);
        if week.end >= today {
            break; // semana abierta o futura
        }
        if let Some(w) = weekly_week(spec, entries, week, period) {
            c.required += w.required;
            c.done += w.done.min(w.required);
            c.extra += w.done.saturating_sub(w.required);
            c.unrecorded += w.unrecorded;
            c.excluded_not_applicable += w.na;
            c.paused_days += w.paused;
            c.missed += w.required.saturating_sub(w.done);
        }
    }
    c.finish()
}

struct WeekEval {
    required: u32,
    done: u32,
    unrecorded: u32,
    na: u32,
    paused: u32,
    target: u8,
}

/// Evalúa una semana ISO para la parte con versión semanal, recortada a `within`.
fn weekly_week(spec: &HabitSpec, entries: &Entries, week: Period, within: Period) -> Option<WeekEval> {
    let mut eligible = 0;
    let (mut done, mut unrec, mut na, mut paused) = (0, 0, 0, 0);
    let mut target = None;
    for d in week.iter_days() {
        if !within.contains(d) {
            continue;
        }
        let Some(v) = spec.version_at(d) else { continue };
        let Frequency::TimesPerWeek(n) = v.frequency else {
            continue;
        };
        target = Some(n); // versión del último día de la semana con versión semanal
        if spec.is_paused(d) {
            paused += 1;
            continue;
        }
        match entries.get(&d) {
            Some(EntryStatus::NotApplicable) => {
                na += 1;
                continue;
            }
            Some(EntryStatus::Done) => done += 1,
            Some(EntryStatus::NotDone) => {}
            None => unrec += 1,
        }
        eligible += 1;
    }
    let n = target?;
    Some(WeekEval {
        required: required_for_partial_week(n, eligible),
        done,
        unrecorded: unrec,
        na,
        paused,
        target: n,
    })
}

pub fn today_progress(spec: &HabitSpec, entries: &Entries, today: NaiveDate) -> TodayProgress {
    let v = spec.version_at(today);
    let paused = spec.is_paused(today);
    let (week_done, week_target) = match v.map(|v| v.frequency) {
        Some(Frequency::TimesPerWeek(n)) if !paused => {
            let w = iso_week_period(today);
            let done = w
                .iter_days()
                .filter(|d| *d <= today && entries.get(d) == Some(&EntryStatus::Done))
                .count() as u32;
            (Some(done), Some(n as u32))
        }
        _ => (None, None),
    };
    TodayProgress {
        date: today,
        shown: spec.shown_on(today),
        required_today: !paused && v.is_some_and(|v| v.frequency.requires_day(today)),
        status: entries.get(&today).copied(),
        week_done,
        week_target,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreakUnit {
    Days,
    Weeks,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Streaks {
    pub current: u32,
    pub longest: u32,
    /// Unidad de la versión vigente hoy (días exigibles o semanas que alcanzan N).
    pub unit: StreakUnit,
}

/// Rachas desde la creación del hábito hasta `today` (ver reglas en el módulo).
pub fn streaks(spec: &HabitSpec, entries: &Entries, today: NaiveDate) -> Streaks {
    let unit = match spec
        .version_at(today)
        .or_else(|| spec.versions.iter().max_by_key(|v| v.valid_from))
    {
        Some(v) if v.frequency.is_weekly() => StreakUnit::Weeks,
        _ => StreakUnit::Days,
    };
    let start = spec.created_on;
    if start > today {
        return Streaks {
            current: 0,
            longest: 0,
            unit,
        };
    }
    let history = Period { start, end: today };
    // (fecha de cierre de la ocurrencia, éxito)
    let mut seq: Vec<(NaiveDate, bool)> = Vec::new();
    for d in history.iter_days() {
        let Some(v) = spec.version_at(d) else { continue };
        if v.frequency.is_weekly() || !v.frequency.requires_day(d) || spec.is_paused(d) {
            continue;
        }
        match entries.get(&d) {
            Some(EntryStatus::NotApplicable) => {}
            Some(EntryStatus::Done) => seq.push((d, true)),
            _ if d == today => {} // el día abierto no rompe
            _ => seq.push((d, false)),
        }
    }
    let mut ws = iso_week_period(start).start;
    while ws <= today {
        let week = Period {
            start: ws,
            end: ws + Duration::days(6),
        };
        ws += Duration::days(7);
        if week.end < today {
            if let Some(w) = weekly_week(spec, entries, week, history)
                && w.required > 0
            {
                seq.push((week.end, w.done >= w.required));
            }
        } else if let Some(w) = weekly_week(
            spec,
            entries,
            Period {
                start: week.start,
                end: today,
            },
            history,
        ) {
            // Semana en curso: solo suma si ya alcanzó N completo.
            if w.done >= w.target as u32 {
                seq.push((week.end, true));
            }
        }
    }
    seq.sort_by_key(|(d, _)| *d);
    let (mut run, mut longest) = (0u32, 0u32);
    for (_, ok) in &seq {
        run = if *ok { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    Streaks {
        current: run,
        longest,
        unit,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodKind {
    Week,
    Month,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareMode {
    /// Mismo tramo: del inicio del periodo a `date` contra el mismo número de días del periodo anterior.
    SameSpan,
    /// Periodos completos (etiquetado así en la UI).
    Full,
}

/// Periodo actual y anterior para comparar (C13).
pub fn comparison(kind: PeriodKind, date: NaiveDate, mode: CompareMode) -> (Period, Period) {
    let cur_full = match kind {
        PeriodKind::Week => iso_week_period(date),
        PeriodKind::Month => month_period(date),
    };
    let prev_full = match kind {
        PeriodKind::Week => iso_week_period(cur_full.start - Duration::days(1)),
        PeriodKind::Month => month_period(cur_full.start - Duration::days(1)),
    };
    match mode {
        CompareMode::Full => (cur_full, prev_full),
        CompareMode::SameSpan => {
            let elapsed = (date - cur_full.start).num_days();
            let prev_end = (prev_full.start + Duration::days(elapsed)).min(prev_full.end);
            (
                Period {
                    start: cur_full.start,
                    end: date,
                },
                Period {
                    start: prev_full.start,
                    end: prev_end,
                },
            )
        }
    }
}

/// Primer y último valor medido dentro del periodo, sin interpolar días sin dato.
pub fn first_last<T: Copy>(points: &[(NaiveDate, T)], p: Period) -> Option<((NaiveDate, T), (NaiveDate, T))> {
    let mut inside = points.iter().filter(|(d, _)| p.contains(*d));
    let first = *inside.next()?;
    let last = *points
        .iter()
        .rev()
        .find(|(d, _)| p.contains(*d))
        .unwrap_or(&first);
    Some((first, last))
}

/// Días del mes de `date` (para vistas de calendario).
pub fn days_in_month(date: NaiveDate) -> u32 {
    month_period(date).end.day()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::habits::{HabitVersion, Pause};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn daily(from: NaiveDate) -> HabitSpec {
        HabitSpec {
            created_on: from,
            archived_on: None,
            versions: vec![HabitVersion {
                valid_from: from,
                valid_to: None,
                frequency: Frequency::Daily,
            }],
            pauses: vec![],
        }
    }
    fn ent(items: &[(NaiveDate, EntryStatus)]) -> Entries {
        items.iter().copied().collect()
    }
    use EntryStatus::*;

    #[test]
    fn open_day_is_not_penalized_and_missing_is_distinguished() {
        let spec = daily(d(2026, 11, 9));
        let today = d(2026, 11, 12);
        let e = ent(&[(d(2026, 11, 9), Done), (d(2026, 11, 10), NotDone)]);
        let c = compliance(
            &spec,
            &e,
            Period {
                start: d(2026, 11, 9),
                end: d(2026, 11, 15),
            },
            today,
        );
        // 9, 10, 11 cerrados; 12 abierto; 13–15 futuros.
        assert_eq!((c.required, c.done, c.missed, c.unrecorded), (3, 1, 1, 1));
        assert!((c.rate.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        let t = today_progress(&spec, &e, today);
        assert!(t.required_today && t.status.is_none());
    }

    #[test]
    fn no_data_is_none_not_zero() {
        let spec = daily(d(2026, 11, 12));
        let c = compliance(
            &spec,
            &Entries::new(),
            Period {
                start: d(2026, 11, 12),
                end: d(2026, 11, 30),
            },
            d(2026, 11, 12),
        );
        assert_eq!(c.required, 0);
        assert_eq!(c.rate, None);
    }

    #[test]
    fn not_applicable_and_pauses_leave_denominator() {
        let mut spec = daily(d(2026, 11, 1));
        spec.pauses.push(Pause {
            starts_on: d(2026, 11, 3),
            ends_on: Some(d(2026, 11, 4)),
        });
        let e = ent(&[
            (d(2026, 11, 1), Done),
            (d(2026, 11, 2), NotApplicable),
            (d(2026, 11, 5), Done),
        ]);
        let c = compliance(
            &spec,
            &e,
            Period {
                start: d(2026, 11, 1),
                end: d(2026, 11, 5),
            },
            d(2026, 11, 6),
        );
        assert_eq!(
            (c.required, c.done, c.excluded_not_applicable, c.paused_days),
            (2, 2, 1, 2)
        );
        assert_eq!(c.rate, Some(1.0));
    }

    #[test]
    fn selected_weekdays_only_count_scheduled_days() {
        let spec = HabitSpec {
            created_on: d(2026, 11, 9),
            archived_on: None,
            versions: vec![HabitVersion {
                valid_from: d(2026, 11, 9),
                valid_to: None,
                frequency: Frequency::Weekdays(1 | 4 | 16),
            }],
            pauses: vec![],
        };
        // Semana 9–15 nov: lunes, miércoles, viernes exigibles.
        let e = ent(&[(d(2026, 11, 9), Done), (d(2026, 11, 13), Done)]);
        let c = compliance(
            &spec,
            &e,
            Period {
                start: d(2026, 11, 9),
                end: d(2026, 11, 15),
            },
            d(2026, 11, 16),
        );
        assert_eq!((c.required, c.done, c.unrecorded), (3, 2, 1));
    }

    #[test]
    fn times_per_week_is_not_daily_and_caps_overcompliance() {
        let spec = HabitSpec {
            created_on: d(2026, 11, 2),
            archived_on: None,
            versions: vec![HabitVersion {
                valid_from: d(2026, 11, 2),
                valid_to: None,
                frequency: Frequency::TimesPerWeek(4),
            }],
            pauses: vec![],
        };
        // Semana 2–8: 5 sesiones (sobrecumple). Semana 9–15: 2 sesiones (falla 2).
        let mut e = Entries::new();
        for day in [2, 3, 4, 5, 6, 10, 12] {
            e.insert(d(2026, 11, day), Done);
        }
        let c = compliance(
            &spec,
            &e,
            Period {
                start: d(2026, 11, 2),
                end: d(2026, 11, 15),
            },
            d(2026, 11, 16),
        );
        assert_eq!((c.required, c.done, c.extra, c.missed), (8, 6, 1, 2));
        // Ningún día individual sin gym es falla por sí mismo: la semana abierta no cuenta.
        let c2 = compliance(
            &spec,
            &e,
            Period {
                start: d(2026, 11, 9),
                end: d(2026, 11, 15),
            },
            d(2026, 11, 12),
        );
        assert_eq!(c2.required, 0);
        let t = today_progress(&spec, &e, d(2026, 11, 12));
        assert_eq!(
            (t.week_done, t.week_target, t.required_today),
            (Some(2), Some(4), false)
        );
    }

    #[test]
    fn fully_paused_week_counts_paused_days_not_failures() {
        let spec = HabitSpec {
            created_on: d(2026, 11, 2),
            archived_on: None,
            versions: vec![HabitVersion {
                valid_from: d(2026, 11, 2),
                valid_to: None,
                frequency: Frequency::TimesPerWeek(4),
            }],
            pauses: vec![Pause {
                starts_on: d(2026, 11, 2),
                ends_on: Some(d(2026, 11, 8)),
            }],
        };
        let c = compliance(
            &spec,
            &Entries::new(),
            Period {
                start: d(2026, 11, 2),
                end: d(2026, 11, 8),
            },
            d(2026, 11, 20),
        );
        assert_eq!((c.required, c.paused_days, c.missed, c.rate), (0, 7, 0, None));
        assert_eq!(streaks(&spec, &Entries::new(), d(2026, 11, 9)).current, 0);
    }

    #[test]
    fn partial_week_prorates_target() {
        // Hábito 4/semana creado el viernes 2026-11-13: quedan 3 días → exigidas round(4×3/7)=2.
        let spec = HabitSpec {
            created_on: d(2026, 11, 13),
            archived_on: None,
            versions: vec![HabitVersion {
                valid_from: d(2026, 11, 13),
                valid_to: None,
                frequency: Frequency::TimesPerWeek(4),
            }],
            pauses: vec![],
        };
        let e = ent(&[(d(2026, 11, 14), Done)]);
        let c = compliance(
            &spec,
            &e,
            Period {
                start: d(2026, 11, 1),
                end: d(2026, 11, 30),
            },
            d(2026, 11, 20),
        );
        assert_eq!((c.required, c.done), (2, 1));
    }

    #[test]
    fn frequency_change_does_not_rewrite_past() {
        let spec = HabitSpec {
            created_on: d(2026, 11, 2),
            archived_on: None,
            versions: vec![
                HabitVersion {
                    valid_from: d(2026, 11, 2),
                    valid_to: Some(d(2026, 11, 8)),
                    frequency: Frequency::Daily,
                },
                HabitVersion {
                    valid_from: d(2026, 11, 9),
                    valid_to: None,
                    frequency: Frequency::TimesPerWeek(3),
                },
            ],
            pauses: vec![],
        };
        let mut e = Entries::new();
        for day in 2..=8 {
            e.insert(d(2026, 11, day), Done);
        }
        for day in [9, 11, 13] {
            e.insert(d(2026, 11, day), Done);
        }
        let c = compliance(
            &spec,
            &e,
            Period {
                start: d(2026, 11, 2),
                end: d(2026, 11, 15),
            },
            d(2026, 11, 16),
        );
        // 7 días diarios + 3 de la semana 3/semana.
        assert_eq!((c.required, c.done), (10, 10));
        let s = streaks(&spec, &e, d(2026, 11, 16));
        assert_eq!(s.unit, StreakUnit::Weeks);
        assert_eq!((s.current, s.longest), (8, 8));
    }

    #[test]
    fn streaks_daily_open_day_and_na() {
        let spec = daily(d(2026, 11, 1));
        let e = ent(&[
            (d(2026, 11, 1), Done),
            (d(2026, 11, 2), Done),
            (d(2026, 11, 3), Done),
            (d(2026, 11, 4), NotDone),
            (d(2026, 11, 5), Done),
            (d(2026, 11, 6), NotApplicable),
            (d(2026, 11, 7), Done),
        ]);
        // Hoy (8) sin registro: no rompe.
        let s = streaks(&spec, &e, d(2026, 11, 8));
        assert_eq!((s.current, s.longest, s.unit), (2, 3, StreakUnit::Days));
        let mut e2 = e.clone();
        e2.insert(d(2026, 11, 8), Done);
        assert_eq!(streaks(&spec, &e2, d(2026, 11, 8)).current, 3);
        // Un día cerrado sin registro sí rompe.
        assert_eq!(streaks(&spec, &e, d(2026, 11, 9)).current, 0);
    }

    #[test]
    fn weekly_streak_across_iso_year() {
        // 2026-W53 (28 dic–3 ene) y 2027-W01.
        let spec = HabitSpec {
            created_on: d(2026, 12, 21),
            archived_on: None,
            versions: vec![HabitVersion {
                valid_from: d(2026, 12, 21),
                valid_to: None,
                frequency: Frequency::TimesPerWeek(2),
            }],
            pauses: vec![],
        };
        let e = ent(&[
            (d(2026, 12, 22), Done),
            (d(2026, 12, 23), Done),
            (d(2026, 12, 31), Done),
            (d(2027, 1, 2), Done),
            (d(2027, 1, 5), Done),
        ]);
        let s = streaks(&spec, &e, d(2027, 1, 6));
        // W52 ✓, W53 ✓ (cruza año), W01 en curso con 1/2 no suma ni rompe.
        assert_eq!((s.current, s.longest), (2, 2));
    }

    #[test]
    fn comparison_same_span_and_full() {
        let (cur, prev) = comparison(PeriodKind::Month, d(2026, 3, 31), CompareMode::SameSpan);
        assert_eq!(
            cur,
            Period {
                start: d(2026, 3, 1),
                end: d(2026, 3, 31)
            }
        );
        // Febrero recortado a su último día.
        assert_eq!(
            prev,
            Period {
                start: d(2026, 2, 1),
                end: d(2026, 2, 28)
            }
        );
        let (cur, prev) = comparison(PeriodKind::Week, d(2027, 1, 1), CompareMode::SameSpan);
        assert_eq!(cur.start, d(2026, 12, 28));
        assert_eq!(
            prev,
            Period {
                start: d(2026, 12, 21),
                end: d(2026, 12, 25)
            }
        );
        let (_, prev) = comparison(PeriodKind::Month, d(2026, 11, 12), CompareMode::Full);
        assert_eq!(
            prev,
            Period {
                start: d(2026, 10, 1),
                end: d(2026, 10, 31)
            }
        );
    }

    #[test]
    fn first_last_without_interpolation() {
        let pts = vec![(d(2026, 11, 2), 80), (d(2026, 11, 9), 79), (d(2026, 12, 1), 78)];
        let p = Period {
            start: d(2026, 11, 1),
            end: d(2026, 11, 30),
        };
        assert_eq!(
            first_last(&pts, p),
            Some(((d(2026, 11, 2), 80), (d(2026, 11, 9), 79)))
        );
        assert_eq!(
            first_last(
                &pts,
                Period {
                    start: d(2026, 10, 1),
                    end: d(2026, 10, 31)
                }
            ),
            None
        );
    }

    #[test]
    fn merge_rates() {
        let a = Compliance {
            required: 2,
            done: 1,
            ..Default::default()
        };
        let b = Compliance {
            required: 2,
            done: 2,
            ..Default::default()
        };
        assert_eq!(Compliance::merge(&[a, b]).rate, Some(0.75));
        assert_eq!(Compliance::merge(&[]).rate, None);
    }
}
