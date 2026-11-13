//! Reglas financieras puras (T36–T43). Dinero en centavos `i64`; nunca `f64`.
//!
//! Clases de movimiento y efectos (ver docs/DATA_MODEL.md §Finanzas):
//! * `expense`: consumo (según rol de categoría); resta de la cuenta/tarjeta que lo pagó.
//! * `income`: ingreso (ganado si el rol es `earned`); suma a la cuenta receptora.
//! * `loan_received`: financiamiento, no ingreso ganado; suma a la cuenta receptora.
//! * `card_payment`: resta de la cuenta origen y libera crédito en la tarjeta; es pago de deuda, no consumo.
//! * `debt_payment`: abono a una deuda (préstamo/familiar/tarjeta); no es consumo.
//! * `transfer`: neutro (resta en origen, suma en destino); no crea ingresos ni gastos.
//! * `adjustment`: corrige saldo (`adjustment_sign`); fuera de consumo y de flujo.
//!
//! Corrección C3 del "neto" del vault: `finanzas.js` calculaba `ingresos − todo lo registrado`, de modo que una compra
//! con tarjeta (gasto) más su pago posterior (otro "gasto" de categoría Pago de deuda) restaban dos veces el mismo
//! dinero. Aquí el **neto de consumo** = ingreso ganado − consumo (la compra cuenta una vez; el pago no es consumo) y el
//! **flujo de efectivo** = entradas − salidas de cuentas de efectivo/débito (el pago de la tarjeta sí es salida de
//! efectivo; la compra con tarjeta no). Ver `tests::net_correction_card_purchase_and_payment`.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::DomainError;
use crate::text::fold;
use crate::time::{DayOfMonth, last_day_of_month};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Expense,
    Income,
    Transfer,
    CardPayment,
    DebtPayment,
    LoanReceived,
    Adjustment,
}

impl Kind {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        Ok(match s {
            "expense" => Self::Expense,
            "income" => Self::Income,
            "transfer" => Self::Transfer,
            "card_payment" => Self::CardPayment,
            "debt_payment" => Self::DebtPayment,
            "loan_received" => Self::LoanReceived,
            "adjustment" => Self::Adjustment,
            _ => {
                return Err(DomainError::invalid(
                    "kind",
                    format!("tipo de movimiento inválido: {s}"),
                ));
            }
        })
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Expense => "expense",
            Self::Income => "income",
            Self::Transfer => "transfer",
            Self::CardPayment => "card_payment",
            Self::DebtPayment => "debt_payment",
            Self::LoanReceived => "loan_received",
            Self::Adjustment => "adjustment",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Consumption,
    DebtPayment,
    Investment,
    Payroll,
    Earned,
    Financing,
    OtherIncome,
}

impl Role {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "consumption" => Self::Consumption,
            "debt_payment" => Self::DebtPayment,
            "investment" => Self::Investment,
            "payroll" => Self::Payroll,
            "earned" => Self::Earned,
            "financing" => Self::Financing,
            "other_income" => Self::OtherIncome,
            _ => return None,
        })
    }
    pub fn is_expense_role(&self) -> bool {
        matches!(
            self,
            Self::Consumption | Self::DebtPayment | Self::Investment | Self::Payroll
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Debit,
    Cash,
    Savings,
    CreditCard,
}

impl AccountKind {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        Ok(match s {
            "debit" => Self::Debit,
            "cash" => Self::Cash,
            "savings" => Self::Savings,
            "credit_card" => Self::CreditCard,
            _ => return Err(DomainError::invalid("kind", "tipo de cuenta inválido")),
        })
    }
    pub fn is_cash_like(&self) -> bool {
        !matches!(self, Self::CreditCard)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Confirmed,
    PendingReconciliation,
    Void,
}

impl Status {
    pub fn parse(s: &str) -> Self {
        match s {
            "confirmed" => Self::Confirmed,
            "pending_reconciliation" => Self::PendingReconciliation,
            _ => Self::Void,
        }
    }
}

/// Movimiento mínimo para cálculos.
#[derive(Debug, Clone)]
pub struct Movement {
    pub id: Uuid,
    pub kind: Kind,
    pub status: Status,
    pub amount_cents: i64,
    pub currency: String,
    pub occurred_on: NaiveDate,
    pub occurred_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub role: Option<Role>,
    pub account_id: Option<Uuid>,
    pub counter_account_id: Option<Uuid>,
    pub adjustment_sign: i16,
    pub financed_by_person: bool,
    /// Generado por una regla de suscripción (fijo, no variable).
    pub from_subscription: bool,
}

/// Efecto con signo del movimiento sobre el saldo (débito) o el disponible (tarjeta) de `account`.
pub fn account_effect(m: &Movement, account: Uuid) -> i64 {
    let mut e = 0;
    if m.account_id == Some(account) {
        e += match m.kind {
            Kind::Income | Kind::LoanReceived => m.amount_cents,
            Kind::Expense | Kind::Transfer | Kind::CardPayment | Kind::DebtPayment => -m.amount_cents,
            Kind::Adjustment => m.amount_cents * i64::from(m.adjustment_sign.signum()),
        };
    }
    if m.counter_account_id == Some(account) {
        // Destino de transferencia o tarjeta que recibe un pago.
        e += m.amount_cents;
    }
    e
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Semantics {
    /// Instante observado: movimientos posteriores a `observed_at`.
    Instant,
    /// Vault (cuentas): saldo al **iniciar** el día base → incluye movimientos del día base.
    LegacyDayStartInclusive,
    /// Vault (tarjetas): disponible visto el día base → solo movimientos después de ese día.
    LegacyAfterDay,
}

impl Semantics {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        Ok(match s {
            "instant" => Self::Instant,
            "legacy_day_start_inclusive" => Self::LegacyDayStartInclusive,
            "legacy_after_day" => Self::LegacyAfterDay,
            _ => return Err(DomainError::invalid("semantics", "semántica inválida")),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub balance_cents: i64,
    pub observed_at: DateTime<Utc>,
    pub observed_on: NaiveDate,
    pub semantics: Semantics,
}

/// ¿El movimiento ocurrió después del snapshot (y por tanto no está reflejado en él)?
///
/// `Instant` (D-020): con hora → `occurred_at > observed_at`. Sin hora: día posterior → sí; día anterior → no
/// (registro retroactivo ya reflejado en el saldo observado); **mismo día** → se decide por el momento en que se
/// registró (`created_at > observed_at`): lo capturado después de observar el saldo no estaba en él.
pub fn included_after_snapshot(s: &Snapshot, m: &Movement) -> bool {
    match s.semantics {
        Semantics::LegacyDayStartInclusive => m.occurred_on >= s.observed_on,
        Semantics::LegacyAfterDay => m.occurred_on > s.observed_on,
        Semantics::Instant => match m.occurred_at {
            Some(at) => at > s.observed_at,
            None if m.occurred_on > s.observed_on => true,
            None if m.occurred_on < s.observed_on => false,
            None => m.created_at > s.observed_at,
        },
    }
}

/// Saldo (o disponible) estimado = observado + efectos confirmados posteriores en la moneda de la cuenta.
pub fn estimate_balance(s: &Snapshot, account: Uuid, currency: &str, movements: &[Movement]) -> i64 {
    s.balance_cents
        + movements
            .iter()
            .filter(|m| {
                m.status == Status::Confirmed && m.currency == currency && included_after_snapshot(s, m)
            })
            .map(|m| account_effect(m, account))
            .sum::<i64>()
}

/// Totales separados de un conjunto de movimientos (solo confirmados en la moneda base).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub consumption_cents: i64,
    pub debt_payments_cents: i64,
    pub earned_income_cents: i64,
    pub other_income_cents: i64,
    pub financing_cents: i64,
    pub investment_cents: i64,
    pub cash_in_cents: i64,
    pub cash_out_cents: i64,
    pub excluded_foreign_count: i64,
}

impl Totals {
    /// Neto de consumo corregido (C3): ingreso ganado − consumo.
    pub fn consumption_net(&self) -> i64 {
        self.earned_income_cents - self.consumption_cents
    }
    pub fn cash_net(&self) -> i64 {
        self.cash_in_cents - self.cash_out_cents
    }
}

/// ¿Es consumo? Gastos de rol consumo/nómina (o sin categoría). El préstamo familiar cuenta aquí una vez.
pub fn is_consumption(m: &Movement) -> bool {
    m.kind == Kind::Expense && matches!(m.role, None | Some(Role::Consumption) | Some(Role::Payroll))
}

pub fn summarize(movements: &[Movement], accounts: &HashMap<Uuid, AccountKind>, base: &str) -> Totals {
    let mut t = Totals::default();
    let cash = |id: Option<Uuid>| {
        id.and_then(|i| accounts.get(&i))
            .is_some_and(|k| k.is_cash_like())
    };
    for m in movements.iter().filter(|m| m.status == Status::Confirmed) {
        if m.currency != base {
            t.excluded_foreign_count += 1;
            continue;
        }
        let a = m.amount_cents;
        match m.kind {
            Kind::Expense => {
                match m.role {
                    Some(Role::DebtPayment) => t.debt_payments_cents += a,
                    Some(Role::Investment) => t.investment_cents += a,
                    _ => t.consumption_cents += a,
                }
                // Sin cuenta y sin financiador: se asume efectivo. Con tarjeta o pagado por otra persona: no sale efectivo.
                let paid_with_cash = match m.account_id {
                    Some(_) => cash(m.account_id),
                    None => !m.financed_by_person,
                };
                if paid_with_cash {
                    t.cash_out_cents += a;
                }
            }
            Kind::Income => {
                match m.role {
                    Some(Role::Financing) => t.financing_cents += a,
                    Some(Role::OtherIncome) => t.other_income_cents += a,
                    _ => t.earned_income_cents += a,
                }
                if m.account_id.is_none() || cash(m.account_id) {
                    t.cash_in_cents += a;
                }
            }
            Kind::LoanReceived => {
                t.financing_cents += a;
                if m.account_id.is_none() || cash(m.account_id) {
                    t.cash_in_cents += a;
                }
            }
            Kind::CardPayment | Kind::DebtPayment => {
                t.debt_payments_cents += a;
                if m.account_id.is_none() || cash(m.account_id) {
                    t.cash_out_cents += a;
                }
            }
            Kind::Transfer => {
                // Entre cuentas de efectivo es neutro; de efectivo a tarjeta es salida; de tarjeta a efectivo, entrada.
                match (cash(m.account_id), cash(m.counter_account_id)) {
                    (true, false) => t.cash_out_cents += a,
                    (false, true) => t.cash_in_cents += a,
                    _ => {}
                }
            }
            Kind::Adjustment => {}
        }
    }
    t
}

/// Proyección de fin de mes (finanzas.js): gasto variable al ritmo diario + suscripciones previstas del mes completas.
/// `variable_cents` excluye las suscripciones ya registradas para no duplicarlas.
pub fn month_projection(
    variable_cents: i64,
    days_elapsed: u32,
    days_in_month: u32,
    subscriptions_month_cents: i64,
) -> i64 {
    let elapsed = i64::from(days_elapsed.max(1));
    let per_month =
        (variable_cents as i128 * i64::from(days_in_month) as i128 + (elapsed as i128 / 2)) / elapsed as i128;
    per_month as i64 + subscriptions_month_cents
}

/// Ritmo presupuestado acumulado por día (índice 0 = día 1).
pub fn budget_pace(budget_cents: i64, days_in_month: u32) -> Vec<i64> {
    let d = i64::from(days_in_month.max(1));
    (1..=d)
        .map(|i| ((budget_cents as i128 * i as i128) / d as i128) as i64)
        .collect()
}

/// Rubro sugerido por palabras clave (insensible a acentos/mayúsculas, por palabra). La asignación explícita prevalece.
pub fn match_budget_item(
    explicit: Option<Uuid>,
    concept: &str,
    items: &[(Uuid, Vec<String>)],
) -> Option<Uuid> {
    if explicit.is_some() {
        return explicit;
    }
    let c = fold(concept);
    let words: Vec<&str> = c
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    items
        .iter()
        .find(|(_, kws)| {
            kws.iter()
                .map(|k| fold(k.trim()))
                .filter(|k| !k.is_empty())
                .any(|k| {
                    if k.contains(' ') {
                        c.contains(&k)
                    } else {
                        words.iter().any(|w| *w == k)
                    }
                })
        })
        .map(|(id, _)| *id)
}

/// Próxima fecha (>= `from`) en que cae el día `day` del mes, ajustando meses cortos.
pub fn next_monthly(day: u32, from: NaiveDate) -> NaiveDate {
    let this = DayOfMonth::Day(day).resolve(from.year(), from.month());
    if this >= from {
        return this;
    }
    let (y, m) = if from.month() == 12 {
        (from.year() + 1, 1)
    } else {
        (from.year(), from.month() + 1)
    };
    DayOfMonth::Day(day).resolve(y, m)
}

/// Fecha de una ocurrencia recurrente en un mes.
pub fn occurrence_date(rule: DayOfMonth, year: i32, month: u32) -> NaiveDate {
    rule.resolve(year, month)
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    last_day_of_month(year, month).day()
}

// ---------------- Sugerencia de tarjeta (D-022) ----------------

#[derive(Debug, Clone)]
pub struct CardCandidate {
    pub id: Uuid,
    pub name: String,
    pub status: String,
    pub limit_cents: Option<i64>,
    pub cut_day: Option<u32>,
    /// Disponible estimado; `None` si no hay snapshot.
    pub available_cents: Option<i64>,
    /// Tiene deuda abierta vinculada (tabla debts).
    pub has_open_debt: bool,
}

#[derive(Debug, Clone)]
pub struct PlanRules {
    pub max_usage_pct: i64,
    pub excluded: Vec<Uuid>,
    pub forbid_cards_with_debt: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CardVerdict {
    pub account_id: Uuid,
    pub name: String,
    pub eligible: bool,
    /// Motivos de exclusión en español (vacío si es elegible).
    pub reasons: Vec<String>,
    pub days_to_cut: Option<i64>,
    pub next_cut_on: Option<NaiveDate>,
    pub available_cents: Option<i64>,
    pub usage_after_pct: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    /// Tarjeta elegida; `None` → usar débito.
    pub card_id: Option<Uuid>,
    pub card_name: Option<String>,
    /// Sin monto planeado la sugerencia es provisional.
    pub provisional: bool,
    pub explanation: String,
    pub verdicts: Vec<CardVerdict>,
}

pub fn suggest_card(
    cards: &[CardCandidate],
    plan: &PlanRules,
    today: NaiveDate,
    amount_cents: Option<i64>,
) -> Suggestion {
    let amount = amount_cents.unwrap_or(0).max(0);
    let mut verdicts: Vec<CardVerdict> = cards
        .iter()
        .map(|c| {
            let mut reasons = Vec::new();
            match c.status.as_str() {
                "cancelled" | "closed" => reasons.push("cancelada".to_string()),
                "to_cancel" => reasons.push("prevista para cancelar".to_string()),
                _ => {}
            }
            if plan.excluded.contains(&c.id) {
                reasons.push("excluida por el plan de crédito".into());
            }
            if c.has_open_debt && plan.forbid_cards_with_debt {
                reasons.push("tiene deuda sin liquidar".into());
            }
            let next_cut = c.cut_day.map(|d| next_monthly(d, today));
            match next_cut {
                None => reasons.push("sin día de corte".into()),
                Some(nc) if nc == today => reasons.push("hoy es su día de corte".into()),
                _ => {}
            }
            let mut usage_after = None;
            match (c.available_cents, c.limit_cents) {
                (None, _) => reasons.push("sin disponible observado".into()),
                (Some(av), lim) => {
                    if av < amount || av <= 0 {
                        reasons.push("sin disponible suficiente".into());
                    }
                    if let Some(l) = lim.filter(|l| *l > 0) {
                        let used_after = l - av + amount;
                        let pct = ((used_after as i128 * 100 + (l as i128 - 1)) / l as i128) as i64;
                        usage_after = Some(pct);
                        if used_after * 100 > l * plan.max_usage_pct {
                            reasons.push(format!(
                                "quedaría en {pct}% de uso (máximo {}%)",
                                plan.max_usage_pct
                            ));
                        }
                    } else {
                        reasons.push("sin límite registrado".into());
                    }
                }
            }
            // Corte "hoy" ya está excluido; el siguiente corte válido para contar días:
            let effective_cut = next_cut.map(|nc| {
                if nc == today {
                    next_monthly(c.cut_day.unwrap_or(1), today.succ_opt().unwrap_or(today))
                } else {
                    nc
                }
            });
            CardVerdict {
                account_id: c.id,
                name: c.name.clone(),
                eligible: reasons.is_empty(),
                reasons,
                days_to_cut: effective_cut.map(|nc| (nc - today).num_days()),
                next_cut_on: effective_cut,
                available_cents: c.available_cents,
                usage_after_pct: usage_after,
            }
        })
        .collect();
    verdicts.sort_by(|a, b| {
        b.eligible
            .cmp(&a.eligible)
            .then(b.days_to_cut.unwrap_or(-1).cmp(&a.days_to_cut.unwrap_or(-1)))
            .then(
                b.available_cents
                    .unwrap_or(0)
                    .cmp(&a.available_cents.unwrap_or(0)),
            )
    });
    let provisional = amount_cents.is_none();
    match verdicts.iter().find(|v| v.eligible) {
        Some(best) => Suggestion {
            card_id: Some(best.account_id),
            card_name: Some(best.name.clone()),
            provisional,
            explanation: format!(
                "{}: {} días al próximo corte (la compra entra al siguiente estado de cuenta){}",
                best.name,
                best.days_to_cut.unwrap_or(0),
                if provisional {
                    "; provisional hasta saber cuánto planeas gastar"
                } else {
                    ""
                }
            ),
            verdicts,
        },
        None => {
            let why = verdicts
                .iter()
                .map(|v| format!("{} ({})", v.name, v.reasons.join(", ")))
                .collect::<Vec<_>>()
                .join("; ");
            Suggestion {
                card_id: None,
                card_name: None,
                provisional,
                explanation: if verdicts.is_empty() {
                    "Sin tarjetas registradas: usa débito".into()
                } else {
                    format!("Ninguna tarjeta sirve hoy, usa débito: {why}")
                },
                verdicts,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn mv(
        kind: Kind,
        amount: i64,
        on: NaiveDate,
        acc: Option<Uuid>,
        counter: Option<Uuid>,
        role: Option<Role>,
    ) -> Movement {
        Movement {
            id: Uuid::new_v4(),
            kind,
            status: Status::Confirmed,
            amount_cents: amount,
            currency: "MXN".into(),
            occurred_on: on,
            occurred_at: None,
            created_at: Utc.with_ymd_and_hms(2026, 11, 12, 18, 0, 0).unwrap(),
            role,
            account_id: acc,
            counter_account_id: counter,
            adjustment_sign: 1,
            financed_by_person: false,
            from_subscription: false,
        }
    }

    #[test]
    fn net_correction_card_purchase_and_payment() {
        let bank = Uuid::new_v4();
        let card = Uuid::new_v4();
        let accounts = HashMap::from([(bank, AccountKind::Debit), (card, AccountKind::CreditCard)]);
        let day = d(2026, 11, 5);
        let ms = vec![
            mv(Kind::Income, 1_000_000, day, Some(bank), None, Some(Role::Earned)),
            mv(
                Kind::Expense,
                100_000,
                day,
                Some(card),
                None,
                Some(Role::Consumption),
            ),
            mv(
                Kind::CardPayment,
                100_000,
                d(2026, 11, 20),
                Some(bank),
                Some(card),
                None,
            ),
            mv(
                Kind::Transfer,
                50_000,
                day,
                Some(bank),
                Some(Uuid::new_v4()),
                None,
            ),
        ];
        let mut acc2 = accounts.clone();
        acc2.insert(ms[3].counter_account_id.unwrap(), AccountKind::Savings);
        let t = summarize(&ms, &acc2, "MXN");
        // Vault: ingresos − (compra + pago) = 10,000 − 2,000 = 8,000 (doble conteo).
        let vault_net = 1_000_000 - (100_000 + 100_000);
        assert_eq!(vault_net, 800_000);
        assert_eq!(
            t.consumption_cents, 100_000,
            "la compra con tarjeta cuenta una sola vez"
        );
        assert_eq!(t.debt_payments_cents, 100_000);
        assert_eq!(t.consumption_net(), 900_000);
        assert_eq!(t.cash_in_cents, 1_000_000);
        assert_eq!(
            t.cash_out_cents, 100_000,
            "solo el pago de la tarjeta sale de efectivo; la transferencia es neutra"
        );
        assert_eq!(t.cash_net(), 900_000);
    }

    #[test]
    fn card_effects_and_transfer_are_consistent() {
        let bank = Uuid::new_v4();
        let card = Uuid::new_v4();
        let buy = mv(
            Kind::Expense,
            25_000,
            d(2026, 11, 3),
            Some(card),
            None,
            Some(Role::Consumption),
        );
        let pay = mv(
            Kind::CardPayment,
            25_000,
            d(2026, 11, 4),
            Some(bank),
            Some(card),
            None,
        );
        assert_eq!(account_effect(&buy, card), -25_000);
        assert_eq!(account_effect(&pay, card), 25_000);
        assert_eq!(account_effect(&pay, bank), -25_000);
        assert_eq!(account_effect(&buy, bank), 0);
    }

    #[test]
    fn snapshot_semantics_legacy_and_instant() {
        let acc = Uuid::new_v4();
        let base = d(2026, 11, 10);
        let observed_at = Utc.with_ymd_and_hms(2026, 11, 10, 21, 0, 0).unwrap();
        let same_day = mv(Kind::Expense, 1_000, base, Some(acc), None, None);
        let next_day = mv(Kind::Expense, 2_000, d(2026, 11, 11), Some(acc), None, None);
        let before = mv(Kind::Expense, 4_000, d(2026, 11, 9), Some(acc), None, None);
        let all = vec![same_day.clone(), next_day.clone(), before.clone()];
        let s = |sem| Snapshot {
            balance_cents: 100_000,
            observed_at,
            observed_on: base,
            semantics: sem,
        };
        // Cuentas legacy: el saldo es al INICIAR el día → incluye el gasto del día base.
        assert_eq!(
            estimate_balance(&s(Semantics::LegacyDayStartInclusive), acc, "MXN", &all),
            97_000
        );
        // Tarjetas legacy: solo después del día base.
        assert_eq!(
            estimate_balance(&s(Semantics::LegacyAfterDay), acc, "MXN", &all),
            98_000
        );
        // Instant: mismo día sin hora registrado antes del snapshot (created 2026-11-12 > observed) → incluido.
        assert_eq!(estimate_balance(&s(Semantics::Instant), acc, "MXN", &all), 97_000);
        let mut recorded_before = same_day.clone();
        recorded_before.created_at = Utc.with_ymd_and_hms(2026, 11, 10, 15, 0, 0).unwrap();
        assert_eq!(
            estimate_balance(
                &s(Semantics::Instant),
                acc,
                "MXN",
                &[recorded_before, next_day.clone()]
            ),
            98_000
        );
        let mut timed = same_day;
        timed.occurred_at = Some(Utc.with_ymd_and_hms(2026, 11, 10, 22, 0, 0).unwrap());
        assert!(included_after_snapshot(&s(Semantics::Instant), &timed));
        // Un registro retroactivo (día anterior) no se resta otra vez.
        assert!(!included_after_snapshot(&s(Semantics::Instant), &before));
    }

    #[test]
    fn pending_and_foreign_are_excluded() {
        let acc = Uuid::new_v4();
        let mut p = mv(Kind::Expense, 500, d(2026, 11, 12), Some(acc), None, None);
        p.status = Status::PendingReconciliation;
        let mut usd = mv(Kind::Expense, 1_000, d(2026, 11, 12), Some(acc), None, None);
        usd.currency = "USD".into();
        let t = summarize(&[p, usd], &HashMap::from([(acc, AccountKind::Debit)]), "MXN");
        assert_eq!(t.consumption_cents, 0);
        assert_eq!(t.excluded_foreign_count, 1);
    }

    #[test]
    fn family_loan_counts_once() {
        let bank = Uuid::new_v4();
        let mut lent = mv(
            Kind::Expense,
            30_000,
            d(2026, 11, 1),
            None,
            None,
            Some(Role::Consumption),
        );
        lent.financed_by_person = true;
        let repay = mv(Kind::DebtPayment, 30_000, d(2026, 11, 15), Some(bank), None, None);
        let t = summarize(
            &[lent, repay],
            &HashMap::from([(bank, AccountKind::Debit)]),
            "MXN",
        );
        assert_eq!(t.consumption_cents, 30_000);
        assert_eq!(t.debt_payments_cents, 30_000);
        assert_eq!(t.cash_out_cents, 30_000, "solo la devolución sale de mi efectivo");
    }

    #[test]
    fn projection_and_pace() {
        // 10 días, $1,000 variable, mes de 30 días, $300 de suscripciones del mes.
        assert_eq!(month_projection(100_000, 10, 30, 30_000), 330_000);
        assert_eq!(month_projection(0, 0, 31, 0), 0);
        let p = budget_pace(310_000, 31);
        assert_eq!(p[0], 10_000);
        assert_eq!(p[30], 310_000);
    }

    #[test]
    fn budget_item_explicit_beats_keyword() {
        let ads = Uuid::new_v4();
        let wa = Uuid::new_v4();
        let items = vec![
            (wa, vec!["WhatsApp".into()]),
            (ads, vec!["Ads".into(), "anuncio".into(), "Facebook".into()]),
        ];
        assert_eq!(match_budget_item(None, "Ads de negocio", &items), Some(ads));
        assert_eq!(match_budget_item(None, "Anúncio Facebook", &items), Some(ads));
        assert_eq!(
            match_budget_item(None, "Leads", &items),
            None,
            "por palabra, no subcadena"
        );
        assert_eq!(match_budget_item(Some(wa), "Ads de negocio", &items), Some(wa));
    }

    #[test]
    fn next_monthly_handles_short_months() {
        assert_eq!(next_monthly(31, d(2026, 2, 10)), d(2026, 2, 28));
        assert_eq!(next_monthly(5, d(2026, 2, 10)), d(2026, 3, 5));
        assert_eq!(next_monthly(31, d(2026, 4, 30)), d(2026, 4, 30));
        assert_eq!(next_monthly(30, d(2026, 12, 31)), d(2027, 1, 30));
    }

    fn card(name: &str, cut: Option<u32>, available: i64, limit: i64) -> CardCandidate {
        CardCandidate {
            id: Uuid::new_v4(),
            name: name.into(),
            status: "active".into(),
            limit_cents: Some(limit),
            cut_day: cut,
            available_cents: Some(available),
            has_open_debt: false,
        }
    }

    #[test]
    fn card_suggestion_rules() {
        let today = d(2026, 11, 12);
        let plan = PlanRules {
            max_usage_pct: 30,
            excluded: vec![],
            forbid_cards_with_debt: true,
        };
        let a = card("A", Some(10), 100_000, 100_000); // corte pasó hace 2 días → 28 días al próximo
        let b = card("B", Some(20), 100_000, 100_000); // 8 días
        let s = suggest_card(&[a.clone(), b.clone()], &plan, today, Some(10_000));
        assert_eq!(s.card_id, Some(a.id));
        assert!(!s.provisional);
        // Corte hoy excluye.
        let c = card("C", Some(12), 100_000, 100_000);
        let s = suggest_card(std::slice::from_ref(&c), &plan, today, Some(1_000));
        assert_eq!(s.card_id, None);
        assert!(s.verdicts[0].reasons.iter().any(|r| r.contains("corte")));
        // 30%: $35,000 sobre $100,000 → excluida; sin disponible suficiente.
        let s = suggest_card(std::slice::from_ref(&a), &plan, today, Some(35_000));
        assert!(s.verdicts[0].reasons.iter().any(|r| r.contains("30%")));
        let low = card("D", Some(1), 500, 100_000);
        let s = suggest_card(&[low], &plan, today, Some(1_000));
        assert!(s.verdicts[0].reasons.iter().any(|r| r.contains("disponible")));
        // Cancelada, por cancelar, sin corte, excluida por plan, con deuda.
        let mut cancelled = card("E", Some(1), 100_000, 100_000);
        cancelled.status = "cancelled".into();
        let mut to_cancel = card("F", Some(1), 100_000, 100_000);
        to_cancel.status = "to_cancel".into();
        let no_cut = card("G", None, 100_000, 100_000);
        let mut debt = card("H", Some(1), 100_000, 100_000);
        debt.has_open_debt = true;
        let excluded = card("I", Some(1), 100_000, 100_000);
        let plan2 = PlanRules {
            excluded: vec![excluded.id],
            ..plan.clone()
        };
        let s = suggest_card(
            &[cancelled, to_cancel, no_cut, debt, excluded],
            &plan2,
            today,
            Some(1_000),
        );
        assert_eq!(s.card_id, None);
        assert!(s.explanation.contains("débito"));
        assert!(s.verdicts.iter().all(|v| !v.eligible && !v.reasons.is_empty()));
        // Sin monto: provisional.
        let s = suggest_card(&[b], &plan, today, None);
        assert!(s.provisional);
        assert!(s.card_id.is_some());
    }
}
