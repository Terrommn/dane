//! Propietario: agente C-finance. Acceso a datos financieros (T36–T43). Todas las consultas filtran por `user_id`.
//! CONTRATO con el dashboard general (agente A): mantener firma y campos de `period_summary`.

pub mod budgets;
pub mod recurring;

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use domain::finance::{self as f, AccountKind, Movement};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

// ---------------------------------------------------------------- Resumen (contrato)

/// Resumen financiero confirmado de un periodo, en centavos de la moneda base del usuario.
/// Solo movimientos `confirmed` en la moneda base; los de otra moneda sin conversión se cuentan aparte.
#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct FinanceSummary {
    /// Consumo (gastos con rol consumption/payroll), una sola vez aunque se pague con tarjeta.
    pub consumption_cents: i64,
    /// Pagos de deuda (pagos de tarjeta + abonos a deudas).
    pub debt_payments_cents: i64,
    /// Ingreso ganado (rol earned).
    pub earned_income_cents: i64,
    /// Financiamiento recibido (préstamos).
    pub financing_cents: i64,
    pub investment_cents: i64,
    /// Entradas y salidas de cuentas de efectivo/débito (flujo de efectivo).
    pub cash_in_cents: i64,
    pub cash_out_cents: i64,
    /// Movimientos en otra moneda excluidos de los totales.
    pub excluded_foreign_count: i64,
}

impl From<f::Totals> for FinanceSummary {
    fn from(t: f::Totals) -> Self {
        FinanceSummary {
            consumption_cents: t.consumption_cents,
            debt_payments_cents: t.debt_payments_cents,
            earned_income_cents: t.earned_income_cents,
            financing_cents: t.financing_cents,
            investment_cents: t.investment_cents,
            cash_in_cents: t.cash_in_cents,
            cash_out_cents: t.cash_out_cents,
            excluded_foreign_count: t.excluded_foreign_count,
        }
    }
}

/// Resumen del periodo `[start, end]` (fechas locales inclusivas). Una sola consulta (firma del contrato).
pub async fn period_summary(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    start: NaiveDate,
    end: NaiveDate,
) -> RepoResult<FinanceSummary> {
    #[derive(FromRow)]
    struct Row {
        #[sqlx(flatten)]
        m: MovementRow,
        acc_kind: Option<String>,
        counter_kind: Option<String>,
        base: String,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT t.id, t.kind, t.status, t.amount_cents, t.currency::text AS currency, t.occurred_on, t.occurred_at,
                t.created_at, c.role, t.account_id, t.counter_account_id, t.adjustment_sign,
                (t.financed_by_person_id IS NOT NULL) AS financed,
                coalesce(r.kind = 'subscription', false) AS from_subscription,
                a.kind AS acc_kind, ca.kind AS counter_kind,
                coalesce((SELECT currency::text FROM user_settings WHERE user_id = $1), 'MXN') AS base
         FROM transactions t
         LEFT JOIN fin_categories c ON c.id = t.category_id
         LEFT JOIN fin_recurring_rules r ON r.id = t.recurring_rule_id
         LEFT JOIN fin_accounts a ON a.id = t.account_id
         LEFT JOIN fin_accounts ca ON ca.id = t.counter_account_id
         WHERE t.user_id = $1 AND t.status = 'confirmed' AND t.occurred_on BETWEEN $2 AND $3",
    )
    .bind(user_id)
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;
    let mut kinds = HashMap::new();
    let mut base = "MXN".to_string();
    let mut ms = Vec::with_capacity(rows.len());
    for r in rows {
        base = r.base;
        for (id, k) in [
            (r.m.account_id, r.acc_kind),
            (r.m.counter_account_id, r.counter_kind),
        ] {
            if let (Some(id), Some(k)) = (id, k.and_then(|k| AccountKind::parse(&k).ok())) {
                kinds.insert(id, k);
            }
        }
        if let Some(m) = r.m.into_domain() {
            ms.push(m);
        }
    }
    Ok(f::summarize(&ms, &kinds, &base).into())
}

pub async fn base_currency(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<String> {
    Ok(
        sqlx::query_scalar("SELECT currency::text FROM user_settings WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?
            .unwrap_or_else(|| "MXN".into()),
    )
}

pub async fn user_timezone(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<String> {
    Ok(
        sqlx::query_scalar("SELECT timezone FROM user_settings WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?
            .unwrap_or_else(|| "America/Monterrey".into()),
    )
}

// ---------------------------------------------------------------- Movimientos para cálculo

#[derive(FromRow)]
struct MovementRow {
    id: Uuid,
    kind: String,
    status: String,
    amount_cents: i64,
    currency: String,
    occurred_on: NaiveDate,
    occurred_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    role: Option<String>,
    account_id: Option<Uuid>,
    counter_account_id: Option<Uuid>,
    adjustment_sign: Option<i16>,
    financed: bool,
    from_subscription: bool,
}

impl MovementRow {
    fn into_domain(self) -> Option<Movement> {
        Some(Movement {
            id: self.id,
            kind: f::Kind::parse(&self.kind).ok()?,
            status: f::Status::parse(&self.status),
            amount_cents: self.amount_cents,
            currency: self.currency,
            occurred_on: self.occurred_on,
            occurred_at: self.occurred_at,
            created_at: self.created_at,
            role: self.role.as_deref().and_then(f::Role::parse),
            account_id: self.account_id,
            counter_account_id: self.counter_account_id,
            adjustment_sign: self.adjustment_sign.unwrap_or(1),
            financed_by_person: self.financed,
            from_subscription: self.from_subscription,
        })
    }
}

/// Movimientos no anulados en un rango (incluye pendientes; los cálculos filtran por estado).
/// `account`: solo los que tocan esa cuenta (origen o contraparte).
pub async fn movements(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    account: Option<Uuid>,
) -> RepoResult<Vec<Movement>> {
    let rows = sqlx::query_as::<_, MovementRow>(
        "SELECT t.id, t.kind, t.status, t.amount_cents, t.currency::text AS currency, t.occurred_on, t.occurred_at,
                t.created_at, c.role, t.account_id, t.counter_account_id, t.adjustment_sign,
                (t.financed_by_person_id IS NOT NULL) AS financed,
                coalesce(r.kind = 'subscription', false) AS from_subscription
         FROM transactions t
         LEFT JOIN fin_categories c ON c.id = t.category_id
         LEFT JOIN fin_recurring_rules r ON r.id = t.recurring_rule_id
         WHERE t.user_id = $1 AND t.status <> 'void'
           AND ($2::date IS NULL OR t.occurred_on >= $2) AND ($3::date IS NULL OR t.occurred_on <= $3)
           AND ($4::uuid IS NULL OR t.account_id = $4 OR t.counter_account_id = $4)
         ORDER BY t.occurred_on, t.created_at",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(account)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(MovementRow::into_domain).collect())
}

pub async fn account_kinds(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<HashMap<Uuid, AccountKind>> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, kind FROM fin_accounts WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, k)| AccountKind::parse(&k).ok().map(|k| (id, k)))
        .collect())
}

// ---------------------------------------------------------------- Categorías

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    /// `expense | income`
    pub kind: String,
    /// `consumption | debt_payment | investment | payroll | earned | financing | other_income`
    pub role: String,
    pub color: Option<String>,
    pub position: i32,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
}

const CAT_COLS: &str = "id, name, kind, role, color, position, archived_at, version";

pub async fn list_categories(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    archived: bool,
) -> RepoResult<Vec<Category>> {
    Ok(sqlx::query_as::<_, Category>(crate::sqlf!(
        "SELECT {CAT_COLS} FROM fin_categories WHERE user_id = $1 AND ($2 OR archived_at IS NULL) ORDER BY kind, position, name"
    ))
    .bind(user_id)
    .bind(archived)
    .fetch_all(db)
    .await?)
}

pub async fn get_category(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Category> {
    sqlx::query_as::<_, Category>(crate::sqlf!(
        "SELECT {CAT_COLS} FROM fin_categories WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct CategoryInput<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub role: &'a str,
    pub color: Option<&'a str>,
    pub position: i32,
    pub archived: bool,
}

pub async fn create_category(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    c: &CategoryInput<'_>,
) -> RepoResult<Category> {
    Ok(sqlx::query_as::<_, Category>(crate::sqlf!(
        "INSERT INTO fin_categories (user_id, name, kind, role, color, position) VALUES ($1, $2, $3, $4, $5, $6) RETURNING {CAT_COLS}"
    ))
    .bind(user_id)
    .bind(c.name)
    .bind(c.kind)
    .bind(c.role)
    .bind(c.color)
    .bind(c.position)
    .fetch_one(db)
    .await?)
}

pub async fn update_category(
    db: impl PgExecutor<'_> + Copy,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    c: &CategoryInput<'_>,
) -> RepoResult<Category> {
    let row = sqlx::query_as::<_, Category>(crate::sqlf!(
        "UPDATE fin_categories SET name = $4, role = $5, color = $6, position = $7,
            archived_at = CASE WHEN $8 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {CAT_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(c.name)
    .bind(c.role)
    .bind(c.color)
    .bind(c.position)
    .bind(c.archived)
    .fetch_optional(db)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_category(db, user_id, id).await)),
    }
}

// ---------------------------------------------------------------- Cuentas y tarjetas

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Account {
    pub id: Uuid,
    pub name: String,
    pub bank: String,
    /// `debit | cash | savings | credit_card`
    pub kind: String,
    pub currency: String,
    pub credit_limit_cents: Option<i64>,
    pub cut_day: Option<i16>,
    pub pay_day: Option<i16>,
    /// `active | to_cancel | cancelled | closed`
    pub status: String,
    pub is_default_debit: bool,
    pub notes_md: String,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const ACC_COLS: &str =
    "id, name, bank, kind, currency::text AS currency, credit_limit_cents, cut_day, pay_day, status,
    is_default_debit, notes_md, archived_at, version, updated_at";

pub async fn list_accounts(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    archived: bool,
) -> RepoResult<Vec<Account>> {
    Ok(sqlx::query_as::<_, Account>(crate::sqlf!(
        "SELECT {ACC_COLS} FROM fin_accounts WHERE user_id = $1 AND ($2 OR archived_at IS NULL)
         ORDER BY (kind = 'credit_card'), name"
    ))
    .bind(user_id)
    .bind(archived)
    .fetch_all(db)
    .await?)
}

pub async fn get_account(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Account> {
    sqlx::query_as::<_, Account>(crate::sqlf!(
        "SELECT {ACC_COLS} FROM fin_accounts WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct AccountInput<'a> {
    pub name: &'a str,
    pub bank: &'a str,
    pub kind: &'a str,
    pub currency: &'a str,
    pub credit_limit_cents: Option<i64>,
    pub cut_day: Option<i16>,
    pub pay_day: Option<i16>,
    pub status: &'a str,
    pub is_default_debit: bool,
    pub notes_md: &'a str,
    pub archived: bool,
}

pub async fn create_account(tx: &mut Tx<'_>, user_id: Uuid, a: &AccountInput<'_>) -> RepoResult<Account> {
    if a.is_default_debit {
        clear_default_debit(tx, user_id).await?;
    }
    Ok(sqlx::query_as::<_, Account>(crate::sqlf!(
        "INSERT INTO fin_accounts (user_id, name, bank, kind, currency, credit_limit_cents, cut_day, pay_day, status,
            is_default_debit, notes_md)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING {ACC_COLS}"
    ))
    .bind(user_id)
    .bind(a.name)
    .bind(a.bank)
    .bind(a.kind)
    .bind(a.currency)
    .bind(a.credit_limit_cents)
    .bind(a.cut_day)
    .bind(a.pay_day)
    .bind(a.status)
    .bind(a.is_default_debit)
    .bind(a.notes_md)
    .fetch_one(&mut **tx)
    .await?)
}

async fn clear_default_debit(tx: &mut Tx<'_>, user_id: Uuid) -> RepoResult<()> {
    sqlx::query("UPDATE fin_accounts SET is_default_debit = false WHERE user_id = $1 AND is_default_debit")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn update_account(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    a: &AccountInput<'_>,
) -> RepoResult<Account> {
    if a.is_default_debit {
        clear_default_debit(tx, user_id).await?;
    }
    // El tipo de cuenta no cambia: cambiarlo alteraría la semántica de movimientos históricos.
    let row = sqlx::query_as::<_, Account>(crate::sqlf!(
        "UPDATE fin_accounts SET name = $4, bank = $5, currency = $6, credit_limit_cents = $7, cut_day = $8,
            pay_day = $9, status = $10, is_default_debit = $11, notes_md = $12,
            archived_at = CASE WHEN $13 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {ACC_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(a.name)
    .bind(a.bank)
    .bind(a.currency)
    .bind(a.credit_limit_cents)
    .bind(a.cut_day)
    .bind(a.pay_day)
    .bind(a.status)
    .bind(a.is_default_debit)
    .bind(a.notes_md)
    .bind(a.archived)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_account(&mut **tx, user_id, id).await)),
    }
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct BalanceSnapshot {
    pub id: Uuid,
    pub account_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub observed_on: NaiveDate,
    /// Débito: saldo. Tarjeta: crédito disponible.
    pub balance_cents: i64,
    /// `instant | legacy_day_start_inclusive | legacy_after_day`
    pub semantics: String,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

const SNAP_COLS: &str =
    "id, account_id, observed_at, observed_on, balance_cents, semantics, note, created_at";

#[allow(clippy::too_many_arguments)]
pub async fn add_snapshot(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    account_id: Uuid,
    observed_at: DateTime<Utc>,
    observed_on: NaiveDate,
    balance_cents: i64,
    semantics: &str,
    note: &str,
) -> RepoResult<BalanceSnapshot> {
    Ok(sqlx::query_as::<_, BalanceSnapshot>(crate::sqlf!(
        "INSERT INTO balance_snapshots (user_id, account_id, observed_at, observed_on, balance_cents, semantics, note)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {SNAP_COLS}"
    ))
    .bind(user_id)
    .bind(account_id)
    .bind(observed_at)
    .bind(observed_on)
    .bind(balance_cents)
    .bind(semantics)
    .bind(note)
    .fetch_one(db)
    .await?)
}

pub async fn list_snapshots(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    account_id: Uuid,
) -> RepoResult<Vec<BalanceSnapshot>> {
    Ok(sqlx::query_as::<_, BalanceSnapshot>(crate::sqlf!(
        "SELECT {SNAP_COLS} FROM balance_snapshots WHERE user_id = $1 AND account_id = $2 ORDER BY observed_at DESC LIMIT 100"
    ))
    .bind(user_id)
    .bind(account_id)
    .fetch_all(db)
    .await?)
}

pub async fn latest_snapshot(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    account_id: Uuid,
) -> RepoResult<Option<BalanceSnapshot>> {
    Ok(sqlx::query_as::<_, BalanceSnapshot>(crate::sqlf!(
        "SELECT {SNAP_COLS} FROM balance_snapshots WHERE user_id = $1 AND account_id = $2
         ORDER BY observed_at DESC, created_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .bind(account_id)
    .fetch_optional(db)
    .await?)
}

/// Saldo/disponible estimado de una cuenta a partir del último snapshot (None si no hay snapshot).
pub async fn estimate_account(
    db: impl PgExecutor<'_> + Copy,
    user_id: Uuid,
    account: &Account,
) -> RepoResult<Option<(BalanceSnapshot, i64)>> {
    let Some(s) = latest_snapshot(db, user_id, account.id).await? else {
        return Ok(None);
    };
    let ms = movements(db, user_id, Some(s.observed_on), None, Some(account.id)).await?;
    let snap = f::Snapshot {
        balance_cents: s.balance_cents,
        observed_at: s.observed_at,
        observed_on: s.observed_on,
        semantics: f::Semantics::parse(&s.semantics).unwrap_or(f::Semantics::Instant),
    };
    let est = f::estimate_balance(&snap, account.id, &account.currency, &ms);
    Ok(Some((s, est)))
}

// ---------------------------------------------------------------- Transacciones

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Transaction {
    pub id: Uuid,
    /// `expense | income | transfer | card_payment | debt_payment | loan_received | adjustment`
    pub kind: String,
    /// `confirmed | pending_reconciliation | void`
    pub status: String,
    /// `manual | recurring | import | llm | bot`
    pub origin: String,
    pub occurred_on: NaiveDate,
    pub occurred_at: Option<DateTime<Utc>>,
    pub concept: String,
    pub amount_cents: i64,
    pub currency: String,
    pub original_amount_cents: Option<i64>,
    pub original_currency: Option<String>,
    #[schema(value_type = Option<String>)]
    pub fx_rate: Option<Decimal>,
    pub fx_date: Option<NaiveDate>,
    pub fx_source: Option<String>,
    pub category_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub counter_account_id: Option<Uuid>,
    pub debt_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub financed_by_person_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub budget_item_id: Option<Uuid>,
    pub recurring_rule_id: Option<Uuid>,
    pub recurrence_period: Option<String>,
    pub idempotency_key: Option<String>,
    pub adjustment_sign: Option<i16>,
    pub notes_md: String,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const TX_COLS: &str = "id, kind, status, origin, occurred_on, occurred_at, concept, amount_cents, currency::text AS currency,
    original_amount_cents, original_currency::text AS original_currency, fx_rate, fx_date, fx_source, category_id,
    account_id, counter_account_id, debt_id, person_id, financed_by_person_id, project_id, area_id, budget_item_id,
    recurring_rule_id, recurrence_period, idempotency_key, adjustment_sign, notes_md, confirmed_at, version, created_at,
    updated_at";

/// Datos validados de un movimiento.
#[derive(Debug, Clone)]
pub struct TxInput {
    pub kind: String,
    pub status: String,
    pub origin: String,
    pub occurred_on: NaiveDate,
    pub occurred_at: Option<DateTime<Utc>>,
    pub concept: String,
    pub amount_cents: i64,
    pub currency: String,
    pub original_amount_cents: Option<i64>,
    pub original_currency: Option<String>,
    pub fx_rate: Option<Decimal>,
    pub fx_date: Option<NaiveDate>,
    pub fx_source: Option<String>,
    pub category_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub counter_account_id: Option<Uuid>,
    pub debt_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub financed_by_person_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub budget_item_id: Option<Uuid>,
    pub adjustment_sign: Option<i16>,
    pub notes_md: String,
}

/// Inserta un movimiento. Con `idempotency_key`, un reintento devuelve el existente (`created = false`).
pub async fn insert_transaction(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    t: &TxInput,
    idempotency_key: Option<&str>,
    recorded_at: DateTime<Utc>,
) -> RepoResult<(Transaction, bool)> {
    let row = sqlx::query_as::<_, Transaction>(crate::sqlf!(
        "INSERT INTO transactions (user_id, kind, status, origin, occurred_on, occurred_at, concept, amount_cents, currency,
            original_amount_cents, original_currency, fx_rate, fx_date, fx_source, category_id, account_id,
            counter_account_id, debt_id, person_id, financed_by_person_id, project_id, area_id, budget_item_id,
            adjustment_sign, notes_md, idempotency_key, confirmed_at, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22,
            $23, $24, $25, $26, CASE WHEN $3 = 'confirmed' THEN $27 END, $27)
         ON CONFLICT (user_id, idempotency_key) DO NOTHING
         RETURNING {TX_COLS}"
    ))
    .bind(user_id)
    .bind(&t.kind)
    .bind(&t.status)
    .bind(&t.origin)
    .bind(t.occurred_on)
    .bind(t.occurred_at)
    .bind(&t.concept)
    .bind(t.amount_cents)
    .bind(&t.currency)
    .bind(t.original_amount_cents)
    .bind(&t.original_currency)
    .bind(t.fx_rate)
    .bind(t.fx_date)
    .bind(&t.fx_source)
    .bind(t.category_id)
    .bind(t.account_id)
    .bind(t.counter_account_id)
    .bind(t.debt_id)
    .bind(t.person_id)
    .bind(t.financed_by_person_id)
    .bind(t.project_id)
    .bind(t.area_id)
    .bind(t.budget_item_id)
    .bind(t.adjustment_sign)
    .bind(&t.notes_md)
    .bind(idempotency_key)
    .bind(recorded_at)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => Ok((r, true)),
        None => {
            let existing = sqlx::query_as::<_, Transaction>(crate::sqlf!(
                "SELECT {TX_COLS} FROM transactions WHERE user_id = $1 AND idempotency_key = $2"
            ))
            .bind(user_id)
            .bind(idempotency_key)
            .fetch_one(&mut **tx)
            .await?;
            Ok((existing, false))
        }
    }
}

pub async fn get_transaction(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Transaction> {
    sqlx::query_as::<_, Transaction>(crate::sqlf!(
        "SELECT {TX_COLS} FROM transactions WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

/// Actualiza campos editables de un movimiento (no cambia el tipo ni el origen).
pub async fn update_transaction(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    t: &TxInput,
) -> RepoResult<Transaction> {
    let row = sqlx::query_as::<_, Transaction>(crate::sqlf!(
        "UPDATE transactions SET occurred_on = $4, occurred_at = $5, concept = $6, amount_cents = $7, currency = $8,
            original_amount_cents = $9, original_currency = $10, fx_rate = $11, fx_date = $12, fx_source = $13,
            category_id = $14, account_id = $15, counter_account_id = $16, debt_id = $17, person_id = $18,
            financed_by_person_id = $19, project_id = $20, area_id = $21, budget_item_id = $22, adjustment_sign = $23,
            notes_md = $24, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 AND status <> 'void' RETURNING {TX_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(t.occurred_on)
    .bind(t.occurred_at)
    .bind(&t.concept)
    .bind(t.amount_cents)
    .bind(&t.currency)
    .bind(t.original_amount_cents)
    .bind(&t.original_currency)
    .bind(t.fx_rate)
    .bind(t.fx_date)
    .bind(&t.fx_source)
    .bind(t.category_id)
    .bind(t.account_id)
    .bind(t.counter_account_id)
    .bind(t.debt_id)
    .bind(t.person_id)
    .bind(t.financed_by_person_id)
    .bind(t.project_id)
    .bind(t.area_id)
    .bind(t.budget_item_id)
    .bind(t.adjustment_sign)
    .bind(&t.notes_md)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_transaction(&mut **tx, user_id, id).await)),
    }
}

pub async fn set_transaction_status(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    from: &[&str],
    to: &str,
) -> RepoResult<Transaction> {
    let from: Vec<String> = from.iter().map(|s| s.to_string()).collect();
    let row = sqlx::query_as::<_, Transaction>(crate::sqlf!(
        "UPDATE transactions SET status = $4, version = version + 1,
            confirmed_at = CASE WHEN $4 = 'confirmed' THEN coalesce(confirmed_at, now()) ELSE confirmed_at END
         WHERE user_id = $1 AND id = $2 AND version = $3 AND status = ANY($5) RETURNING {TX_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(to)
    .bind(&from)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_transaction(&mut **tx, user_id, id).await)),
    }
}

pub async fn set_transaction_debt(tx: &mut Tx<'_>, user_id: Uuid, id: Uuid, debt_id: Uuid) -> RepoResult<()> {
    sqlx::query("UPDATE transactions SET debt_id = $3 WHERE user_id = $1 AND id = $2")
        .bind(user_id)
        .bind(id)
        .bind(debt_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Filtros de lista de movimientos.
#[derive(Debug, Clone, Default)]
pub struct TxFilter {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub category_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub debt_id: Option<Uuid>,
    pub q: Option<String>,
    /// `date_desc` (defecto) | `date_asc` | `amount_desc` | `amount_asc`
    pub sort: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

const TX_WHERE: &str = "user_id = $1
    AND ($2::date IS NULL OR occurred_on >= $2) AND ($3::date IS NULL OR occurred_on <= $3)
    AND ($4::text IS NULL OR kind = $4)
    AND (($5::text IS NULL AND status <> 'void') OR status = $5)
    AND ($6::uuid IS NULL OR category_id = $6)
    AND ($7::uuid IS NULL OR account_id = $7 OR counter_account_id = $7)
    AND ($8::uuid IS NULL OR project_id = $8)
    AND ($9::uuid IS NULL OR person_id = $9 OR financed_by_person_id = $9)
    AND ($10::uuid IS NULL OR debt_id = $10)
    AND ($11::text IS NULL OR immutable_unaccent(lower(concept)) LIKE '%' || immutable_unaccent(lower($11)) || '%')";

macro_rules! bind_filter {
    ($q:expr, $user:expr, $f:expr) => {
        $q.bind($user)
            .bind($f.from)
            .bind($f.to)
            .bind(&$f.kind)
            .bind(&$f.status)
            .bind($f.category_id)
            .bind($f.account_id)
            .bind($f.project_id)
            .bind($f.person_id)
            .bind($f.debt_id)
            .bind(&$f.q)
    };
}

/// Totales de una lista filtrada (confirmados en la moneda base; pendientes aparte).
#[derive(Debug, Clone, Default, FromRow, Serialize, ToSchema)]
pub struct TxTotals {
    pub count: i64,
    pub expense_cents: i64,
    pub income_cents: i64,
    pub pending_cents: i64,
    pub foreign_count: i64,
}

pub async fn list_transactions(
    db: impl PgExecutor<'_> + Copy,
    user_id: Uuid,
    base: &str,
    f: &TxFilter,
) -> RepoResult<(Vec<Transaction>, TxTotals)> {
    let order = match f.sort.as_deref() {
        Some("date_asc") => "occurred_on ASC, created_at ASC",
        Some("amount_desc") => "amount_cents DESC, occurred_on DESC",
        Some("amount_asc") => "amount_cents ASC, occurred_on DESC",
        _ => "occurred_on DESC, created_at DESC",
    };
    let items = bind_filter!(
        sqlx::query_as::<_, Transaction>(crate::sqlf!(
            "SELECT {TX_COLS} FROM transactions WHERE {TX_WHERE} ORDER BY {order} LIMIT $12 OFFSET $13"
        )),
        user_id,
        f
    )
    .bind(f.limit)
    .bind(f.offset)
    .fetch_all(db)
    .await?;
    let totals = bind_filter!(
        sqlx::query_as::<_, TxTotals>(crate::sqlf!(
            "SELECT count(*) AS count,
                coalesce(sum(amount_cents) FILTER (WHERE status = 'confirmed' AND currency = $12 AND kind = 'expense'), 0)::bigint AS expense_cents,
                coalesce(sum(amount_cents) FILTER (WHERE status = 'confirmed' AND currency = $12 AND kind IN ('income', 'loan_received')), 0)::bigint AS income_cents,
                coalesce(sum(amount_cents) FILTER (WHERE status = 'pending_reconciliation'), 0)::bigint AS pending_cents,
                count(*) FILTER (WHERE currency <> $12) AS foreign_count
             FROM transactions WHERE {TX_WHERE}"
        )),
        user_id,
        f
    )
    .bind(base)
    .fetch_one(db)
    .await?;
    Ok((items, totals))
}

// ---------------------------------------------------------------- Deudas

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Debt {
    pub id: Uuid,
    pub name: String,
    /// `credit_card | loan | family | other`
    pub kind: String,
    pub account_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub principal_cents: Option<i64>,
    pub currency: String,
    pub opened_on: NaiveDate,
    /// `open | paid | cancelled`
    pub status: String,
    pub paid_on: Option<NaiveDate>,
    pub goal_id: Option<Uuid>,
    pub notes_md: String,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const DEBT_COLS: &str =
    "id, name, kind, account_id, person_id, principal_cents, currency::text AS currency, opened_on,
    status, paid_on, goal_id, notes_md, version, updated_at";

pub async fn list_debts(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<Debt>> {
    Ok(sqlx::query_as::<_, Debt>(crate::sqlf!(
        "SELECT {DEBT_COLS} FROM debts WHERE user_id = $1 ORDER BY (status = 'open') DESC, opened_on DESC"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn get_debt(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Debt> {
    sqlx::query_as::<_, Debt>(crate::sqlf!(
        "SELECT {DEBT_COLS} FROM debts WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct DebtInput<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub account_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub principal_cents: Option<i64>,
    pub currency: &'a str,
    pub opened_on: NaiveDate,
    pub goal_id: Option<Uuid>,
    pub notes_md: &'a str,
    pub cancelled: bool,
}

pub async fn create_debt(db: impl PgExecutor<'_>, user_id: Uuid, d: &DebtInput<'_>) -> RepoResult<Debt> {
    Ok(sqlx::query_as::<_, Debt>(crate::sqlf!(
        "INSERT INTO debts (user_id, name, kind, account_id, person_id, principal_cents, currency, opened_on, goal_id, notes_md)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING {DEBT_COLS}"
    ))
    .bind(user_id)
    .bind(d.name)
    .bind(d.kind)
    .bind(d.account_id)
    .bind(d.person_id)
    .bind(d.principal_cents)
    .bind(d.currency)
    .bind(d.opened_on)
    .bind(d.goal_id)
    .bind(d.notes_md)
    .fetch_one(db)
    .await?)
}

pub async fn update_debt(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    d: &DebtInput<'_>,
) -> RepoResult<Debt> {
    let row = sqlx::query_as::<_, Debt>(crate::sqlf!(
        "UPDATE debts SET name = $4, account_id = $5, person_id = $6, principal_cents = $7, opened_on = $8,
            goal_id = $9, notes_md = $10,
            status = CASE WHEN $11 THEN 'cancelled' WHEN status = 'cancelled' THEN 'open' ELSE status END,
            version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {DEBT_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(d.name)
    .bind(d.account_id)
    .bind(d.person_id)
    .bind(d.principal_cents)
    .bind(d.opened_on)
    .bind(d.goal_id)
    .bind(d.notes_md)
    .bind(d.cancelled)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_debt(&mut **tx, user_id, id).await)),
    }
}

pub async fn set_debt_status(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Uuid,
    status: &str,
    paid_on: Option<NaiveDate>,
) -> RepoResult<()> {
    sqlx::query(
        "UPDATE debts SET status = $3, paid_on = $4, version = version + 1 WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(id)
    .bind(status)
    .bind(paid_on)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Suma de abonos confirmados a una deuda (y fecha del último).
pub async fn debt_payments(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    debt_id: Uuid,
) -> RepoResult<(i64, Option<NaiveDate>)> {
    let row: (Option<i64>, Option<NaiveDate>) = sqlx::query_as(
        "SELECT sum(amount_cents)::bigint, max(occurred_on) FROM transactions
         WHERE user_id = $1 AND debt_id = $2 AND status = 'confirmed' AND kind IN ('debt_payment', 'card_payment')",
    )
    .bind(user_id)
    .bind(debt_id)
    .fetch_one(db)
    .await?;
    Ok((row.0.unwrap_or(0), row.1))
}

/// Deudas abiertas de tarjeta vinculadas a una tarjeta.
pub async fn open_card_debts(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<(Uuid, Uuid)>> {
    Ok(sqlx::query_as(
        "SELECT id, account_id FROM debts WHERE user_id = $1 AND status = 'open' AND account_id IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

// ---------------------------------------------------------------- Plan de crédito

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct CreditPlan {
    pub goal_text: String,
    pub target_date: Option<NaiveDate>,
    /// Lista de `{ "text": "...", "done": bool }`.
    #[schema(value_type = Vec<Object>)]
    pub steps: serde_json::Value,
    /// Lista de `{ "text": "..." }`.
    #[schema(value_type = Vec<Object>)]
    pub rules: serde_json::Value,
    pub max_usage_pct: i16,
    pub excluded_account_ids: Vec<Uuid>,
    pub forbid_cards_with_debt: bool,
    pub version: i32,
}

const PLAN_COLS: &str = "goal_text, target_date, steps, rules, max_usage_pct, excluded_account_ids, forbid_cards_with_debt, version";

pub async fn get_plan(db: impl PgExecutor<'_> + Copy, user_id: Uuid) -> RepoResult<CreditPlan> {
    sqlx::query("INSERT INTO credit_plans (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(sqlx::query_as::<_, CreditPlan>(crate::sqlf!(
        "SELECT {PLAN_COLS} FROM credit_plans WHERE user_id = $1"
    ))
    .bind(user_id)
    .fetch_one(db)
    .await?)
}

pub async fn save_plan(
    db: impl PgExecutor<'_> + Copy,
    user_id: Uuid,
    p: &CreditPlan,
) -> RepoResult<CreditPlan> {
    let row = sqlx::query_as::<_, CreditPlan>(crate::sqlf!(
        "UPDATE credit_plans SET goal_text = $3, target_date = $4, steps = $5, rules = $6, max_usage_pct = $7,
            excluded_account_ids = $8, forbid_cards_with_debt = $9, version = version + 1
         WHERE user_id = $1 AND version = $2 RETURNING {PLAN_COLS}"
    ))
    .bind(user_id)
    .bind(p.version)
    .bind(&p.goal_text)
    .bind(p.target_date)
    .bind(&p.steps)
    .bind(&p.rules)
    .bind(p.max_usage_pct)
    .bind(&p.excluded_account_ids)
    .bind(p.forbid_cards_with_debt)
    .fetch_optional(db)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_plan(db, user_id).await)),
    }
}

// ---------------------------------------------------------------- Filas para reportes

/// Gasto/ingreso confirmado para reportes (categoría, proyecto, rubro y si viene de una suscripción).
#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct ReportRow {
    pub id: Uuid,
    pub kind: String,
    pub occurred_on: NaiveDate,
    pub concept: String,
    pub amount_cents: i64,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    pub role: Option<String>,
    pub project_id: Option<Uuid>,
    pub budget_item_id: Option<Uuid>,
    pub from_subscription: bool,
}

/// Gastos e ingresos confirmados en la moneda base dentro del rango.
pub async fn report_rows(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    base: &str,
    start: NaiveDate,
    end: NaiveDate,
    project_id: Option<Uuid>,
) -> RepoResult<Vec<ReportRow>> {
    Ok(sqlx::query_as::<_, ReportRow>(
        "SELECT t.id, t.kind, t.occurred_on, t.concept, t.amount_cents, t.category_id, c.name AS category_name, c.role,
                t.project_id, t.budget_item_id, coalesce(r.kind = 'subscription', false) AS from_subscription
         FROM transactions t
         LEFT JOIN fin_categories c ON c.id = t.category_id
         LEFT JOIN fin_recurring_rules r ON r.id = t.recurring_rule_id
         WHERE t.user_id = $1 AND t.status = 'confirmed' AND t.currency = $2 AND t.kind IN ('expense', 'income')
           AND t.occurred_on BETWEEN $3 AND $4 AND ($5::uuid IS NULL OR t.project_id = $5)
         ORDER BY t.occurred_on, t.created_at",
    )
    .bind(user_id)
    .bind(base)
    .bind(start)
    .bind(end)
    .bind(project_id)
    .fetch_all(db)
    .await?)
}

/// Meses (`YYYY-MM`) con movimientos confirmados o presupuesto, más recientes primero.
pub async fn active_months(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT m FROM (
            SELECT to_char(occurred_on, 'YYYY-MM') AS m FROM transactions WHERE user_id = $1 AND status = 'confirmed'
            UNION SELECT month FROM budget_months WHERE user_id = $1) x
         ORDER BY m DESC LIMIT 60",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// Tope de gasto guardado de un mes (sin categorías de pago de deuda); None si el mes no tiene snapshot.
pub async fn stored_spending_cap(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    month: &str,
) -> RepoResult<Option<i64>> {
    let row: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT (SELECT sum(l.amount_cents) FROM budget_lines l JOIN fin_categories c ON c.id = l.category_id
                 WHERE l.budget_month_id = m.id AND c.role <> 'debt_payment')::bigint
         FROM budget_months m WHERE m.user_id = $1 AND m.month = $2",
    )
    .bind(user_id)
    .bind(month)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| r.0.unwrap_or(0)))
}
