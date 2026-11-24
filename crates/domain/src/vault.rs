//! Propietario: agente E-integrations. Parser puro del formato del vault de Obsidian (T53).
//!
//! No ejecuta nada: lee frontmatter YAML, secciones, tablas (con `\|` escapado), checkboxes y wikilinks, repara
//! mojibake (UTF-8 leído como Latin-1) y normaliza a NFC. Clasifica cada archivo por ruta y extrae registros tipados.
//! Plantillas, comandos, scripts, vistas `.base` y placeholders (`<% %>`, `{{date}}`, `<tu banco>`, `<VIEW_ID>`)
//! nunca se convierten en datos.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::money::parse_amount_to_cents;
use crate::text::fold;

// ---------------------------------------------------------------------------------------------------------------
// Texto

/// Repara mojibake típico (`Ã³` → `ó`) si el texto es UTF-8 decodificado como Latin-1/Windows-1252, y normaliza a NFC.
pub fn normalize_text(s: &str) -> String {
    let fixed = if s.contains('Ã') || s.contains('Â') {
        let bytes: Option<Vec<u8>> = s.chars().map(|c| u8::try_from(c as u32).ok()).collect();
        match bytes.and_then(|b| String::from_utf8(b).ok()) {
            Some(f) => f,
            None => s.to_string(),
        }
    } else {
        s.to_string()
    };
    fixed.nfc().collect::<String>().replace("\r\n", "\n")
}

/// Detecta sintaxis de plantilla o placeholders de ejemplo que no son datos del usuario.
pub fn is_placeholder(s: &str) -> bool {
    let t = s.trim();
    t.contains("<%")
        || t.contains("{{date")
        || t.contains("{{title")
        || t.contains("<tu ")
        || t.contains("<VIEW_ID>")
        || t.contains("<URL de")
        || t.contains("<qué ")
        || t.contains("tp.file")
        || t.contains("tp.date")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiLink {
    pub target: String,
    pub alias: Option<String>,
}

impl WikiLink {
    /// Nombre de la nota destino sin carpeta ni extensión (`Proyectos/x/Tarjeta A` → `Tarjeta A`).
    pub fn basename(&self) -> String {
        let t = self.target.split('#').next().unwrap_or_default();
        let b = t.rsplit('/').next().unwrap_or(t);
        b.trim_end_matches(".md").trim().to_string()
    }
}

/// Extrae `[[destino|alias]]` (también `[[destino\|alias]]` dentro de tablas).
pub fn wikilinks(s: &str) -> Vec<WikiLink> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find("[[") {
        let after = &rest[i + 2..];
        let Some(j) = after.find("]]") else { break };
        let inner = after[..j].replace("\\|", "|");
        let (target, alias) = match inner.split_once('|') {
            Some((t, a)) => (t.trim().to_string(), Some(a.trim().to_string())),
            None => (inner.trim().to_string(), None),
        };
        if !target.is_empty() {
            out.push(WikiLink { target, alias });
        }
        rest = &after[j + 2..];
    }
    out
}

/// Reemplaza wikilinks por su alias (o nombre base) y quita escapes de corchetes: texto plano legible.
pub fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("[[") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let Some(j) = after.find("]]") else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let inner = after[..j].replace("\\|", "|");
        let link = match inner.split_once('|') {
            Some((t, a)) => WikiLink {
                target: t.into(),
                alias: Some(a.into()),
            },
            None => WikiLink {
                target: inner.clone(),
                alias: None,
            },
        };
        out.push_str(link.alias.as_deref().map(str::trim).unwrap_or(&link.basename()));
        rest = &after[j + 2..];
    }
    out.push_str(rest);
    out.replace("\\[", "[").replace("\\]", "]").trim().to_string()
}

// ---------------------------------------------------------------------------------------------------------------
// Estructura de nota

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub level: usize,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub path: String,
    pub frontmatter: BTreeMap<String, Value>,
    pub body: String,
    pub sections: Vec<Section>,
}

/// Separa frontmatter YAML (convertido a JSON) y cuerpo. YAML inválido → frontmatter vacío + advertencia.
pub fn split_frontmatter(text: &str) -> (BTreeMap<String, Value>, String, Option<String>) {
    let t = text.trim_start_matches('\u{feff}');
    if let Some(rest) = t.strip_prefix("---\n")
        && let Some(end) = rest.find("\n---")
    {
        let yaml = &rest[..end];
        let body = rest[end + 4..]
            .trim_start_matches(['-'])
            .trim_start_matches('\n')
            .to_string();
        return match serde_norway::from_str::<serde_norway::Value>(yaml) {
            Ok(v) => {
                let json = serde_json::to_value(v).unwrap_or(Value::Null);
                let map = match json {
                    Value::Object(m) => m.into_iter().collect(),
                    _ => BTreeMap::new(),
                };
                (map, body, None)
            }
            Err(e) => (BTreeMap::new(), body, Some(format!("frontmatter inválido: {e}"))),
        };
    }
    (BTreeMap::new(), t.to_string(), None)
}

pub fn sections(body: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut in_code = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if !in_code && (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
            out.push(Section {
                level: hashes,
                title: line[hashes..].trim().to_string(),
                content: String::new(),
            });
        } else if let Some(s) = out.last_mut() {
            s.content.push_str(line);
            s.content.push('\n');
        }
    }
    for s in &mut out {
        s.content = strip_comments(&s.content).trim().to_string();
    }
    out
}

fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("<!--") {
        out.push_str(&rest[..i]);
        match rest[i..].find("-->") {
            Some(j) => rest = &rest[i + j + 3..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn parse_note(path: &str, raw: &str) -> (Note, Vec<String>) {
    let text = normalize_text(raw);
    let (frontmatter, body, warn) = split_frontmatter(&text);
    let sections = sections(&body);
    (
        Note {
            path: path.to_string(),
            frontmatter,
            body,
            sections,
        },
        warn.into_iter().collect(),
    )
}

impl Note {
    /// Contenido de la primera sección cuyo título empieza con `prefix` (sin distinguir acentos/mayúsculas).
    pub fn section(&self, prefix: &str) -> Option<&str> {
        let p = fold(prefix);
        self.sections
            .iter()
            .find(|s| fold(&s.title).starts_with(&p))
            .map(|s| s.content.as_str())
    }
    pub fn fm_str(&self, key: &str) -> Option<String> {
        match self.frontmatter.get(key)? {
            Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty() && !is_placeholder(s)),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }
    pub fn fm_bool(&self, key: &str) -> Option<bool> {
        match self.frontmatter.get(key)? {
            Value::Bool(b) => Some(*b),
            Value::String(s) => match s.trim() {
                "true" | "sí" | "si" | "yes" => Some(true),
                "false" | "no" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }
    pub fn fm_num(&self, key: &str) -> Option<f64> {
        match self.frontmatter.get(key)? {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().replace(',', ".").parse().ok(),
            _ => None,
        }
    }
    pub fn fm_date(&self, key: &str) -> Option<NaiveDate> {
        parse_date(&self.fm_str(key)?)
    }
    pub fn fm_list(&self, key: &str) -> Vec<String> {
        match self.frontmatter.get(key) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect(),
            Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
            _ => Vec::new(),
        }
    }
    pub fn stem(&self) -> String {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        name.trim_end_matches(".md").to_string()
    }
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.trim().get(..10)?, "%Y-%m-%d").ok()
}

// ---------------------------------------------------------------------------------------------------------------
// Tablas, listas y checkboxes

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn col(&self, name: &str) -> Option<usize> {
        let n = fold(name);
        self.headers.iter().position(|h| fold(h).starts_with(&n))
    }
    pub fn cell<'a>(&self, row: &'a [String], name: &str) -> &'a str {
        self.col(name)
            .and_then(|i| row.get(i))
            .map(String::as_str)
            .unwrap_or("")
    }
}

/// Separa celdas respetando `\|` escapado (que se conserva como `\|` para que los wikilinks sigan reconocibles).
fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut prev_bs = false;
    for c in t.chars() {
        if c == '|' && !prev_bs {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
        prev_bs = c == '\\';
    }
    cells.push(cur.trim().to_string());
    cells
}

pub fn tables(text: &str) -> Vec<Table> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim();
        let sep = lines.get(i + 1).map(|s| s.trim()).unwrap_or("");
        if l.starts_with('|')
            && sep.starts_with('|')
            && sep.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
        {
            let headers = split_row(l);
            let mut rows = Vec::new();
            i += 2;
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let r = split_row(lines[i]);
                if r.iter().any(|c| !c.is_empty()) {
                    rows.push(r);
                }
                i += 1;
            }
            out.push(Table { headers, rows });
        } else {
            i += 1;
        }
    }
    out
}

/// Celda con `\|` → `|` (para texto plano de la celda).
pub fn unescape_cell(s: &str) -> String {
    s.replace("\\|", "|")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkbox {
    pub checked: bool,
    pub text: String,
}

pub fn checkboxes(text: &str) -> Vec<Checkbox> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim_start();
            let rest = t.strip_prefix("- [").or_else(|| t.strip_prefix("* ["))?;
            let mut ch = rest.chars();
            let mark = ch.next()?;
            let rest = ch.as_str().strip_prefix(']')?.trim();
            if rest.is_empty() || is_placeholder(rest) {
                return None;
            }
            Some(Checkbox {
                checked: matches!(mark, 'x' | 'X'),
                text: rest.to_string(),
            })
        })
        .collect()
}

/// Viñetas `- texto` (no checkboxes), sin vacías ni placeholders.
pub fn bullets(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim_start();
            let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "))?;
            if rest.starts_with('[') && rest.get(2..3) == Some("]") {
                return None;
            }
            let r = rest.trim();
            (!r.is_empty() && !is_placeholder(r)).then(|| r.to_string())
        })
        .collect()
}

/// Lista numerada `1. texto`.
pub fn numbered(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim_start();
            let dot = t.find(". ")?;
            t[..dot]
                .chars()
                .all(|c| c.is_ascii_digit())
                .then(|| t[dot + 2..].trim().to_string())
        })
        .filter(|s| !s.is_empty() && !is_placeholder(s))
        .collect()
}

/// Texto libre de una sección sin comentarios ni líneas vacías de plantilla (`- `).
pub fn prose(text: &str) -> String {
    text.lines()
        .filter(|l| {
            let t = l.trim();
            !(t == "-" || t == "- [ ]" || t == "1." || is_placeholder(t))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn md_link_url(s: &str) -> Option<(String, String)> {
    let i = s.find("](")?;
    // Corchete de apertura no escapado (`[\[Tag\] nombre](url)`).
    let head = &s[..i];
    let start = head
        .char_indices()
        .rev()
        .find(|(j, c)| *c == '[' && !head[..*j].ends_with('\\'))
        .map(|(j, _)| j)?;
    let end = s[i + 2..].find(')')?;
    Some((s[start + 1..i].to_string(), s[i + 2..i + 2 + end].to_string()))
}

pub fn first_url(s: &str) -> Option<String> {
    s.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '<' || c == '>')
        .find(|w| w.starts_with("https://") || w.starts_with("http://"))
        .map(|w| w.trim_end_matches(['.', ',', ']']).to_string())
}

// ---------------------------------------------------------------------------------------------------------------
// Clasificación por ruta

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileKind {
    Daily { date: NaiveDate },
    GoalMonth { period: String },
    GoalWeek { period: String },
    Weekly { week: String },
    Person,
    Reading,
    Content,
    Swipe,
    ClassNote { subject: String },
    Devlog { date: NaiveDate },
    Adr { project: String },
    Expenses { month: String },
    Income { month: String },
    Card,
    Account,
    Subscriptions,
    FixedIncome,
    CreditPlan,
    RecurringTasks,
    Inbox,
    Project { slug: String },
    Note { note_kind: String },
    Ignored { reason: String },
    Unsupported { reason: String },
}

pub fn classify(path: &str) -> FileKind {
    let p = path.trim_start_matches("./");
    // Quitar una carpeta raíz contenedora (ej. "Vault/") si no es una carpeta conocida.
    let known = [
        "Daily/",
        "Proyectos/",
        "Personas/",
        "Lecturas/",
        "Contenido/",
        "Swipe/",
        "Dev/",
        "Weekly/",
        "Templates/",
        "Scripts/",
        "Bases/",
        ".claude/",
        ".obsidian/",
        "Attachments/",
        "Inbox.md",
        "Home.md",
        "AGENTS.md",
        "CLAUDE.md",
        "LEEME",
    ];
    let p = if known.iter().any(|k| p.starts_with(k)) {
        p
    } else {
        match p.split_once('/') {
            Some((_, rest)) if known.iter().any(|k| rest.starts_with(k)) => rest,
            _ => p,
        }
    };
    let ignored = |r: &str| FileKind::Ignored { reason: r.into() };
    let unsupported = |r: &str| FileKind::Unsupported { reason: r.into() };
    let parts: Vec<&str> = p.split('/').collect();
    let file = *parts.last().unwrap_or(&"");
    let stem = file.trim_end_matches(".md");
    if p.starts_with(".obsidian/") || p.starts_with(".trash/") {
        return ignored("configuración de Obsidian");
    }
    if p.starts_with(".claude/") {
        return ignored("comando del vault (especificación, no datos)");
    }
    if p.starts_with("Templates/") {
        return ignored("plantilla");
    }
    if p.starts_with("Scripts/") {
        return ignored("script DataviewJS (no se ejecuta)");
    }
    if p.starts_with("Bases/") || file.ends_with(".base") {
        return ignored("vista Bases (existe como vista en la app)");
    }
    if file.starts_with('.') || file == ".gitkeep" {
        return ignored("archivo oculto");
    }
    if matches!(p, "AGENTS.md" | "CLAUDE.md" | "Home.md") || p.starts_with("LEEME") {
        return ignored("instrucciones/tablero del vault original");
    }
    if file.ends_with(".excalidraw.md") || file.ends_with(".excalidraw") || file.ends_with(".canvas") {
        return unsupported("dibujo de Obsidian: se conserva como referencia");
    }
    if !file.ends_with(".md") {
        return unsupported("archivo no Markdown: se conserva como referencia");
    }
    if p == "Inbox.md" {
        return FileKind::Inbox;
    }
    match parts.as_slice() {
        ["Daily", _] => parse_date(stem)
            .map(|date| FileKind::Daily { date })
            .unwrap_or(FileKind::Note {
                note_kind: "note".into(),
            }),
        ["Weekly", _] if stem.contains("-W") => FileKind::Weekly {
            week: stem.to_string(),
        },
        ["Personas", _] => FileKind::Person,
        ["Lecturas", _] => FileKind::Reading,
        ["Contenido", _] => FileKind::Content,
        ["Swipe", _] => FileKind::Swipe,
        ["Dev", "devlog", _] => {
            parse_date(stem)
                .map(|date| FileKind::Devlog { date })
                .unwrap_or(FileKind::Note {
                    note_kind: "note".into(),
                })
        }
        ["Dev", "til", ..] => FileKind::Note {
            note_kind: "til".into(),
        },
        ["Dev", "snippets", ..] => FileKind::Note {
            note_kind: "snippet".into(),
        },
        // Dane ya no tiene devocionales: esas notas se omiten en vez de importarse como notas sueltas.
        ["Proyectos", "personal", "Devocionales", ..] => unsupported("los devocionales ya no se importan"),
        ["Proyectos", "personal", "Objetivos", _] => {
            if let Some(m) = stem.strip_prefix("Mes ") {
                FileKind::GoalMonth {
                    period: m.trim().into(),
                }
            } else if let Some(w) = stem.strip_prefix("Semana ") {
                FileKind::GoalWeek {
                    period: w.trim().into(),
                }
            } else {
                FileKind::Note {
                    note_kind: "note".into(),
                }
            }
        }
        ["Proyectos", "personal", "Finanzas", "Gastos", _] => stem
            .strip_prefix("Gastos ")
            .map(|m| FileKind::Expenses {
                month: m.trim().into(),
            })
            .unwrap_or(ignored("nota de finanzas sin mes")),
        ["Proyectos", "personal", "Finanzas", "Ingresos", _] => stem
            .strip_prefix("Ingresos ")
            .map(|m| FileKind::Income {
                month: m.trim().into(),
            })
            .unwrap_or(ignored("nota de finanzas sin mes")),
        ["Proyectos", "personal", "Finanzas", "Tarjetas", _] => FileKind::Card,
        ["Proyectos", "personal", "Finanzas", "Cuentas", _] => FileKind::Account,
        ["Proyectos", "personal", "Finanzas", "Suscripciones.md"] => FileKind::Subscriptions,
        ["Proyectos", "personal", "Finanzas", "Ingresos fijos.md"] => FileKind::FixedIncome,
        ["Proyectos", "personal", "Finanzas", "Plan de crédito.md"] => FileKind::CreditPlan,
        [
            "Proyectos",
            "personal",
            "Finanzas",
            "Dashboard.md" | "Tarjetas de crédito.md",
        ] => ignored("tablero calculado (la app lo calcula)"),
        ["Proyectos", "personal", "Pendientes recurrentes.md"] => FileKind::RecurringTasks,
        ["Proyectos", _, "Clases", subject, _] => FileKind::ClassNote {
            subject: (*subject).to_string(),
        },
        ["Proyectos", project, "adr", _] => FileKind::Adr {
            project: (*project).to_string(),
        },
        ["Proyectos", project, f] if f.trim_end_matches(".md") == *project => FileKind::Project {
            slug: (*project).to_string(),
        },
        ["Proyectos", _, f] if fold(f).contains("runbook") => FileKind::Note {
            note_kind: "runbook".into(),
        },
        ["Attachments", ..] => unsupported("adjunto: se conserva como referencia"),
        _ => FileKind::Note {
            note_kind: "note".into(),
        },
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Registros tipados

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DailyImport {
    pub date: NaiveDate,
    pub weight_kg: Option<f64>,
    pub energy: Option<i64>,
    /// Solo `true` explícito (D-026: el `false` de la plantilla no equivale a "no").
    pub fixed_done: Vec<String>,
    pub fixed_false_ignored: usize,
    pub planned_spend: Option<String>,
    pub agenda: Vec<String>,
    pub month_habits: Vec<Checkbox>,
    pub week_habits: Vec<Checkbox>,
    pub log: Vec<String>,
    pub people: Vec<String>,
    /// (área en minúsculas, checkbox)
    pub tasks: Vec<(String, Checkbox)>,
    pub good: Vec<String>,
    pub bad: Vec<String>,
}

pub fn daily(note: &Note, date: NaiveDate) -> DailyImport {
    let mut fixed_done = Vec::new();
    let mut fixed_false_ignored = 0;
    for (key, habit) in [("ejercicio", "exercise"), ("lectura", "reading")] {
        match note.fm_bool(key) {
            Some(true) => fixed_done.push(habit.to_string()),
            Some(false) => fixed_false_ignored += 1,
            None => {}
        }
    }
    let mut tasks = Vec::new();
    let mut area: Option<String> = None;
    let mut in_pending = false;
    for s in &note.sections {
        if s.level == 2 {
            in_pending = fold(&s.title).starts_with("pendientes");
            area = None;
            if in_pending {
                for c in checkboxes(&s.content) {
                    tasks.push(("personal".to_string(), c));
                }
            }
            continue;
        }
        if in_pending && s.level >= 3 {
            area = Some(fold(&s.title));
            for c in checkboxes(&s.content) {
                tasks.push((area.clone().unwrap_or_default(), c));
            }
        }
    }
    let _ = area;
    let gastos = note.section("Gastos de hoy").unwrap_or("");
    let planned_spend = gastos
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("- Planeo gastar:")
                .map(|v| v.trim().to_string())
        })
        .filter(|v| !v.is_empty() && !is_placeholder(v));
    let only_items = |sec: &str| -> Vec<String> {
        note.section(sec)
            .map(bullets)
            .unwrap_or_default()
            .into_iter()
            .filter(|b| !b.starts_with("Respecto a mis"))
            .collect()
    };
    DailyImport {
        date,
        weight_kg: note.fm_num("peso"),
        energy: note
            .fm_num("energia")
            .map(|e| e as i64)
            .filter(|e| (1..=5).contains(e)),
        fixed_done,
        fixed_false_ignored,
        planned_spend,
        agenda: note.section("Agenda").map(bullets).unwrap_or_default(),
        month_habits: note
            .section("Hábitos del mes")
            .map(checkboxes)
            .unwrap_or_default(),
        week_habits: note
            .section("Hábitos de la semana")
            .map(checkboxes)
            .unwrap_or_default(),
        log: only_items("Log"),
        people: only_items("Personas"),
        tasks,
        good: only_items("Lo que hice bien"),
        bad: only_items("Lo que hice mal"),
    }
}

/// `- 10:00–11:00 Título` → (inicio, fin, título).
pub fn agenda_line(s: &str) -> Option<(String, Option<String>, String)> {
    let t = s.trim();
    let (times, title) = t.split_once(' ')?;
    let mut parts = times.split(['–', '-']);
    let a = parts.next()?;
    let ok = |x: &str| x.len() == 5 && x.as_bytes()[2] == b':';
    if !ok(a) {
        return None;
    }
    let b = parts.next().filter(|x| ok(x)).map(str::to_string);
    Some((a.to_string(), b, title.trim().to_string()))
}

/// Hábito de la daily: `Texto del hábito · objetivo corto` → texto del hábito.
pub fn habit_name(checkbox_text: &str) -> String {
    plain(checkbox_text.split(" · ").next().unwrap_or(checkbox_text))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalImport {
    pub title: String,
    pub area: Option<String>,
    pub motivation: String,
    pub success: String,
    pub status: String,
    pub supports: Option<String>,
    pub habits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GoalsNoteImport {
    pub kind: String,
    pub period: String,
    pub starts_on: Option<NaiveDate>,
    pub ends_on: Option<NaiveDate>,
    pub goals: Vec<GoalImport>,
    pub closing: String,
}

pub fn map_goal_status(s: &str) -> String {
    match fold(s.trim()).as_str() {
        "logrado" => "achieved",
        "no logrado" => "not_achieved",
        _ => "active",
    }
    .into()
}

pub fn goals_note(note: &Note, kind: &str, period: &str) -> GoalsNoteImport {
    let mut goals = Vec::new();
    let mut cur: Option<GoalImport> = None;
    let mut in_habits = false;
    let mut in_goals_area = false;
    let field = |content: &str, label: &str| -> String {
        content
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(&format!("- **{label}:**"))
                    .map(|v| v.trim().to_string())
            })
            .filter(|v| !is_placeholder(v))
            .unwrap_or_default()
    };
    for s in &note.sections {
        if s.level == 2 {
            in_goals_area = fold(&s.title).starts_with("objetivos");
            if let Some(g) = cur.take() {
                goals.push(g);
            }
            in_habits = false;
            continue;
        }
        if in_goals_area && s.level == 3 {
            if let Some(g) = cur.take() {
                goals.push(g);
            }
            let title = s
                .title
                .split_once(". ")
                .map(|(n, t)| {
                    if n.chars().all(|c| c.is_ascii_digit()) {
                        t
                    } else {
                        s.title.as_str()
                    }
                })
                .unwrap_or(&s.title);
            let title = plain(title);
            if title.is_empty() || is_placeholder(&title) {
                cur = None;
                continue;
            }
            let area_raw = field(&s.content, "Área");
            cur = Some(GoalImport {
                title,
                area: wikilinks(&area_raw)
                    .first()
                    .map(|l| l.basename())
                    .or_else(|| Some(plain(&area_raw)).filter(|a| !a.is_empty())),
                motivation: plain(&field(&s.content, "Por qué importa")),
                success: plain(&field(&s.content, "Cómo sé que lo logré")),
                status: map_goal_status(&field(&s.content, "Estado")),
                supports: Some(plain(&field(&s.content, "Apoya a (mes)"))).filter(|x| !x.is_empty()),
                habits: Vec::new(),
            });
            in_habits = false;
            continue;
        }
        if in_goals_area && s.level == 4 && fold(&s.title).starts_with("habitos") {
            in_habits = true;
            if let Some(g) = cur.as_mut() {
                g.habits
                    .extend(checkboxes(&s.content).into_iter().map(|c| plain(&c.text)));
            }
        } else if in_habits {
            in_habits = false;
        }
    }
    if let Some(g) = cur.take() {
        goals.push(g);
    }
    GoalsNoteImport {
        kind: kind.into(),
        period: period.into(),
        starts_on: note.fm_date("inicio"),
        ends_on: note.fm_date("fin"),
        goals,
        closing: note.section("Cierre").map(bullets).unwrap_or_default().join("\n"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeeklyImport {
    pub week: String,
    pub peace: Option<i64>,
    pub what_happened: String,
    pub what_didnt: String,
    pub content: String,
    pub habits: String,
    pub goals: String,
    pub areas: String,
    pub people: String,
    pub priorities: Vec<String>,
}

pub fn weekly(note: &Note, week: &str) -> WeeklyImport {
    let sec = |p: &str| {
        note.section(p)
            .map(prose)
            .unwrap_or_default()
            .replace("- Devocional: /7 · Ejercicio: /4 (meta) · Lectura: /7", "")
            .trim()
            .to_string()
    };
    WeeklyImport {
        week: note.fm_str("semana").unwrap_or_else(|| week.to_string()),
        peace: note
            .fm_num("paz")
            .map(|p| p as i64)
            .filter(|p| (1..=5).contains(p)),
        what_happened: sec("Qué pasó"),
        what_didnt: sec("Qué no pasó"),
        content: sec("Contenido publicado"),
        habits: sec("Hábitos y devocional"),
        goals: sec("Objetivos de la semana"),
        areas: sec("Proyectos"),
        people: sec("Personas con las que hablé"),
        priorities: note
            .section("Prioridades")
            .map(numbered)
            .unwrap_or_default()
            .into_iter()
            .take(3)
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersonImport {
    pub name: String,
    pub aliases: Vec<String>,
    pub area: Option<String>,
    pub relationship: Option<String>,
    pub org: Option<String>,
    pub role: Option<String>,
    pub contact: Option<String>,
    pub email: Option<String>,
    pub last_contact: Option<NaiveDate>,
    /// (mes, día, año opcional)
    pub birthday: Option<(u32, u32, Option<i32>)>,
    pub context: String,
    pub notes: Vec<(Option<NaiveDate>, String)>,
}

pub fn parse_birthday(s: &str) -> Option<(u32, u32, Option<i32>)> {
    let t = s.trim();
    if let Some(d) = parse_date(t) {
        use chrono::Datelike;
        return Some((d.month(), d.day(), Some(d.year())));
    }
    let (m, d) = t.trim_start_matches("--").split_once('-')?;
    let (m, d): (u32, u32) = (m.parse().ok()?, d.parse().ok()?);
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some((m, d, None))
}

pub fn person(note: &Note) -> PersonImport {
    const RELS: [&str; 8] = [
        "jefe",
        "subordinado",
        "compañero",
        "cliente",
        "familia",
        "pareja",
        "amigo",
        "otro",
    ];
    let rel = note
        .fm_str("relacion")
        .map(|r| fold(&r))
        .and_then(|r| RELS.iter().find(|x| fold(x) == r).map(|x| x.to_string()));
    let contact = note.fm_str("contacto");
    let email = contact
        .as_deref()
        .filter(|c| c.contains('@') && !c.contains(' '))
        .map(str::to_lowercase);
    let notes = note
        .section("Notas")
        .map(bullets)
        .unwrap_or_default()
        .into_iter()
        .map(|b| match b.split_once(':') {
            Some((d, rest)) if parse_date(d).is_some() => (parse_date(d), plain(rest)),
            _ => (None, plain(&b)),
        })
        .filter(|(_, t)| !t.is_empty())
        .collect();
    let context: Vec<String> = note
        .section("Contexto")
        .map(bullets)
        .unwrap_or_default()
        .into_iter()
        .filter(|b| b.trim_end() != "Quién es para el usuario:")
        .collect();
    PersonImport {
        name: note.stem(),
        aliases: note.fm_list("aliases"),
        area: note
            .fm_str("area")
            .and_then(|a| wikilinks(&a).first().map(|l| l.basename()).or(Some(a))),
        relationship: rel,
        org: note.fm_str("org"),
        role: note.fm_str("rol"),
        contact,
        email,
        last_contact: note.fm_date("ultimo_contacto"),
        birthday: note.fm_str("cumpleaños").and_then(|b| parse_birthday(&b)),
        context: context.join("\n"),
        notes,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingImport {
    pub title: String,
    pub kind: String,
    pub author: String,
    pub status: String,
    pub rating: Option<i64>,
    pub started_on: Option<NaiveDate>,
    pub finished_on: Option<NaiveDate>,
    pub ideas: String,
    pub quotes: String,
    pub application: String,
}

pub fn reading(note: &Note) -> ReadingImport {
    let status = match fold(&note.fm_str("estado").unwrap_or_default()).as_str() {
        "leido" | "terminado" | "finished" | "leida" => "finished",
        "abandonado" | "abandonada" => "abandoned",
        "pendiente" | "por leer" | "quiero leer" | "want" => "want",
        _ => "reading",
    };
    ReadingImport {
        title: note.stem(),
        kind: if note.fm_str("tipo").as_deref() == Some("articulo")
            || note.fm_str("tipo").as_deref() == Some("artículo")
        {
            "article".into()
        } else {
            "book".into()
        },
        author: note.fm_str("autor").unwrap_or_default(),
        status: status.into(),
        rating: note
            .fm_num("rating")
            .map(|r| r as i64)
            .filter(|r| (1..=5).contains(r)),
        started_on: note.fm_date("inicio"),
        finished_on: note.fm_date("fin"),
        ideas: note.section("Ideas clave").map(prose).unwrap_or_default(),
        quotes: note.section("Citas").map(prose).unwrap_or_default(),
        application: note.section("Qué voy a aplicar").map(prose).unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContentImport {
    pub title: String,
    pub status: String,
    pub format: String,
    pub platforms: Vec<String>,
    pub pillar: Option<String>,
    pub idea: String,
    pub hooks: Vec<String>,
    pub chosen_hook: Option<usize>,
    pub script_hook: String,
    pub script_body: String,
    pub script_cta: String,
    pub shot_list: Vec<Value>,
    pub edit_notes: String,
    pub result_worked: String,
    pub result_change: String,
    pub publish_on: Option<NaiveDate>,
    pub raw_path: Option<String>,
    pub render_path: Option<String>,
    pub views: Option<i64>,
    pub retention_pct: Option<f64>,
}

pub fn content(note: &Note) -> ContentImport {
    let status = match note.fm_str("estado").as_deref() {
        Some("guion") => "script",
        Some("grabado") => "recorded",
        Some("editado") => "edited",
        Some("publicado") => "published",
        _ => "idea",
    };
    let hooks: Vec<String> = note
        .section("Hooks")
        .map(numbered)
        .unwrap_or_default()
        .into_iter()
        .take(10)
        .collect();
    let fm_hook = note.fm_str("hook");
    let chosen_hook = fm_hook
        .as_ref()
        .and_then(|h| hooks.iter().position(|x| fold(x) == fold(h)));
    let guion = note.section("Guion").unwrap_or("");
    let script_part = |label: &str| {
        guion
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(&format!("**{label}"))
                    .map(|r| r.split_once("**").map(|x| x.1).unwrap_or(r).trim().to_string())
            })
            .unwrap_or_default()
    };
    let shot_list = note
        .section("Shot list")
        .map(tables)
        .unwrap_or_default()
        .into_iter()
        .flat_map(|t| {
            t.rows
                .iter()
                .map(|r| serde_json::json!({ "n": t.cell(r, "#"), "shot": unescape_cell(t.cell(r, "Plano")), "text": unescape_cell(t.cell(r, "Texto")), "motion": unescape_cell(t.cell(r, "Motion")) }))
                .collect::<Vec<_>>()
        })
        .filter(|v| v["shot"].as_str().is_some_and(|s| !s.is_empty()) || v["text"].as_str().is_some_and(|s| !s.is_empty()))
        .collect();
    let result = note.section("Resultado").unwrap_or("");
    let res = |label: &str| {
        result
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(&format!("- {label}"))
                    .map(|v| v.trim().to_string())
            })
            .unwrap_or_default()
    };
    let mut hooks = hooks;
    if let (Some(h), None) = (&fm_hook, chosen_hook)
        && hooks.len() < 10
    {
        hooks.push(h.clone());
    }
    let chosen_hook = fm_hook
        .as_ref()
        .and_then(|h| hooks.iter().position(|x| fold(x) == fold(h)));
    ContentImport {
        title: note.stem(),
        status: status.into(),
        format: note.fm_str("formato").unwrap_or_else(|| "reel".into()),
        platforms: note.fm_list("plataforma"),
        pillar: note.fm_str("pilar"),
        idea: note.section("Idea en una frase").map(prose).unwrap_or_default(),
        hooks,
        chosen_hook,
        script_hook: script_part("Hook"),
        script_body: script_part("Desarrollo"),
        script_cta: script_part("Cierre"),
        shot_list,
        edit_notes: note
            .section("Notas de edición")
            .map(|s| bullets(s).join("\n"))
            .unwrap_or_default(),
        result_worked: res("Lo que funcionó:"),
        result_change: res("Lo que cambiaría:"),
        publish_on: note.fm_date("fecha_pub"),
        raw_path: note.fm_str("raw_path"),
        render_path: note.fm_str("render_path"),
        views: note.fm_num("views").map(|v| v as i64).filter(|v| *v >= 0),
        retention_pct: note.fm_num("retencion").filter(|r| (0.0..=100.0).contains(r)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SwipeImport {
    pub title: String,
    pub url: Option<String>,
    pub why: String,
    pub hook: String,
    pub creator: Option<String>,
}

pub fn swipe(note: &Note) -> SwipeImport {
    let url = note.fm_str("url").or_else(|| first_url(&note.body));
    SwipeImport {
        title: note.stem(),
        url,
        why: note.section("Por qué funciona").map(prose).unwrap_or_default(),
        hook: note
            .fm_str("hook")
            .unwrap_or_else(|| note.section("Hook").map(prose).unwrap_or_default()),
        creator: note.fm_str("creador").or_else(|| note.fm_str("creator")),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClassImport {
    pub subject: String,
    pub date: Option<NaiveDate>,
    pub topic: String,
    pub topics: String,
    pub questions: String,
    pub deliverables: Vec<Checkbox>,
}

pub fn class_note(note: &Note, subject: &str) -> ClassImport {
    let stem = note.stem();
    let (date, topic) = match stem.split_once(' ') {
        Some((d, t)) if parse_date(d).is_some() => (parse_date(d), t.to_string()),
        _ => (None, stem.clone()),
    };
    ClassImport {
        subject: note
            .fm_str("materia")
            .map(|m| plain(&m))
            .unwrap_or_else(|| subject.to_string()),
        date: note.fm_date("fecha").or(date),
        topic,
        topics: note.section("Temas").map(prose).unwrap_or_default(),
        questions: note.section("Dudas").map(prose).unwrap_or_default(),
        deliverables: note.section("Tareas").map(checkboxes).unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DevlogSessionImport {
    pub repo: String,
    pub at_time: Option<String>,
    pub changes: String,
    pub bugs: String,
    pub decisions: String,
    pub next_steps: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DevlogImport {
    pub date: NaiveDate,
    pub projects: Vec<String>,
    pub sessions: Vec<DevlogSessionImport>,
}

pub fn devlog(note: &Note, date: NaiveDate) -> DevlogImport {
    let projects = note
        .fm_list("proyectos")
        .iter()
        .filter_map(|p| wikilinks(p).first().map(|l| l.basename()))
        .collect();
    let mut sessions = Vec::new();
    for s in note.sections.iter().filter(|s| s.level == 3) {
        let (repo, time) = match s.title.split_once(" — ").or_else(|| s.title.split_once(" - ")) {
            Some((r, t)) => (r.trim().to_string(), Some(t.trim().to_string())),
            None => (s.title.clone(), None),
        };
        sessions.push(DevlogSessionImport {
            repo,
            at_time: time,
            changes: prose(&s.content),
            bugs: String::new(),
            decisions: String::new(),
            next_steps: String::new(),
        });
    }
    let whole = |p: &str| note.section(p).map(prose).unwrap_or_default();
    let (changes, bugs, decisions, next) = (
        whole("Qué hice"),
        whole("Bloqueos"),
        whole("Decisiones"),
        whole("Siguiente"),
    );
    if sessions.is_empty()
        && !(changes.is_empty() && bugs.is_empty() && decisions.is_empty() && next.is_empty())
    {
        sessions.push(DevlogSessionImport {
            repo: String::new(),
            at_time: None,
            changes,
            bugs,
            decisions,
            next_steps: next,
        });
    }
    DevlogImport {
        date,
        projects,
        sessions,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdrImport {
    pub project: String,
    pub number: Option<i32>,
    pub title: String,
    pub status: String,
    pub date: Option<NaiveDate>,
    pub context: String,
    pub decision: String,
    pub alternatives: String,
    pub consequences: String,
}

pub fn adr(note: &Note, project_folder: &str) -> AdrImport {
    let stem = note.stem();
    let (number, title) = match stem.strip_prefix("ADR-").and_then(|r| r.split_once(' ')) {
        Some((n, t)) => (n.parse().ok(), t.trim().to_string()),
        None => (None, stem.clone()),
    };
    let status = match note.fm_str("estado").map(|s| fold(&s)).as_deref() {
        Some("aceptada") => "accepted",
        Some("reemplazada") => "superseded",
        _ => "proposed",
    };
    let project = note
        .fm_str("proyecto")
        .and_then(|p| wikilinks(&p).first().map(|l| l.basename()))
        .unwrap_or_else(|| project_folder.to_string());
    AdrImport {
        project,
        number,
        title,
        status: status.into(),
        date: note.fm_date("fecha"),
        context: note.section("Contexto").map(prose).unwrap_or_default(),
        decision: note.section("Decisión").map(prose).unwrap_or_default(),
        alternatives: note.section("Alternativas").map(prose).unwrap_or_default(),
        consequences: note.section("Consecuencias").map(prose).unwrap_or_default(),
    }
}

/// Medio de pago/cuenta de una celda del vault (C4: se resuelve por nombre con advertencia).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentRef {
    Card { name: String },
    Bank { kind: String, bank: String },
    Cash,
    Borrowed { person: String },
    Other { text: String },
    None,
}

pub fn payment_ref(cell: &str) -> PaymentRef {
    let t = cell.trim();
    if t.is_empty() || is_placeholder(t) {
        return PaymentRef::None;
    }
    let low = fold(t);
    if low.starts_with("prestado") {
        let person = wikilinks(t)
            .first()
            .map(|l| l.basename())
            .unwrap_or_else(|| plain(t.trim_start_matches(|c: char| c != '(').trim_matches(['(', ')'])));
        return PaymentRef::Borrowed { person };
    }
    if let Some(l) = wikilinks(t)
        .into_iter()
        .find(|l| fold(&l.basename()).starts_with("tarjeta"))
    {
        return PaymentRef::Card { name: l.basename() };
    }
    if low == "efectivo" {
        return PaymentRef::Cash;
    }
    for kind in ["debito", "transferencia"] {
        if let Some(rest) = low.strip_prefix(kind) {
            let bank = plain(&t[t.len() - rest.len()..]).trim().to_string();
            return PaymentRef::Bank {
                kind: kind.into(),
                bank,
            };
        }
    }
    PaymentRef::Other { text: plain(t) }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TxRow {
    pub date: NaiveDate,
    pub concept: String,
    /// Concepto con wikilinks intactos (para detectar `[[Tarjeta X]]` y proyectos).
    pub concept_raw: String,
    pub amount_cents: i64,
    pub category: String,
    pub payment: PaymentRef,
    /// Índice de ocurrencia entre filas idénticas (fecha, concepto, monto): compras iguales legítimas no se fusionan.
    pub occurrence: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MoneyNoteImport {
    pub month: String,
    pub rows: Vec<TxRow>,
    pub budget: Vec<(String, i64)>,
    pub warnings: Vec<String>,
}

/// Tabla de gastos (`| Fecha | Concepto | Monto | Categoría | Pago |`) o ingresos (`… | Cuenta |`).
pub fn money_note(note: &Note, month: &str, account_col: &str) -> MoneyNoteImport {
    let mut out = MoneyNoteImport {
        month: month.into(),
        ..Default::default()
    };
    let registro = note.section("Registro").unwrap_or(&note.body);
    let mut seen: BTreeMap<(NaiveDate, String, i64), usize> = BTreeMap::new();
    for t in tables(registro) {
        if t.col("Fecha").is_none() || t.col("Monto").is_none() {
            continue;
        }
        for r in &t.rows {
            let date_s = t.cell(r, "Fecha");
            let Some(date) = parse_date(date_s) else {
                out.warnings
                    .push(format!("fila sin fecha válida en {}: '{date_s}'", note.path));
                continue;
            };
            let concept_raw = unescape_cell(t.cell(r, "Concepto"));
            let concept = plain(&concept_raw);
            let amount_s = t.cell(r, "Monto");
            let amount = match parse_amount_to_cents(amount_s) {
                Ok(a) if a > 0 => a,
                _ => {
                    out.warnings.push(format!(
                        "monto inválido en {} ({date}, {concept}): '{amount_s}'",
                        note.path
                    ));
                    continue;
                }
            };
            let key = (date, fold(&concept), amount);
            let occ = seen.entry(key).or_insert(0);
            out.rows.push(TxRow {
                date,
                concept,
                concept_raw: concept_raw.clone(),
                amount_cents: amount,
                category: plain(&unescape_cell(t.cell(r, "Categor"))),
                payment: payment_ref(t.cell(r, account_col)),
                occurrence: *occ,
            });
            *occ += 1;
        }
    }
    if let Some(p) = note.section("Presupuesto") {
        for t in tables(p) {
            for r in &t.rows {
                let cat = plain(t.cell(r, "Categor"));
                if let Ok(c) = parse_amount_to_cents(t.cell(r, "Presupuesto"))
                    && !cat.is_empty()
                    && c > 0
                {
                    out.budget.push((cat, c));
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CardImport {
    pub name: String,
    pub bank: String,
    pub limit_cents: Option<i64>,
    pub available_cents: Option<i64>,
    pub updated: Option<NaiveDate>,
    pub cut_day: Option<i16>,
    pub pay_day: Option<i16>,
    pub status: String,
    pub notes: String,
}

fn cents(note: &Note, key: &str) -> Option<i64> {
    let s = note.fm_str(key)?;
    parse_amount_to_cents(&s).ok()
}

pub fn card(note: &Note) -> CardImport {
    let status = match fold(&note.fm_str("estado").unwrap_or_default()).as_str() {
        "cancelada" => "cancelled",
        "por cancelar" | "cancelar" => "to_cancel",
        _ => "active",
    };
    CardImport {
        name: note.stem(),
        bank: note.fm_str("banco").unwrap_or_default(),
        limit_cents: cents(note, "limite"),
        available_cents: cents(note, "disponible"),
        updated: note.fm_date("actualizado"),
        cut_day: note
            .fm_num("dia_corte")
            .map(|d| d as i16)
            .filter(|d| (1..=31).contains(d)),
        pay_day: note
            .fm_num("dia_pago")
            .map(|d| d as i16)
            .filter(|d| (1..=31).contains(d)),
        status: status.into(),
        notes: note
            .section("Notas")
            .map(|s| bullets(s).join("\n"))
            .unwrap_or_default(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountImport {
    pub name: String,
    pub bank: String,
    pub balance_cents: Option<i64>,
    pub updated: Option<NaiveDate>,
    pub status: String,
    pub notes: String,
}

pub fn account(note: &Note) -> AccountImport {
    let status = if fold(&note.fm_str("estado").unwrap_or_default()) == "cerrada" {
        "closed"
    } else {
        "active"
    };
    AccountImport {
        name: note.stem(),
        bank: note
            .fm_str("banco")
            .unwrap_or_else(|| note.stem().trim_start_matches("Cuenta ").to_string()),
        balance_cents: note.fm_str("saldo").and_then(|s| parse_signed_cents(&s)),
        updated: note.fm_date("actualizado"),
        status: status.into(),
        notes: note
            .section("Notas")
            .map(|s| bullets(s).join("\n"))
            .unwrap_or_default(),
    }
}

fn parse_signed_cents(s: &str) -> Option<i64> {
    let t = s.trim();
    match t.strip_prefix('-').or_else(|| t.strip_prefix('−')) {
        Some(r) => parse_amount_to_cents(r).ok().map(|c| -c),
        None => parse_amount_to_cents(t).ok(),
    }
}

/// Regla recurrente (suscripción o ingreso fijo).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecurringImport {
    pub name: String,
    pub amount_cents: i64,
    pub day: crate::time::DayOfMonth,
    pub category: String,
    pub account: PaymentRef,
    pub area: Option<String>,
    pub active: bool,
}

pub fn day_spec(s: &str) -> Option<crate::time::DayOfMonth> {
    match fold(s.trim()).as_str() {
        "ultimo" => Some(crate::time::DayOfMonth::Last),
        "penultimo" => Some(crate::time::DayOfMonth::Penultimate),
        n => n
            .parse::<u32>()
            .ok()
            .filter(|d| (1..=31).contains(d))
            .map(crate::time::DayOfMonth::Day),
    }
}

pub fn recurring_list(note: &Note, subscriptions: bool) -> (Vec<RecurringImport>, Vec<String>) {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let list = note.section("Lista").unwrap_or(&note.body);
    for t in tables(list) {
        for r in &t.rows {
            let name = plain(t.cell(r, if subscriptions { "Servicio" } else { "Fuente" }));
            if name.is_empty() {
                continue;
            }
            let Ok(amount) = parse_amount_to_cents(t.cell(r, "Monto")) else {
                warnings.push(format!("{name}: monto inválido"));
                continue;
            };
            let Some(day) = day_spec(t.cell(r, "Día")) else {
                warnings.push(format!("{name}: día inválido"));
                continue;
            };
            let status = fold(t.cell(r, "Estado"));
            out.push(RecurringImport {
                name,
                amount_cents: amount,
                day,
                category: plain(t.cell(r, "Categor")),
                account: payment_ref(t.cell(r, if subscriptions { "Pago" } else { "Cuenta" })),
                area: wikilinks(t.cell(r, "Área")).first().map(|l| l.basename()),
                active: matches!(status.as_str(), "activa" | "activo" | ""),
            });
        }
    }
    (out, warnings)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecurringTaskImport {
    pub title: String,
    pub day: crate::time::DayOfMonth,
    pub area: Option<String>,
    pub active: bool,
}

pub fn recurring_tasks(note: &Note) -> Vec<RecurringTaskImport> {
    let list = note.section("Lista").unwrap_or(&note.body);
    tables(list)
        .into_iter()
        .flat_map(|t| {
            t.rows
                .iter()
                .filter_map(|r| {
                    let title = plain(t.cell(r, "Pendiente"));
                    let day = day_spec(t.cell(r, "Día"))?;
                    (!title.is_empty()).then(|| RecurringTaskImport {
                        title,
                        day,
                        area: Some(fold(&plain(t.cell(r, "Área")))).filter(|a| !a.is_empty()),
                        active: !matches!(
                            fold(t.cell(r, "Estado")).as_str(),
                            "inactivo" | "pausado" | "terminado"
                        ),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectImport {
    pub slug: String,
    pub description: String,
    pub status: String,
    pub repo: Option<String>,
    pub links: Vec<(String, String)>,
    pub budget_items: Vec<(String, i64, Vec<String>)>,
}

pub fn project(note: &Note, slug: &str) -> ProjectImport {
    let links = note
        .section("Enlaces")
        .map(bullets)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|b| md_link_url(&b).or_else(|| first_url(&b).map(|u| (u.clone(), u))))
        .collect();
    let mut budget_items = Vec::new();
    if let Some(p) = note.section("Presupuesto") {
        for t in tables(p) {
            for r in &t.rows {
                let name = plain(t.cell(r, "Rubro"));
                if let Ok(c) = parse_amount_to_cents(t.cell(r, "Presupuesto")) {
                    let kws = t
                        .cell(r, "Palabras")
                        .split(',')
                        .map(|k| k.trim().to_string())
                        .filter(|k| !k.is_empty())
                        .collect();
                    if !name.is_empty() {
                        budget_items.push((name, c, kws));
                    }
                }
            }
        }
    }
    let status = match fold(&note.fm_str("estado").unwrap_or_default()).as_str() {
        "pausado" => "paused",
        "terminado" | "hecho" => "done",
        "archivado" => "archived",
        _ => "active",
    };
    ProjectImport {
        slug: slug.to_string(),
        description: note.section("Qué es").map(prose).unwrap_or_default(),
        status: status.into(),
        repo: note.fm_str("repo").filter(|r| r.starts_with("http")),
        links,
        budget_items,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreditPlanImport {
    pub goal: String,
    pub steps: Vec<Checkbox>,
    pub rules: Vec<String>,
}

pub fn credit_plan(note: &Note) -> CreditPlanImport {
    let goal = note
        .fm_str("meta")
        .or_else(|| {
            note.body
                .lines()
                .find_map(|l| l.trim().strip_prefix("Meta:").map(|m| m.trim().to_string()))
                .filter(|m| !is_placeholder(m))
        })
        .unwrap_or_default();
    CreditPlanImport {
        goal,
        steps: note.section("Pasos").map(checkboxes).unwrap_or_default(),
        rules: note
            .section("Reglas")
            .map(|s| bullets(s).into_iter().map(|b| b.replace("**", "")).collect())
            .unwrap_or_default(),
    }
}

/// Elemento de Inbox (viñetas no vacías).
pub fn inbox(note: &Note) -> Vec<String> {
    bullets(&note.body)
}

/// Enlace a ClickUp en un pendiente: `[nombre](https://app.clickup.com/t/abc)` → (nombre, url, id).
pub fn clickup_link(s: &str) -> Option<(String, String, String)> {
    let (name, url) = md_link_url(s)?;
    if !url.contains("clickup.com") {
        return None;
    }
    let id = url.trim_end_matches('/').rsplit('/').next()?.to_string();
    Some((plain(&name.replace("\\[", "[").replace("\\]", "]")), url, id))
}

/// Pendiente consolidado a través de varias dailies: el arrastre del vault copiaba el mismo pendiente cada día;
/// en la app es **una** tarea con historial de arrastres.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskImport {
    pub key: String,
    pub title: String,
    pub area: String,
    pub first_date: NaiveDate,
    pub carried_to: Vec<NaiveDate>,
    pub done_on: Option<NaiveDate>,
    pub external: Option<(String, String)>,
    pub clickup_flag: bool,
}

pub fn consolidate_tasks(dailies: &[DailyImport]) -> Vec<TaskImport> {
    let mut sorted: Vec<&DailyImport> = dailies.iter().collect();
    sorted.sort_by_key(|d| d.date);
    let mut out: Vec<TaskImport> = Vec::new();
    let mut open: BTreeMap<String, usize> = BTreeMap::new();
    for d in sorted {
        for (area, cb) in &d.tasks {
            let external = clickup_link(&cb.text);
            let title = match &external {
                Some((name, _, _)) => name.clone(),
                None => plain(cb.text.split(" · ").next().unwrap_or(&cb.text))
                    .replace("#clickup", "")
                    .trim()
                    .to_string(),
            };
            if title.is_empty() {
                continue;
            }
            let key = match &external {
                Some((_, _, id)) => format!("clickup:{id}"),
                None => format!("{}:{}", area, fold(&title)),
            };
            match open.get(&key).copied() {
                Some(i) => {
                    let t = &mut out[i];
                    if t.carried_to.last() != Some(&d.date) && t.first_date != d.date {
                        t.carried_to.push(d.date);
                    }
                    if cb.checked {
                        t.done_on = Some(d.date);
                        open.remove(&key);
                    }
                }
                None => {
                    let occurrence = out.iter().filter(|t| t.key.starts_with(&key)).count();
                    let t = TaskImport {
                        key: if occurrence == 0 {
                            key.clone()
                        } else {
                            format!("{key}#{occurrence}")
                        },
                        title,
                        area: area.clone(),
                        first_date: d.date,
                        carried_to: Vec::new(),
                        done_on: cb.checked.then_some(d.date),
                        external: external.map(|(_, url, id)| (id, url)),
                        clickup_flag: cb.text.contains("#clickup"),
                    };
                    if !cb.checked {
                        open.insert(key, out.len());
                    }
                    out.push(t);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }

    #[test]
    fn fixes_mojibake_and_nfc() {
        assert_eq!(normalize_text("CanciÃ³n"), "Canción");
        let nfd = "Cancio\u{301}n";
        assert_eq!(normalize_text(nfd), "Canción");
        assert_eq!(
            normalize_text("Ãrbol raro ñ✓"),
            "Ãrbol raro ñ✓",
            "sin poder repararlo se deja igual"
        );
    }

    #[test]
    fn tables_respect_escaped_pipes() {
        let t = "| Fecha | Concepto | Monto | Categoría | Pago |\n|---|---|---|---|---|\n| 2026-11-02 | Cena con [[Ana López\\|Ana]] | $1,250.50 | Pareja | [[Tarjeta A\\|A]] |\n";
        let tb = &tables(t)[0];
        assert_eq!(tb.rows[0].len(), 5);
        assert_eq!(plain(&unescape_cell(&tb.rows[0][1])), "Cena con Ana");
        assert_eq!(
            payment_ref(&tb.rows[0][4]),
            PaymentRef::Card {
                name: "Tarjeta A".into()
            }
        );
    }

    #[test]
    fn payments_are_classified() {
        assert_eq!(
            payment_ref("Débito BBVA"),
            PaymentRef::Bank {
                kind: "debito".into(),
                bank: "BBVA".into()
            }
        );
        assert_eq!(payment_ref("Efectivo"), PaymentRef::Cash);
        assert_eq!(
            payment_ref("Prestado ([[Ana López\\|Ana]])"),
            PaymentRef::Borrowed {
                person: "Ana López".into()
            }
        );
        assert_eq!(payment_ref(""), PaymentRef::None);
        assert_eq!(payment_ref("Débito <tu banco>"), PaymentRef::None);
    }

    #[test]
    fn classifies_paths_and_ignores_templates() {
        assert_eq!(
            classify("Vault/Daily/2026-11-02.md"),
            FileKind::Daily {
                date: d("2026-11-02")
            }
        );
        assert!(matches!(classify("Templates/Daily.md"), FileKind::Ignored { .. }));
        assert!(matches!(
            classify(".claude/commands/daily.md"),
            FileKind::Ignored { .. }
        ));
        assert!(matches!(classify("Scripts/home.js"), FileKind::Ignored { .. }));
        assert!(matches!(
            classify("Bases/Personas.base"),
            FileKind::Ignored { .. }
        ));
        assert_eq!(
            classify("Proyectos/personal/Objetivos/Semana 2026-W45.md"),
            FileKind::GoalWeek {
                period: "2026-W45".into()
            }
        );
        assert_eq!(
            classify("Proyectos/escuela/Clases/Cálculo/2026-11-02 Límites.md"),
            FileKind::ClassNote {
                subject: "Cálculo".into()
            }
        );
        assert_eq!(
            classify("Proyectos/negocio/negocio.md"),
            FileKind::Project {
                slug: "negocio".into()
            }
        );
        assert_eq!(
            classify("Proyectos/trabajo/adr/ADR-0001 Usar Postgres.md"),
            FileKind::Adr {
                project: "trabajo".into()
            }
        );
        assert!(matches!(
            classify("Attachments/foto.png"),
            FileKind::Unsupported { .. }
        ));
    }

    #[test]
    fn daily_parses_sections_and_ignores_template_false() {
        let raw = "---\nfecha: 2026-11-02\ndevocional: true\nejercicio: false\nlectura: false\nenergia: 4\npeso: 72.5\n---\n\n## Devocional\n- [[x|Devocional de hoy]]\n\n## Agenda\n- 10:00–11:00 Reunión\n\n## Gastos de hoy\n- Planeo gastar: 200 comida\n- Tarjeta: \n\n## Hábitos del mes\nObjetivos: [[Mes]]\n- [x] Leer 20 min · libros\n- [ ] Pesarme · peso\n\n## Log\n- Cerré el fix\n\n## Personas\n- [[Ana López|Ana]]: proyecto\n\n## Pendientes\n### Personal\n- [ ] Llamar al dentista\n\n### Trabajo\n- [x] [\\[Config\\] Ajustar campo](https://app.clickup.com/t/86abc) · alta\n- [ ] \n\n## Lo que hice mal hoy\nRespecto a mis [[x|objetivos]]:\n- Dormí tarde\n\n## Lo que hice bien hoy\n- Hice ejercicio\n";
        let (n, w) = parse_note("Daily/2026-11-02.md", raw);
        assert!(w.is_empty());
        let x = daily(&n, d("2026-11-02"));
        // `devocional: true` se ignora: Dane ya no tiene devocionales.
        assert!(x.fixed_done.is_empty());
        assert_eq!(x.fixed_false_ignored, 2);
        assert_eq!(x.energy, Some(4));
        assert_eq!(x.weight_kg, Some(72.5));
        assert_eq!(x.planned_spend.as_deref(), Some("200 comida"));
        assert_eq!(x.month_habits.len(), 2);
        assert_eq!(habit_name(&x.month_habits[0].text), "Leer 20 min");
        assert_eq!(x.tasks.len(), 2);
        assert_eq!(x.tasks[1].0, "trabajo");
        assert_eq!(x.bad, vec!["Dormí tarde"]);
        assert_eq!(x.good, vec!["Hice ejercicio"]);
        assert_eq!(
            agenda_line(&x.agenda[0]).unwrap(),
            ("10:00".into(), Some("11:00".into()), "Reunión".into())
        );
        let tasks = consolidate_tasks(&[x]);
        assert_eq!(tasks[1].external.as_ref().unwrap().0, "86abc");
        assert_eq!(tasks[1].title, "[Config] Ajustar campo");
    }

    #[test]
    fn carried_tasks_consolidate_into_one() {
        let mk = |date: &str, checked: bool| DailyImport {
            date: d(date),
            weight_kg: None,
            energy: None,
            fixed_done: vec![],
            fixed_false_ignored: 0,
            planned_spend: None,
            agenda: vec![],
            month_habits: vec![],
            week_habits: vec![],
            log: vec![],
            people: vec![],
            tasks: vec![(
                "personal".into(),
                Checkbox {
                    checked,
                    text: "Pagar luz".into(),
                },
            )],
            good: vec![],
            bad: vec![],
        };
        let t = consolidate_tasks(&[
            mk("2026-11-03", false),
            mk("2026-11-01", false),
            mk("2026-11-02", false),
            mk("2026-11-04", true),
            mk("2026-11-20", false),
        ]);
        assert_eq!(t.len(), 2, "{t:?}");
        assert_eq!(t[0].first_date, d("2026-11-01"));
        assert_eq!(
            t[0].carried_to,
            vec![d("2026-11-02"), d("2026-11-03"), d("2026-11-04")]
        );
        assert_eq!(t[0].done_on, Some(d("2026-11-04")));
        assert_ne!(t[0].key, t[1].key, "el mismo texto semanas después es otra tarea");
    }

    #[test]
    fn goals_blocks_and_template_placeholder() {
        let raw = "---\ntipo: mes\nperiodo: 2026-11\ninicio: 2026-11-01\nfin: 2026-11-30\nestado: activo\n---\n# Objetivos\n\n## Objetivos\n<!-- c -->\n\n### 1. Leer 4 libros\n- **Área:** [[personal]]\n- **Por qué importa:** crecer\n- **Cómo sé que lo logré:** 4 terminados\n- **Estado:** logrado\n\n#### Hábitos\n- [ ] Leer 20 min\n\n### 2. <% tp.file.cursor() %>\n- **Área:** \n\n## Seguimiento\n| Fecha | Hechos | Faltó |\n|---|---|---|\n\n## Cierre (fin de mes)\n- Aprendí a leer en la mañana\n";
        let (n, _) = parse_note("x.md", raw);
        let g = goals_note(&n, "month", "2026-11");
        assert_eq!(g.goals.len(), 1);
        assert_eq!(g.goals[0].status, "achieved");
        assert_eq!(g.goals[0].area.as_deref(), Some("personal"));
        assert_eq!(g.goals[0].habits, vec!["Leer 20 min"]);
        assert_eq!(g.closing, "Aprendí a leer en la mañana");
    }

    #[test]
    fn money_rows_keep_identical_purchases_distinct() {
        let raw = "## Registro\n| Fecha | Concepto | Monto | Categoría | Pago |\n|---|---|---|---|---|\n| 2026-11-02 | Café | $50 | Comida | Efectivo |\n| 2026-11-02 | Café | $50 | Comida | Efectivo |\n| bad | x | 1 | Comida | |\n";
        let (n, _) = parse_note("Gastos 2026-11.md", raw);
        let m = money_note(&n, "2026-11", "Pago");
        assert_eq!(m.rows.len(), 2);
        assert_eq!((m.rows[0].occurrence, m.rows[1].occurrence), (0, 1));
        assert_eq!(m.warnings.len(), 1);
    }

    #[test]
    fn person_birthday_without_year() {
        let raw = "---\narea: \"[[trabajo]]\"\nrelacion: compañero\ncumpleaños: 03-14\nultimo_contacto: 2026-11-01\ncontacto: ana@x.com\n---\n# Ana\n## Contexto\n- Quién es para el usuario:\n- Diseñadora\n## Notas\n- 2026-10-01: Habló del proyecto\n";
        let (n, _) = parse_note("Personas/Ana López.md", raw);
        let p = person(&n);
        assert_eq!(p.name, "Ana López");
        assert_eq!(p.birthday, Some((3, 14, None)));
        assert_eq!(p.area.as_deref(), Some("trabajo"));
        assert_eq!(p.email.as_deref(), Some("ana@x.com"));
        assert_eq!(p.context, "Diseñadora");
        assert_eq!(p.notes[0].0, Some(d("2026-10-01")));
    }

    #[test]
    fn invalid_yaml_is_warning_not_crash() {
        let (n, w) = parse_note("x.md", "---\na: [unclosed\n---\nbody");
        assert!(n.frontmatter.is_empty());
        assert_eq!(w.len(), 1);
    }
}
