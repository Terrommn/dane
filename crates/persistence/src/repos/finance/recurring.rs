//! Suscripciones e ingresos fijos (T40). Una ocurrencia por regla y periodo (`UNIQUE(recurring_rule_id, recurrence_period)`),
//! materializada como transacción `pending_reconciliation`: la fecha programada no prueba el cobro. Confirmar/corregir
//! actualiza esa misma fila. Las ocurrencias copian el monto al crearse, así que cambiar o cancelar la regla no altera
//! la historia.

use chrono::{DateTime, Datelike, Months, NaiveDate, Utc};
use domain::time::DayOfMonth;
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct RecurringRule {
    pub id: Uuid,
    /// `subscription | fixed_income`
    pub kind: String,
    pub name: String,
    pub amount_cents: i64,
    pub currency: String,
    /// `day | last | penultimate`
    pub day_kind: String,
    pub day: Option<i16>,
    pub category_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// `active | cancelled | ended`
    pub status: String,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub notes_md: String,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const RULE_COLS: &str =
    "id, kind, name, amount_cents, currency::text AS currency, day_kind, day, category_id, account_id,
    area_id, project_id, status, starts_on, ends_on, notes_md, version, updated_at";

impl RecurringRule {
    pub fn day_rule(&self) -> DayOfMonth {
        DayOfMonth::from_parts(&self.day_kind, self.day).unwrap_or(DayOfMonth::Last)
    }
    /// Fecha de la ocurrencia del mes si la regla está vigente ese mes.
    pub fn occurrence_in(&self, year: i32, month: u32) -> Option<NaiveDate> {
        let d = self.day_rule().resolve(year, month);
        let in_range = d >= self.starts_on && self.ends_on.is_none_or(|e| d <= e);
        in_range.then_some(d)
    }
}

pub async fn list(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<RecurringRule>> {
    Ok(sqlx::query_as::<_, RecurringRule>(crate::sqlf!(
        "SELECT {RULE_COLS} FROM fin_recurring_rules WHERE user_id = $1 ORDER BY kind, status, name"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<RecurringRule> {
    sqlx::query_as::<_, RecurringRule>(crate::sqlf!(
        "SELECT {RULE_COLS} FROM fin_recurring_rules WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct RuleInput<'a> {
    pub kind: &'a str,
    pub name: &'a str,
    pub amount_cents: i64,
    pub currency: &'a str,
    pub day_kind: &'a str,
    pub day: Option<i16>,
    pub category_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub status: &'a str,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub notes_md: &'a str,
}

pub async fn create(db: impl PgExecutor<'_>, user_id: Uuid, r: &RuleInput<'_>) -> RepoResult<RecurringRule> {
    Ok(sqlx::query_as::<_, RecurringRule>(crate::sqlf!(
        "INSERT INTO fin_recurring_rules (user_id, kind, name, amount_cents, currency, day_kind, day, category_id,
            account_id, area_id, project_id, status, starts_on, ends_on, notes_md)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) RETURNING {RULE_COLS}"
    ))
    .bind(user_id)
    .bind(r.kind)
    .bind(r.name)
    .bind(r.amount_cents)
    .bind(r.currency)
    .bind(r.day_kind)
    .bind(r.day)
    .bind(r.category_id)
    .bind(r.account_id)
    .bind(r.area_id)
    .bind(r.project_id)
    .bind(r.status)
    .bind(r.starts_on)
    .bind(r.ends_on)
    .bind(r.notes_md)
    .fetch_one(db)
    .await?)
}

pub async fn update(
    db: impl PgExecutor<'_> + Copy,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    r: &RuleInput<'_>,
) -> RepoResult<RecurringRule> {
    let row = sqlx::query_as::<_, RecurringRule>(crate::sqlf!(
        "UPDATE fin_recurring_rules SET name = $4, amount_cents = $5, currency = $6, day_kind = $7, day = $8,
            category_id = $9, account_id = $10, area_id = $11, project_id = $12, status = $13, starts_on = $14,
            ends_on = $15, notes_md = $16, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {RULE_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(r.name)
    .bind(r.amount_cents)
    .bind(r.currency)
    .bind(r.day_kind)
    .bind(r.day)
    .bind(r.category_id)
    .bind(r.account_id)
    .bind(r.area_id)
    .bind(r.project_id)
    .bind(r.status)
    .bind(r.starts_on)
    .bind(r.ends_on)
    .bind(r.notes_md)
    .fetch_optional(db)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(db, user_id, id).await)),
    }
}

/// Inserta la ocurrencia del periodo si no existe. Devuelve `true` si la creó.
pub async fn materialize_one(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    rule: &RecurringRule,
    on: NaiveDate,
) -> RepoResult<bool> {
    let period = format!("{:04}-{:02}", on.year(), on.month());
    let (kind, concept) = if rule.kind == "subscription" {
        ("expense", format!("Suscripción {}", rule.name))
    } else {
        ("income", rule.name.clone())
    };
    let n = sqlx::query(
        "INSERT INTO transactions (user_id, kind, status, origin, occurred_on, concept, amount_cents, currency,
            category_id, account_id, area_id, project_id, recurring_rule_id, recurrence_period)
         VALUES ($1, $2, 'pending_reconciliation', 'recurring', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (recurring_rule_id, recurrence_period) DO NOTHING",
    )
    .bind(user_id)
    .bind(kind)
    .bind(on)
    .bind(concept)
    .bind(rule.amount_cents)
    .bind(&rule.currency)
    .bind(rule.category_id)
    .bind(rule.account_id)
    .bind(rule.area_id)
    .bind(rule.project_id)
    .bind(rule.id)
    .bind(period)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Meses de catch-up hacia atrás como máximo (evita inventar años de historia).
pub const CATCH_UP_MONTHS: u32 = 12;

/// Materializa todas las ocurrencias vencidas (fecha ≤ hoy local) de las reglas activas del usuario.
/// Idempotente y seguro con varios procesos (ON CONFLICT). Devuelve cuántas creó.
pub async fn materialize_due_for_user(pool: &PgPool, user_id: Uuid, today: NaiveDate) -> RepoResult<u64> {
    let rules: Vec<RecurringRule> = list(pool, user_id)
        .await?
        .into_iter()
        .filter(|r| r.status == "active")
        .collect();
    let floor = today
        .checked_sub_months(Months::new(CATCH_UP_MONTHS))
        .unwrap_or(today);
    let mut created = 0;
    let mut tx = pool.begin().await?;
    for r in &rules {
        let from = r.starts_on.max(floor);
        for (y, m) in domain::time::months_between(from, today) {
            if let Some(d) = r.occurrence_in(y, m)
                && d <= today
                && materialize_one(&mut tx, user_id, r, d).await?
            {
                created += 1;
            }
        }
    }
    tx.commit().await?;
    Ok(created)
}

/// Usuarios con reglas activas y su zona horaria.
pub async fn users_with_active_rules(db: impl PgExecutor<'_>) -> RepoResult<Vec<(Uuid, String)>> {
    Ok(sqlx::query_as(
        "SELECT DISTINCT r.user_id, coalesce(s.timezone, 'America/Monterrey')
         FROM fin_recurring_rules r LEFT JOIN user_settings s ON s.user_id = r.user_id WHERE r.status = 'active'",
    )
    .fetch_all(db)
    .await?)
}

/// Ocurrencias registradas (no anuladas) de un periodo: (rule_id, transaction_id, status, amount, fecha).
#[derive(Debug, Clone, FromRow)]
pub struct OccurrenceRow {
    pub rule_id: Uuid,
    pub transaction_id: Uuid,
    pub status: String,
    pub amount_cents: i64,
    pub occurred_on: NaiveDate,
    pub version: i32,
}

pub async fn occurrences(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    period: &str,
) -> RepoResult<Vec<OccurrenceRow>> {
    Ok(sqlx::query_as::<_, OccurrenceRow>(
        "SELECT recurring_rule_id AS rule_id, id AS transaction_id, status, amount_cents, occurred_on, version
         FROM transactions WHERE user_id = $1 AND recurrence_period = $2 AND recurring_rule_id IS NOT NULL",
    )
    .bind(user_id)
    .bind(period)
    .fetch_all(db)
    .await?)
}
