//! T36/T37: catálogo de categorías, cuentas, tarjetas y snapshots de saldo con estimado y antigüedad.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use domain::finance::{Role, next_monthly};
use domain::money::Currency;
use domain::text::required;
use persistence::repos::finance::{self as repo, Account, BalanceSnapshot, Category};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{ctx, require_version};
use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_categories, create_category))
        .routes(routes!(update_category))
        .routes(routes!(list_accounts, create_account))
        .routes(routes!(get_account, update_account))
        .routes(routes!(list_snapshots, add_snapshot))
}

#[derive(Deserialize, IntoParams)]
pub struct ArchivedQuery {
    pub archived: Option<bool>,
}

#[derive(Deserialize, ToSchema)]
pub struct CategoryBody {
    pub name: String,
    /// `expense | income` (no cambia al actualizar).
    pub kind: String,
    /// Gasto: `consumption | debt_payment | investment | payroll`. Ingreso: `earned | financing | other_income`.
    pub role: String,
    pub color: Option<String>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

fn validate_category(b: &CategoryBody) -> ApiResult<String> {
    let name = required("name", &b.name, 60)?;
    if !["expense", "income"].contains(&b.kind.as_str()) {
        return Err(ApiError::validation("kind", "expense o income"));
    }
    let role = Role::parse(&b.role).ok_or_else(|| ApiError::validation("role", "rol inválido"))?;
    if role.is_expense_role() != (b.kind == "expense") {
        return Err(ApiError::validation(
            "role",
            "el rol no corresponde al tipo de categoría",
        ));
    }
    Ok(name)
}

/// Catálogo de categorías (gasto e ingreso).
#[utoipa::path(get, path = "/finance/categories", tag = "finance", params(ArchivedQuery), responses((status = 200, body = Vec<Category>)))]
async fn list_categories(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ArchivedQuery>,
) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(
        repo::list_categories(&s.pool, auth.user_id, q.archived.unwrap_or(false)).await?,
    ))
}

#[utoipa::path(post, path = "/finance/categories", tag = "finance", request_body = CategoryBody, responses((status = 201, body = Category)))]
async fn create_category(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<CategoryBody>,
) -> ApiResult<(StatusCode, Json<Category>)> {
    let name = validate_category(&b)?;
    let c = repo::create_category(
        &s.pool,
        auth.user_id,
        &repo::CategoryInput {
            name: &name,
            kind: &b.kind,
            role: &b.role,
            color: b.color.as_deref(),
            position: b.position,
            archived: false,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "fin_category",
        Some(c.id),
        "create",
        serde_json::json!({ "name": name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(c)))
}

/// Renombrar/archivar conserva el ID y los movimientos históricos.
#[utoipa::path(put, path = "/finance/categories/{id}", tag = "finance", request_body = CategoryBody,
    responses((status = 200, body = Category), (status = 409, body = crate::error::ErrorBody)))]
async fn update_category(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<CategoryBody>,
) -> ApiResult<Json<Category>> {
    let name = validate_category(&b)?;
    let cur = repo::get_category(&s.pool, auth.user_id, id).await?;
    if cur.kind != b.kind {
        return Err(ApiError::validation(
            "kind",
            "el tipo de una categoría no se puede cambiar",
        ));
    }
    let c = repo::update_category(
        &s.pool,
        auth.user_id,
        id,
        require_version(b.version)?,
        &repo::CategoryInput {
            name: &name,
            kind: &b.kind,
            role: &b.role,
            color: b.color.as_deref(),
            position: b.position,
            archived: b.archived,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "fin_category",
        Some(id),
        "update",
        serde_json::json!({ "name": name }),
    )
    .await;
    Ok(Json(c))
}

// ---------------------------------------------------------------- Cuentas

#[derive(Serialize, ToSchema)]
pub struct AccountView {
    pub account: Account,
    /// Último saldo/disponible observado.
    pub snapshot: Option<BalanceSnapshot>,
    /// Estimado = observado + movimientos confirmados posteriores. No es un dato del banco.
    pub estimated_cents: Option<i64>,
    /// Días desde la observación (antigüedad del dato).
    pub snapshot_age_days: Option<i64>,
    /// Tarjetas: usado = límite − disponible estimado.
    pub used_cents: Option<i64>,
    pub usage_pct: Option<i64>,
    pub next_cut_on: Option<NaiveDate>,
    pub next_pay_on: Option<NaiveDate>,
    pub days_to_pay: Option<i64>,
}

pub(crate) async fn account_view(
    s: &AppState,
    user_id: Uuid,
    a: Account,
    today: NaiveDate,
) -> ApiResult<AccountView> {
    let est = repo::estimate_account(&s.pool, user_id, &a).await?;
    let (snapshot, estimated) = match est {
        Some((sn, e)) => (Some(sn), Some(e)),
        None => (None, None),
    };
    let age = snapshot.as_ref().map(|sn| (today - sn.observed_on).num_days());
    let used = match (a.credit_limit_cents, estimated) {
        (Some(l), Some(e)) if a.kind == "credit_card" => Some(l - e),
        _ => None,
    };
    let usage_pct = match (used, a.credit_limit_cents) {
        (Some(u), Some(l)) if l > 0 => domain::money::pct(u, l),
        _ => None,
    };
    let next_cut_on = a.cut_day.map(|d| next_monthly(d as u32, today));
    let next_pay_on = a.pay_day.map(|d| next_monthly(d as u32, today));
    Ok(AccountView {
        days_to_pay: next_pay_on.map(|p| (p - today).num_days()),
        account: a,
        snapshot,
        estimated_cents: estimated,
        snapshot_age_days: age,
        used_cents: used,
        usage_pct,
        next_cut_on,
        next_pay_on,
    })
}

/// Cuentas y tarjetas con saldo/disponible estimado y antigüedad del snapshot (sin conexión al banco).
#[utoipa::path(get, path = "/finance/accounts", tag = "finance", params(ArchivedQuery), responses((status = 200, body = Vec<AccountView>)))]
async fn list_accounts(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ArchivedQuery>,
) -> ApiResult<Json<Vec<AccountView>>> {
    let c = ctx(&s, auth.user_id).await?;
    let mut out = Vec::new();
    for a in repo::list_accounts(&s.pool, auth.user_id, q.archived.unwrap_or(false)).await? {
        out.push(account_view(&s, auth.user_id, a, c.today).await?);
    }
    Ok(Json(out))
}

#[derive(Deserialize, ToSchema)]
pub struct AccountBody {
    pub name: String,
    #[serde(default)]
    pub bank: String,
    /// `debit | cash | savings | credit_card` (no cambia al actualizar).
    pub kind: String,
    pub currency: Option<String>,
    pub credit_limit_cents: Option<i64>,
    pub cut_day: Option<i16>,
    pub pay_day: Option<i16>,
    /// `active | to_cancel | cancelled | closed`
    #[serde(default = "active")]
    pub status: String,
    #[serde(default)]
    pub is_default_debit: bool,
    #[serde(default)]
    pub notes_md: String,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

fn active() -> String {
    "active".into()
}

struct ValidAccount {
    name: String,
    currency: String,
}

fn validate_account(b: &AccountBody, base: &str) -> ApiResult<ValidAccount> {
    let name = required("name", &b.name, 80)?;
    domain::finance::AccountKind::parse(&b.kind)?;
    if !["active", "to_cancel", "cancelled", "closed"].contains(&b.status.as_str()) {
        return Err(ApiError::validation("status", "estado inválido"));
    }
    let card = b.kind == "credit_card";
    if !card && (b.credit_limit_cents.is_some() || b.cut_day.is_some() || b.pay_day.is_some()) {
        return Err(ApiError::validation(
            "kind",
            "límite, corte y pago solo aplican a tarjetas",
        ));
    }
    if card && b.credit_limit_cents.is_none_or(|l| l <= 0) {
        return Err(ApiError::validation(
            "credit_limit_cents",
            "la tarjeta necesita un límite positivo",
        ));
    }
    for (f, v) in [("cut_day", b.cut_day), ("pay_day", b.pay_day)] {
        if v.is_some_and(|d| !(1..=31).contains(&d)) {
            return Err(ApiError::validation(f, "día entre 1 y 31"));
        }
    }
    if card && b.is_default_debit {
        return Err(ApiError::validation(
            "is_default_debit",
            "una tarjeta de crédito no puede ser el débito por defecto",
        ));
    }
    let currency = Currency::parse(b.currency.as_deref().unwrap_or(base))?.to_string();
    Ok(ValidAccount { name, currency })
}

fn account_input<'a>(b: &'a AccountBody, v: &'a ValidAccount) -> repo::AccountInput<'a> {
    repo::AccountInput {
        name: &v.name,
        bank: b.bank.trim(),
        kind: &b.kind,
        currency: &v.currency,
        credit_limit_cents: b.credit_limit_cents,
        cut_day: b.cut_day,
        pay_day: b.pay_day,
        status: &b.status,
        is_default_debit: b.is_default_debit,
        notes_md: &b.notes_md,
        archived: b.archived,
    }
}

#[utoipa::path(post, path = "/finance/accounts", tag = "finance", request_body = AccountBody, responses((status = 201, body = AccountView)))]
async fn create_account(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<AccountBody>,
) -> ApiResult<(StatusCode, Json<AccountView>)> {
    let c = ctx(&s, auth.user_id).await?;
    let v = validate_account(&b, &c.base)?;
    let mut tx = s.pool.begin().await?;
    let a = repo::create_account(&mut tx, auth.user_id, &account_input(&b, &v)).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "fin_account",
        Some(a.id),
        "create",
        serde_json::json!({ "name": a.name, "kind": a.kind }),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(account_view(&s, auth.user_id, a, c.today).await?),
    ))
}

#[utoipa::path(get, path = "/finance/accounts/{id}", tag = "finance", responses((status = 200, body = AccountView)))]
async fn get_account(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AccountView>> {
    let c = ctx(&s, auth.user_id).await?;
    let a = repo::get_account(&s.pool, auth.user_id, id).await?;
    Ok(Json(account_view(&s, auth.user_id, a, c.today).await?))
}

#[utoipa::path(put, path = "/finance/accounts/{id}", tag = "finance", request_body = AccountBody,
    responses((status = 200, body = AccountView), (status = 409, body = crate::error::ErrorBody)))]
async fn update_account(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<AccountBody>,
) -> ApiResult<Json<AccountView>> {
    let c = ctx(&s, auth.user_id).await?;
    let v = validate_account(&b, &c.base)?;
    let cur = repo::get_account(&s.pool, auth.user_id, id).await?;
    if cur.kind != b.kind {
        return Err(ApiError::validation(
            "kind",
            "el tipo de cuenta no se puede cambiar",
        ));
    }
    let mut tx = s.pool.begin().await?;
    let a = repo::update_account(
        &mut tx,
        auth.user_id,
        id,
        require_version(b.version)?,
        &account_input(&b, &v),
    )
    .await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "fin_account",
        Some(id),
        "update",
        serde_json::json!({ "status": a.status }),
    )
    .await;
    Ok(Json(account_view(&s, auth.user_id, a, c.today).await?))
}

#[utoipa::path(get, path = "/finance/accounts/{id}/snapshots", tag = "finance", responses((status = 200, body = Vec<BalanceSnapshot>)))]
async fn list_snapshots(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<BalanceSnapshot>>> {
    repo::get_account(&s.pool, auth.user_id, id).await?;
    Ok(Json(repo::list_snapshots(&s.pool, auth.user_id, id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct SnapshotBody {
    /// Débito: saldo actual. Tarjeta: crédito disponible que muestra la app del banco. Centavos.
    pub balance_cents: Option<i64>,
    /// Alternativa en texto (`"4,500.50"`).
    pub balance: Option<String>,
    /// Momento de la observación; por defecto, ahora.
    pub observed_at: Option<DateTime<Utc>>,
    /// Solo semánticas legacy del vault: fecha base (`actualizado`).
    pub observed_on: Option<NaiveDate>,
    /// `instant` (por defecto) | `legacy_day_start_inclusive` (cuentas del vault) | `legacy_after_day` (tarjetas del vault).
    pub semantics: Option<String>,
    #[serde(default)]
    pub note: String,
}

/// Registra el saldo/disponible observado (conciliación manual). Los movimientos posteriores se suman/restan al estimado.
#[utoipa::path(post, path = "/finance/accounts/{id}/snapshots", tag = "finance", request_body = SnapshotBody,
    responses((status = 201, body = AccountView)))]
async fn add_snapshot(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<SnapshotBody>,
) -> ApiResult<(StatusCode, Json<AccountView>)> {
    let c = ctx(&s, auth.user_id).await?;
    let a = repo::get_account(&s.pool, auth.user_id, id).await?;
    let balance = match (b.balance_cents, b.balance.as_deref()) {
        (Some(v), _) => v,
        (None, Some(t)) => {
            let neg = t.trim().starts_with('-');
            let v = domain::money::parse_amount_to_cents(t.trim().trim_start_matches('-'))?;
            if neg { -v } else { v }
        }
        _ => return Err(ApiError::validation("balance_cents", "indica el saldo observado")),
    };
    let semantics = b.semantics.clone().unwrap_or_else(|| "instant".into());
    domain::finance::Semantics::parse(&semantics)?;
    let (observed_at, observed_on) = if semantics == "instant" {
        let at = b.observed_at.unwrap_or_else(|| s.now());
        if at > s.now() + chrono::Duration::minutes(5) {
            return Err(ApiError::validation("observed_at", "no puede estar en el futuro"));
        }
        (at, domain::time::local_date(at, c.tz))
    } else {
        let on = b
            .observed_on
            .ok_or_else(|| ApiError::validation("observed_on", "obligatorio en semántica legacy"))?;
        (domain::time::start_of_local_day(on, c.tz), on)
    };
    repo::add_snapshot(
        &s.pool,
        auth.user_id,
        id,
        observed_at,
        observed_on,
        balance,
        &semantics,
        b.note.trim(),
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "fin_account",
        Some(id),
        "snapshot",
        serde_json::json!({ "balance_cents": balance, "semantics": semantics }),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(account_view(&s, auth.user_id, a, c.today).await?),
    ))
}
