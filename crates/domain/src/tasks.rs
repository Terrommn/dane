//! Propietario: agente B-modules. Reglas puras de tareas recurrentes, Inbox, personas (cumpleaños) y devlog.

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::text::fold;
use crate::time::{DayOfMonth, months_between};

/// Ventana de recuperación (catch-up) de pendientes recurrentes tras reinicios: 31 días hacia atrás.
pub const RECURRENCE_CATCH_UP_DAYS: i64 = 31;

/// Ocurrencia de una regla recurrente en un mes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    /// Clave del periodo `YYYY-MM` (UNIQUE con la regla).
    pub period: String,
    pub date: NaiveDate,
}

/// Ocurrencias que deben existir hasta `today` inclusive: fecha ≥ `starts_on`, dentro de la ventana de catch-up y nunca
/// futuras. Un día inexistente (31 en abril) cae en el último día del mes.
pub fn due_occurrences(rule: DayOfMonth, starts_on: NaiveDate, today: NaiveDate) -> Vec<Occurrence> {
    let from = (today - Duration::days(RECURRENCE_CATCH_UP_DAYS)).max(starts_on);
    if from > today {
        return Vec::new();
    }
    months_between(from, today)
        .into_iter()
        .map(|(y, m)| Occurrence {
            period: format!("{y:04}-{m:02}"),
            date: rule.resolve(y, m),
        })
        .filter(|o| o.date >= from && o.date <= today)
        .collect()
}

/// Destino propuesto para un elemento del Inbox (determinista, revisable por el usuario).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxProposal {
    /// `task | note | person | discard`
    pub destination: String,
    pub title: String,
    /// Slug de área sugerida (`personal`, `trabajo`, `negocio`, `escuela`) si aplica.
    pub area_slug: Option<String>,
    pub reason: String,
}

const TASK_VERBS: &[&str] = &[
    "llamar",
    "comprar",
    "pagar",
    "enviar",
    "mandar",
    "escribir",
    "revisar",
    "terminar",
    "hacer",
    "agendar",
    "preparar",
    "contestar",
    "responder",
    "entregar",
    "estudiar",
    "leer",
    "ir",
    "recoger",
    "arreglar",
    "configurar",
    "subir",
    "publicar",
    "grabar",
    "editar",
    "cancelar",
    "renovar",
    "reservar",
    "cotizar",
    "buscar",
    "investigar",
    "limpiar",
    "organizar",
];

/// Reglas deterministas simples: verbo de acción → tarea; URL o idea → nota; "conocí a"/"contacto" → persona.
pub fn propose_inbox(text: &str) -> InboxProposal {
    let t = text.trim();
    let f = fold(t);
    let first = f.split_whitespace().next().unwrap_or_default();
    let area = guess_area(&f);
    let title: String = t.chars().take(200).collect();
    if f.starts_with("conoci a ") || f.starts_with("contacto ") || f.contains("su numero es") {
        return InboxProposal {
            destination: "person".into(),
            title,
            area_slug: area,
            reason: "Menciona a una persona nueva".into(),
        };
    }
    if TASK_VERBS.contains(&first) || f.starts_with("no olvidar") || f.starts_with("recordar") {
        return InboxProposal {
            destination: "task".into(),
            title,
            area_slug: area.or(Some("personal".into())),
            reason: format!("Empieza con una acción («{first}»)"),
        };
    }
    if f.contains("http://")
        || f.contains("https://")
        || f.starts_with("idea")
        || f.split_whitespace().count() > 25
    {
        return InboxProposal {
            destination: "note".into(),
            title,
            area_slug: area,
            reason: "Parece una referencia o idea para guardar".into(),
        };
    }
    InboxProposal {
        destination: "task".into(),
        title,
        area_slug: area.or(Some("personal".into())),
        reason: "Por defecto, una acción pendiente".into(),
    }
}

fn guess_area(f: &str) -> Option<String> {
    let has = |ws: &[&str]| {
        ws.iter()
            .any(|w| f.split(|c: char| !c.is_alphanumeric()).any(|x| x == *w))
    };
    if has(&[
        "clase", "clases", "tarea", "examen", "profe", "profesor", "escuela", "materia", "entrega",
    ]) {
        Some("escuela".into())
    } else if has(&[
        "ads", "anuncio", "anuncios", "whatsapp", "negocio", "render", "servidor",
    ]) {
        Some("negocio".into())
    } else if has(&[
        "trabajo", "cliente", "reunion", "jefe", "oficina", "clickup", "junta",
    ]) {
        Some("trabajo".into())
    } else {
        None
    }
}

/// Próximo cumpleaños desde `today` (29 de febrero → 28 en años no bisiestos) y años que cumple si se conoce el año.
pub fn next_birthday(
    month: u32,
    day: u32,
    year: Option<i32>,
    today: NaiveDate,
) -> Option<(NaiveDate, Option<i32>)> {
    let at = |y: i32| {
        NaiveDate::from_ymd_opt(y, month, day).or_else(|| {
            (month == 2 && day == 29)
                .then(|| NaiveDate::from_ymd_opt(y, 2, 28))
                .flatten()
        })
    };
    let this = at(today.year())?;
    let date = if this >= today {
        this
    } else {
        at(today.year() + 1)?
    };
    Some((date, year.map(|y| date.year() - y)))
}

/// Días desde el último contacto (None si nunca).
pub fn days_since(last: Option<NaiveDate>, today: NaiveDate) -> Option<i64> {
    last.map(|d| (today - d).num_days())
}

/// Días para publicar (negativo si ya pasó la fecha). Fórmula de Pipeline.base.
pub fn days_until(date: Option<NaiveDate>, today: NaiveDate) -> Option<i64> {
    date.map(|d| (d - today).num_days())
}

/// Sesión de devlog parseada desde texto pegado (equivalente a /log-session).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedSession {
    pub repo: String,
    pub branch: String,
    pub at_time: Option<String>,
    pub changes_md: String,
    pub bugs_md: String,
    pub decisions_md: String,
    pub next_steps_md: String,
    /// Proyectos mencionados como `[[slug]]` o `proyecto: slug`.
    pub project_slugs: Vec<String>,
}

/// Parser determinista: reconoce encabezados (`## Qué hice`, `Bloqueos / bugs`, `Decisiones`, `Siguiente`) y líneas
/// `repo:`, `rama:`/`branch:`, `hora:`, `### <repo> — <hora>`. El texto se trata como datos; nunca se ejecuta.
pub fn parse_devlog_text(text: &str) -> ParsedSession {
    let mut s = ParsedSession::default();
    let mut section = 0u8; // 0 cambios, 1 bugs, 2 decisiones, 3 siguiente
    let mut seen_header = false;
    for raw in text.lines() {
        let line = raw.trim_end();
        let t = line.trim();
        let f = fold(t.trim_start_matches('#').trim());
        if t.starts_with("### ") && (t.contains('—') || t.contains(" - ")) {
            let h = t.trim_start_matches('#').trim();
            let (r, h2) = h
                .split_once('—')
                .or_else(|| h.split_once(" - "))
                .unwrap_or((h, ""));
            s.repo = r.trim().to_string();
            let hour = h2.trim();
            if !hour.is_empty() {
                s.at_time = Some(hour.to_string());
            }
            continue;
        }
        if t.starts_with('#') || t.ends_with(':') && t.len() < 40 {
            let key = f.trim_end_matches(':');
            let next = if key.starts_with("que hice")
                || key.starts_with("cambios")
                || key.starts_with("hecho")
            {
                Some(0)
            } else if key.starts_with("bloqueos") || key.starts_with("bugs") || key.starts_with("problemas") {
                Some(1)
            } else if key.starts_with("decisiones") {
                Some(2)
            } else if key.starts_with("siguiente")
                || key.starts_with("pendiente")
                || key.starts_with("proximos")
            {
                Some(3)
            } else {
                None
            };
            if let Some(n) = next {
                section = n;
                seen_header = true;
                continue;
            }
        }
        if let Some((k, v)) = t.split_once(':') {
            let k = fold(k.trim().trim_start_matches(['-', '*']).trim());
            let v = v.trim();
            match k.as_str() {
                "repo" | "repositorio" => {
                    s.repo = v.to_string();
                    continue;
                }
                "rama" | "branch" => {
                    s.branch = v.to_string();
                    continue;
                }
                "hora" => {
                    s.at_time = Some(v.to_string());
                    continue;
                }
                "proyecto" | "proyectos" => {
                    for p in v.split(',') {
                        let p = p.trim().trim_start_matches("[[").trim_end_matches("]]").trim();
                        if !p.is_empty() {
                            s.project_slugs.push(crate::text::slugify(p));
                        }
                    }
                    continue;
                }
                _ => {}
            }
        }
        let mut rest = t;
        while let Some(i) = rest.find("[[") {
            if let Some(j) = rest[i..].find("]]") {
                let inner = &rest[i + 2..i + j];
                let name = inner.split('|').next().unwrap_or(inner);
                let slug = crate::text::slugify(name);
                if !s.project_slugs.contains(&slug) {
                    s.project_slugs.push(slug);
                }
                rest = &rest[i + j + 2..];
            } else {
                break;
            }
        }
        let target = match section {
            0 => &mut s.changes_md,
            1 => &mut s.bugs_md,
            2 => &mut s.decisions_md,
            _ => &mut s.next_steps_md,
        };
        if !t.is_empty() || !target.is_empty() {
            target.push_str(line);
            target.push('\n');
        }
    }
    let _ = seen_header;
    for f in [
        &mut s.changes_md,
        &mut s.bugs_md,
        &mut s.decisions_md,
        &mut s.next_steps_md,
    ] {
        *f = f.trim().to_string();
    }
    s.project_slugs.dedup();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn occurrences_handle_short_months_and_no_future() {
        let start = d(2025, 1, 1);
        // Día 31 en febrero no bisiesto → 28.
        let o = due_occurrences(DayOfMonth::Day(31), start, d(2026, 3, 1));
        assert_eq!(
            o.last(),
            Some(&Occurrence {
                period: "2026-02".into(),
                date: d(2026, 2, 28)
            })
        );
        assert_eq!(
            o.len(),
            2,
            "enero 31 y febrero 28 dentro de la ventana de catch-up"
        );
        // Penúltimo en febrero bisiesto = 28; en no bisiesto = 27.
        assert_eq!(
            due_occurrences(DayOfMonth::Penultimate, start, d(2028, 2, 28))
                .last()
                .unwrap()
                .date,
            d(2028, 2, 28)
        );
        assert_eq!(
            due_occurrences(DayOfMonth::Penultimate, start, d(2026, 2, 27))
                .last()
                .unwrap()
                .date,
            d(2026, 2, 27)
        );
        assert!(
            due_occurrences(DayOfMonth::Penultimate, start, d(2026, 2, 26))
                .iter()
                .all(|o| o.period != "2026-02")
        );
        // Último día y 29 en bisiesto.
        assert_eq!(
            due_occurrences(DayOfMonth::Last, start, d(2028, 2, 29))
                .last()
                .unwrap()
                .date,
            d(2028, 2, 29)
        );
        assert_eq!(
            due_occurrences(DayOfMonth::Day(29), start, d(2028, 2, 29))
                .last()
                .unwrap()
                .date,
            d(2028, 2, 29)
        );
        // Nunca futuras ni anteriores al inicio de la regla.
        assert!(due_occurrences(DayOfMonth::Day(20), d(2026, 11, 12), d(2026, 11, 12)).is_empty());
        assert!(due_occurrences(DayOfMonth::Day(5), d(2026, 11, 12), d(2026, 11, 12)).is_empty());
    }

    #[test]
    fn catch_up_recovers_missed_days_within_window() {
        // El servidor estuvo apagado del 10 al 20: la ocurrencia del 14 se recupera.
        let o = due_occurrences(DayOfMonth::Day(14), d(2026, 1, 1), d(2026, 11, 20));
        assert!(o.contains(&Occurrence {
            period: "2026-11".into(),
            date: d(2026, 11, 14)
        }));
        assert!(o.iter().all(|x| x.date >= d(2026, 10, 20)));
    }

    #[test]
    fn inbox_proposals_are_deterministic() {
        assert_eq!(propose_inbox("Llamar al dentista").destination, "task");
        assert_eq!(
            propose_inbox("Entregar tarea de cálculo").area_slug.as_deref(),
            Some("escuela")
        );
        assert_eq!(
            propose_inbox("pagar ads de whatsapp").area_slug.as_deref(),
            Some("negocio")
        );
        assert_eq!(
            propose_inbox("https://youtube.com/x buen hook").destination,
            "note"
        );
        assert_eq!(propose_inbox("Conocí a Ana en la junta").destination, "person");
    }

    #[test]
    fn birthdays_with_and_without_year() {
        let today = d(2026, 11, 12);
        assert_eq!(next_birthday(11, 12, None, today), Some((d(2026, 11, 12), None)));
        assert_eq!(
            next_birthday(1, 3, Some(1990), today),
            Some((d(2027, 1, 3), Some(37)))
        );
        assert_eq!(
            next_birthday(2, 29, None, d(2026, 12, 1)).unwrap().0,
            d(2027, 2, 28)
        );
        assert_eq!(
            next_birthday(2, 29, None, d(2027, 12, 1)).unwrap().0,
            d(2028, 2, 29)
        );
        assert_eq!(days_since(Some(d(2026, 11, 2)), today), Some(10));
        assert_eq!(days_until(Some(d(2026, 11, 10)), today), Some(-2));
    }

    #[test]
    fn parses_log_session_text() {
        let text = "### vida — 18:30\nrama: main\nproyectos: [[personal]]\n## Qué hice\n- Migraciones de [[negocio]]\n## Bloqueos / bugs raros\n- sqlx 0.9 exige AssertSqlSafe\n## Decisiones\n- Usar FK compuesta\n## Siguiente\n- [ ] Pruebas";
        let p = parse_devlog_text(text);
        assert_eq!(p.repo, "vida");
        assert_eq!(p.at_time.as_deref(), Some("18:30"));
        assert_eq!(p.branch, "main");
        assert!(p.changes_md.contains("Migraciones"));
        assert!(p.bugs_md.contains("AssertSqlSafe"));
        assert!(p.decisions_md.contains("FK compuesta"));
        assert!(p.next_steps_md.contains("Pruebas"));
        assert_eq!(
            p.project_slugs,
            vec!["personal".to_string(), "negocio".to_string()]
        );
    }
}
