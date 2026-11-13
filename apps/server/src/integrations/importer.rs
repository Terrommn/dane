//! Importación del vault (T53): lectura segura (ZIP o carpeta) → plan en dry-run → confirmación → aplicación.
//!
//! Seguridad: sin ejecutar contenido; límites de tamaño/archivos; rechazo de rutas absolutas, `..`, symlinks y
//! zip bombs (tamaño real leído con límite, no el declarado). Idempotencia: `import_records(source_key)` + hash; reimportar
//! no duplica; si el archivo cambió se reporta conflicto y no se sobrescribe lo que hay en la app.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};

use chrono::{Datelike, Duration, NaiveDate};
use domain::text::{fold, validate_external_url};
use domain::vault::{self, FileKind, Note, PaymentRef};
use persistence::Tx;
use persistence::hooks::{self, HabitTrigger};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::apply::{Resolver, create_goal_habit};
use crate::state::AppState;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    /// Relación máxima descomprimido/comprimido por archivo (zip bomb).
    pub max_ratio: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            max_file_bytes: 10 * 1024 * 1024,
            max_total_bytes: 200 * 1024 * 1024,
            max_ratio: 200,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ImportError {
    #[error("el archivo no es un ZIP válido")]
    NotZip,
    #[error("ruta peligrosa rechazada: {0}")]
    UnsafePath(String),
    #[error("enlace simbólico rechazado: {0}")]
    Symlink(String),
    #[error("demasiados archivos (máximo {0})")]
    TooManyFiles(usize),
    #[error("archivo demasiado grande: {0}")]
    FileTooLarge(String),
    #[error("el contenido descomprimido excede el límite")]
    TooLarge,
    #[error("compresión sospechosa (posible zip bomb): {0}")]
    Bomb(String),
}

#[derive(Debug, Clone)]
pub struct VaultFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Normaliza y valida una ruta relativa: sin raíz, sin `..`, sin unidades de Windows ni caracteres de control.
pub fn safe_path(p: &str) -> Result<String, ImportError> {
    let unsafe_ = || ImportError::UnsafePath(p.chars().take(200).collect());
    let t = p.replace('\\', "/");
    if t.starts_with('/') || t.contains(':') || t.chars().any(|c| c.is_control()) || t.len() > 1024 {
        return Err(unsafe_());
    }
    let mut parts = Vec::new();
    for seg in t.split('/') {
        match seg {
            "" | "." => {}
            ".." => return Err(unsafe_()),
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        return Err(unsafe_());
    }
    Ok(parts.join("/"))
}

pub fn read_zip(bytes: &[u8], limits: Limits) -> Result<Vec<VaultFile>, ImportError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| ImportError::NotZip)?;
    if archive.len() > limits.max_files {
        return Err(ImportError::TooManyFiles(limits.max_files));
    }
    let mut total = 0u64;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|_| ImportError::NotZip)?;
        let raw_name = f.name().map_err(|_| ImportError::NotZip)?.to_string();
        let path = safe_path(&raw_name)?;
        if f.enclosed_name().is_none() {
            return Err(ImportError::UnsafePath(raw_name));
        }
        if f.is_symlink() {
            return Err(ImportError::Symlink(raw_name));
        }
        if f.is_dir() {
            continue;
        }
        if f.size() > limits.max_file_bytes {
            return Err(ImportError::FileTooLarge(path));
        }
        let declared_ratio = f.size() / f.compressed_size().max(1);
        if f.size() > 1024 * 1024 && declared_ratio > limits.max_ratio {
            return Err(ImportError::Bomb(path));
        }
        // Se lee con límite real: el tamaño declarado en el ZIP puede mentir.
        let mut buf = Vec::new();
        (&mut f)
            .take(limits.max_file_bytes + 1)
            .read_to_end(&mut buf)
            .map_err(|_| ImportError::NotZip)?;
        if buf.len() as u64 > limits.max_file_bytes {
            return Err(ImportError::FileTooLarge(path));
        }
        total += buf.len() as u64;
        if total > limits.max_total_bytes {
            return Err(ImportError::TooLarge);
        }
        out.push(VaultFile { path, bytes: buf });
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ Plan

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Payload {
    Project(vault::ProjectImport),
    Person(vault::PersonImport),
    Card(vault::CardImport),
    Account(vault::AccountImport),
    Recurring {
        subscription: bool,
        item: vault::RecurringImport,
    },
    RecurringTask(vault::RecurringTaskImport),
    CreditPlan(vault::CreditPlanImport),
    Reading(vault::ReadingImport),
    Content(vault::ContentImport),
    Swipe(vault::SwipeImport),
    Note {
        kind: String,
        title: String,
        body: String,
        tags: Vec<String>,
        aliases: Vec<String>,
        project: Option<String>,
        metadata: Value,
    },
    Adr(vault::AdrImport),
    Devlog(vault::DevlogImport),
    Class(vault::ClassImport),
    Goal {
        kind: String,
        period: String,
        starts_on: NaiveDate,
        ends_on: NaiveDate,
        closing: String,
        goal: vault::GoalImport,
    },
    Devotional(vault::DevotionalImport),
    Daily(vault::DailyImport),
    Task(vault::TaskImport),
    Weekly(vault::WeeklyImport),
    Transaction {
        income: bool,
        row: vault::TxRow,
    },
    Budget {
        month: String,
        lines: Vec<(String, i64)>,
    },
    Inbox {
        text: String,
    },
}

impl Payload {
    /// Orden de aplicación (las dependencias primero).
    fn rank(&self) -> u8 {
        match self {
            Payload::Project(_) => 0,
            Payload::Person(_) => 1,
            Payload::Card(_) | Payload::Account(_) => 2,
            Payload::Recurring { .. } | Payload::RecurringTask(_) | Payload::CreditPlan(_) => 3,
            Payload::Reading(_)
            | Payload::Content(_)
            | Payload::Swipe(_)
            | Payload::Note { .. }
            | Payload::Adr(_)
            | Payload::Devlog(_)
            | Payload::Class(_) => 4,
            Payload::Goal { kind, .. } => {
                if kind == "month" {
                    5
                } else {
                    6
                }
            }
            Payload::Devotional(_) => 7,
            Payload::Daily(_) => 8,
            Payload::Task(_) => 9,
            Payload::Weekly(_) => 10,
            Payload::Transaction { .. } | Payload::Budget { .. } => 11,
            Payload::Inbox { .. } => 12,
        }
    }
    pub fn entity_type(&self) -> &'static str {
        match self {
            Payload::Project(_) => "project",
            Payload::Person(_) => "person",
            Payload::Card(_) | Payload::Account(_) => "account",
            Payload::Recurring { .. } => "recurring",
            Payload::RecurringTask(_) => "recurring_task",
            Payload::CreditPlan(_) => "credit_plan",
            Payload::Reading(_) => "reading",
            Payload::Content(_) => "content",
            Payload::Swipe(_) => "swipe",
            Payload::Note { .. } => "note",
            Payload::Adr(_) => "adr",
            Payload::Devlog(_) => "devlog",
            Payload::Class(_) => "class_note",
            Payload::Goal { .. } => "goal",
            Payload::Devotional(_) => "devotional",
            Payload::Daily(_) => "checkin",
            Payload::Task(_) => "task",
            Payload::Weekly(_) => "weekly_review",
            Payload::Transaction { .. } => "transaction",
            Payload::Budget { .. } => "budget",
            Payload::Inbox { .. } => "inbox",
        }
    }
}

fn table_of(entity_type: &str) -> Option<&'static str> {
    Some(match entity_type {
        "project" => "projects",
        "person" => "people",
        "account" => "fin_accounts",
        "recurring" => "fin_recurring_rules",
        "recurring_task" => "task_recurrence_rules",
        "reading" => "readings",
        "content" => "content_pieces",
        "swipe" => "swipe_refs",
        "note" => "notes",
        "adr" => "adrs",
        "devlog" => "devlogs",
        "class_note" => "class_notes",
        "goal" => "goals",
        "devotional" => "devotionals",
        "checkin" => "daily_checkins",
        "task" => "tasks",
        "weekly_review" => "weekly_reviews",
        "transaction" => "transactions",
        "budget" => "budget_months",
        "inbox" => "inbox_items",
        _ => return None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub source_key: String,
    pub path: String,
    pub title: String,
    pub entity_type: String,
    /// `create | link | unchanged | conflict`
    pub action: String,
    pub reason: Option<String>,
    pub hash: String,
    pub unknown_fields: Value,
    pub payload: Payload,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileRef {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Plan {
    pub items: Vec<PlanItem>,
    pub warnings: Vec<String>,
    /// Archivos no soportados que se conservan como referencia.
    pub references: Vec<FileRef>,
    pub ignored: Vec<FileRef>,
}

impl Plan {
    pub fn summary(&self) -> Value {
        let mut by: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for i in &self.items {
            *by.entry(i.entity_type.clone())
                .or_default()
                .entry(i.action.clone())
                .or_default() += 1;
        }
        json!({
            "by_entity": by,
            "total": self.items.len(),
            "create": self.items.iter().filter(|i| i.action == "create").count(),
            "link": self.items.iter().filter(|i| i.action == "link").count(),
            "unchanged": self.items.iter().filter(|i| i.action == "unchanged").count(),
            "conflicts": self.items.iter().filter(|i| i.action == "conflict").count(),
            "references": self.references.len(),
            "ignored": self.ignored.len(),
            "warnings": self.warnings.len(),
        })
    }
}

fn hash(p: &Payload) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(p).unwrap_or_default()))
}

fn known_keys(kind: &FileKind) -> &'static [&'static str] {
    match kind {
        FileKind::Daily { .. } => &[
            "fecha",
            "categories",
            "devocional",
            "ejercicio",
            "lectura",
            "energia",
            "peso",
        ],
        FileKind::Devotional { .. } => &["fecha", "categories", "pasaje"],
        FileKind::GoalMonth { .. } | FileKind::GoalWeek { .. } => {
            &["categories", "tipo", "periodo", "inicio", "fin", "estado", "mes"]
        }
        FileKind::Weekly { .. } => &["semana", "paz", "categories"],
        FileKind::Person => &[
            "categories",
            "area",
            "relacion",
            "org",
            "rol",
            "ultimo_contacto",
            "cumpleaños",
            "contacto",
            "aliases",
        ],
        FileKind::Reading => &["categories", "tipo", "autor", "estado", "rating", "inicio", "fin"],
        FileKind::Content => &[
            "categories",
            "estado",
            "formato",
            "plataforma",
            "hook",
            "pilar",
            "fecha_pub",
            "raw_path",
            "render_path",
            "views",
            "retencion",
        ],
        FileKind::Card => &[
            "categories",
            "banco",
            "limite",
            "disponible",
            "actualizado",
            "dia_corte",
            "dia_pago",
            "estado",
        ],
        FileKind::Account => &["categories", "banco", "saldo", "actualizado", "estado"],
        FileKind::Adr { .. } => &["categories", "proyecto", "estado", "fecha"],
        FileKind::Devlog { .. } => &["categories", "fecha", "proyectos"],
        FileKind::ClassNote { .. } => &["categories", "materia", "fecha"],
        FileKind::Project { .. } => &["categories", "estado", "repo"],
        FileKind::Expenses { .. } | FileKind::Income { .. } => &["categories", "mes"],
        _ => &["categories", "tags", "aliases"],
    }
}

fn unknown(note: &Note, kind: &FileKind) -> Value {
    let known = known_keys(kind);
    Value::Object(
        note.frontmatter
            .iter()
            .filter(|(k, _)| !known.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    )
}

fn item(path: &str, key_suffix: &str, title: String, payload: Payload, unknown_fields: Value) -> PlanItem {
    PlanItem {
        source_key: if key_suffix.is_empty() {
            path.to_string()
        } else {
            format!("{path}#{key_suffix}")
        },
        path: path.to_string(),
        title,
        entity_type: payload.entity_type().to_string(),
        action: "create".into(),
        reason: None,
        hash: hash(&payload),
        unknown_fields,
        payload,
    }
}

/// Presupuesto por rubro del esqueleto (ejemplo, no dato del usuario).
fn is_example_budget(items: &[(String, i64, Vec<String>)]) -> bool {
    let v: Vec<(&str, i64)> = items.iter().map(|(n, c, _)| (n.as_str(), *c)).collect();
    v == [("Servidores", 30_000), ("WhatsApp", 100_000), ("Ads", 650_000)]
}

fn is_example_credit_plan(p: &vault::CreditPlanImport) -> bool {
    p.goal.is_empty()
        && p.steps.is_empty()
        && p.rules.iter().map(|r| fold(r)).collect::<Vec<_>>()
            == [
                "pagar siempre el total, nunca el minimo.",
                "uso de cada tarjeta < 30% de su linea.",
                "nada de tarjetas nuevas fuera de este plan.",
            ]
}

fn inline_tags(body: &str) -> Vec<String> {
    let mut tags: Vec<String> = body
        .split_whitespace()
        .filter_map(|w| w.strip_prefix('#'))
        .filter(|t| {
            !t.is_empty()
                && t.chars().next().is_some_and(|c| c.is_alphabetic())
                && t.chars().all(|c| c.is_alphanumeric() || "/_-".contains(c))
        })
        .map(str::to_string)
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

/// Construye el plan a partir de los archivos (puro: no consulta la BD).
pub fn build_plan(files: &[VaultFile]) -> Plan {
    let mut plan = Plan::default();
    let mut dailies: Vec<(String, vault::DailyImport)> = Vec::new();
    for f in files {
        let kind = vault::classify(&f.path);
        match &kind {
            FileKind::Ignored { reason } => {
                plan.ignored.push(FileRef {
                    path: f.path.clone(),
                    reason: reason.clone(),
                });
                continue;
            }
            FileKind::Unsupported { reason } => {
                plan.references.push(FileRef {
                    path: f.path.clone(),
                    reason: reason.clone(),
                });
                continue;
            }
            _ => {}
        }
        let raw = match std::str::from_utf8(&f.bytes) {
            Ok(s) => s.to_string(),
            // Archivo en Latin-1: cada byte es un carácter.
            Err(_) => f.bytes.iter().map(|b| *b as char).collect(),
        };
        if raw.contains("<%") || raw.contains("{{date") {
            plan.ignored.push(FileRef {
                path: f.path.clone(),
                reason: "contiene sintaxis de plantilla".into(),
            });
            continue;
        }
        let (note, warns) = vault::parse_note(&f.path, &raw);
        plan.warnings
            .extend(warns.into_iter().map(|w| format!("{}: {w}", f.path)));
        let unk = unknown(&note, &kind);
        let p = f.path.as_str();
        match kind {
            FileKind::Daily { date } => dailies.push((p.to_string(), vault::daily(&note, date))),
            FileKind::Devotional { date } => {
                let d = vault::devotional(&note, date);
                if d.has_content() {
                    plan.items.push(item(
                        p,
                        "",
                        format!("Devocional {date}"),
                        Payload::Devotional(d),
                        unk,
                    ));
                } else {
                    plan.ignored.push(FileRef {
                        path: p.into(),
                        reason: "devocional vacío".into(),
                    });
                }
            }
            FileKind::GoalMonth { period } | FileKind::GoalWeek { period } => {
                let is_month = vault::classify(p)
                    == FileKind::GoalMonth {
                        period: period.clone(),
                    };
                let g = vault::goals_note(&note, if is_month { "month" } else { "week" }, &period);
                let range = if is_month {
                    domain::time::parse_month(&period)
                } else {
                    domain::time::parse_iso_week(&period)
                };
                let Ok(range) = range else {
                    plan.warnings.push(format!("{p}: periodo inválido '{period}'"));
                    continue;
                };
                if g.goals.is_empty() {
                    plan.ignored.push(FileRef {
                        path: p.into(),
                        reason: "nota de objetivos sin objetivos".into(),
                    });
                }
                for (i, goal) in g.goals.iter().enumerate() {
                    plan.items.push(item(
                        p,
                        &(i + 1).to_string(),
                        goal.title.clone(),
                        Payload::Goal {
                            kind: g.kind.clone(),
                            period: period.clone(),
                            starts_on: g.starts_on.unwrap_or(range.start),
                            ends_on: g.ends_on.unwrap_or(range.end),
                            closing: g.closing.clone(),
                            goal: goal.clone(),
                        },
                        unk.clone(),
                    ));
                }
            }
            FileKind::Weekly { week } => {
                let w = vault::weekly(&note, &week);
                if domain::time::parse_iso_week(&w.week).is_ok() {
                    plan.items
                        .push(item(p, "", format!("Semana {}", w.week), Payload::Weekly(w), unk));
                } else {
                    plan.warnings.push(format!("{p}: semana inválida"));
                }
            }
            FileKind::Person => {
                let x = vault::person(&note);
                plan.items
                    .push(item(p, "", x.name.clone(), Payload::Person(x), unk));
            }
            FileKind::Reading => {
                let x = vault::reading(&note);
                plan.items
                    .push(item(p, "", x.title.clone(), Payload::Reading(x), unk));
            }
            FileKind::Content => {
                let x = vault::content(&note);
                plan.items
                    .push(item(p, "", x.title.clone(), Payload::Content(x), unk));
            }
            FileKind::Swipe => {
                let x = vault::swipe(&note);
                if x.url.is_some() {
                    plan.items
                        .push(item(p, "", x.title.clone(), Payload::Swipe(x), unk));
                } else {
                    plan.warnings
                        .push(format!("{p}: referencia Swipe sin URL; se importa como nota"));
                    plan.items.push(item(
                        p,
                        "",
                        note.stem(),
                        Payload::Note {
                            kind: "note".into(),
                            title: note.stem(),
                            body: note.body.clone(),
                            tags: vec!["swipe".into()],
                            aliases: vec![],
                            project: None,
                            metadata: unk.clone(),
                        },
                        unk,
                    ));
                }
            }
            FileKind::ClassNote { subject } => {
                let x = vault::class_note(&note, &subject);
                if x.date.is_none() {
                    plan.warnings.push(format!("{p}: clase sin fecha"));
                    continue;
                }
                plan.items
                    .push(item(p, "", x.topic.clone(), Payload::Class(x), unk));
            }
            FileKind::Devlog { date } => {
                let x = vault::devlog(&note, date);
                if !x.sessions.is_empty() {
                    plan.items
                        .push(item(p, "", format!("Devlog {date}"), Payload::Devlog(x), unk));
                }
            }
            FileKind::Adr { project } => {
                let x = vault::adr(&note, &project);
                plan.items
                    .push(item(p, "", x.title.clone(), Payload::Adr(x), unk));
            }
            FileKind::Expenses { month } | FileKind::Income { month } => {
                let income = matches!(vault::classify(p), FileKind::Income { .. });
                let m = vault::money_note(&note, &month, if income { "Cuenta" } else { "Pago" });
                plan.warnings.extend(m.warnings.iter().cloned());
                for row in m.rows {
                    let key = format!(
                        "{}|{}|{}|{}",
                        row.date,
                        fold(&row.concept),
                        row.amount_cents,
                        row.occurrence
                    );
                    plan.items.push(item(
                        p,
                        &key,
                        format!("{} {}", row.date, row.concept),
                        Payload::Transaction { income, row },
                        Value::Object(Default::default()),
                    ));
                }
                if !m.budget.is_empty() {
                    plan.items.push(item(
                        p,
                        "budget",
                        format!("Presupuesto {month}"),
                        Payload::Budget {
                            month,
                            lines: m.budget,
                        },
                        Value::Object(Default::default()),
                    ));
                }
            }
            FileKind::Card => {
                let x = vault::card(&note);
                plan.items
                    .push(item(p, "", x.name.clone(), Payload::Card(x), unk));
            }
            FileKind::Account => {
                let x = vault::account(&note);
                plan.items
                    .push(item(p, "", x.name.clone(), Payload::Account(x), unk));
            }
            FileKind::Subscriptions | FileKind::FixedIncome => {
                let subscription = matches!(vault::classify(p), FileKind::Subscriptions);
                let (rules, warns) = vault::recurring_list(&note, subscription);
                plan.warnings
                    .extend(warns.into_iter().map(|w| format!("{p}: {w}")));
                for r in rules {
                    plan.items.push(item(
                        p,
                        &fold(&r.name),
                        r.name.clone(),
                        Payload::Recurring {
                            subscription,
                            item: r,
                        },
                        Value::Object(Default::default()),
                    ));
                }
            }
            FileKind::RecurringTasks => {
                for r in vault::recurring_tasks(&note) {
                    plan.items.push(item(
                        p,
                        &fold(&r.title),
                        r.title.clone(),
                        Payload::RecurringTask(r),
                        Value::Object(Default::default()),
                    ));
                }
            }
            FileKind::CreditPlan => {
                let x = vault::credit_plan(&note);
                if is_example_credit_plan(&x) {
                    plan.ignored.push(FileRef {
                        path: p.into(),
                        reason: "plan de crédito de ejemplo (sin cambios del usuario)".into(),
                    });
                } else {
                    plan.items
                        .push(item(p, "", "Plan de crédito".into(), Payload::CreditPlan(x), unk));
                }
            }
            FileKind::Inbox => {
                for (i, t) in vault::inbox(&note).into_iter().enumerate() {
                    plan.items.push(item(
                        p,
                        &format!("{i}|{}", fold(&t)),
                        t.clone(),
                        Payload::Inbox { text: t },
                        Value::Object(Default::default()),
                    ));
                }
            }
            FileKind::Project { slug } => {
                let mut x = vault::project(&note, &slug);
                if is_example_budget(&x.budget_items) {
                    plan.warnings.push(format!(
                        "{p}: presupuesto por rubro de ejemplo omitido (configúralo en Finanzas)"
                    ));
                    x.budget_items.clear();
                }
                plan.items
                    .push(item(p, "", slug.clone(), Payload::Project(x), unk));
            }
            FileKind::Note { note_kind } => {
                let project = match p.split('/').collect::<Vec<_>>().as_slice() {
                    ["Proyectos", proj, ..] => Some(proj.to_string()),
                    _ => None,
                };
                let mut tags = note.fm_list("tags");
                tags.extend(inline_tags(&note.body));
                tags.sort();
                tags.dedup();
                let body = note.body.trim().to_string();
                if body.is_empty() && note.frontmatter.is_empty() {
                    plan.ignored.push(FileRef {
                        path: p.into(),
                        reason: "nota vacía".into(),
                    });
                    continue;
                }
                plan.items.push(item(
                    p,
                    "",
                    note.stem(),
                    Payload::Note {
                        kind: note_kind,
                        title: note.stem(),
                        body,
                        tags,
                        aliases: note.fm_list("aliases"),
                        project,
                        metadata: unk.clone(),
                    },
                    unk,
                ));
            }
            FileKind::Ignored { .. } | FileKind::Unsupported { .. } => unreachable!(),
        }
    }
    // Pendientes consolidados a través de las dailies (el arrastre del vault los copiaba cada día).
    let only: Vec<vault::DailyImport> = dailies.iter().map(|d| d.1.clone()).collect();
    for t in vault::consolidate_tasks(&only) {
        plan.items.push(item(
            "Daily/*",
            &format!("task:{}", t.key),
            t.title.clone(),
            Payload::Task(t),
            Value::Object(Default::default()),
        ));
    }
    for (path, mut d) in dailies {
        if d.fixed_false_ignored > 0 {
            plan.warnings.push(format!("{path}: {} hábito(s) fijo(s) en false omitidos (la plantilla los ponía por defecto; no equivalen a «no»)", d.fixed_false_ignored));
        }
        d.tasks.clear(); // ya consolidados
        let date = d.date;
        plan.items.push(item(
            &path,
            "",
            format!("Daily {date}"),
            Payload::Daily(d),
            Value::Object(Default::default()),
        ));
    }
    plan.items.sort_by_key(|i| i.payload.rank());
    plan
}

/// Completa las acciones del plan con el estado de la BD: unchanged / conflict / link.
pub async fn resolve_actions(state: &AppState, user_id: Uuid, plan: &mut Plan) -> sqlx::Result<()> {
    let records: Vec<(String, String, Uuid, String, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as("SELECT source_key, entity_type, entity_id, content_hash, imported_at FROM import_records WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&state.pool)
            .await?;
    let map: HashMap<(String, String), (Uuid, String, chrono::DateTime<chrono::Utc>)> = records
        .into_iter()
        .map(|(k, t, id, h, at)| ((k, t), (id, h, at)))
        .collect();
    for it in &mut plan.items {
        if let Some((id, h, at)) = map.get(&(it.source_key.clone(), it.entity_type.clone())) {
            if *h == it.hash {
                it.action = "unchanged".into();
                continue;
            }
            let modified: Option<bool> = match table_of(&it.entity_type) {
                Some(t) => sqlx::query_scalar(persistence::sqlf!(
                    "SELECT updated_at > $3 FROM {t} WHERE user_id = $1 AND id = $2"
                ))
                .bind(user_id)
                .bind(id)
                .bind(*at)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten(),
                None => None,
            };
            it.action = match modified {
                None => {
                    it.reason = Some("se eliminó en la app; no se vuelve a crear".into());
                    "unchanged".into()
                }
                Some(true) => {
                    it.reason =
                        Some("cambió en el archivo y también en la app; revisa y edita en la app".into());
                    "conflict".into()
                }
                Some(false) => {
                    it.reason = Some(
                        "el archivo cambió desde la última importación; no se sobrescribe automáticamente"
                            .into(),
                    );
                    "conflict".into()
                }
            };
            continue;
        }
        // Colisiones por clave natural con datos creados en la app.
        let (sql, arg): (Option<&str>, String) = match &it.payload {
            Payload::Daily(d) => (
                Some("SELECT 1 FROM daily_checkins WHERE user_id = $1 AND on_date = $2::date"),
                d.date.to_string(),
            ),
            Payload::Devotional(d) => (
                Some("SELECT 1 FROM devotionals WHERE user_id = $1 AND on_date = $2::date"),
                d.date.to_string(),
            ),
            Payload::Weekly(w) => (
                Some("SELECT 1 FROM weekly_reviews WHERE user_id = $1 AND iso_week = $2"),
                w.week.clone(),
            ),
            Payload::Devlog(d) => (
                Some("SELECT 1 FROM devlogs WHERE user_id = $1 AND on_date = $2::date"),
                d.date.to_string(),
            ),
            _ => (None, String::new()),
        };
        if let Some(sql) = sql {
            let exists: Option<i32> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
                .bind(user_id)
                .bind(&arg)
                .fetch_optional(&state.pool)
                .await?;
            if exists.is_some() {
                it.action = "conflict".into();
                it.reason = Some("ya existe un registro en la app para esa fecha; no se sobrescribe".into());
            }
            continue;
        }
        let link: Option<i32> = match &it.payload {
            Payload::Person(p) => sqlx::query_scalar("SELECT 1 FROM people WHERE user_id = $1 AND lower(immutable_unaccent(name)) = lower(immutable_unaccent($2))")
                .bind(user_id)
                .bind(&p.name)
                .fetch_optional(&state.pool)
                .await?,
            Payload::Project(p) => sqlx::query_scalar("SELECT 1 FROM projects WHERE user_id = $1 AND slug = $2").bind(user_id).bind(domain::text::slugify(&p.slug)).fetch_optional(&state.pool).await?,
            Payload::Card(c) => sqlx::query_scalar("SELECT 1 FROM fin_accounts WHERE user_id = $1 AND name = $2").bind(user_id).bind(&c.name).fetch_optional(&state.pool).await?,
            Payload::Account(c) => sqlx::query_scalar("SELECT 1 FROM fin_accounts WHERE user_id = $1 AND name = $2").bind(user_id).bind(&c.name).fetch_optional(&state.pool).await?,
            _ => None,
        };
        if link.is_some() {
            it.action = "link".into();
            it.reason = Some("ya existe en la app: se enlaza sin duplicar".into());
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ Aplicar

#[derive(Debug, Default, Serialize)]
pub struct ApplyReport {
    pub created: usize,
    pub linked: usize,
    pub skipped: usize,
    pub conflicts: usize,
    pub warnings: Vec<String>,
}

struct Ctx {
    r: Resolver,
    tz: chrono_tz::Tz,
    today: NaiveDate,
    run_id: Uuid,
    warnings: Vec<String>,
    /// título (fold) → id de objetivos mensuales por mes, para "Apoya a (mes)".
    month_goals: HashMap<(String, String), Uuid>,
    imported_debt: Option<Uuid>,
}

/// Aplica en una sola transacción todo lo que el plan marca como `create`/`link`.
pub async fn apply_plan(
    state: &AppState,
    user_id: Uuid,
    run_id: Uuid,
    plan: &Plan,
) -> Result<ApplyReport, String> {
    let db = |e: sqlx::Error| format!("error de base de datos: {e}");
    let mut tx = state.pool.begin().await.map_err(db)?;
    let tz = super::user_tz(state, user_id).await;
    let today = domain::time::local_date(state.now(), tz);
    let mut ctx = Ctx {
        r: Resolver::load(&mut tx, user_id).await.map_err(db)?,
        tz,
        today,
        run_id,
        warnings: vec![],
        month_goals: HashMap::new(),
        imported_debt: None,
    };
    let mut rep = ApplyReport::default();
    for it in &plan.items {
        match it.action.as_str() {
            "unchanged" => {
                rep.skipped += 1;
                continue;
            }
            "conflict" => {
                rep.conflicts += 1;
                continue;
            }
            _ => {}
        }
        let linked = it.action == "link";
        let entity = apply_item(&mut tx, user_id, &mut ctx, it)
            .await
            .map_err(|e| format!("{}: {e}", it.path))?;
        if let Some(id) = entity {
            sqlx::query(
                "INSERT INTO import_records (user_id, source_key, entity_type, entity_id, content_hash, unknown_fields, provenance, run_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (user_id, source_key, entity_type) DO UPDATE SET content_hash = EXCLUDED.content_hash, imported_at = now(), run_id = EXCLUDED.run_id",
            )
            .bind(user_id)
            .bind(&it.source_key)
            .bind(&it.entity_type)
            .bind(id)
            .bind(&it.hash)
            .bind(&it.unknown_fields)
            .bind(json!({ "path": it.path, "run_id": run_id, "action": it.action }))
            .bind(run_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            if linked {
                rep.linked += 1;
            } else {
                rep.created += 1;
            }
        } else {
            rep.skipped += 1;
        }
    }
    tx.commit().await.map_err(db)?;
    rep.warnings = ctx.warnings;
    Ok(rep)
}

fn noon_utc(d: NaiveDate, tz: chrono_tz::Tz) -> chrono::DateTime<chrono::Utc> {
    domain::time::local_to_utc(d, chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap(), tz)
}

async fn ensure_area(
    tx: &mut Tx<'_>,
    ctx: &mut Ctx,
    user_id: Uuid,
    name: &str,
) -> sqlx::Result<Option<Uuid>> {
    if let Some(a) = ctx.r.area(Some(name)) {
        return Ok(Some(a));
    }
    let n = name.trim();
    if n.is_empty() {
        return Ok(ctx.r.default_area());
    }
    let slug = domain::text::slugify(n);
    let display = vault::plain(n);
    let display = display
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + &display[c.len_utf8()..])
        .unwrap_or(display);
    let id: Uuid = sqlx::query_scalar("INSERT INTO areas (user_id, name, slug) VALUES ($1, $2, $3) ON CONFLICT (user_id, slug) DO UPDATE SET name = areas.name RETURNING id")
        .bind(user_id)
        .bind(&display)
        .bind(&slug)
        .fetch_one(&mut **tx)
        .await?;
    ctx.warnings
        .push(format!("Se creó el área «{display}» encontrada en el vault"));
    ctx.r.reload(tx).await?;
    Ok(Some(id))
}

async fn resolve_payment(
    tx: &mut Tx<'_>,
    ctx: &mut Ctx,
    user_id: Uuid,
    p: &PaymentRef,
    path: &str,
) -> sqlx::Result<(Option<Uuid>, Option<String>, Option<Uuid>)> {
    // (account_id, account_kind, financed_by_person)
    Ok(match p {
        PaymentRef::None => (None, None, None),
        PaymentRef::Card { name } => match ctx.r.account(name) {
            Some((id, k)) => (Some(id), Some(k), None),
            None => {
                let id: Uuid = sqlx::query_scalar("INSERT INTO fin_accounts (user_id, name, kind) VALUES ($1, $2, 'credit_card') RETURNING id")
                    .bind(user_id)
                    .bind(name)
                    .fetch_one(&mut **tx)
                    .await?;
                ctx.r
                    .push_account(id, name.clone(), String::new(), "credit_card".into());
                ctx.warnings.push(format!(
                    "{path}: se creó la tarjeta «{name}» sin límite ni corte; complétala en Finanzas"
                ));
                (Some(id), Some("credit_card".into()), None)
            }
        },
        PaymentRef::Bank { bank, .. } => match ctx.r.account(bank) {
            Some((id, k)) => {
                ctx.warnings.push(format!(
                    "{path}: medio «{bank}» asociado a una cuenta por nombre (revisa C4)"
                ));
                (Some(id), Some(k), None)
            }
            None => {
                let name = format!("Cuenta {bank}");
                let id: Uuid = sqlx::query_scalar("INSERT INTO fin_accounts (user_id, name, bank, kind) VALUES ($1, $2, $3, 'debit') RETURNING id")
                    .bind(user_id)
                    .bind(&name)
                    .bind(bank)
                    .fetch_one(&mut **tx)
                    .await?;
                ctx.r.push_account(id, name.clone(), bank.clone(), "debit".into());
                ctx.warnings.push(format!(
                    "{path}: se creó la cuenta «{name}» a partir del texto de pago"
                ));
                (Some(id), Some("debit".into()), None)
            }
        },
        PaymentRef::Cash => match ctx.r.account("Efectivo") {
            Some((id, k)) => (Some(id), Some(k), None),
            None => {
                let id: Uuid = sqlx::query_scalar("INSERT INTO fin_accounts (user_id, name, kind) VALUES ($1, 'Efectivo', 'cash') RETURNING id").bind(user_id).fetch_one(&mut **tx).await?;
                ctx.r
                    .push_account(id, "Efectivo".into(), String::new(), "cash".into());
                (Some(id), Some("cash".into()), None)
            }
        },
        PaymentRef::Borrowed { person } => {
            let pid = ctx.r.person(person);
            if pid.is_none() {
                ctx.warnings
                    .push(format!("{path}: «{person}» (préstamo) no tiene nota de persona"));
            }
            (None, None, pid)
        }
        PaymentRef::Other { text } => match ctx.r.account(text) {
            Some((id, k)) => (Some(id), Some(k), None),
            None => {
                ctx.warnings.push(format!(
                    "{path}: medio de pago «{text}» no reconocido; queda sin cuenta"
                ));
                (None, None, None)
            }
        },
    })
}

#[allow(clippy::too_many_lines)]
async fn apply_item(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    ctx: &mut Ctx,
    it: &PlanItem,
) -> sqlx::Result<Option<Uuid>> {
    let tz = ctx.tz;
    match &it.payload {
        Payload::Project(p) => {
            let slug = domain::text::slugify(&p.slug);
            let links: Vec<Value> = p
                .links
                .iter()
                .filter_map(|(l, u)| {
                    validate_external_url("links", u)
                        .ok()
                        .map(|u| json!({ "label": l, "url": u }))
                })
                .collect();
            let existing: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM projects WHERE user_id = $1 AND slug = $2")
                    .bind(user_id)
                    .bind(&slug)
                    .fetch_optional(&mut **tx)
                    .await?;
            let id = match existing {
                Some(id) => {
                    sqlx::query(
                        "UPDATE projects SET description = CASE WHEN description = '' THEN $3 ELSE description END,
                            repo_url = coalesce(repo_url, $4), links = CASE WHEN links = '[]' THEN $5 ELSE links END WHERE user_id = $1 AND id = $2",
                    )
                    .bind(user_id)
                    .bind(id)
                    .bind(&p.description)
                    .bind(&p.repo)
                    .bind(Value::Array(links))
                    .execute(&mut **tx)
                    .await?;
                    id
                }
                None => {
                    let area = ensure_area(tx, ctx, user_id, &p.slug).await?;
                    sqlx::query_scalar("INSERT INTO projects (user_id, area_id, name, slug, description, status, repo_url, links) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id")
                        .bind(user_id)
                        .bind(area)
                        .bind(&p.slug)
                        .bind(&slug)
                        .bind(&p.description)
                        .bind(&p.status)
                        .bind(&p.repo)
                        .bind(Value::Array(links))
                        .fetch_one(&mut **tx)
                        .await?
                }
            };
            ctx.r.reload(tx).await?;
            if !p.budget_items.is_empty() {
                let month = domain::time::month_key(ctx.today);
                let cat = ctx
                    .r
                    .category("expense", &p.slug)
                    .or_else(|| ctx.r.category("expense", "Negocio"));
                if let Some((cat, _)) = cat {
                    let bm = budget_month(tx, user_id, &month).await?;
                    let mut total = 0i64;
                    for (i, (name, cents, kws)) in p.budget_items.iter().enumerate() {
                        total += cents;
                        sqlx::query(
                            "INSERT INTO budget_items (user_id, budget_month_id, project_id, category_id, name, amount_cents, keywords, position)
                             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (budget_month_id, project_id, name) DO NOTHING",
                        )
                        .bind(user_id)
                        .bind(bm)
                        .bind(id)
                        .bind(cat)
                        .bind(name)
                        .bind(cents)
                        .bind(kws)
                        .bind(i as i32)
                        .execute(&mut **tx)
                        .await?;
                    }
                    sqlx::query("INSERT INTO budget_lines (user_id, budget_month_id, category_id, amount_cents) VALUES ($1, $2, $3, $4) ON CONFLICT (budget_month_id, category_id) DO UPDATE SET amount_cents = EXCLUDED.amount_cents")
                        .bind(user_id)
                        .bind(bm)
                        .bind(cat)
                        .bind(total)
                        .execute(&mut **tx)
                        .await?;
                }
            }
            Ok(Some(id))
        }
        Payload::Person(p) => {
            if let Some(id) = ctx.r.person(&p.name).filter(|_| it.action == "link") {
                return Ok(Some(id));
            }
            let area = match &p.area {
                Some(a) => ctx.r.area(Some(a)),
                None => None,
            };
            let (bm, bd, by) = p
                .birthday
                .map(|(m, d, y)| (Some(m as i16), Some(d as i16), y.map(|y| y as i16)))
                .unwrap_or((None, None, None));
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO people (user_id, name, aliases, area_id, relationship, org, role, contact, email, context_md, birthday_month, birthday_day, birthday_year, last_contact_on)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING id",
            )
            .bind(user_id)
            .bind(&p.name)
            .bind(&p.aliases)
            .bind(area)
            .bind(&p.relationship)
            .bind(&p.org)
            .bind(&p.role)
            .bind(&p.contact)
            .bind(&p.email)
            .bind(&p.context)
            .bind(bm)
            .bind(bd)
            .bind(by)
            .bind(p.last_contact)
            .fetch_one(&mut **tx)
            .await?;
            for (date, text) in &p.notes {
                sqlx::query(
                    "INSERT INTO person_notes (user_id, person_id, on_date, text) VALUES ($1, $2, $3, $4)",
                )
                .bind(user_id)
                .bind(id)
                .bind(date.unwrap_or(ctx.today))
                .bind(text)
                .execute(&mut **tx)
                .await?;
            }
            ctx.r.push_person(id, p.name.clone(), p.aliases.clone());
            Ok(Some(id))
        }
        Payload::Card(c) => {
            let id = match ctx.r.account(&c.name).filter(|_| it.action == "link") {
                Some((id, _)) => id,
                None => {
                    let id: Uuid = sqlx::query_scalar(
                        "INSERT INTO fin_accounts (user_id, name, bank, kind, credit_limit_cents, cut_day, pay_day, status, notes_md)
                         VALUES ($1, $2, $3, 'credit_card', $4, $5, $6, $7, $8) RETURNING id",
                    )
                    .bind(user_id)
                    .bind(&c.name)
                    .bind(&c.bank)
                    .bind(c.limit_cents)
                    .bind(c.cut_day)
                    .bind(c.pay_day)
                    .bind(&c.status)
                    .bind(&c.notes)
                    .fetch_one(&mut **tx)
                    .await?;
                    ctx.r
                        .push_account(id, c.name.clone(), c.bank.clone(), "credit_card".into());
                    id
                }
            };
            if let (Some(av), Some(d)) = (c.available_cents, c.updated) {
                // Semántica legacy de tarjetas (D-020): solo movimientos posteriores al día `actualizado`.
                sqlx::query(
                    "INSERT INTO balance_snapshots (user_id, account_id, observed_at, observed_on, balance_cents, semantics, note)
                     VALUES ($1, $2, $3, $4, $5, 'legacy_after_day', 'Importado del vault (disponible)')",
                )
                .bind(user_id)
                .bind(id)
                .bind(domain::time::start_of_local_day(d + Duration::days(1), tz))
                .bind(d)
                .bind(av)
                .execute(&mut **tx)
                .await?;
            }
            Ok(Some(id))
        }
        Payload::Account(a) => {
            let id = match ctx.r.account(&a.name).filter(|_| it.action == "link") {
                Some((id, _)) => id,
                None => {
                    let id: Uuid = sqlx::query_scalar("INSERT INTO fin_accounts (user_id, name, bank, kind, status, notes_md) VALUES ($1, $2, $3, 'debit', $4, $5) RETURNING id")
                        .bind(user_id)
                        .bind(&a.name)
                        .bind(&a.bank)
                        .bind(&a.status)
                        .bind(&a.notes)
                        .fetch_one(&mut **tx)
                        .await?;
                    ctx.r
                        .push_account(id, a.name.clone(), a.bank.clone(), "debit".into());
                    id
                }
            };
            if let (Some(bal), Some(d)) = (a.balance_cents, a.updated) {
                // Semántica legacy de cuentas (D-020): saldo al iniciar el día, incluye movimientos de ese día.
                sqlx::query(
                    "INSERT INTO balance_snapshots (user_id, account_id, observed_at, observed_on, balance_cents, semantics, note)
                     VALUES ($1, $2, $3, $4, $5, 'legacy_day_start_inclusive', 'Importado del vault (saldo al iniciar el día)')",
                )
                .bind(user_id)
                .bind(id)
                .bind(domain::time::start_of_local_day(d, tz))
                .bind(d)
                .bind(bal)
                .execute(&mut **tx)
                .await?;
            }
            Ok(Some(id))
        }
        Payload::Recurring {
            subscription,
            item: r,
        } => {
            let kind = if *subscription { "expense" } else { "income" };
            let cat = ctx
                .r
                .category(kind, &r.category)
                .or_else(|| ctx.r.category(kind, "Otro"))
                .map(|c| c.0);
            let (account, _, _) = resolve_payment(tx, ctx, user_id, &r.account, &it.path).await?;
            let area = r.area.as_deref().and_then(|a| ctx.r.area(Some(a)));
            let (dk, day) = r.day.parts();
            let status = if r.active {
                "active"
            } else if *subscription {
                "cancelled"
            } else {
                "ended"
            };
            // Las ocurrencias anteriores ya están registradas en el vault: la regla empieza el mes siguiente.
            let next_month = domain::time::month_period(ctx.today).end + Duration::days(1);
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO fin_recurring_rules (user_id, kind, name, amount_cents, currency, day_kind, day, category_id, account_id, area_id, status, starts_on)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
            )
            .bind(user_id)
            .bind(if *subscription { "subscription" } else { "fixed_income" })
            .bind(&r.name)
            .bind(r.amount_cents)
            .bind(&ctx.r.currency)
            .bind(dk)
            .bind(day)
            .bind(cat)
            .bind(account)
            .bind(area)
            .bind(status)
            .bind(next_month)
            .fetch_one(&mut **tx)
            .await?;
            Ok(Some(id))
        }
        Payload::RecurringTask(r) => {
            let area = match &r.area {
                Some(a) => ensure_area(tx, ctx, user_id, a).await?,
                None => ctx.r.default_area(),
            };
            let (dk, day) = r.day.parts();
            let next_month = domain::time::month_period(ctx.today).end + Duration::days(1);
            Ok(Some(
                sqlx::query_scalar("INSERT INTO task_recurrence_rules (user_id, title, area_id, day_kind, day, active, starts_on) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id")
                    .bind(user_id)
                    .bind(&r.title)
                    .bind(area)
                    .bind(dk)
                    .bind(day)
                    .bind(r.active)
                    .bind(next_month)
                    .fetch_one(&mut **tx)
                    .await?,
            ))
        }
        Payload::CreditPlan(c) => {
            let steps: Vec<Value> = c
                .steps
                .iter()
                .map(|s| json!({ "text": vault::plain(&s.text), "done": s.checked }))
                .collect();
            let rules: Vec<Value> = c.rules.iter().map(|r| json!({ "text": r })).collect();
            sqlx::query(
                "INSERT INTO credit_plans (user_id, goal_text, steps, rules) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id) DO UPDATE SET goal_text = EXCLUDED.goal_text, steps = EXCLUDED.steps, rules = EXCLUDED.rules, version = credit_plans.version + 1",
            )
            .bind(user_id)
            .bind(&c.goal)
            .bind(Value::Array(steps))
            .bind(Value::Array(rules))
            .execute(&mut **tx)
            .await?;
            Ok(Some(user_id))
        }
        Payload::Reading(r) => {
            let (status, finished) = match (r.status.as_str(), r.finished_on) {
                ("finished", None) => {
                    ctx.warnings.push(format!(
                        "{}: lectura terminada sin fecha `fin`; se importa como «leyendo»",
                        it.path
                    ));
                    ("reading", None)
                }
                (s, f) => (s, f),
            };
            Ok(Some(
                sqlx::query_scalar(
                    "INSERT INTO readings (user_id, kind, title, author, status, rating, started_on, finished_on, ideas_md, quotes_md, application_md)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
                )
                .bind(user_id)
                .bind(&r.kind)
                .bind(&r.title)
                .bind(&r.author)
                .bind(status)
                .bind(r.rating.map(|x| x as i16))
                .bind(r.started_on)
                .bind(finished)
                .bind(&r.ideas)
                .bind(&r.quotes)
                .bind(&r.application)
                .fetch_one(&mut **tx)
                .await?,
            ))
        }
        Payload::Content(c) => {
            let mut notes = c.edit_notes.clone();
            let mut url = |v: &Option<String>, label: &str| -> Option<String> {
                let v = v.as_deref()?.trim();
                match validate_external_url(label, v) {
                    Ok(u) => Some(u),
                    Err(_) => {
                        notes.push_str(&format!("\nRuta original ({label}): {v}"));
                        None
                    }
                }
            };
            let raw = url(&c.raw_path, "raw");
            let render = url(&c.render_path, "render");
            let published_on = if c.status == "published" {
                c.publish_on
            } else {
                None
            };
            Ok(Some(
                sqlx::query_scalar(
                    "INSERT INTO content_pieces (user_id, title, idea, status, format, platforms, pillar, hooks, chosen_hook, script_hook, script_body, script_cta,
                        shot_list, edit_notes, result_worked, result_change, publish_on, published_on, raw_url, render_url, views, retention_pct)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22) RETURNING id",
                )
                .bind(user_id)
                .bind(&c.title)
                .bind(&c.idea)
                .bind(&c.status)
                .bind(&c.format)
                .bind(&c.platforms)
                .bind(&c.pillar)
                .bind(&c.hooks)
                .bind(c.chosen_hook.map(|h| h as i16))
                .bind(&c.script_hook)
                .bind(&c.script_body)
                .bind(&c.script_cta)
                .bind(Value::Array(c.shot_list.clone()))
                .bind(notes.trim())
                .bind(&c.result_worked)
                .bind(&c.result_change)
                .bind(c.publish_on)
                .bind(published_on)
                .bind(raw)
                .bind(render)
                .bind(c.views)
                .bind(c.retention_pct.and_then(rust_decimal::Decimal::from_f64_retain).map(|d| d.round_dp(2)))
                .fetch_one(&mut **tx)
                .await?,
            ))
        }
        Payload::Swipe(s) => {
            let Some(url) = s
                .url
                .as_deref()
                .and_then(|u| validate_external_url("url", u).ok())
            else {
                ctx.warnings.push(format!("{}: URL inválida", it.path));
                return Ok(None);
            };
            Ok(Some(
                sqlx::query_scalar("INSERT INTO swipe_refs (user_id, title, url, creator, why_it_works, hook) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id")
                    .bind(user_id)
                    .bind(&s.title)
                    .bind(url)
                    .bind(&s.creator)
                    .bind(&s.why)
                    .bind(&s.hook)
                    .fetch_one(&mut **tx)
                    .await?,
            ))
        }
        Payload::Note {
            kind,
            title,
            body,
            tags,
            aliases,
            project,
            metadata,
        } => {
            let project_id = project.as_deref().and_then(|p| ctx.r.project(p));
            Ok(Some(
                sqlx::query_scalar("INSERT INTO notes (user_id, kind, title, body_md, tags, aliases, project_id, metadata) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id")
                    .bind(user_id)
                    .bind(kind)
                    .bind(title)
                    .bind(body)
                    .bind(tags)
                    .bind(aliases)
                    .bind(project_id)
                    .bind(json!({ "frontmatter": metadata, "source": it.path }))
                    .fetch_one(&mut **tx)
                    .await?,
            ))
        }
        Payload::Adr(a) => {
            let Some(project) = ctx.r.project(&a.project) else {
                ctx.warnings.push(format!(
                    "{}: proyecto «{}» no existe; ADR omitido",
                    it.path, a.project
                ));
                return Ok(None);
            };
            let number = match a.number {
                Some(n) => {
                    let taken: Option<i32> = sqlx::query_scalar(
                        "SELECT 1 FROM adrs WHERE user_id = $1 AND project_id = $2 AND number = $3",
                    )
                    .bind(user_id)
                    .bind(project)
                    .bind(n)
                    .fetch_optional(&mut **tx)
                    .await?;
                    if taken.is_some() {
                        ctx.warnings.push(format!(
                            "{}: el número ADR-{n:04} ya existe; se asignó el siguiente libre",
                            it.path
                        ));
                        None
                    } else {
                        Some(n)
                    }
                }
                None => None,
            };
            let number = match number {
                Some(n) => n,
                None => sqlx::query_scalar(
                    "SELECT coalesce(max(number), 0) + 1 FROM adrs WHERE user_id = $1 AND project_id = $2",
                )
                .bind(user_id)
                .bind(project)
                .fetch_one(&mut **tx)
                .await?,
            };
            let status = if a.status == "superseded" {
                ctx.warnings.push(format!(
                    "{}: ADR reemplazado sin referencia al reemplazo; se importa como aceptado",
                    it.path
                ));
                "accepted"
            } else {
                a.status.as_str()
            };
            Ok(Some(
                sqlx::query_scalar(
                    "INSERT INTO adrs (user_id, project_id, number, title, status, decided_on, context_md, decision_md, alternatives_md, consequences_md)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
                )
                .bind(user_id)
                .bind(project)
                .bind(number)
                .bind(&a.title)
                .bind(status)
                .bind(a.date.unwrap_or(ctx.today))
                .bind(&a.context)
                .bind(&a.decision)
                .bind(&a.alternatives)
                .bind(&a.consequences)
                .fetch_one(&mut **tx)
                .await?,
            ))
        }
        Payload::Devlog(d) => {
            let id: Uuid =
                sqlx::query_scalar("INSERT INTO devlogs (user_id, on_date) VALUES ($1, $2) RETURNING id")
                    .bind(user_id)
                    .bind(d.date)
                    .fetch_one(&mut **tx)
                    .await?;
            for (i, s) in d.sessions.iter().enumerate() {
                let at = s
                    .at_time
                    .as_deref()
                    .and_then(|t| chrono::NaiveTime::parse_from_str(t, "%H:%M").ok());
                let sid: Uuid = sqlx::query_scalar(
                    "INSERT INTO devlog_sessions (user_id, devlog_id, repo, at_time, changes_md, bugs_md, decisions_md, next_steps_md, position)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
                )
                .bind(user_id)
                .bind(id)
                .bind(&s.repo)
                .bind(at)
                .bind(&s.changes)
                .bind(&s.bugs)
                .bind(&s.decisions)
                .bind(&s.next_steps)
                .bind(i as i32)
                .fetch_one(&mut **tx)
                .await?;
                for p in &d.projects {
                    if let Some(pid) = ctx.r.project(p) {
                        sqlx::query("INSERT INTO devlog_session_projects (user_id, session_id, project_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
                            .bind(user_id)
                            .bind(sid)
                            .bind(pid)
                            .execute(&mut **tx)
                            .await?;
                    }
                }
            }
            Ok(Some(id))
        }
        Payload::Class(c) => {
            let area = ctx.r.area(Some("escuela"));
            let subject: Uuid = sqlx::query_scalar(
                "INSERT INTO subjects (user_id, name, area_id) VALUES ($1, $2, $3) ON CONFLICT (user_id, name) DO UPDATE SET name = subjects.name RETURNING id",
            )
            .bind(user_id)
            .bind(&c.subject)
            .bind(area)
            .fetch_one(&mut **tx)
            .await?;
            let date = c.date.unwrap_or(ctx.today);
            let id: Uuid = sqlx::query_scalar("INSERT INTO class_notes (user_id, subject_id, on_date, topic, topics_md, questions_md) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id")
                .bind(user_id)
                .bind(subject)
                .bind(date)
                .bind(&c.topic)
                .bind(&c.topics)
                .bind(&c.questions)
                .fetch_one(&mut **tx)
                .await?;
            for d in &c.deliverables {
                sqlx::query("INSERT INTO tasks (user_id, title, area_id, class_note_id, status, planned_on, done_at) VALUES ($1, $2, $3, $4, $5, $6, $7)")
                    .bind(user_id)
                    .bind(vault::plain(&d.text))
                    .bind(area)
                    .bind(id)
                    .bind(if d.checked { "done" } else { "open" })
                    .bind(date)
                    .bind(d.checked.then(|| noon_utc(date, tz)))
                    .execute(&mut **tx)
                    .await?;
            }
            Ok(Some(id))
        }
        Payload::Goal {
            kind,
            period,
            starts_on,
            ends_on,
            closing,
            goal,
        } => {
            let area = match &goal.area {
                Some(a) => ensure_area(tx, ctx, user_id, a).await?,
                None => None,
            };
            let parent = if kind == "week" {
                goal.supports.as_ref().and_then(|s| {
                    let key = fold(s.split_once(". ").map(|x| x.1).unwrap_or(s));
                    ctx.month_goals
                        .iter()
                        .find(|((m, t), _)| {
                            (*m == domain::time::month_key(*starts_on)
                                || *m == domain::time::month_key(*ends_on))
                                && (t.contains(&key) || key.contains(t.as_str()))
                        })
                        .map(|(_, id)| *id)
                })
            } else {
                None
            };
            let closing_json =
                (!closing.is_empty() && goal.status != "active").then(|| json!({ "notes": closing }));
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, area_id, motivation, success_criteria, status, parent_goal_id, closing, closed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, CASE WHEN $10 <> 'active' THEN now() END) RETURNING id",
            )
            .bind(user_id)
            .bind(kind)
            .bind(period)
            .bind(starts_on)
            .bind(ends_on)
            .bind(&goal.title)
            .bind(area)
            .bind(&goal.motivation)
            .bind(&goal.success)
            .bind(&goal.status)
            .bind(parent)
            .bind(closing_json)
            .fetch_one(&mut **tx)
            .await?;
            if kind == "month" {
                ctx.month_goals.insert((period.clone(), fold(&goal.title)), id);
            }
            for h in &goal.habits {
                create_goal_habit(tx, user_id, id, h, *starts_on, Some(*ends_on)).await?;
            }
            Ok(Some(id))
        }
        Payload::Devotional(d) => {
            let va: Vec<Value> = d
                .verse_actions
                .iter()
                .map(|(v, a)| json!({ "verse": v, "action": a }))
                .collect();
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO devotionals (user_id, on_date, passage, verse_actions, learned, teaching, prayer, song) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
            )
            .bind(user_id)
            .bind(d.date)
            .bind(&d.passage)
            .bind(Value::Array(va))
            .bind(&d.learned)
            .bind(&d.teaching)
            .bind(&d.prayer)
            .bind(&d.song)
            .fetch_one(&mut **tx)
            .await?;
            hooks::mark_linked_habit(tx, user_id, HabitTrigger::DevotionalContent, d.date)
                .await
                .map_err(repo_to_sqlx)?;
            Ok(Some(id))
        }
        Payload::Daily(d) => {
            let weight = d
                .weight_kg
                .and_then(rust_decimal::Decimal::from_f64_retain)
                .map(|w| w.round_dp(2));
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO daily_checkins (user_id, on_date, weight_kg, energy, planned_spend_text, evening_done_at) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            )
            .bind(user_id)
            .bind(d.date)
            .bind(weight)
            .bind(d.energy.map(|e| e as i16))
            .bind(&d.planned_spend)
            .bind((d.date < ctx.today).then(|| domain::time::start_of_local_day(d.date + Duration::days(1), tz)))
            .fetch_one(&mut **tx)
            .await?;
            if weight.is_some() {
                hooks::mark_linked_habit(tx, user_id, HabitTrigger::WeightLogged, d.date)
                    .await
                    .map_err(repo_to_sqlx)?;
            }
            for key in &d.fixed_done {
                let hid = fixed_habit(tx, ctx, user_id, key).await?;
                upsert_entry(tx, user_id, hid, d.date, "done").await?;
            }
            for cb in d.month_habits.iter().chain(d.week_habits.iter()) {
                let name = vault::habit_name(&cb.text);
                let hid: Option<Uuid> = sqlx::query_scalar(
                    "SELECT h.id FROM habits h JOIN habit_versions v ON v.habit_id = h.id
                     WHERE h.user_id = $1 AND h.kind = 'goal' AND lower(immutable_unaccent(h.name)) = lower(immutable_unaccent($2))
                       AND v.valid_from <= $3 AND (v.valid_to IS NULL OR v.valid_to >= $3) ORDER BY h.created_at DESC LIMIT 1",
                )
                .bind(user_id)
                .bind(&name)
                .bind(d.date)
                .fetch_optional(&mut **tx)
                .await?;
                match hid {
                    // Casilla copiada a la daily: marcada = cumplido; sin marcar en día cerrado = no cumplido (semántica del vault).
                    Some(h) if cb.checked || d.date < ctx.today => {
                        upsert_entry(
                            tx,
                            user_id,
                            h,
                            d.date,
                            if cb.checked { "done" } else { "not_done" },
                        )
                        .await?
                    }
                    Some(_) => {}
                    None => ctx.warnings.push(format!(
                        "{}: hábito «{name}» sin objetivo vigente; registro omitido",
                        it.path
                    )),
                }
            }
            for (i, line) in d.log.iter().enumerate() {
                sqlx::query("INSERT INTO daily_log_entries (user_id, on_date, text, position, source) VALUES ($1, $2, $3, $4, 'import')")
                    .bind(user_id)
                    .bind(d.date)
                    .bind(vault::plain(line))
                    .bind(i as i32)
                    .execute(&mut **tx)
                    .await?;
            }
            for (polarity, items) in [("good", &d.good), ("bad", &d.bad)] {
                for (i, t) in items.iter().enumerate() {
                    sqlx::query("INSERT INTO reflection_items (user_id, on_date, polarity, text, position) VALUES ($1, $2, $3, $4, $5)")
                        .bind(user_id)
                        .bind(d.date)
                        .bind(polarity)
                        .bind(vault::plain(t))
                        .bind(i as i32)
                        .execute(&mut **tx)
                        .await?;
                }
            }
            for (i, line) in d.people.iter().enumerate() {
                let link = vault::wikilinks(line).into_iter().next();
                let (name, topic) = match &link {
                    Some(l) => (
                        l.basename(),
                        line.split_once("]]")
                            .map(|x| x.1.trim_start_matches([':', ' ']).to_string())
                            .unwrap_or_default(),
                    ),
                    None => match line.split_once(':') {
                        Some((n, t)) => (n.trim().to_string(), t.trim().to_string()),
                        None => (line.trim().to_string(), String::new()),
                    },
                };
                let source_ref = format!("import:{}:{i}", d.date);
                match ctx.r.person(&name) {
                    Some(pid) => {
                        sqlx::query("INSERT INTO interactions (user_id, person_id, on_date, topic, source, source_ref) VALUES ($1, $2, $3, $4, 'import', $5) ON CONFLICT DO NOTHING")
                            .bind(user_id)
                            .bind(pid)
                            .bind(d.date)
                            .bind(vault::plain(&topic))
                            .bind(&source_ref)
                            .execute(&mut **tx)
                            .await?;
                        sqlx::query("UPDATE people SET last_contact_on = greatest(coalesce(last_contact_on, $3), $3) WHERE user_id = $1 AND id = $2")
                            .bind(user_id)
                            .bind(pid)
                            .bind(d.date)
                            .execute(&mut **tx)
                            .await?;
                    }
                    None => {
                        sqlx::query("INSERT INTO person_suggestions (user_id, name, source, source_ref, on_date, context) VALUES ($1, $2, 'import', $3, $4, $5) ON CONFLICT DO NOTHING")
                            .bind(user_id)
                            .bind(vault::plain(&name))
                            .bind(&source_ref)
                            .bind(d.date)
                            .bind(vault::plain(&topic))
                            .execute(&mut **tx)
                            .await?;
                    }
                }
            }
            for (i, a) in d.agenda.iter().enumerate() {
                if let Some((start, end, title)) = vault::agenda_line(a) {
                    let t = |s: &str| {
                        chrono::NaiveTime::parse_from_str(s, "%H:%M")
                            .ok()
                            .map(|t| domain::time::local_to_utc(d.date, t, tz))
                    };
                    sqlx::query(
                        "INSERT INTO calendar_events (user_id, source, calendar_id, provider_event_id, on_date, starts_at, ends_at, title)
                         VALUES ($1, 'manual', 'import', $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
                    )
                    .bind(user_id)
                    .bind(format!("{}:{i}", d.date))
                    .bind(d.date)
                    .bind(t(&start))
                    .bind(end.as_deref().and_then(t))
                    .bind(vault::plain(&title))
                    .execute(&mut **tx)
                    .await?;
                }
            }
            Ok(Some(id))
        }
        Payload::Task(t) => {
            let area = ensure_area(tx, ctx, user_id, &t.area).await?;
            if let Some((ext, url)) = &t.external {
                let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM tasks WHERE user_id = $1 AND external_source = 'clickup' AND external_id = $2")
                    .bind(user_id)
                    .bind(ext)
                    .fetch_optional(&mut **tx)
                    .await?;
                if let Some(id) = existing {
                    return Ok(Some(id));
                }
                let _ = url;
            }
            let (ext_source, ext_id, ext_url) = match &t.external {
                Some((id, url)) => (
                    Some("clickup"),
                    Some(id.clone()),
                    validate_external_url("url", url).ok(),
                ),
                None => (None, None, None),
            };
            let notes = if t.clickup_flag {
                "Marcado #clickup en el vault"
            } else {
                ""
            };
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO tasks (user_id, title, notes_md, area_id, status, planned_on, done_at, external_source, external_id, external_url)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
            )
            .bind(user_id)
            .bind(&t.title)
            .bind(notes)
            .bind(area)
            .bind(if t.done_on.is_some() { "done" } else { "open" })
            .bind(t.first_date)
            .bind(t.done_on.map(|d| noon_utc(d, tz)))
            .bind(ext_source)
            .bind(ext_id)
            .bind(ext_url)
            .fetch_one(&mut **tx)
            .await?;
            sqlx::query("INSERT INTO task_events (user_id, task_id, at, kind, to_date, data) VALUES ($1, $2, $3, 'created', $4, $5)")
                .bind(user_id)
                .bind(id)
                .bind(noon_utc(t.first_date, tz))
                .bind(t.first_date)
                .bind(json!({ "source": "import" }))
                .execute(&mut **tx)
                .await?;
            let mut prev = t.first_date;
            for c in &t.carried_to {
                sqlx::query("INSERT INTO task_events (user_id, task_id, at, kind, from_date, to_date) VALUES ($1, $2, $3, 'carried_over', $4, $5)")
                    .bind(user_id)
                    .bind(id)
                    .bind(domain::time::start_of_local_day(*c, tz))
                    .bind(prev)
                    .bind(c)
                    .execute(&mut **tx)
                    .await?;
                prev = *c;
            }
            if let Some(d) = t.done_on {
                sqlx::query(
                    "INSERT INTO task_events (user_id, task_id, at, kind) VALUES ($1, $2, $3, 'completed')",
                )
                .bind(user_id)
                .bind(id)
                .bind(noon_utc(d, tz))
                .execute(&mut **tx)
                .await?;
            }
            Ok(Some(id))
        }
        Payload::Weekly(w) => {
            let start = domain::time::parse_iso_week(&w.week)
                .map(|p| p.start)
                .unwrap_or(ctx.today);
            let pri: Vec<Value> = w
                .priorities
                .iter()
                .map(|p| json!({ "text": vault::plain(p) }))
                .collect();
            Ok(Some(
                sqlx::query_scalar(
                    "INSERT INTO weekly_reviews (user_id, iso_week, week_start, peace, what_happened, what_didnt, content_md, habits_md, goals_md, areas_md, people_md, priorities, completed_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
                )
                .bind(user_id)
                .bind(&w.week)
                .bind(start)
                .bind(w.peace.map(|p| p as i16))
                .bind(&w.what_happened)
                .bind(&w.what_didnt)
                .bind(&w.content)
                .bind(&w.habits)
                .bind(&w.goals)
                .bind(&w.areas)
                .bind(&w.people)
                .bind(Value::Array(pri))
                .bind((start + Duration::days(7) <= ctx.today).then(|| noon_utc(start + Duration::days(6), tz)))
                .fetch_one(&mut **tx)
                .await?,
            ))
        }
        Payload::Transaction { income, row } => {
            let kind_cat = if *income { "income" } else { "expense" };
            let (cat, role) = match ctx.r.category(kind_cat, &row.category) {
                Some(c) => c,
                None => {
                    if !row.category.is_empty() {
                        ctx.warnings.push(format!(
                            "{}: categoría «{}» desconocida → Otro",
                            it.path, row.category
                        ));
                    }
                    ctx.r
                        .category(kind_cat, "Otro")
                        .unwrap_or((Uuid::nil(), "consumption".into()))
                }
            };
            let cat = (!cat.is_nil()).then_some(cat);
            let (account, account_kind, financed_by) =
                resolve_payment(tx, ctx, user_id, &row.payment, &it.path).await?;
            let project = vault::wikilinks(&row.concept_raw)
                .iter()
                .find_map(|l| ctx.r.project(&l.basename()));
            let card_in_concept = vault::wikilinks(&row.concept_raw)
                .iter()
                .find(|l| fold(&l.basename()).starts_with("tarjeta"))
                .and_then(|l| ctx.r.account(&l.basename()).filter(|a| a.1 == "credit_card"))
                .map(|a| a.0);
            let (kind, account_id, counter, debt) = if *income {
                (
                    if role == "financing" {
                        "loan_received"
                    } else {
                        "income"
                    },
                    account,
                    None,
                    None,
                )
            } else if role == "debt_payment" {
                match card_in_concept {
                    // Pago a tarjeta: debita la cuenta origen y libera crédito; no es consumo.
                    Some(card) if account.is_some() && account_kind.as_deref() != Some("credit_card") => {
                        ("card_payment", account, Some(card), None)
                    }
                    Some(card) => ("debt_payment", None, Some(card), None),
                    None => {
                        let debt = match ctx.imported_debt {
                            Some(d) => d,
                            None => {
                                let d: Uuid = sqlx::query_scalar("INSERT INTO debts (user_id, name, kind, principal_cents, opened_on) VALUES ($1, 'Deudas importadas del vault', 'other', 0, $2) RETURNING id")
                                    .bind(user_id)
                                    .bind(row.date)
                                    .fetch_one(&mut **tx)
                                    .await?;
                                ctx.warnings.push("Pagos de deuda sin tarjeta enlazada se asociaron a «Deudas importadas del vault»; reasígnalos en Finanzas".into());
                                ctx.imported_debt = Some(d);
                                d
                            }
                        };
                        (
                            "debt_payment",
                            account.filter(|_| account_kind.as_deref() != Some("credit_card")),
                            None,
                            Some(debt),
                        )
                    }
                }
            } else {
                ("expense", account, None, None)
            };
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO transactions (user_id, kind, status, origin, occurred_on, concept, amount_cents, currency, category_id, account_id, counter_account_id,
                     debt_id, financed_by_person_id, project_id, idempotency_key, confirmed_at)
                 VALUES ($1, $2, 'confirmed', 'import', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now()) RETURNING id",
            )
            .bind(user_id)
            .bind(kind)
            .bind(row.date)
            .bind(&row.concept)
            .bind(row.amount_cents)
            .bind(&ctx.r.currency)
            .bind(cat)
            .bind(account_id)
            .bind(counter)
            .bind(debt)
            .bind(financed_by)
            .bind(project)
            .bind(format!("import:{}:{}", ctx.run_id, it.source_key))
            .fetch_one(&mut **tx)
            .await?;
            Ok(Some(id))
        }
        Payload::Budget { month, lines } => {
            let bm = budget_month(tx, user_id, month).await?;
            for (cat, cents) in lines {
                match ctx.r.category("expense", cat) {
                    Some((c, _)) => {
                        sqlx::query("INSERT INTO budget_lines (user_id, budget_month_id, category_id, amount_cents) VALUES ($1, $2, $3, $4) ON CONFLICT (budget_month_id, category_id) DO NOTHING")
                            .bind(user_id)
                            .bind(bm)
                            .bind(c)
                            .bind(cents)
                            .execute(&mut **tx)
                            .await?;
                    }
                    None => ctx.warnings.push(format!(
                        "{}: categoría de presupuesto «{cat}» desconocida",
                        it.path
                    )),
                }
            }
            Ok(Some(bm))
        }
        Payload::Inbox { text } => Ok(Some(
            sqlx::query_scalar(
                "INSERT INTO inbox_items (user_id, text, source) VALUES ($1, $2, 'import') RETURNING id",
            )
            .bind(user_id)
            .bind(vault::plain(text))
            .fetch_one(&mut **tx)
            .await?,
        )),
    }
}

fn repo_to_sqlx(e: persistence::RepoError) -> sqlx::Error {
    match e {
        persistence::RepoError::Db(d) => d,
        other => sqlx::Error::Protocol(other.to_string()),
    }
}

async fn budget_month(tx: &mut Tx<'_>, user_id: Uuid, month: &str) -> sqlx::Result<Uuid> {
    sqlx::query_scalar("INSERT INTO budget_months (user_id, month) VALUES ($1, $2) ON CONFLICT (user_id, month) DO UPDATE SET month = budget_months.month RETURNING id")
        .bind(user_id)
        .bind(month)
        .fetch_one(&mut **tx)
        .await
}

/// Hábito fijo; si el vault trae registros y no existe, se crea con su frecuencia por defecto (editable) y se avisa.
async fn fixed_habit(tx: &mut Tx<'_>, ctx: &mut Ctx, user_id: Uuid, key: &str) -> sqlx::Result<Uuid> {
    if let Some(id) = sqlx::query_scalar("SELECT id FROM habits WHERE user_id = $1 AND fixed_key = $2")
        .bind(user_id)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?
    {
        return Ok(id);
    }
    let (name, freq, per_week) = match key {
        "devotional" => ("Devocional", "daily", None),
        "exercise" => ("Ejercicio", "times_per_week", Some(4i16)),
        _ => ("Lectura", "daily", None),
    };
    let start = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
    let id: Uuid = sqlx::query_scalar("INSERT INTO habits (user_id, name, kind, fixed_key, created_on) VALUES ($1, $2, 'fixed', $3, $4) RETURNING id")
        .bind(user_id)
        .bind(name)
        .bind(key)
        .bind(ctx.today)
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO habit_versions (user_id, habit_id, valid_from, frequency, times_per_week) VALUES ($1, $2, $3, $4, $5)")
        .bind(user_id)
        .bind(id)
        .bind(start.with_year(ctx.today.year() - 10).unwrap_or(start))
        .bind(freq)
        .bind(per_week)
        .execute(&mut **tx)
        .await?;
    ctx.warnings.push(format!(
        "Se creó el hábito fijo «{name}» porque el vault tiene registros; revisa su frecuencia en Hábitos"
    ));
    Ok(id)
}

async fn upsert_entry(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    habit: Uuid,
    date: NaiveDate,
    status: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO habit_entries (user_id, habit_id, on_date, status, source) VALUES ($1, $2, $3, $4, 'import')
         ON CONFLICT (habit_id, on_date) DO NOTHING",
    )
    .bind(user_id)
    .bind(habit)
    .bind(date)
    .bind(status)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_paths() {
        for p in ["../etc/passwd", "/abs/x.md", "a/../../x", "C:/x.md", "a/\u{0}b"] {
            assert!(safe_path(p).is_err(), "{p}");
        }
        assert_eq!(
            safe_path("./Vault//Daily/2026-11-02.md").unwrap(),
            "Vault/Daily/2026-11-02.md"
        );
    }

    #[test]
    fn skeleton_examples_are_not_data() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault-skeleton");
        let mut files = Vec::new();
        let mut stack = vec![base.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(&base).unwrap().to_string_lossy().to_string();
                    files.push(VaultFile {
                        path: rel,
                        bytes: std::fs::read(&p).unwrap(),
                    });
                }
            }
        }
        let plan = build_plan(&files);
        // El esqueleto no tiene datos operativos: solo proyectos (sin presupuesto de ejemplo) y nada más.
        let kinds: Vec<&str> = plan.items.iter().map(|i| i.entity_type.as_str()).collect();
        assert!(kinds.iter().all(|k| *k == "project"), "{kinds:?}");
        assert!(
            plan.items
                .iter()
                .all(|i| matches!(&i.payload, Payload::Project(p) if p.budget_items.is_empty()))
        );
        assert!(plan.ignored.iter().any(|f| f.path.starts_with("Templates/")));
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("presupuesto por rubro de ejemplo"))
        );
    }
}
