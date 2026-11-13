//! T43 dashboard financiero y vista de gastos por proyecto (derivada del mismo registro; nunca registros duplicados).
//!
//! Corrección del "neto" del vault (C3): ver `domain::finance` (doc del módulo). El dashboard expone por separado
//! consumo, pagos de deuda, ingreso ganado, financiamiento y flujo de efectivo, más `consumption_net_cents`
//! (ingreso ganado − consumo) y `cash_net_cents` (entradas − salidas de efectivo). Una compra con tarjeta y su pago
//! posterior ya no restan dos veces.

use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{Datelike, Months, NaiveDate};
use domain::finance::{budget_pace, days_in_month, match_budget_item, month_projection, summarize};
use persistence::repos::finance::{self as repo, ReportRow};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::catalog::account_view;
use super::plans::budget_view;
use super::recurring::{OccurrenceView, month_occurrences};
use super::{ctx, month_key, parse_month_param};
use crate::auth::AuthUser;
use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(dashboard))
        .routes(routes!(project_report))
}

#[derive(Deserialize, IntoParams)]
pub struct MonthQuery {
    /// `YYYY-MM` (por defecto, mes actual).
    pub month: Option<String>,
}

#[derive(Serialize, ToSchema, Default)]
pub struct FlowTotals {
    /// Consumo: una sola vez aunque se pague con tarjeta o lo financie otra persona.
    pub consumption_cents: i64,
    /// Pagos de tarjeta y abonos a deudas (no son consumo).
    pub debt_payments_cents: i64,
    pub earned_income_cents: i64,
    pub other_income_cents: i64,
    /// Préstamos recibidos (no son ingreso ganado).
    pub financing_cents: i64,
    pub investment_cents: i64,
    pub cash_in_cents: i64,
    pub cash_out_cents: i64,
    /// Ingreso ganado − consumo (neto corregido, C3).
    pub consumption_net_cents: i64,
    /// Entradas − salidas de cuentas de efectivo/débito.
    pub cash_net_cents: i64,
    /// Movimientos en otra moneda sin conversión, excluidos de los totales.
    pub excluded_foreign_count: i64,
}

impl From<domain::finance::Totals> for FlowTotals {
    fn from(t: domain::finance::Totals) -> Self {
        FlowTotals {
            consumption_net_cents: t.consumption_net(),
            cash_net_cents: t.cash_net(),
            consumption_cents: t.consumption_cents,
            debt_payments_cents: t.debt_payments_cents,
            earned_income_cents: t.earned_income_cents,
            other_income_cents: t.other_income_cents,
            financing_cents: t.financing_cents,
            investment_cents: t.investment_cents,
            cash_in_cents: t.cash_in_cents,
            cash_out_cents: t.cash_out_cents,
            excluded_foreign_count: t.excluded_foreign_count,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct CategoryVsBudget {
    pub category_id: Option<Uuid>,
    pub category_name: String,
    pub spent_cents: i64,
    pub budget_cents: Option<i64>,
    /// Presupuesto − gastado (negativo = excedido).
    pub remaining_cents: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct DayPoint {
    pub day: u32,
    /// Gasto acumulado (sin pagos de deuda) hasta ese día; `None` en días que aún no ocurren.
    pub spent_cents: Option<i64>,
    /// Ritmo presupuestado acumulado.
    pub pace_cents: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct IncomeStatus {
    /// Ingresos confirmados del mes (manuales + ocurrencias conciliadas).
    pub confirmed_cents: i64,
    /// Ocurrencias de ingresos fijos registradas pero sin conciliar.
    pub pending_cents: i64,
    /// Ingresos fijos que aún no se registran este mes.
    pub projected_cents: i64,
    /// Esperado del mes por ingresos fijos (pendientes + proyectados + conciliados de reglas).
    pub expected_fixed_cents: i64,
    /// Esperado − presupuesto total (lo que planeo que me quede).
    pub expected_minus_budget_cents: i64,
}

#[derive(Serialize, ToSchema)]
pub struct MonthSummary {
    pub month: String,
    pub spending_cap_cents: Option<i64>,
    /// Gasto sin pagos de deuda.
    pub spent_cents: i64,
    pub consumption_cents: i64,
    pub earned_income_cents: i64,
    pub consumption_net_cents: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectSpend {
    pub project_id: Uuid,
    pub project_name: String,
    pub spent_cents: i64,
}

#[derive(Serialize, ToSchema)]
pub struct AccountsSummary {
    /// Suma del saldo estimado de cuentas de débito/efectivo/ahorro con snapshot.
    pub cash_estimated_cents: i64,
    pub cards_available_cents: i64,
    pub cards_limit_cents: i64,
    /// Cuentas sin saldo observado (no se incluyen en los totales).
    pub without_snapshot: i64,
    /// Antigüedad máxima (días) de los saldos observados usados.
    pub oldest_snapshot_days: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct FinanceDashboard {
    pub month: String,
    pub today: NaiveDate,
    pub days_in_month: u32,
    pub days_elapsed: u32,
    pub totals: FlowTotals,
    /// Gasto del mes sin pagos de deuda (equivale al "gastado" del vault).
    pub spent_cents: i64,
    pub spending_cap_cents: i64,
    /// El presupuesto mostrado es una propuesta copiada del mes anterior (aún sin snapshot propio).
    pub budget_is_proposal: bool,
    /// Tope − gastado (negativo = excedido).
    pub budget_remaining_cents: i64,
    pub by_category: Vec<CategoryVsBudget>,
    pub daily: Vec<DayPoint>,
    /// Proyección (estimación): variable/días transcurridos × días del mes + suscripciones previstas del mes.
    pub projection_cents: i64,
    pub variable_spent_cents: i64,
    pub subscriptions_month_cents: i64,
    pub income: IncomeStatus,
    pub occurrences: Vec<OccurrenceView>,
    pub last_six_months: Vec<MonthSummary>,
    pub history: Vec<MonthSummary>,
    pub top_expenses: Vec<ReportRow>,
    pub by_project: Vec<ProjectSpend>,
    pub accounts: AccountsSummary,
}

fn is_spend(r: &ReportRow) -> bool {
    r.kind == "expense" && r.role.as_deref() != Some("debt_payment")
}

async fn month_summary(s: &AppState, user_id: Uuid, base: &str, key: &str) -> ApiResult<MonthSummary> {
    let p = domain::time::parse_month(key)?;
    let t = repo::period_summary(&s.pool, user_id, p.start, p.end).await?;
    let rows = repo::report_rows(&s.pool, user_id, base, p.start, p.end, None).await?;
    Ok(MonthSummary {
        month: key.to_string(),
        spending_cap_cents: repo::stored_spending_cap(&s.pool, user_id, key).await?,
        spent_cents: rows.iter().filter(|r| is_spend(r)).map(|r| r.amount_cents).sum(),
        consumption_cents: t.consumption_cents,
        earned_income_cents: t.earned_income_cents,
        consumption_net_cents: t.earned_income_cents - t.consumption_cents,
    })
}

/// Dashboard financiero del mes (por defecto, el actual).
#[utoipa::path(get, path = "/finance/dashboard", tag = "finance", params(MonthQuery), responses((status = 200, body = FinanceDashboard)))]
async fn dashboard(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<MonthQuery>,
) -> ApiResult<Json<FinanceDashboard>> {
    let uid = auth.user_id;
    let c = ctx(&s, uid).await?;
    let (key, p) = parse_month_param(q.month.as_deref(), c.today)?;
    let dim = days_in_month(p.start.year(), p.start.month());
    let elapsed: u32 = if p.contains(c.today) {
        c.today.day()
    } else if p.end < c.today {
        dim
    } else {
        0
    };

    let movements = repo::movements(&s.pool, uid, Some(p.start), Some(p.end), None).await?;
    let kinds = repo::account_kinds(&s.pool, uid).await?;
    let totals = summarize(&movements, &kinds, &c.base);
    let rows = repo::report_rows(&s.pool, uid, &c.base, p.start, p.end, None).await?;
    let spend: Vec<&ReportRow> = rows.iter().filter(|r| is_spend(r)).collect();
    let spent: i64 = spend.iter().map(|r| r.amount_cents).sum();

    // Presupuesto vs categorías.
    let budget = budget_view(&s, uid, &key).await?;
    let mut by_cat: BTreeMap<Option<Uuid>, (String, i64)> = BTreeMap::new();
    for r in &spend {
        let e = by_cat.entry(r.category_id).or_insert((
            r.category_name.clone().unwrap_or_else(|| "Sin categoría".into()),
            0,
        ));
        e.1 += r.amount_cents;
    }
    let mut by_category: Vec<CategoryVsBudget> = Vec::new();
    for l in budget.lines.iter().filter(|l| l.role != "debt_payment") {
        let sp = by_cat.remove(&Some(l.category_id)).map(|x| x.1).unwrap_or(0);
        by_category.push(CategoryVsBudget {
            category_id: Some(l.category_id),
            category_name: l.category_name.clone(),
            spent_cents: sp,
            budget_cents: Some(l.amount_cents),
            remaining_cents: Some(l.amount_cents - sp),
        });
    }
    for (id, (name, sp)) in by_cat {
        by_category.push(CategoryVsBudget {
            category_id: id,
            category_name: name,
            spent_cents: sp,
            budget_cents: None,
            remaining_cents: None,
        });
    }
    by_category.sort_by(|a, b| b.spent_cents.cmp(&a.spent_cents));

    // Serie diaria acumulada vs ritmo.
    let pace = budget_pace(budget.spending_cap_cents, dim);
    let mut per_day = vec![0i64; dim as usize + 1];
    for r in &spend {
        per_day[r.occurred_on.day() as usize] += r.amount_cents;
    }
    let mut acc = 0;
    let daily = (1..=dim)
        .map(|d| {
            acc += per_day[d as usize];
            DayPoint {
                day: d,
                spent_cents: (d <= elapsed).then_some(acc),
                pace_cents: (budget.spending_cap_cents > 0).then(|| pace[(d - 1) as usize]),
            }
        })
        .collect();

    // Proyección sin duplicar suscripciones: el variable excluye las registradas; se suman las del mes una vez.
    let occurrences = month_occurrences(&s, uid, &key).await?;
    let subs_month: i64 = occurrences
        .iter()
        .filter(|o| o.kind == "subscription" && o.state != "void")
        .map(|o| o.amount_cents)
        .sum();
    let variable: i64 = spend
        .iter()
        .filter(|r| !r.from_subscription)
        .map(|r| r.amount_cents)
        .sum();
    let projection = if elapsed == 0 {
        subs_month
    } else {
        month_projection(variable, elapsed, dim, subs_month)
    };

    let fixed = |state: &str| -> i64 {
        occurrences
            .iter()
            .filter(|o| o.kind == "fixed_income" && o.state == state)
            .map(|o| o.amount_cents)
            .sum()
    };
    let expected_fixed = fixed("pending") + fixed("projected") + fixed("confirmed");
    let income = IncomeStatus {
        confirmed_cents: totals.earned_income_cents + totals.other_income_cents,
        pending_cents: fixed("pending"),
        projected_cents: fixed("projected"),
        expected_fixed_cents: expected_fixed,
        expected_minus_budget_cents: expected_fixed - budget.total_cents,
    };

    // Seis meses e histórico.
    let mut last_six = Vec::new();
    for i in (0..6u32).rev() {
        let m = p.start.checked_sub_months(Months::new(i)).unwrap_or(p.start);
        last_six.push(month_summary(&s, uid, &c.base, &month_key(m)).await?);
    }
    let mut history = Vec::new();
    for m in repo::active_months(&s.pool, uid).await? {
        history.push(month_summary(&s, uid, &c.base, &m).await?);
    }

    let mut top: Vec<ReportRow> = spend.iter().map(|r| (*r).clone()).collect();
    top.sort_by(|a, b| b.amount_cents.cmp(&a.amount_cents));
    top.truncate(5);

    let projects: HashMap<Uuid, String> = persistence::repos::areas::list_projects(&s.pool, uid, None, true)
        .await?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();
    let mut by_proj: HashMap<Uuid, i64> = HashMap::new();
    for r in spend.iter().filter(|r| r.project_id.is_some()) {
        *by_proj.entry(r.project_id.unwrap()).or_default() += r.amount_cents;
    }
    let mut by_project: Vec<ProjectSpend> = by_proj
        .into_iter()
        .map(|(id, v)| ProjectSpend {
            project_id: id,
            project_name: projects.get(&id).cloned().unwrap_or_default(),
            spent_cents: v,
        })
        .collect();
    by_project.sort_by(|a, b| b.spent_cents.cmp(&a.spent_cents));

    let mut accounts = AccountsSummary {
        cash_estimated_cents: 0,
        cards_available_cents: 0,
        cards_limit_cents: 0,
        without_snapshot: 0,
        oldest_snapshot_days: None,
    };
    for a in repo::list_accounts(&s.pool, uid, false).await? {
        if a.currency != c.base || a.status == "cancelled" || a.status == "closed" {
            continue;
        }
        let v = account_view(&s, uid, a, c.today).await?;
        match v.estimated_cents {
            None => accounts.without_snapshot += 1,
            Some(e) => {
                accounts.oldest_snapshot_days = accounts.oldest_snapshot_days.max(v.snapshot_age_days);
                if v.account.kind == "credit_card" {
                    accounts.cards_available_cents += e;
                    accounts.cards_limit_cents += v.account.credit_limit_cents.unwrap_or(0);
                } else {
                    accounts.cash_estimated_cents += e;
                }
            }
        }
    }

    Ok(Json(FinanceDashboard {
        month: key,
        today: c.today,
        days_in_month: dim,
        days_elapsed: elapsed,
        totals: totals.into(),
        spent_cents: spent,
        spending_cap_cents: budget.spending_cap_cents,
        budget_is_proposal: !budget.persisted,
        budget_remaining_cents: budget.spending_cap_cents - spent,
        by_category,
        daily,
        projection_cents: projection,
        variable_spent_cents: variable,
        subscriptions_month_cents: subs_month,
        income,
        occurrences,
        last_six_months: last_six,
        history,
        top_expenses: top,
        by_project,
        accounts,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct RubroSpend {
    pub budget_item_id: Option<Uuid>,
    /// Nombre del rubro o "Sin rubro".
    pub name: String,
    pub budget_cents: Option<i64>,
    pub spent_cents: i64,
    pub remaining_cents: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectExpense {
    #[serde(flatten)]
    pub row: ReportRow,
    /// Rubro asignado: explícito o sugerido por palabra clave.
    pub rubro_id: Option<Uuid>,
    pub rubro_name: String,
    /// `explicit | keyword | none`
    pub rubro_source: String,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectMonth {
    pub month: String,
    pub spent_cents: i64,
    pub budget_cents: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectSubscription {
    pub rule_id: Uuid,
    pub name: String,
    pub amount_cents: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectReport {
    pub project_id: Uuid,
    pub project_name: String,
    pub month: String,
    pub rubros: Vec<RubroSpend>,
    pub spent_cents: i64,
    pub budget_cents: i64,
    /// Avisos (ej. el desglose no suma lo mismo que la categoría del mes).
    pub warnings: Vec<String>,
    pub expenses: Vec<ProjectExpense>,
    pub by_month: Vec<ProjectMonth>,
    pub subscriptions: Vec<ProjectSubscription>,
    pub subscriptions_month_cents: i64,
}

/// Gastos de un proyecto derivados del registro general (sin duplicar movimientos), por rubro y por mes.
#[utoipa::path(get, path = "/finance/projects/{id}/report", tag = "finance", params(MonthQuery),
    responses((status = 200, body = ProjectReport)))]
async fn project_report(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<MonthQuery>,
) -> ApiResult<Json<ProjectReport>> {
    let uid = auth.user_id;
    let project = persistence::repos::areas::get_project(&s.pool, uid, id).await?;
    let c = ctx(&s, uid).await?;
    let (key, p) = parse_month_param(q.month.as_deref(), c.today)?;
    let budget = budget_view(&s, uid, &key).await?;
    let items: Vec<_> = budget.items.iter().filter(|i| i.project_id == id).collect();
    let kw: Vec<(Uuid, Vec<String>)> = items
        .iter()
        .filter_map(|i| i.id.map(|x| (x, i.keywords.clone())))
        .collect();

    let rows: Vec<ReportRow> = repo::report_rows(&s.pool, uid, &c.base, p.start, p.end, Some(id))
        .await?
        .into_iter()
        .filter(is_spend)
        .collect();
    let mut expenses = Vec::new();
    let mut per_item: HashMap<Option<Uuid>, i64> = HashMap::new();
    for r in rows {
        let explicit = r
            .budget_item_id
            .filter(|b| items.iter().any(|i| i.id == Some(*b)));
        let chosen = match_budget_item(explicit, &r.concept, &kw);
        let source = if explicit.is_some() {
            "explicit"
        } else if chosen.is_some() {
            "keyword"
        } else {
            "none"
        };
        *per_item.entry(chosen).or_default() += r.amount_cents;
        let name = chosen
            .and_then(|c| items.iter().find(|i| i.id == Some(c)).map(|i| i.name.clone()))
            .unwrap_or_else(|| "Sin rubro".into());
        expenses.push(ProjectExpense {
            row: r,
            rubro_id: chosen,
            rubro_name: name,
            rubro_source: source.into(),
        });
    }
    let mut rubros: Vec<RubroSpend> = items
        .iter()
        .map(|i| {
            let sp = per_item.get(&i.id).copied().unwrap_or(0);
            RubroSpend {
                budget_item_id: i.id,
                name: i.name.clone(),
                budget_cents: Some(i.amount_cents),
                spent_cents: sp,
                remaining_cents: Some(i.amount_cents - sp),
            }
        })
        .collect();
    if let Some(sp) = per_item.get(&None).filter(|v| **v > 0) {
        rubros.push(RubroSpend {
            budget_item_id: None,
            name: "Sin rubro".into(),
            budget_cents: None,
            spent_cents: *sp,
            remaining_cents: None,
        });
    }
    let spent: i64 = expenses.iter().map(|e| e.row.amount_cents).sum();
    let budget_total: i64 = items.iter().map(|i| i.amount_cents).sum();

    let mut warnings = Vec::new();
    for cat in items
        .iter()
        .map(|i| i.category_id)
        .collect::<std::collections::BTreeSet<_>>()
    {
        let sum_items: i64 = budget
            .items
            .iter()
            .filter(|i| i.category_id == cat)
            .map(|i| i.amount_cents)
            .sum();
        if let Some(l) = budget.lines.iter().find(|l| l.category_id == cat)
            && l.amount_cents != sum_items
        {
            warnings.push(format!(
                "El desglose suma {} pero la categoría {} del mes dice {}",
                domain::money::format_cents(sum_items),
                l.category_name,
                domain::money::format_cents(l.amount_cents)
            ));
        }
    }
    if per_item.get(&None).is_some_and(|v| *v > 0) && !items.is_empty() {
        warnings.push("Hay gastos sin rubro: asígnalos o agrega palabras clave".into());
    }

    // Por mes (últimos 12).
    let from = p.start.checked_sub_months(Months::new(11)).unwrap_or(p.start);
    let hist = repo::report_rows(&s.pool, uid, &c.base, from, p.end, Some(id)).await?;
    let mut by_month: BTreeMap<String, i64> = BTreeMap::new();
    for r in hist.iter().filter(|r| is_spend(r)) {
        *by_month.entry(month_key(r.occurred_on)).or_default() += r.amount_cents;
    }
    let mut months = Vec::new();
    for (m, v) in by_month.into_iter().rev() {
        let b = match persistence::repos::finance::budgets::get_month(&s.pool, uid, &m).await? {
            Some(bm) => Some(
                persistence::repos::finance::budgets::items(&s.pool, uid, bm.id)
                    .await?
                    .iter()
                    .filter(|i| i.project_id == id)
                    .map(|i| i.amount_cents)
                    .sum(),
            ),
            None => None,
        };
        months.push(ProjectMonth {
            month: m,
            spent_cents: v,
            budget_cents: b,
        });
    }

    let subs: Vec<ProjectSubscription> = repo::recurring::list(&s.pool, uid)
        .await?
        .into_iter()
        .filter(|r| r.kind == "subscription" && r.status == "active" && r.project_id == Some(id))
        .map(|r| ProjectSubscription {
            rule_id: r.id,
            name: r.name,
            amount_cents: r.amount_cents,
        })
        .collect();
    Ok(Json(ProjectReport {
        project_id: id,
        project_name: project.name,
        month: key,
        rubros,
        spent_cents: spent,
        budget_cents: budget_total,
        warnings,
        expenses,
        by_month: months,
        subscriptions_month_cents: subs.iter().map(|s| s.amount_cents).sum(),
        subscriptions: subs,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn date_parsing_sanity() {
        assert_eq!(
            super::month_key(chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()),
            "2026-01"
        );
    }
}
