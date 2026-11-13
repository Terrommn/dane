//! T41 deudas (abonos, saldo, vínculo con objetivos) y T42 sugerencia explicable de tarjeta.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::finance::{CardCandidate, PlanRules, suggest_card};
use domain::money::Currency;
use domain::text::required;
use persistence::repos::finance::{self as repo, Debt};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{ctx, debt_remaining, reevaluate_debt, require_version};
use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_debts, create_debt))
        .routes(routes!(get_debt, update_debt))
        .routes(routes!(card_suggestion))
}

#[derive(Serialize, ToSchema)]
pub struct DebtView {
    pub debt: Debt,
    /// Abonos confirmados.
    pub paid_cents: i64,
    /// Saldo pendiente (tarjeta: límite − disponible estimado; desconocido sin snapshot).
    pub remaining_cents: Option<i64>,
    pub last_payment_on: Option<NaiveDate>,
}

async fn view(s: &AppState, user_id: Uuid, d: Debt) -> ApiResult<DebtView> {
    let mut tx = s.pool.begin().await?;
    let (paid, remaining, last) = debt_remaining(&mut tx, user_id, &d).await?;
    tx.commit().await?;
    Ok(DebtView {
        debt: d,
        paid_cents: paid,
        remaining_cents: remaining,
        last_payment_on: last,
    })
}

#[utoipa::path(get, path = "/finance/debts", tag = "finance", responses((status = 200, body = Vec<DebtView>)))]
async fn list_debts(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<DebtView>>> {
    let mut out = Vec::new();
    for d in repo::list_debts(&s.pool, auth.user_id).await? {
        out.push(view(&s, auth.user_id, d).await?);
    }
    Ok(Json(out))
}

#[derive(Deserialize, ToSchema)]
pub struct DebtBody {
    pub name: String,
    /// `credit_card | loan | family | other`
    pub kind: String,
    /// Tarjeta asociada (kind = credit_card).
    pub account_id: Option<Uuid>,
    /// Acreedor (préstamo familiar).
    pub person_id: Option<Uuid>,
    /// Monto original (obligatorio salvo tarjeta, cuyo saldo se deriva de su disponible).
    pub principal_cents: Option<i64>,
    pub currency: Option<String>,
    pub opened_on: NaiveDate,
    /// Objetivo que se completa al saldarla (además de los enlaces `contributes_to`).
    pub goal_id: Option<Uuid>,
    #[serde(default)]
    pub notes_md: String,
    #[serde(default)]
    pub cancelled: bool,
    pub version: Option<i32>,
}

fn validate(b: &DebtBody, base: &str) -> ApiResult<(String, String)> {
    let name = required("name", &b.name, 120)?;
    if !["credit_card", "loan", "family", "other"].contains(&b.kind.as_str()) {
        return Err(ApiError::validation("kind", "tipo de deuda inválido"));
    }
    if b.kind == "credit_card" && b.account_id.is_none() {
        return Err(ApiError::validation("account_id", "indica la tarjeta"));
    }
    if b.kind != "credit_card" && b.principal_cents.is_none_or(|p| p <= 0) {
        return Err(ApiError::validation(
            "principal_cents",
            "indica el monto de la deuda",
        ));
    }
    Ok((
        name,
        Currency::parse(b.currency.as_deref().unwrap_or(base))?.to_string(),
    ))
}

#[utoipa::path(post, path = "/finance/debts", tag = "finance", request_body = DebtBody, responses((status = 201, body = DebtView)))]
async fn create_debt(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<DebtBody>,
) -> ApiResult<(StatusCode, Json<DebtView>)> {
    let c = ctx(&s, auth.user_id).await?;
    let (name, currency) = validate(&b, &c.base)?;
    if let Some(acc) = b.account_id {
        let a = repo::get_account(&s.pool, auth.user_id, acc)
            .await
            .map_err(|_| ApiError::validation("account_id", "tarjeta inexistente"))?;
        if a.kind != "credit_card" {
            return Err(ApiError::validation(
                "account_id",
                "debe ser una tarjeta de crédito",
            ));
        }
    }
    let d = repo::create_debt(
        &s.pool,
        auth.user_id,
        &repo::DebtInput {
            name: &name,
            kind: &b.kind,
            account_id: b.account_id,
            person_id: b.person_id,
            principal_cents: b.principal_cents,
            currency: &currency,
            opened_on: b.opened_on,
            goal_id: b.goal_id,
            notes_md: &b.notes_md,
            cancelled: false,
        },
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "debt",
        Some(d.id),
        "create",
        serde_json::json!({ "name": name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(view(&s, auth.user_id, d).await?)))
}

#[utoipa::path(get, path = "/finance/debts/{id}", tag = "finance", responses((status = 200, body = DebtView)))]
async fn get_debt(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<DebtView>> {
    let d = repo::get_debt(&s.pool, auth.user_id, id).await?;
    Ok(Json(view(&s, auth.user_id, d).await?))
}

#[utoipa::path(put, path = "/finance/debts/{id}", tag = "finance", request_body = DebtBody,
    responses((status = 200, body = DebtView), (status = 409, body = crate::error::ErrorBody)))]
async fn update_debt(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<DebtBody>,
) -> ApiResult<Json<DebtView>> {
    let c = ctx(&s, auth.user_id).await?;
    let (name, currency) = validate(&b, &c.base)?;
    let mut tx = s.pool.begin().await?;
    let cur = repo::get_debt(&mut *tx, auth.user_id, id).await?;
    if cur.kind != b.kind {
        return Err(ApiError::validation(
            "kind",
            "el tipo de deuda no se puede cambiar",
        ));
    }
    repo::update_debt(
        &mut tx,
        auth.user_id,
        id,
        require_version(b.version)?,
        &repo::DebtInput {
            name: &name,
            kind: &b.kind,
            account_id: b.account_id,
            person_id: b.person_id,
            principal_cents: b.principal_cents,
            currency: &currency,
            opened_on: b.opened_on,
            goal_id: b.goal_id,
            notes_md: &b.notes_md,
            cancelled: b.cancelled,
        },
    )
    .await?;
    // Cambiar el principal puede liquidar o reabrir la deuda.
    reevaluate_debt(&mut tx, auth.user_id, id).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "debt",
        Some(id),
        "update",
        serde_json::json!({ "name": name }),
    )
    .await;
    let d = repo::get_debt(&s.pool, auth.user_id, id).await?;
    Ok(Json(view(&s, auth.user_id, d).await?))
}

#[derive(Deserialize, IntoParams)]
pub struct SuggestionQuery {
    /// Fecha local (por defecto, hoy).
    pub date: Option<NaiveDate>,
    /// Gasto planeado en centavos. Sin monto la sugerencia es provisional.
    pub amount_cents: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct CardVerdictView {
    pub account_id: Uuid,
    pub name: String,
    pub eligible: bool,
    /// Motivos de exclusión.
    pub reasons: Vec<String>,
    pub days_to_cut: Option<i64>,
    pub next_cut_on: Option<NaiveDate>,
    pub available_cents: Option<i64>,
    pub usage_after_pct: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct CardSuggestionView {
    pub date: NaiveDate,
    pub amount_cents: Option<i64>,
    pub card_id: Option<Uuid>,
    pub card_name: Option<String>,
    /// Si ninguna tarjeta sirve: cuenta de débito por defecto.
    pub debit_account_id: Option<Uuid>,
    pub debit_account_name: Option<String>,
    pub provisional: bool,
    pub explanation: String,
    pub max_usage_pct: i16,
    pub cards: Vec<CardVerdictView>,
}

/// Tarjeta sugerida para gastar hoy según disponible, uso máximo del plan, corte, deudas y gasto planeado (D-022).
/// Es una recomendación personal; no ejecuta pagos ni consulta bancos.
#[utoipa::path(get, path = "/finance/card-suggestion", tag = "finance", params(SuggestionQuery),
    responses((status = 200, body = CardSuggestionView)))]
async fn card_suggestion(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<SuggestionQuery>,
) -> ApiResult<Json<CardSuggestionView>> {
    let c = ctx(&s, auth.user_id).await?;
    let date = q.date.unwrap_or(c.today);
    if q.amount_cents.is_some_and(|a| a < 0) {
        return Err(ApiError::validation("amount_cents", "no puede ser negativo"));
    }
    let plan = repo::get_plan(&s.pool, auth.user_id).await?;
    let debts = repo::open_card_debts(&s.pool, auth.user_id).await?;
    let accounts = repo::list_accounts(&s.pool, auth.user_id, false).await?;
    let mut cards = Vec::new();
    for a in accounts.iter().filter(|a| a.kind == "credit_card") {
        let est = repo::estimate_account(&s.pool, auth.user_id, a).await?;
        cards.push(CardCandidate {
            id: a.id,
            name: a.name.clone(),
            status: a.status.clone(),
            limit_cents: a.credit_limit_cents,
            cut_day: a.cut_day.map(|d| d as u32),
            available_cents: est.map(|(_, e)| e),
            has_open_debt: debts.iter().any(|(_, acc)| *acc == a.id),
        });
    }
    let sug = suggest_card(
        &cards,
        &PlanRules {
            max_usage_pct: plan.max_usage_pct as i64,
            excluded: plan.excluded_account_ids.clone(),
            forbid_cards_with_debt: plan.forbid_cards_with_debt,
        },
        date,
        q.amount_cents,
    );
    let debit = accounts
        .iter()
        .find(|a| a.is_default_debit && a.status == "active")
        .or_else(|| {
            accounts
                .iter()
                .find(|a| a.kind == "debit" && a.status == "active")
        });
    let explanation = match (&sug.card_id, debit) {
        (None, Some(d)) => format!("{} → {}", sug.explanation, d.name),
        _ => sug.explanation.clone(),
    };
    Ok(Json(CardSuggestionView {
        date,
        amount_cents: q.amount_cents,
        card_id: sug.card_id,
        card_name: sug.card_name,
        debit_account_id: if sug.card_id.is_none() {
            debit.map(|d| d.id)
        } else {
            None
        },
        debit_account_name: if sug.card_id.is_none() {
            debit.map(|d| d.name.clone())
        } else {
            None
        },
        provisional: sug.provisional,
        explanation,
        max_usage_pct: plan.max_usage_pct,
        cards: sug
            .verdicts
            .into_iter()
            .map(|v| CardVerdictView {
                account_id: v.account_id,
                name: v.name,
                eligible: v.eligible,
                reasons: v.reasons,
                days_to_cut: v.days_to_cut,
                next_cut_on: v.next_cut_on,
                available_cents: v.available_cents,
                usage_after_pct: v.usage_after_pct,
            })
            .collect(),
    }))
}
