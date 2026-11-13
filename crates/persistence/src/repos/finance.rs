//! Propietario: agente C-finance. Ver docs/CONVENTIONS.md.

//! CONTRATO con el dashboard general (agente A): mantener firma y campos de `period_summary`.

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgExecutor;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::RepoResult;

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

/// Resumen del periodo `[start, end]` (fechas locales inclusivas). Implementación: agente C.
pub async fn period_summary(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    start: NaiveDate,
    end: NaiveDate,
) -> RepoResult<FinanceSummary> {
    let _ = (db, user_id, start, end);
    Ok(FinanceSummary::default())
}
