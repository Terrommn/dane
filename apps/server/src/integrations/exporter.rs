//! Exportación completa (T54): JSON de todas las tablas propias, CSV útiles y Markdown con el formato del vault
//! (reimportable). Se excluyen sesiones, identidades, credenciales cifradas, flujos OAuth e idempotency keys.

use std::io::Write;

use serde_json::{Value, json};
use uuid::Uuid;

use crate::state::AppState;

/// Tablas propias exportadas. Las columnas sensibles se quitan con `- 'col'`.
pub const TABLES: &[(&str, &str)] = &[
    ("user_settings", ""),
    ("areas", ""),
    ("projects", ""),
    ("goals", ""),
    ("goal_progress_events", ""),
    ("habits", ""),
    ("habit_versions", ""),
    ("habit_pauses", ""),
    ("habit_entries", ""),
    ("habit_links", ""),
    ("daily_checkins", ""),
    ("daily_log_entries", ""),
    ("reflection_items", ""),
    ("calendar_events", ""),
    ("people", ""),
    ("person_notes", ""),
    ("interactions", ""),
    ("person_suggestions", ""),
    ("interaction_suggestions", ""),
    ("task_recurrence_rules", ""),
    ("tasks", ""),
    ("task_people", ""),
    ("task_events", ""),
    ("inbox_items", ""),
    ("weekly_reviews", ""),
    ("notes", ""),
    ("entity_links", ""),
    ("favorites", ""),
    ("drawings", ""),
    ("devotionals", ""),
    ("readings", ""),
    ("content_pieces", ""),
    ("swipe_refs", ""),
    ("subjects", ""),
    ("class_notes", ""),
    ("devlogs", ""),
    ("devlog_sessions", ""),
    ("devlog_session_projects", ""),
    ("adrs", ""),
    ("fin_categories", ""),
    ("fin_accounts", ""),
    ("balance_snapshots", ""),
    ("debts", ""),
    ("fin_recurring_rules", ""),
    ("budget_months", ""),
    ("budget_lines", ""),
    ("budget_items", ""),
    ("transactions", "- 'idempotency_key'"),
    ("credit_plans", ""),
    ("integration_connections", "- 'credentials_enc' - 'cursor'"),
    ("capture_proposals", ""),
    ("import_records", ""),
];

pub async fn export_json(state: &AppState, user_id: Uuid) -> anyhow::Result<Value> {
    let mut tables = serde_json::Map::new();
    for (t, strip) in TABLES {
        let rows: Value = sqlx::query_scalar(persistence::sqlf!(
            "SELECT coalesce(jsonb_agg(to_jsonb(t) {strip}), '[]'::jsonb) FROM {t} t WHERE t.user_id = $1"
        ))
        .bind(user_id)
        .fetch_one(&state.pool)
        .await?;
        tables.insert((*t).to_string(), rows);
    }
    let user: Value = sqlx::query_scalar("SELECT to_jsonb(u) FROM users u WHERE id = $1").bind(user_id).fetch_one(&state.pool).await?;
    Ok(json!({
        "format": "vida-export",
        "version": 1,
        "exported_at": state.now(),
        "user": { "id": user["id"], "display_name": user["display_name"], "created_at": user["created_at"] },
        "excluded": ["sessions", "identities", "oauth_flows", "idempotency_keys", "credenciales cifradas"],
        "tables": tables,
    }))
}

fn csv_of(rows: Vec<Vec<String>>, header: &[&str]) -> anyhow::Result<Vec<u8>> {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(header)?;
    for r in rows {
        w.write_record(&r)?;
    }
    Ok(w.into_inner()?)
}

fn cents(c: i64) -> String {
    format!("{}{}.{:02}", if c < 0 { "-" } else { "" }, c.unsigned_abs() / 100, c.unsigned_abs() % 100)
}

fn yaml_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// Escapa `|` para celdas de tabla Markdown (conserva wikilinks como `[[x\|y]]`).
fn cell(s: &str) -> String {
    s.replace('\n', " ").replace('|', "\\|")
}

pub async fn export_zip(state: &AppState, user_id: Uuid) -> anyhow::Result<Vec<u8>> {
    let pool = &state.pool;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let add = |zip: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, path: &str, data: &[u8]| -> anyhow::Result<()> {
        zip.start_file(path, opts)?;
        zip.write_all(data)?;
        Ok(())
    };
    let json = export_json(state, user_id).await?;
    add(&mut zip, "vida-export.json", &serde_json::to_vec_pretty(&json)?)?;

    // ---- CSV
    let tx: Vec<(chrono::NaiveDate, String, String, String, i64, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.occurred_on, t.kind, t.status, t.concept, t.amount_cents, t.currency::text, c.name, a.name
         FROM transactions t LEFT JOIN fin_categories c ON c.id = t.category_id LEFT JOIN fin_accounts a ON a.id = t.account_id
         WHERE t.user_id = $1 ORDER BY t.occurred_on, t.created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    add(
        &mut zip,
        "csv/transacciones.csv",
        &csv_of(
            tx.iter().map(|r| vec![r.0.to_string(), r.1.clone(), r.2.clone(), r.3.clone(), cents(r.4), r.5.clone(), r.6.clone().unwrap_or_default(), r.7.clone().unwrap_or_default()]).collect(),
            &["fecha", "tipo", "estado", "concepto", "monto", "moneda", "categoria", "cuenta"],
        )?,
    )?;
    let he: Vec<(chrono::NaiveDate, String, String, Option<rust_decimal::Decimal>, Option<String>)> = sqlx::query_as(
        "SELECT e.on_date, h.name, e.status, e.quantity, e.reason FROM habit_entries e JOIN habits h ON h.id = e.habit_id WHERE e.user_id = $1 ORDER BY e.on_date, h.name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    add(
        &mut zip,
        "csv/habitos.csv",
        &csv_of(
            he.iter().map(|r| vec![r.0.to_string(), r.1.clone(), r.2.clone(), r.3.map(|q| q.to_string()).unwrap_or_default(), r.4.clone().unwrap_or_default()]).collect(),
            &["fecha", "habito", "estado", "cantidad", "motivo"],
        )?,
    )?;
    let ts: Vec<(String, String, Option<String>, Option<String>, Option<chrono::NaiveDate>, Option<chrono::NaiveDate>, Option<String>)> = sqlx::query_as(
        "SELECT t.title, t.status, a.name, p.name, t.planned_on, t.due_on, t.external_url FROM tasks t
         LEFT JOIN areas a ON a.id = t.area_id LEFT JOIN projects p ON p.id = t.project_id WHERE t.user_id = $1 ORDER BY t.created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    add(
        &mut zip,
        "csv/tareas.csv",
        &csv_of(
            ts.iter()
                .map(|r| vec![r.0.clone(), r.1.clone(), r.2.clone().unwrap_or_default(), r.3.clone().unwrap_or_default(), r.4.map(|d| d.to_string()).unwrap_or_default(), r.5.map(|d| d.to_string()).unwrap_or_default(), r.6.clone().unwrap_or_default()])
                .collect(),
            &["titulo", "estado", "area", "proyecto", "planeada", "vence", "enlace_externo"],
        )?,
    )?;

    // ---- Markdown con formato del vault
    let people: Vec<(String, Vec<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<chrono::NaiveDate>, String, Option<i16>, Option<i16>, Option<i16>)> = sqlx::query_as(
        "SELECT p.name, p.aliases, a.slug, p.relationship, p.org, p.role, p.contact, p.last_contact_on, p.context_md, p.birthday_month, p.birthday_day, p.birthday_year
         FROM people p LEFT JOIN areas a ON a.id = p.area_id WHERE p.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    for p in &people {
        let birthday = match (p.9, p.10, p.11) {
            (Some(m), Some(d), Some(y)) => format!("{y:04}-{m:02}-{d:02}"),
            (Some(m), Some(d), None) => format!("{m:02}-{d:02}"),
            _ => String::new(),
        };
        let mut md = format!("---\ncategories:\n  - \"[[Personas]]\"\n");
        if !p.1.is_empty() {
            md.push_str(&format!("aliases: {}\n", serde_json::to_string(&p.1)?));
        }
        for (k, v) in [("area", p.2.as_ref().map(|a| format!("[[{a}]]"))), ("relacion", p.3.clone()), ("org", p.4.clone()), ("rol", p.5.clone()), ("contacto", p.6.clone()), ("ultimo_contacto", p.7.map(|d| d.to_string()))] {
            if let Some(v) = v {
                md.push_str(&format!("{k}: {}\n", yaml_str(&v)));
            }
        }
        if !birthday.is_empty() {
            md.push_str(&format!("cumpleaños: \"{birthday}\"\n"));
        }
        md.push_str(&format!("---\n# {}\n\n## Contexto\n", p.0));
        for l in p.8.lines().filter(|l| !l.trim().is_empty()) {
            md.push_str(&format!("- {l}\n"));
        }
        add(&mut zip, &format!("Personas/{}.md", file_safe(&p.0)), md.as_bytes())?;
    }

    let days: Vec<(chrono::NaiveDate, Option<rust_decimal::Decimal>, Option<i16>, Option<String>)> =
        sqlx::query_as("SELECT on_date, weight_kg, energy, planned_spend_text FROM daily_checkins WHERE user_id = $1 ORDER BY on_date").bind(user_id).fetch_all(pool).await?;
    for (date, weight, energy, planned) in days {
        let fixed: Vec<(String, String)> = sqlx::query_as(
            "SELECT h.fixed_key, e.status FROM habit_entries e JOIN habits h ON h.id = e.habit_id WHERE e.user_id = $1 AND e.on_date = $2 AND h.fixed_key IS NOT NULL",
        )
        .bind(user_id)
        .bind(date)
        .fetch_all(pool)
        .await?;
        let mut md = format!("---\nfecha: {date}\ncategories:\n  - \"[[Daily]]\"\n");
        for (key, es) in [("devotional", "devocional"), ("exercise", "ejercicio"), ("reading", "lectura")] {
            if let Some((_, st)) = fixed.iter().find(|f| f.0 == key) {
                if st == "done" {
                    md.push_str(&format!("{es}: true\n"));
                }
            }
        }
        if let Some(e) = energy {
            md.push_str(&format!("energia: {e}\n"));
        }
        if let Some(w) = weight {
            md.push_str(&format!("peso: {w}\n"));
        }
        md.push_str("---\n\n## Gastos de hoy\n");
        md.push_str(&format!("- Planeo gastar: {}\n\n## Log\n", planned.unwrap_or_default()));
        let logs: Vec<String> = sqlx::query_scalar("SELECT text FROM daily_log_entries WHERE user_id = $1 AND on_date = $2 ORDER BY position, created_at").bind(user_id).bind(date).fetch_all(pool).await?;
        for l in logs {
            md.push_str(&format!("- {l}\n"));
        }
        md.push_str("\n## Personas\n");
        let ints: Vec<(String, String)> =
            sqlx::query_as("SELECT p.name, i.topic FROM interactions i JOIN people p ON p.id = i.person_id WHERE i.user_id = $1 AND i.on_date = $2 ORDER BY i.created_at").bind(user_id).bind(date).fetch_all(pool).await?;
        for (n, t) in ints {
            md.push_str(&format!("- [[{n}]]: {t}\n"));
        }
        md.push_str("\n## Pendientes\n");
        let tasks: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT t.title, t.status, a.slug FROM tasks t LEFT JOIN areas a ON a.id = t.area_id WHERE t.user_id = $1 AND t.planned_on = $2 ORDER BY a.position, t.created_at",
        )
        .bind(user_id)
        .bind(date)
        .fetch_all(pool)
        .await?;
        let mut current: Option<String> = None;
        for (title, status, area) in tasks {
            let area = area.unwrap_or_else(|| "personal".into());
            if current.as_deref() != Some(area.as_str()) {
                let mut cap = area.clone();
                if let Some(f) = cap.get_mut(0..1) {
                    f.make_ascii_uppercase();
                }
                md.push_str(&format!("### {cap}\n"));
                current = Some(area);
            }
            md.push_str(&format!("- [{}] {title}\n", if status == "done" { "x" } else { " " }));
        }
        for (polarity, heading) in [("bad", "Lo que hice mal hoy"), ("good", "Lo que hice bien hoy")] {
            md.push_str(&format!("\n## {heading}\n"));
            let items: Vec<String> = sqlx::query_scalar("SELECT text FROM reflection_items WHERE user_id = $1 AND on_date = $2 AND polarity = $3 ORDER BY position")
                .bind(user_id)
                .bind(date)
                .bind(polarity)
                .fetch_all(pool)
                .await?;
            for i in items {
                md.push_str(&format!("- {i}\n"));
            }
        }
        add(&mut zip, &format!("Daily/{date}.md"), md.as_bytes())?;
    }

    let readings: Vec<(String, String, String, String, Option<i16>, Option<chrono::NaiveDate>, Option<chrono::NaiveDate>, String, String, String)> = sqlx::query_as(
        "SELECT title, kind, author, status, rating, started_on, finished_on, ideas_md, quotes_md, application_md FROM readings WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    for r in readings {
        let estado = match r.3.as_str() {
            "finished" => "terminado",
            "abandoned" => "abandonado",
            "want" => "pendiente",
            _ => "leyendo",
        };
        let md = format!(
            "---\ncategories:\n  - \"[[Lecturas]]\"\ntipo: {}\nautor: {}\nestado: {estado}\nrating: {}\ninicio: {}\nfin: {}\n---\n# {}\n\n## Ideas clave\n{}\n\n## Citas\n{}\n\n## Qué voy a aplicar\n{}\n",
            if r.1 == "article" { "artículo" } else { "libro" },
            yaml_str(&r.2),
            r.4.map(|x| x.to_string()).unwrap_or_default(),
            r.5.map(|d| d.to_string()).unwrap_or_default(),
            r.6.map(|d| d.to_string()).unwrap_or_default(),
            r.0,
            r.7,
            r.8,
            r.9
        );
        add(&mut zip, &format!("Lecturas/{}.md", file_safe(&r.0)), md.as_bytes())?;
    }

    let notes: Vec<(String, String, String, Vec<String>, Vec<String>)> =
        sqlx::query_as("SELECT kind, title, body_md, tags, aliases FROM notes WHERE user_id = $1").bind(user_id).fetch_all(pool).await?;
    for (kind, title, body, tags, aliases) in notes {
        let folder = match kind.as_str() {
            "til" => "Dev/til",
            "snippet" => "Dev/snippets",
            _ => "Notas",
        };
        let md = format!("---\ntags: {}\naliases: {}\n---\n{body}\n", serde_json::to_string(&tags)?, serde_json::to_string(&aliases)?);
        add(&mut zip, &format!("{folder}/{}.md", file_safe(&title)), md.as_bytes())?;
    }

    // Cuentas y tarjetas (con su último saldo observado) para que los gastos reimportados se asocien.
    let accounts: Vec<(String, String, String, Option<i64>, Option<i16>, Option<i16>, String)> =
        sqlx::query_as("SELECT name, bank, kind, credit_limit_cents, cut_day, pay_day, status FROM fin_accounts WHERE user_id = $1").bind(user_id).fetch_all(pool).await?;
    for (name, bank, kind, limit, cut, pay, status) in accounts {
        if kind == "credit_card" {
            let md = format!(
                "---\ncategories:\n  - \"[[Tarjetas]]\"\nbanco: {}\nlimite: {}\ndia_corte: {}\ndia_pago: {}\nestado: {}\n---\n# {name}\n",
                yaml_str(&bank),
                limit.map(cents).unwrap_or_default(),
                cut.map(|x| x.to_string()).unwrap_or_default(),
                pay.map(|x| x.to_string()).unwrap_or_default(),
                if status == "cancelled" { "cancelada" } else { "activa" }
            );
            add(&mut zip, &format!("Proyectos/personal/Finanzas/Tarjetas/{}.md", file_safe(&name)), md.as_bytes())?;
        } else {
            let md = format!("---\ncategories:\n  - \"[[Cuentas]]\"\nbanco: {}\nestado: {}\n---\n# {name}\n", yaml_str(&bank), if status == "closed" { "cerrada" } else { "activa" });
            add(&mut zip, &format!("Proyectos/personal/Finanzas/Cuentas/{}.md", file_safe(&name)), md.as_bytes())?;
        }
    }

    // Gastos e ingresos por mes (solo confirmados, en el formato de tabla del vault).
    let rows: Vec<(chrono::NaiveDate, String, String, i64, Option<String>, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT t.occurred_on, t.kind, t.concept, t.amount_cents, c.name, a.name, a.kind, cc.name
         FROM transactions t LEFT JOIN fin_categories c ON c.id = t.category_id LEFT JOIN fin_accounts a ON a.id = t.account_id
         LEFT JOIN fin_accounts cc ON cc.id = t.counter_account_id
         WHERE t.user_id = $1 AND t.status = 'confirmed' AND t.kind IN ('expense','income','loan_received','card_payment','debt_payment')
         ORDER BY t.occurred_on, t.created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let mut by_month: std::collections::BTreeMap<(String, bool), String> = std::collections::BTreeMap::new();
    for (date, kind, concept, amount, cat, acc, acc_kind, counter) in rows {
        let income = kind == "income" || kind == "loan_received";
        let month = domain::time::month_key(date);
        let pay = match (acc, acc_kind.as_deref()) {
            (Some(a), Some("credit_card")) => format!("[[{a}\\|{a}]]"),
            (Some(a), Some("cash")) if a == "Efectivo" => "Efectivo".into(),
            (Some(a), _) => a,
            _ => String::new(),
        };
        let concept = match counter {
            Some(card) if kind == "card_payment" || kind == "debt_payment" => format!("Pago [[{card}\\|{card}]]"),
            _ => cell(&concept),
        };
        let line = format!("| {date} | {concept} | {} | {} | {} |\n", cents(amount), cell(&cat.unwrap_or_default()), pay);
        by_month.entry((month, income)).or_default().push_str(&line);
    }
    for ((month, income), lines) in by_month {
        let (folder, title, col) = if income { ("Ingresos", "Ingresos", "Cuenta") } else { ("Gastos", "Gastos", "Pago") };
        let md = format!("---\nmes: {month}\n---\n# {title} {month}\n\n## Registro\n| Fecha | Concepto | Monto | Categoría | {col} |\n|---|---|---|---|---|\n{lines}");
        add(&mut zip, &format!("Proyectos/personal/Finanzas/{folder}/{title} {month}.md"), md.as_bytes())?;
    }

    let cur = zip.finish()?;
    Ok(cur.into_inner())
}

fn file_safe(s: &str) -> String {
    let t: String = s.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '-' } else { c }).collect();
    let t = t.trim().trim_start_matches('.').to_string();
    if t.is_empty() { "sin-titulo".into() } else { t.chars().take(120).collect() }
}
