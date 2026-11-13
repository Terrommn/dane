//! Aplicación de propuestas confirmadas (captura libre / sugerencias de IA) en una sola transacción.
//! También expone resolvedores por nombre (áreas, categorías, cuentas, personas) que usa el importador.

use chrono::NaiveDate;
use domain::capture::{Proposal, Question};
use domain::money::parse_amount_to_cents;
use domain::text::fold;
use persistence::Tx;
use persistence::hooks::{self, HabitTrigger};
use serde_json::{Value, json};
use uuid::Uuid;

/// Catálogos del usuario cargados una vez por transacción (búsqueda sin acentos/mayúsculas).
pub struct Resolver {
    pub user_id: Uuid,
    pub currency: String,
    areas: Vec<(Uuid, String, String)>,
    projects: Vec<(Uuid, String, String, Option<Uuid>)>,
    categories: Vec<(Uuid, String, String, String)>,
    accounts: Vec<(Uuid, String, String, String)>,
    people: Vec<(Uuid, String, Vec<String>)>,
}

impl Resolver {
    pub async fn load(tx: &mut Tx<'_>, user_id: Uuid) -> sqlx::Result<Self> {
        let currency: String = sqlx::query_scalar("SELECT currency::text FROM user_settings WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&mut **tx)
            .await?
            .unwrap_or_else(|| "MXN".into());
        let mut r = Resolver { user_id, currency, areas: vec![], projects: vec![], categories: vec![], accounts: vec![], people: vec![] };
        r.reload(tx).await?;
        Ok(r)
    }

    pub async fn reload(&mut self, tx: &mut Tx<'_>) -> sqlx::Result<()> {
        let u = self.user_id;
        self.areas = sqlx::query_as("SELECT id, name, slug FROM areas WHERE user_id = $1").bind(u).fetch_all(&mut **tx).await?;
        self.projects = sqlx::query_as("SELECT id, name, slug, area_id FROM projects WHERE user_id = $1").bind(u).fetch_all(&mut **tx).await?;
        self.categories = sqlx::query_as("SELECT id, kind, name, role FROM fin_categories WHERE user_id = $1").bind(u).fetch_all(&mut **tx).await?;
        self.accounts = sqlx::query_as("SELECT id, name, bank, kind FROM fin_accounts WHERE user_id = $1").bind(u).fetch_all(&mut **tx).await?;
        self.people = sqlx::query_as("SELECT id, name, aliases FROM people WHERE user_id = $1").bind(u).fetch_all(&mut **tx).await?;
        Ok(())
    }

    /// Área por nombre o slug; también acepta el slug de un proyecto (`[[trabajo]]` → área de ese proyecto).
    pub fn area(&self, name: Option<&str>) -> Option<Uuid> {
        let n = fold(name?.trim());
        if n.is_empty() {
            return None;
        }
        self.areas
            .iter()
            .find(|(_, a, s)| fold(a) == n || fold(s) == n)
            .map(|x| x.0)
            .or_else(|| self.projects.iter().find(|(_, p, s, _)| fold(p) == n || fold(s) == n).and_then(|p| p.3))
    }

    pub fn default_area(&self) -> Option<Uuid> {
        self.area(Some("personal")).or_else(|| self.areas.first().map(|a| a.0))
    }

    pub fn project(&self, name: &str) -> Option<Uuid> {
        let n = fold(name.trim());
        self.projects.iter().find(|(_, p, s, _)| fold(p) == n || fold(s) == n).map(|x| x.0)
    }

    /// (id, rol) de una categoría de gasto o ingreso.
    pub fn category(&self, kind: &str, name: &str) -> Option<(Uuid, String)> {
        let n = fold(name.trim());
        self.categories.iter().find(|(_, k, c, _)| k == kind && fold(c) == n).map(|x| (x.0, x.3.clone()))
    }

    /// Cuenta por nombre, banco o "Tarjeta X". `(id, kind)`.
    pub fn account(&self, text: &str) -> Option<(Uuid, String)> {
        let n = fold(text.trim());
        if n.is_empty() {
            return None;
        }
        self.accounts
            .iter()
            .find(|(_, name, _, _)| fold(name) == n)
            .or_else(|| self.accounts.iter().find(|(_, name, bank, _)| !bank.is_empty() && (n.contains(&fold(bank)) || fold(name).contains(&n))))
            .map(|x| (x.0, x.3.clone()))
    }

    pub fn person(&self, name: &str) -> Option<Uuid> {
        let n = fold(name.trim().trim_start_matches('@'));
        if n.is_empty() {
            return None;
        }
        self.people
            .iter()
            .find(|(_, p, al)| fold(p) == n || al.iter().any(|a| fold(a) == n))
            .or_else(|| self.people.iter().find(|(_, p, _)| fold(p).split(' ').next() == Some(n.as_str())))
            .map(|x| x.0)
    }

    pub fn push_person(&mut self, id: Uuid, name: String, aliases: Vec<String>) {
        self.people.push((id, name, aliases));
    }
    pub fn push_account(&mut self, id: Uuid, name: String, bank: String, kind: String) {
        self.accounts.push((id, name, bank, kind));
    }
}

/// Campos que bloquean la aplicación (montos/fechas/categorías faltantes).
pub fn blocking_questions(p: &Proposal) -> Vec<Question> {
    let mut q = Vec::new();
    for (i, e) in p.expenses.iter().enumerate() {
        for (field, missing) in [("amount", e.amount.is_none()), ("date", e.date.is_none()), ("category", e.category.is_none())] {
            if missing {
                q.push(Question { field: format!("expenses[{i}].{field}"), message: format!("Falta {field} en «{}»", e.concept) });
            }
        }
    }
    for (i, e) in p.incomes.iter().enumerate() {
        for (field, missing) in [("amount", e.amount.is_none()), ("date", e.date.is_none()), ("category", e.category.is_none())] {
            if missing {
                q.push(Question { field: format!("incomes[{i}].{field}"), message: format!("Falta {field} en «{}»", e.concept) });
            }
        }
    }
    q
}

fn d(s: &Option<String>) -> Option<NaiveDate> {
    s.as_deref().and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok())
}

async fn fixed_habit(tx: &mut Tx<'_>, user_id: Uuid, key: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("SELECT id FROM habits WHERE user_id = $1 AND fixed_key = $2 AND archived_at IS NULL").bind(user_id).bind(key).fetch_optional(&mut **tx).await
}

/// Aplica la propuesta. `key_prefix` hace idempotente cada elemento financiero (`<prefijo>:expense:<i>`).
pub async fn apply(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    p: &Proposal,
    reference_date: NaiveDate,
    origin: &str,
    key_prefix: &str,
    meta: &Value,
) -> Result<Value, String> {
    let map_db = |e: sqlx::Error| format!("error al guardar: {e}");
    let r = Resolver::load(tx, user_id).await.map_err(map_db)?;
    let mut created: Vec<Value> = Vec::new();

    for (i, e) in p.expenses.iter().enumerate() {
        let amount = e.amount.as_deref().and_then(|a| parse_amount_to_cents(a).ok()).ok_or("monto faltante")?;
        let date = d(&e.date).ok_or("fecha faltante")?;
        let (cat, role) = r.category("expense", e.category.as_deref().unwrap_or("")).ok_or_else(|| format!("categoría desconocida en «{}»", e.concept))?;
        let account = e.payment.as_deref().and_then(|x| r.account(x));
        let (kind, account_id, counter) = if role == "debt_payment" {
            // Pago a una tarjeta: el concepto o el medio debe identificarla. No cuenta como consumo.
            let card = r.account(&e.concept).filter(|a| a.1 == "credit_card").or(account.clone().filter(|a| a.1 == "credit_card"));
            match card {
                Some(c) => {
                    let src = account.filter(|a| a.1 != "credit_card").map(|a| a.0);
                    (if src.is_some() { "card_payment" } else { "debt_payment" }, src, Some(c.0))
                }
                None => return Err(format!("Indica a qué tarjeta o deuda corresponde «{}»", e.concept)),
            }
        } else {
            ("expense", account.map(|a| a.0), None)
        };
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO transactions (user_id, kind, status, origin, occurred_on, concept, amount_cents, currency, category_id, account_id, counter_account_id, idempotency_key, confirmed_at)
             VALUES ($1, $2, 'confirmed', $3, $4, $5, $6, $7, $8, $9, $10, $11, now())
             ON CONFLICT (user_id, idempotency_key) DO UPDATE SET version = transactions.version RETURNING id",
        )
        .bind(user_id)
        .bind(kind)
        .bind(origin)
        .bind(date)
        .bind(e.concept.trim())
        .bind(amount)
        .bind(&r.currency)
        .bind(cat)
        .bind(account_id)
        .bind(counter)
        .bind(format!("{key_prefix}:expense:{i}"))
        .fetch_one(&mut **tx)
        .await
        .map_err(map_db)?;
        if kind == "expense" {
            hooks::mark_linked_habit(tx, user_id, HabitTrigger::ExpenseLogged, date).await.map_err(|e| e.to_string())?;
        }
        created.push(json!({ "type": "transaction", "id": id }));
    }
    for (i, e) in p.incomes.iter().enumerate() {
        let amount = e.amount.as_deref().and_then(|a| parse_amount_to_cents(a).ok()).ok_or("monto faltante")?;
        let date = d(&e.date).ok_or("fecha del ingreso faltante")?;
        let (cat, role) = r.category("income", e.category.as_deref().unwrap_or("")).ok_or("categoría de ingreso desconocida")?;
        let kind = if role == "financing" { "loan_received" } else { "income" };
        let account = e.account.as_deref().and_then(|x| r.account(x)).map(|a| a.0);
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO transactions (user_id, kind, status, origin, occurred_on, concept, amount_cents, currency, category_id, account_id, idempotency_key, confirmed_at)
             VALUES ($1, $2, 'confirmed', $3, $4, $5, $6, $7, $8, $9, $10, now())
             ON CONFLICT (user_id, idempotency_key) DO UPDATE SET version = transactions.version RETURNING id",
        )
        .bind(user_id)
        .bind(kind)
        .bind(origin)
        .bind(date)
        .bind(e.concept.trim())
        .bind(amount)
        .bind(&r.currency)
        .bind(cat)
        .bind(account)
        .bind(format!("{key_prefix}:income:{i}"))
        .fetch_one(&mut **tx)
        .await
        .map_err(map_db)?;
        created.push(json!({ "type": "transaction", "id": id }));
    }
    let inbox_ids: Vec<Uuid> = meta.get("inbox_ids").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    for t in &p.tasks {
        let area = r.area(t.area.as_deref()).or_else(|| r.default_area());
        let notes = if t.clickup_candidate { "Parece trabajo de equipo: considera moverlo a ClickUp (#clickup)." } else { "" };
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO tasks (user_id, title, notes_md, area_id, planned_on, due_on) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(user_id)
        .bind(t.title.trim())
        .bind(notes)
        .bind(area)
        .bind(reference_date)
        .bind(d(&t.due))
        .fetch_one(&mut **tx)
        .await
        .map_err(map_db)?;
        sqlx::query("INSERT INTO task_events (user_id, task_id, kind, data) VALUES ($1, $2, 'created', $3)")
            .bind(user_id)
            .bind(id)
            .bind(json!({ "source": origin }))
            .execute(&mut **tx)
            .await
            .map_err(map_db)?;
        for person in &t.people {
            if let Some(pid) = r.person(person) {
                sqlx::query("INSERT INTO task_people (user_id, task_id, person_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
                    .bind(user_id)
                    .bind(id)
                    .bind(pid)
                    .execute(&mut **tx)
                    .await
                    .map_err(map_db)?;
            }
        }
        created.push(json!({ "type": "task", "id": id }));
    }
    if !inbox_ids.is_empty() && !p.tasks.is_empty() {
        // Clasificación de Inbox confirmada: los elementos de origen quedan procesados.
        sqlx::query("UPDATE inbox_items SET status = 'processed', processed_at = now(), result_type = 'task' WHERE user_id = $1 AND id = ANY($2) AND status = 'pending'")
            .bind(user_id)
            .bind(&inbox_ids)
            .execute(&mut **tx)
            .await
            .map_err(map_db)?;
    }
    for text in &p.inbox {
        let id: Uuid = sqlx::query_scalar("INSERT INTO inbox_items (user_id, text, source) VALUES ($1, $2, $3) RETURNING id")
            .bind(user_id)
            .bind(text.trim())
            .bind(if origin == "bot" { "telegram" } else { "app" })
            .fetch_one(&mut **tx)
            .await
            .map_err(map_db)?;
        created.push(json!({ "type": "inbox", "id": id }));
    }
    for (i, text) in p.log.iter().enumerate() {
        sqlx::query("INSERT INTO daily_log_entries (user_id, on_date, text, position, source) VALUES ($1, $2, $3, $4, $5)")
            .bind(user_id)
            .bind(reference_date)
            .bind(text.trim())
            .bind(1000 + i as i32)
            .bind(if origin == "bot" { "bot" } else { "llm" })
            .execute(&mut **tx)
            .await
            .map_err(map_db)?;
    }
    for (polarity, items) in [("good", &p.reflections_good), ("bad", &p.reflections_bad)] {
        for (i, text) in items.iter().enumerate() {
            sqlx::query("INSERT INTO reflection_items (user_id, on_date, polarity, text, position) VALUES ($1, $2, $3, $4, $5)")
                .bind(user_id)
                .bind(reference_date)
                .bind(polarity)
                .bind(text.trim())
                .bind(1000 + i as i32)
                .execute(&mut **tx)
                .await
                .map_err(map_db)?;
        }
    }
    if !p.reflections_good.is_empty() {
        hooks::mark_linked_habit(tx, user_id, HabitTrigger::PositiveReflection, reference_date).await.map_err(|e| e.to_string())?;
    }
    for (i, it) in p.interactions.iter().enumerate() {
        match r.person(&it.person) {
            Some(pid) => {
                sqlx::query(
                    "INSERT INTO interactions (user_id, person_id, on_date, topic, source, source_ref) VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT DO NOTHING",
                )
                .bind(user_id)
                .bind(pid)
                .bind(reference_date)
                .bind(it.topic.trim())
                .bind(if origin == "bot" { "bot" } else { "manual" })
                .bind(format!("{key_prefix}:interaction:{i}"))
                .execute(&mut **tx)
                .await
                .map_err(map_db)?;
                sqlx::query("UPDATE people SET last_contact_on = greatest(coalesce(last_contact_on, $3), $3) WHERE user_id = $1 AND id = $2")
                    .bind(user_id)
                    .bind(pid)
                    .bind(reference_date)
                    .execute(&mut **tx)
                    .await
                    .map_err(map_db)?;
            }
            None => {
                // No se crean personas en silencio.
                sqlx::query(
                    "INSERT INTO person_suggestions (user_id, name, source, source_ref, on_date, context) VALUES ($1, $2, 'dictation', $3, $4, $5)
                     ON CONFLICT DO NOTHING",
                )
                .bind(user_id)
                .bind(it.person.trim())
                .bind(format!("{key_prefix}:{i}"))
                .bind(reference_date)
                .bind(it.topic.trim())
                .execute(&mut **tx)
                .await
                .map_err(map_db)?;
            }
        }
    }
    if let Some(c) = &p.checkin {
        let weight = c.weight_kg.as_deref().and_then(|w| w.replace(',', ".").parse::<rust_decimal::Decimal>().ok());
        sqlx::query(
            "INSERT INTO daily_checkins (user_id, on_date, weight_kg, energy, planned_spend_text) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (user_id, on_date) DO UPDATE SET weight_kg = coalesce(EXCLUDED.weight_kg, daily_checkins.weight_kg),
                energy = coalesce(EXCLUDED.energy, daily_checkins.energy),
                planned_spend_text = coalesce(EXCLUDED.planned_spend_text, daily_checkins.planned_spend_text),
                version = daily_checkins.version + 1",
        )
        .bind(user_id)
        .bind(reference_date)
        .bind(weight)
        .bind(c.energy.map(|e| e as i16))
        .bind(c.planned_spend.as_deref())
        .execute(&mut **tx)
        .await
        .map_err(map_db)?;
        if weight.is_some() {
            hooks::mark_linked_habit(tx, user_id, HabitTrigger::WeightLogged, reference_date).await.map_err(|e| e.to_string())?;
        }
        for (key, val) in [("exercise", c.exercise), ("reading", c.reading)] {
            if let (Some(v), Some(h)) = (val, fixed_habit(tx, user_id, key).await.map_err(map_db)?) {
                sqlx::query(
                    "INSERT INTO habit_entries (user_id, habit_id, on_date, status, source) VALUES ($1, $2, $3, $4, 'checkin')
                     ON CONFLICT (habit_id, on_date) DO UPDATE SET status = EXCLUDED.status, version = habit_entries.version + 1",
                )
                .bind(user_id)
                .bind(h)
                .bind(reference_date)
                .bind(if v { "done" } else { "not_done" })
                .execute(&mut **tx)
                .await
                .map_err(map_db)?;
            }
        }
        created.push(json!({ "type": "checkin", "date": reference_date }));
    }
    for g in &p.goals {
        let (period_key, period) = if g.kind == "month" {
            (domain::time::month_key(reference_date), domain::time::month_period(reference_date))
        } else {
            (domain::time::iso_week_key(reference_date), domain::time::iso_week_period(reference_date))
        };
        let gid: Uuid = sqlx::query_scalar(
            "INSERT INTO goals (user_id, kind, period_key, starts_on, ends_on, title, area_id, motivation, success_criteria)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
        )
        .bind(user_id)
        .bind(&g.kind)
        .bind(&period_key)
        .bind(period.start)
        .bind(period.end)
        .bind(g.title.trim())
        .bind(r.area(g.area.as_deref()))
        .bind(g.motivation.clone().unwrap_or_default())
        .bind(g.success_criteria.clone().unwrap_or_default())
        .fetch_one(&mut **tx)
        .await
        .map_err(map_db)?;
        for h in &g.habits {
            create_goal_habit(tx, user_id, gid, h, period.start, Some(period.end)).await.map_err(map_db)?;
        }
        created.push(json!({ "type": "goal", "id": gid }));
    }
    if let Some(dv) = &p.devlog {
        let devlog: Uuid = sqlx::query_scalar(
            "INSERT INTO devlogs (user_id, on_date) VALUES ($1, $2) ON CONFLICT (user_id, on_date) DO UPDATE SET version = devlogs.version RETURNING id",
        )
        .bind(user_id)
        .bind(reference_date)
        .fetch_one(&mut **tx)
        .await
        .map_err(map_db)?;
        let sid: Uuid = sqlx::query_scalar(
            "INSERT INTO devlog_sessions (user_id, devlog_id, repo, branch, changes_md, bugs_md, decisions_md, next_steps_md, raw_text)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
        )
        .bind(user_id)
        .bind(devlog)
        .bind(dv.repo.trim())
        .bind(dv.branch.clone().unwrap_or_default())
        .bind(&dv.changes)
        .bind(dv.bugs.clone().unwrap_or_default())
        .bind(dv.decisions.clone().unwrap_or_default())
        .bind(dv.next_steps.clone().unwrap_or_default())
        .bind(meta.get("raw_text").and_then(|v| v.as_str()))
        .fetch_one(&mut **tx)
        .await
        .map_err(map_db)?;
        created.push(json!({ "type": "devlog_session", "id": sid }));
    }
    if let Some(summary) = &p.summary {
        let week = meta.get("iso_week").and_then(|v| v.as_str()).map(str::to_string).unwrap_or_else(|| domain::time::iso_week_key(reference_date));
        let start = domain::time::parse_iso_week(&week).map_err(|e| e.message)?.start;
        sqlx::query(
            "INSERT INTO weekly_reviews (user_id, iso_week, week_start, what_happened, summary) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (user_id, iso_week) DO UPDATE SET
                what_happened = CASE WHEN weekly_reviews.what_happened = '' THEN EXCLUDED.what_happened ELSE weekly_reviews.what_happened END,
                summary = coalesce(weekly_reviews.summary, '{}') || EXCLUDED.summary, version = weekly_reviews.version + 1",
        )
        .bind(user_id)
        .bind(&week)
        .bind(start)
        .bind(summary)
        .bind(json!({ "assisted_summary": summary }))
        .execute(&mut **tx)
        .await
        .map_err(map_db)?;
        created.push(json!({ "type": "weekly_review", "iso_week": week }));
    }
    if let Some(goal_id) = meta.get("goal_id").and_then(|v| v.as_str()).and_then(|v| v.parse::<Uuid>().ok()) {
        // Sugerencia de hábitos para un objetivo existente.
        let period: Option<(NaiveDate, NaiveDate)> = sqlx::query_as("SELECT starts_on, ends_on FROM goals WHERE user_id = $1 AND id = $2")
            .bind(user_id)
            .bind(goal_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(map_db)?;
        if let Some((s, e)) = period {
            for h in meta.get("habits").and_then(|v| v.as_array()).into_iter().flatten().filter_map(|x| x.as_str()) {
                let id = create_goal_habit(tx, user_id, goal_id, h, s, Some(e)).await.map_err(map_db)?;
                created.push(json!({ "type": "habit", "id": id }));
            }
        }
    }
    Ok(json!({ "created": created }))
}

/// Hábito diario de un objetivo con vigencia del periodo del objetivo.
pub async fn create_goal_habit(tx: &mut Tx<'_>, user_id: Uuid, goal_id: Uuid, name: &str, from: NaiveDate, to: Option<NaiveDate>) -> sqlx::Result<Uuid> {
    let hid: Uuid = sqlx::query_scalar("INSERT INTO habits (user_id, name, kind, goal_id, created_on) VALUES ($1, $2, 'goal', $3, $4) RETURNING id")
        .bind(user_id)
        .bind(name.trim())
        .bind(goal_id)
        .bind(from)
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO habit_versions (user_id, habit_id, valid_from, valid_to, frequency) VALUES ($1, $2, $3, $4, 'daily')")
        .bind(user_id)
        .bind(hid)
        .bind(from)
        .bind(to)
        .execute(&mut **tx)
        .await?;
    Ok(hid)
}
