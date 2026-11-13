//! T36/T38/T41: movimientos con dinero exacto. Captura individual y múltiple con idempotencia por elemento
//! (`client_key`): dos compras iguales con claves distintas se conservan; un reintento con la misma clave no duplica.
//! Movimientos manuales = confirmados por el usuario. Ocurrencias recurrentes = pendientes hasta confirmarse.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use domain::money::{Currency, convert_cents, parse_amount_to_cents};
use domain::text::required;
use persistence::Tx;
use persistence::repos::finance::{self as repo, Transaction, TxFilter, TxInput, TxTotals};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{Ctx, apply_confirmed_effects, ctx, reevaluate_debt, reevaluate_debts_for, require_version};
use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_transactions, create_transaction))
        .routes(routes!(create_batch))
        .routes(routes!(get_transaction, update_transaction))
        .routes(routes!(void_transaction))
        .routes(routes!(confirm_transaction))
}

#[derive(Deserialize, ToSchema, Clone)]
pub struct FxBody {
    /// Monto original en centavos de `currency` (o `amount` en texto).
    pub amount_cents: Option<i64>,
    pub amount: Option<String>,
    pub currency: String,
    /// Tasa explícita: unidades de moneda base por 1 unidad original (ej. "17.2345"). No se inventan cotizaciones.
    pub fx_rate: String,
    pub fx_date: NaiveDate,
    /// Fuente de la tasa (banco, app, Banxico…).
    pub fx_source: String,
}

#[derive(Deserialize, ToSchema, Clone)]
pub struct TxBody {
    /// `expense | income | transfer | card_payment | debt_payment | loan_received | adjustment`
    pub kind: String,
    /// Fecha real del movimiento (local).
    pub occurred_on: NaiveDate,
    pub occurred_at: Option<DateTime<Utc>>,
    pub concept: String,
    /// Centavos exactos; alternativa: `amount` en texto (`"1,250.50"`).
    pub amount_cents: Option<i64>,
    pub amount: Option<String>,
    /// Moneda del monto si no se da conversión; por defecto la base del usuario.
    pub currency: Option<String>,
    /// Moneda distinta con conversión explícita: el monto se registra convertido y se conserva el original.
    pub original: Option<FxBody>,
    pub category_id: Option<Uuid>,
    /// Cuenta/tarjeta que paga (gasto), recibe (ingreso), origen (transferencia/pago).
    pub account_id: Option<Uuid>,
    /// Destino de transferencia o tarjeta que recibe un pago.
    pub counter_account_id: Option<Uuid>,
    pub debt_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    /// Gasto pagado por otra persona (préstamo familiar): cuenta como consumo una sola vez.
    pub financed_by_person_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    /// Rubro explícito del presupuesto del proyecto (prevalece sobre palabras clave).
    pub budget_item_id: Option<Uuid>,
    /// `adjustment`: 1 suma, -1 resta.
    pub adjustment_sign: Option<i16>,
    #[serde(default)]
    pub notes_md: String,
    /// Clave única del cliente para reintentos/sincronización offline.
    pub client_key: Option<String>,
    /// `loan_received`: crear también la deuda correspondiente.
    #[serde(default)]
    pub track_as_debt: bool,
    /// `manual` (defecto) | `bot` | `llm` | `import`.
    pub origin: Option<String>,
    pub version: Option<i32>,
}

struct AccountInfo {
    kind: String,
    currency: String,
}

async fn account_info(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Option<Uuid>,
    field: &str,
) -> ApiResult<Option<AccountInfo>> {
    let Some(id) = id else { return Ok(None) };
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT kind, currency::text FROM fin_accounts WHERE user_id = $1 AND id = $2")
            .bind(user_id)
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
    let (kind, currency) = row.ok_or_else(|| ApiError::validation(field, "cuenta inexistente"))?;
    Ok(Some(AccountInfo { kind, currency }))
}

fn amount_of(cents: Option<i64>, text: Option<&str>, field: &str) -> ApiResult<i64> {
    let v = match (cents, text) {
        (Some(c), _) => c,
        (None, Some(t)) => parse_amount_to_cents(t)?,
        _ => return Err(ApiError::validation(field, "indica el monto")),
    };
    if v <= 0 {
        return Err(ApiError::validation(field, "el monto debe ser mayor que cero"));
    }
    Ok(v)
}

/// Valida un movimiento según su tipo y devuelve el input del repositorio.
pub(crate) async fn validate(tx: &mut Tx<'_>, user_id: Uuid, c: &Ctx, b: &TxBody) -> ApiResult<TxInput> {
    let kind = domain::finance::Kind::parse(&b.kind)?;
    use domain::finance::Kind as K;
    let concept = required("concept", &b.concept, 200)?;
    let (amount_cents, currency, orig_amount, orig_cur, fx_rate, fx_date, fx_source) = match &b.original {
        Some(fx) => {
            let oc = Currency::parse(&fx.currency)?.to_string();
            if oc == c.base {
                return Err(ApiError::validation(
                    "original.currency",
                    "la moneda original es la moneda base",
                ));
            }
            let rate = Decimal::from_str(fx.fx_rate.trim())
                .map_err(|_| ApiError::validation("original.fx_rate", "tasa inválida"))?;
            if rate <= Decimal::ZERO {
                return Err(ApiError::validation(
                    "original.fx_rate",
                    "la tasa debe ser positiva",
                ));
            }
            let source = required("original.fx_source", &fx.fx_source, 120)?;
            let orig = amount_of(fx.amount_cents, fx.amount.as_deref(), "original.amount_cents")?;
            let converted = convert_cents(orig, rate)?;
            (
                converted,
                c.base.clone(),
                Some(orig),
                Some(oc),
                Some(rate),
                Some(fx.fx_date),
                Some(source),
            )
        }
        None => {
            let cur = Currency::parse(b.currency.as_deref().unwrap_or(&c.base))?.to_string();
            (
                amount_of(b.amount_cents, b.amount.as_deref(), "amount_cents")?,
                cur,
                None,
                None,
                None,
                None,
                None,
            )
        }
    };
    if amount_cents <= 0 {
        return Err(ApiError::validation(
            "amount_cents",
            "el monto convertido debe ser mayor que cero",
        ));
    }
    let acc = account_info(tx, user_id, b.account_id, "account_id").await?;
    let counter = account_info(tx, user_id, b.counter_account_id, "counter_account_id").await?;
    let is_card = |a: &Option<AccountInfo>| a.as_ref().is_some_and(|a| a.kind == "credit_card");
    let is_cash = |a: &Option<AccountInfo>| a.as_ref().is_some_and(|a| a.kind != "credit_card");
    if let Some(a) = acc.as_ref().or(counter.as_ref())
        && a.currency != currency
    {
        return Err(ApiError::validation(
            "currency",
            "la moneda del movimiento no coincide con la de la cuenta",
        ));
    }

    // Categoría del tipo correcto.
    let cat_kind: Option<String> = match b.category_id {
        Some(id) => Some(
            sqlx::query_scalar("SELECT kind FROM fin_categories WHERE user_id = $1 AND id = $2")
                .bind(user_id)
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or_else(|| ApiError::validation("category_id", "categoría inexistente"))?,
        ),
        None => None,
    };
    let need_counter_none = |field: &str| -> ApiResult<()> {
        if b.counter_account_id.is_some() {
            return Err(ApiError::validation(
                field,
                "este tipo de movimiento no lleva cuenta destino",
            ));
        }
        Ok(())
    };
    match kind {
        K::Expense => {
            if cat_kind.as_deref() != Some("expense") {
                return Err(ApiError::validation(
                    "category_id",
                    "elige una categoría de gasto",
                ));
            }
            need_counter_none("counter_account_id")?;
            if b.financed_by_person_id.is_some() && b.account_id.is_some() {
                return Err(ApiError::validation(
                    "account_id",
                    "un gasto pagado por otra persona no sale de tus cuentas",
                ));
            }
        }
        K::Income => {
            if cat_kind.as_deref() != Some("income") {
                return Err(ApiError::validation(
                    "category_id",
                    "elige una categoría de ingreso",
                ));
            }
            need_counter_none("counter_account_id")?;
            if is_card(&acc) {
                return Err(ApiError::validation(
                    "account_id",
                    "un ingreso entra a una cuenta, no a una tarjeta",
                ));
            }
        }
        K::LoanReceived => {
            if cat_kind.as_deref().is_some_and(|k| k != "income") {
                return Err(ApiError::validation(
                    "category_id",
                    "categoría de ingreso (financiamiento)",
                ));
            }
            need_counter_none("counter_account_id")?;
        }
        K::Transfer => {
            if !(is_cash(&acc) && is_cash(&counter)) {
                return Err(ApiError::validation(
                    "counter_account_id",
                    "una transferencia es entre cuentas de débito/efectivo; para pagar una tarjeta usa card_payment",
                ));
            }
        }
        K::CardPayment => {
            if !is_cash(&acc) || !is_card(&counter) {
                return Err(ApiError::validation(
                    "counter_account_id",
                    "el pago de tarjeta sale de una cuenta y va a una tarjeta",
                ));
            }
        }
        K::DebtPayment => {
            if b.debt_id.is_none() {
                return Err(ApiError::validation("debt_id", "indica la deuda que abonas"));
            }
            need_counter_none("counter_account_id")?;
        }
        K::Adjustment => {
            if acc.is_none() || !matches!(b.adjustment_sign, Some(1) | Some(-1)) {
                return Err(ApiError::validation(
                    "adjustment_sign",
                    "un ajuste necesita cuenta y signo (1 o -1)",
                ));
            }
        }
    }
    if kind != K::Expense && (b.financed_by_person_id.is_some() || b.budget_item_id.is_some()) {
        return Err(ApiError::validation(
            "kind",
            "persona financiadora y rubro solo aplican a gastos",
        ));
    }
    let origin = b.origin.clone().unwrap_or_else(|| "manual".into());
    if !["manual", "bot", "llm", "import"].contains(&origin.as_str()) {
        return Err(ApiError::validation("origin", "origen inválido"));
    }
    Ok(TxInput {
        kind: b.kind.clone(),
        status: "confirmed".into(),
        origin,
        occurred_on: b.occurred_on,
        occurred_at: b.occurred_at,
        concept,
        amount_cents,
        currency,
        original_amount_cents: orig_amount,
        original_currency: orig_cur,
        fx_rate,
        fx_date,
        fx_source,
        category_id: b.category_id,
        account_id: b.account_id,
        counter_account_id: b.counter_account_id,
        debt_id: b.debt_id,
        person_id: b.person_id,
        financed_by_person_id: b.financed_by_person_id,
        project_id: b.project_id,
        area_id: b.area_id,
        budget_item_id: b.budget_item_id,
        adjustment_sign: if kind == K::Adjustment {
            b.adjustment_sign
        } else {
            None
        },
        notes_md: b.notes_md.clone(),
    })
}

#[derive(Serialize, ToSchema)]
pub struct CreatedTransaction {
    pub transaction: Transaction,
    /// `false` si era un reintento con la misma `client_key` (no se duplicó).
    pub created: bool,
    /// Aviso para el usuario (ej. préstamo recibido = deuda).
    pub notice: Option<String>,
}

fn valid_key(k: Option<&str>) -> ApiResult<Option<String>> {
    match k.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) if (8..=200).contains(&k.len()) => Ok(Some(k.to_string())),
        Some(_) => Err(ApiError::validation("client_key", "entre 8 y 200 caracteres")),
        None => Ok(None),
    }
}

async fn create_one(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    c: &Ctx,
    b: &TxBody,
    now: DateTime<Utc>,
) -> ApiResult<CreatedTransaction> {
    let input = validate(tx, user_id, c, b).await?;
    let key = valid_key(b.client_key.as_deref())?;
    let (t, created) = repo::insert_transaction(tx, user_id, &input, key.as_deref(), now).await?;
    let mut notice = None;
    if created {
        let mut t2 = t.clone();
        if t.kind == "loan_received" {
            if b.track_as_debt {
                let debt = repo::create_debt(
                    &mut **tx,
                    user_id,
                    &repo::DebtInput {
                        name: &format!("Préstamo: {}", t.concept),
                        kind: "loan",
                        account_id: None,
                        person_id: t.person_id,
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
                notice = Some("Registré el préstamo como financiamiento y creé la deuda".into());
            } else {
                notice = Some("Es financiamiento, no ingreso ganado: recuerda que es una deuda".into());
            }
        }
        apply_confirmed_effects(tx, user_id, &t2, c.today).await?;
        reevaluate_debts_for(tx, user_id, &t2).await?;
        t2 = repo::get_transaction(&mut **tx, user_id, t.id).await?;
        return Ok(CreatedTransaction {
            transaction: t2,
            created,
            notice,
        });
    }
    Ok(CreatedTransaction {
        transaction: t,
        created,
        notice,
    })
}

/// Registra un movimiento confirmado por el usuario.
#[utoipa::path(post, path = "/finance/transactions", tag = "finance", request_body = TxBody,
    responses((status = 201, body = CreatedTransaction), (status = 200, body = CreatedTransaction, description = "reintento: ya existía")))]
async fn create_transaction(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<TxBody>,
) -> ApiResult<(StatusCode, Json<CreatedTransaction>)> {
    let c = ctx(&s, auth.user_id).await?;
    let mut tx = s.pool.begin().await?;
    let r = create_one(&mut tx, auth.user_id, &c, &b, s.now()).await?;
    tx.commit().await?;
    if r.created {
        audit(
            &s,
            auth.user_id,
            "transaction",
            Some(r.transaction.id),
            "create",
            serde_json::json!({ "kind": r.transaction.kind, "amount_cents": r.transaction.amount_cents }),
        )
        .await;
    }
    Ok((
        if r.created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(r),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct BatchBody {
    /// Cada elemento debería llevar `client_key` propio para reintentos seguros.
    pub items: Vec<TxBody>,
}

#[derive(Serialize, ToSchema)]
pub struct BatchResult {
    pub results: Vec<CreatedTransaction>,
}

/// Captura múltiple atómica: si un elemento es inválido no se guarda ninguno (422 con el índice en `field`).
#[utoipa::path(post, path = "/finance/transactions/batch", tag = "finance", request_body = BatchBody,
    responses((status = 200, body = BatchResult), (status = 422, body = crate::error::ErrorBody)))]
async fn create_batch(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<BatchBody>,
) -> ApiResult<Json<BatchResult>> {
    if b.items.is_empty() || b.items.len() > 100 {
        return Err(ApiError::validation("items", "entre 1 y 100 movimientos"));
    }
    let c = ctx(&s, auth.user_id).await?;
    let mut tx = s.pool.begin().await?;
    let mut results = Vec::new();
    for (i, item) in b.items.iter().enumerate() {
        let r = create_one(&mut tx, auth.user_id, &c, item, s.now())
            .await
            .map_err(|mut e| {
                e.field = Some(format!("items[{i}].{}", e.field.unwrap_or_default()));
                e
            })?;
        results.push(r);
    }
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "transaction",
        None,
        "batch",
        serde_json::json!({ "count": results.len() }),
    )
    .await;
    Ok(Json(BatchResult { results }))
}

#[derive(Deserialize, IntoParams)]
pub struct TxQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub kind: Option<String>,
    /// `confirmed | pending_reconciliation | void` (por defecto, todos menos anulados).
    pub status: Option<String>,
    pub category_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub person_id: Option<Uuid>,
    pub debt_id: Option<Uuid>,
    pub q: Option<String>,
    /// `date_desc` | `date_asc` | `amount_desc` | `amount_asc`
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct TxPage {
    pub items: Vec<Transaction>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    /// Totales del filtro (confirmados en moneda base; pendientes y otra moneda aparte).
    pub totals: TxTotals,
}

/// Histórico de movimientos con filtros, orden, paginación y totales.
#[utoipa::path(get, path = "/finance/transactions", tag = "finance", params(TxQuery), responses((status = 200, body = TxPage)))]
async fn list_transactions(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<TxQuery>,
) -> ApiResult<Json<TxPage>> {
    let c = ctx(&s, auth.user_id).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let f = TxFilter {
        from: q.from,
        to: q.to,
        kind: q.kind,
        status: q.status,
        category_id: q.category_id,
        account_id: q.account_id,
        project_id: q.project_id,
        person_id: q.person_id,
        debt_id: q.debt_id,
        q: q.q
            .map(|x| x.trim().chars().take(100).collect())
            .filter(|x: &String| !x.is_empty()),
        sort: q.sort,
        limit,
        offset,
    };
    let (items, totals) = repo::list_transactions(&s.pool, auth.user_id, &c.base, &f).await?;
    Ok(Json(TxPage {
        total: totals.count,
        items,
        limit,
        offset,
        totals,
    }))
}

#[utoipa::path(get, path = "/finance/transactions/{id}", tag = "finance", responses((status = 200, body = Transaction)))]
async fn get_transaction(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Transaction>> {
    Ok(Json(repo::get_transaction(&s.pool, auth.user_id, id).await?))
}

/// Edita un movimiento (versión obligatoria). El tipo no cambia; se recalculan deudas afectadas.
#[utoipa::path(put, path = "/finance/transactions/{id}", tag = "finance", request_body = TxBody,
    responses((status = 200, body = Transaction), (status = 409, body = crate::error::ErrorBody)))]
async fn update_transaction(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<TxBody>,
) -> ApiResult<Json<Transaction>> {
    let c = ctx(&s, auth.user_id).await?;
    let version = require_version(b.version)?;
    let mut tx = s.pool.begin().await?;
    let before = repo::get_transaction(&mut *tx, auth.user_id, id).await?;
    if before.kind != b.kind {
        return Err(ApiError::validation(
            "kind",
            "el tipo de movimiento no se puede cambiar; anúlalo y registra otro",
        ));
    }
    let input = validate(&mut tx, auth.user_id, &c, &b).await?;
    let t = repo::update_transaction(&mut tx, auth.user_id, id, version, &input).await?;
    if let Some(old) = before.debt_id.filter(|d| Some(*d) != t.debt_id) {
        reevaluate_debt(&mut tx, auth.user_id, old).await?;
    }
    reevaluate_debts_for(&mut tx, auth.user_id, &t).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "transaction",
        Some(id),
        "update",
        serde_json::json!({ "amount_cents": t.amount_cents }),
    )
    .await;
    Ok(Json(t))
}

#[derive(Deserialize, ToSchema)]
pub struct VersionBody {
    pub version: i32,
}

/// Anula un movimiento sin borrarlo (deja de contar en saldos y totales).
#[utoipa::path(post, path = "/finance/transactions/{id}/void", tag = "finance", request_body = VersionBody,
    responses((status = 200, body = Transaction), (status = 409, body = crate::error::ErrorBody)))]
async fn void_transaction(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<VersionBody>,
) -> ApiResult<Json<Transaction>> {
    let mut tx = s.pool.begin().await?;
    let t = repo::set_transaction_status(
        &mut tx,
        auth.user_id,
        id,
        b.version,
        &["confirmed", "pending_reconciliation"],
        "void",
    )
    .await?;
    reevaluate_debts_for(&mut tx, auth.user_id, &t).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "transaction",
        Some(id),
        "void",
        serde_json::json!({}),
    )
    .await;
    Ok(Json(t))
}

#[derive(Deserialize, ToSchema)]
pub struct ConfirmBody {
    pub version: i32,
    /// Corrección del monto real cobrado/recibido (centavos).
    pub amount_cents: Option<i64>,
    /// Fecha real del cargo/abono si difiere de la programada.
    pub occurred_on: Option<NaiveDate>,
    pub account_id: Option<Uuid>,
}

/// Confirma (y opcionalmente corrige) una ocurrencia pendiente de conciliación. Actualiza la **misma** fila; los efectos
/// se activan una sola vez. Confirmar algo ya confirmado devuelve el movimiento sin repetir efectos.
#[utoipa::path(post, path = "/finance/transactions/{id}/confirm", tag = "finance", request_body = ConfirmBody,
    responses((status = 200, body = Transaction), (status = 409, body = crate::error::ErrorBody)))]
async fn confirm_transaction(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ConfirmBody>,
) -> ApiResult<Json<Transaction>> {
    let c = ctx(&s, auth.user_id).await?;
    let mut tx = s.pool.begin().await?;
    let cur = repo::get_transaction(&mut *tx, auth.user_id, id).await?;
    match cur.status.as_str() {
        "confirmed" => return Ok(Json(cur)),
        "void" => return Err(ApiError::validation("status", "el movimiento está anulado")),
        _ => {}
    }
    if cur.version != b.version {
        return Err(ApiError::conflict(serde_json::to_value(&cur).unwrap_or_default()));
    }
    let mut version = b.version;
    if b.amount_cents.is_some() || b.occurred_on.is_some() || b.account_id.is_some() {
        if b.amount_cents.is_some_and(|a| a <= 0) {
            return Err(ApiError::validation(
                "amount_cents",
                "el monto debe ser mayor que cero",
            ));
        }
        if let Some(acc) = b.account_id {
            account_info(&mut tx, auth.user_id, Some(acc), "account_id").await?;
        }
        let corrected: Option<i32> = sqlx::query_scalar(
            "UPDATE transactions SET amount_cents = coalesce($4, amount_cents), occurred_on = coalesce($5, occurred_on),
                account_id = coalesce($6, account_id), version = version + 1
             WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING version",
        )
        .bind(auth.user_id)
        .bind(id)
        .bind(version)
        .bind(b.amount_cents)
        .bind(b.occurred_on)
        .bind(b.account_id)
        .fetch_optional(&mut *tx)
        .await?;
        version = corrected.ok_or_else(|| ApiError::conflict(serde_json::Value::Null))?;
    }
    let t = repo::set_transaction_status(
        &mut tx,
        auth.user_id,
        id,
        version,
        &["pending_reconciliation"],
        "confirmed",
    )
    .await?;
    apply_confirmed_effects(&mut tx, auth.user_id, &t, c.today).await?;
    reevaluate_debts_for(&mut tx, auth.user_id, &t).await?;
    let t = repo::get_transaction(&mut *tx, auth.user_id, id).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "transaction",
        Some(id),
        "confirm",
        serde_json::json!({ "amount_cents": t.amount_cents }),
    )
    .await;
    Ok(Json(t))
}
