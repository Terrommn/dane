//! Propietario: agente C-finance. Finanzas completas (T36–T43): catálogo, cuentas y tarjetas con snapshots,
//! movimientos con dinero exacto, presupuestos y rubros, recurrentes con conciliación, deudas, plan de crédito,
//! sugerencia de tarjeta y dashboard. Nunca ejecuta pagos ni se conecta a bancos.

mod catalog;
mod dashboard;
mod debts;
mod plans;
mod recurring;
mod transactions;

use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;
use domain::time::{local_date, parse_tz};
use persistence::Tx;
use persistence::hooks::{self, HabitTrigger, ProgressValue};
use persistence::repos::finance as repo;
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(catalog::router())
        .merge(transactions::router())
        .merge(plans::router())
        .merge(recurring::router())
        .merge(debts::router())
        .merge(dashboard::router())
}

/// Tareas periódicas del módulo (materialización idempotente de recurrencias).
pub fn workers(state: &AppState, shutdown: &crate::workers::Shutdown) -> Vec<tokio::task::JoinHandle<()>> {
    let state = state.clone();
    let shutdown = shutdown.clone();
    vec![tokio::spawn(async move {
        loop {
            match materialize_all(&state).await {
                Ok(n) if n > 0 => tracing::info!(created = n, "ocurrencias recurrentes materializadas"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "materialización de recurrentes falló; se reintenta"),
            }
            if !shutdown.sleep(std::time::Duration::from_secs(15 * 60)).await {
                break;
            }
        }
    })]
}

/// Materializa ocurrencias vencidas de todos los usuarios (catch-up tras reinicio incluido).
pub async fn materialize_all(state: &AppState) -> anyhow::Result<u64> {
    let mut total = 0;
    for (user_id, tz) in repo::recurring::users_with_active_rules(&state.pool).await? {
        let tz = parse_tz(&tz).unwrap_or(chrono_tz::America::Monterrey);
        let today = local_date(state.now(), tz);
        total += repo::recurring::materialize_due_for_user(&state.pool, user_id, today).await?;
    }
    Ok(total)
}

/// Contexto del usuario: zona, moneda base y "hoy" local.
pub(crate) struct Ctx {
    pub tz: Tz,
    pub base: String,
    pub today: NaiveDate,
}

pub(crate) async fn ctx(state: &AppState, user_id: Uuid) -> ApiResult<Ctx> {
    let tz =
        parse_tz(&repo::user_timezone(&state.pool, user_id).await?).unwrap_or(chrono_tz::America::Monterrey);
    let base = repo::base_currency(&state.pool, user_id).await?;
    Ok(Ctx {
        tz,
        base,
        today: local_date(state.now(), tz),
    })
}

pub(crate) fn month_key(d: NaiveDate) -> String {
    format!("{:04}-{:02}", d.year(), d.month())
}

pub(crate) fn parse_month_param(
    m: Option<&str>,
    today: NaiveDate,
) -> ApiResult<(String, domain::time::Period)> {
    let key = m.map(str::to_string).unwrap_or_else(|| month_key(today));
    let p = domain::time::parse_month(&key)?;
    Ok((key, p))
}

/// Efectos de un movimiento **confirmado**, en la misma transacción. Se aplican una sola vez: al crear un movimiento
/// confirmado o al confirmar una ocurrencia pendiente.
pub(crate) async fn apply_confirmed_effects(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    t: &repo::Transaction,
    today: NaiveDate,
) -> ApiResult<()> {
    if t.status != "confirmed" {
        return Ok(());
    }
    if t.kind == "expense" {
        // Vínculo explícito y configurable "registrar gasto → hábito de anotar gastos" (nunca por texto).
        hooks::mark_linked_habit(tx, user_id, HabitTrigger::ExpenseLogged, today).await?;
        // El presupuesto del mes queda como snapshot (copiado del mes anterior) al primer gasto.
        repo::budgets::ensure_month(tx, user_id, &month_key(t.occurred_on)).await?;
        // Préstamo familiar: el gasto ya es consumo; el seguimiento de la obligación es configurable (D-021).
        if let (Some(person), None) = (t.financed_by_person_id, t.debt_id) {
            let track: bool =
                sqlx::query_scalar("SELECT track_family_loans FROM user_settings WHERE user_id = $1")
                    .bind(user_id)
                    .fetch_optional(&mut **tx)
                    .await?
                    .unwrap_or(false);
            if track {
                let name: String =
                    sqlx::query_scalar("SELECT name FROM people WHERE user_id = $1 AND id = $2")
                        .bind(user_id)
                        .bind(person)
                        .fetch_one(&mut **tx)
                        .await?;
                let debt = repo::create_debt(
                    &mut **tx,
                    user_id,
                    &repo::DebtInput {
                        name: &format!("Préstamo de {name}: {}", t.concept),
                        kind: "family",
                        account_id: None,
                        person_id: Some(person),
                        principal_cents: Some(t.amount_cents),
                        currency: &t.currency,
                        opened_on: t.occurred_on,
                        goal_id: None,
                        notes_md: "",
                        cancelled: false,
                    },
                )
                .await?;
                repo::set_transaction_debt(tx, user_id, t.id, debt.id).await?;
            }
        }
    }
    Ok(())
}

/// Recalcula el estado de las deudas afectadas por un movimiento (pago, edición o anulación).
pub(crate) async fn reevaluate_debts_for(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    t: &repo::Transaction,
) -> ApiResult<()> {
    if let Some(d) = t.debt_id {
        reevaluate_debt(tx, user_id, d).await?;
    }
    if t.kind == "card_payment"
        && let Some(card) = t.counter_account_id
    {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM debts WHERE user_id = $1 AND account_id = $2 AND kind = 'credit_card' AND status = 'open'",
        )
        .bind(user_id)
        .bind(card)
        .fetch_all(&mut **tx)
        .await?;
        for id in ids {
            reevaluate_debt(tx, user_id, id).await?;
        }
    }
    Ok(())
}

/// Saldo pendiente de una deuda. Préstamos: principal − abonos confirmados. Tarjeta: límite − disponible estimado
/// (desconocido si no hay snapshot).
pub(crate) async fn debt_remaining(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    d: &repo::Debt,
) -> ApiResult<(i64, Option<i64>, Option<NaiveDate>)> {
    let (paid, last) = repo::debt_payments(&mut **tx, user_id, d.id).await?;
    if d.kind == "credit_card" {
        let Some(card_id) = d.account_id else {
            return Ok((paid, None, last));
        };
        let acc = repo::get_account(&mut **tx, user_id, card_id).await?;
        let snap = repo::latest_snapshot(&mut **tx, user_id, card_id).await?;
        let Some(s) = snap else {
            return Ok((paid, None, last));
        };
        let ms = repo::movements(&mut **tx, user_id, Some(s.observed_on), None, Some(card_id)).await?;
        let est = domain::finance::estimate_balance(
            &domain::finance::Snapshot {
                balance_cents: s.balance_cents,
                observed_at: s.observed_at,
                observed_on: s.observed_on,
                semantics: domain::finance::Semantics::parse(&s.semantics)
                    .unwrap_or(domain::finance::Semantics::Instant),
            },
            card_id,
            &acc.currency,
            &ms,
        );
        let remaining = acc.credit_limit_cents.map(|l| (l - est).max(0));
        return Ok((paid, remaining, last));
    }
    Ok((paid, d.principal_cents.map(|p| (p - paid).max(0)), last))
}

/// Liquida o reabre una deuda según su saldo. Al liquidarse completa (una vez) los hitos de objetivos vinculados;
/// un abono parcial no los completa; si se reabre, se retira el aporte.
pub(crate) async fn reevaluate_debt(tx: &mut Tx<'_>, user_id: Uuid, debt_id: Uuid) -> ApiResult<()> {
    let d = repo::get_debt(&mut **tx, user_id, debt_id).await?;
    if d.status == "cancelled" {
        return Ok(());
    }
    let (_, remaining, last) = debt_remaining(tx, user_id, &d).await?;
    let Some(remaining) = remaining else { return Ok(()) };
    if remaining == 0 && d.status == "open" {
        let paid_on = last.unwrap_or(d.opened_on);
        repo::set_debt_status(tx, user_id, d.id, "paid", Some(paid_on)).await?;
        let mut goals = hooks::contributing_goals(tx, user_id, "debt", d.id).await?;
        if let Some(g) = d.goal_id
            && !goals.contains(&g)
        {
            goals.push(g);
        }
        for g in goals {
            hooks::record_goal_progress(
                tx,
                user_id,
                g,
                paid_on,
                ProgressValue::Completed,
                "debt",
                d.id,
                &format!("Deuda saldada: {}", d.name),
            )
            .await?;
        }
    } else if remaining > 0 && d.status == "paid" && d.kind != "credit_card" {
        repo::set_debt_status(tx, user_id, d.id, "open", None).await?;
        hooks::remove_goal_progress(tx, user_id, "debt", d.id, None).await?;
    }
    Ok(())
}

pub(crate) fn require_version(v: Option<i32>) -> ApiResult<i32> {
    v.ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))
}
