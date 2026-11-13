//! T39 presupuestos mensuales (snapshot por mes, rubros por proyecto) y T42 plan de crédito.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use chrono::NaiveDate;
use persistence::repos::finance::budgets::{self, BudgetItem, BudgetLine};
use persistence::repos::finance::{self as repo, CreditPlan};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::ctx;
use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_budget, put_budget))
        .routes(routes!(get_plan, put_plan))
}

#[derive(Serialize, ToSchema, Clone)]
pub struct BudgetLineView {
    pub category_id: Uuid,
    pub category_name: String,
    pub role: String,
    pub amount_cents: i64,
    /// La línea es la suma de rubros de proyecto.
    pub from_items: bool,
}

#[derive(Serialize, ToSchema)]
pub struct BudgetView {
    pub month: String,
    /// `false`: el mes aún no tiene snapshot; se muestra la propuesta copiada del mes anterior.
    pub persisted: bool,
    pub copied_from: Option<String>,
    /// Versión del snapshot (0 si no existe) para `PUT`.
    pub version: i32,
    pub lines: Vec<BudgetLineView>,
    pub items: Vec<BudgetItem>,
    /// Tope de gasto (sin categorías de pago de deuda), como en el vault.
    pub spending_cap_cents: i64,
    /// Total de todas las líneas (incluye pagos de deuda presupuestados).
    pub total_cents: i64,
}

pub(crate) async fn budget_view(s: &AppState, user_id: Uuid, month: &str) -> ApiResult<BudgetView> {
    domain::time::parse_month(month)?;
    let stored = budgets::get_month(&s.pool, user_id, month).await?;
    let (persisted, source) = match &stored {
        Some(m) => (true, Some(m.clone())),
        None => (false, budgets::previous_month(&s.pool, user_id, month).await?),
    };
    let (lines, items) = match &source {
        Some(m) => (
            budgets::lines(&s.pool, user_id, m.id).await?,
            budgets::items(&s.pool, user_id, m.id).await?,
        ),
        None => (Vec::new(), Vec::new()),
    };
    let cats: HashMap<Uuid, repo::Category> = repo::list_categories(&s.pool, user_id, true)
        .await?
        .into_iter()
        .map(|c| (c.id, c))
        .collect();
    let with_items: Vec<Uuid> = items.iter().map(|i| i.category_id).collect();
    let mut views: Vec<BudgetLineView> = lines
        .iter()
        .filter_map(|l| {
            cats.get(&l.category_id).map(|c| BudgetLineView {
                category_id: l.category_id,
                category_name: c.name.clone(),
                role: c.role.clone(),
                amount_cents: l.amount_cents,
                from_items: with_items.contains(&l.category_id),
            })
        })
        .collect();
    views.sort_by(|a, b| b.amount_cents.cmp(&a.amount_cents));
    let spending_cap = views
        .iter()
        .filter(|l| l.role != "debt_payment")
        .map(|l| l.amount_cents)
        .sum();
    let total = views.iter().map(|l| l.amount_cents).sum();
    Ok(BudgetView {
        month: month.to_string(),
        persisted,
        copied_from: match &stored {
            Some(m) => m.copied_from.clone(),
            None => source.as_ref().map(|m| m.month.clone()),
        },
        version: stored.as_ref().map(|m| m.version).unwrap_or(0),
        lines: views,
        items,
        spending_cap_cents: spending_cap,
        total_cents: total,
    })
}

/// Presupuesto del mes (snapshot propio del mes; si no existe, propuesta copiada del mes anterior).
#[utoipa::path(get, path = "/finance/budgets/{month}", tag = "finance", params(("month" = String, Path, description = "YYYY-MM")),
    responses((status = 200, body = BudgetView)))]
async fn get_budget(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(month): Path<String>,
) -> ApiResult<Json<BudgetView>> {
    Ok(Json(budget_view(&s, auth.user_id, &month).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct BudgetBody {
    /// Versión actual del snapshot (0 si aún no existe).
    pub version: i32,
    pub lines: Vec<BudgetLine>,
    #[serde(default)]
    pub items: Vec<BudgetItem>,
}

/// Guarda el presupuesto del mes. Solo modifica este mes; los meses anteriores conservan su snapshot.
/// Las categorías con rubros de proyecto toman como línea la suma de sus rubros.
#[utoipa::path(put, path = "/finance/budgets/{month}", tag = "finance", params(("month" = String, Path, description = "YYYY-MM")),
    request_body = BudgetBody, responses((status = 200, body = BudgetView), (status = 409, body = crate::error::ErrorBody)))]
async fn put_budget(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(month): Path<String>,
    Json(b): Json<BudgetBody>,
) -> ApiResult<Json<BudgetView>> {
    domain::time::parse_month(&month)?;
    for (i, l) in b.lines.iter().enumerate() {
        if l.amount_cents < 0 {
            return Err(ApiError::validation(
                &format!("lines[{i}].amount_cents"),
                "no puede ser negativo",
            ));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for (i, it) in b.items.iter().enumerate() {
        if it.amount_cents < 0 || it.name.trim().is_empty() {
            return Err(ApiError::validation(
                &format!("items[{i}]"),
                "rubro con nombre y monto no negativo",
            ));
        }
        if !seen.insert((it.project_id, it.name.trim().to_lowercase())) {
            return Err(ApiError::validation(
                &format!("items[{i}].name"),
                "rubro repetido en el proyecto",
            ));
        }
    }
    let cats: HashMap<Uuid, String> = repo::list_categories(&s.pool, auth.user_id, true)
        .await?
        .into_iter()
        .map(|c| (c.id, c.kind))
        .collect();
    for (i, id) in b
        .lines
        .iter()
        .map(|l| l.category_id)
        .chain(b.items.iter().map(|i| i.category_id))
        .enumerate()
    {
        if cats.get(&id).map(String::as_str) != Some("expense") {
            return Err(ApiError::validation(
                &format!("category[{i}]"),
                "categoría de gasto inexistente",
            ));
        }
    }
    let mut tx = s.pool.begin().await?;
    let existed = budgets::get_month(&mut *tx, auth.user_id, &month).await?;
    let m = budgets::ensure_month(&mut tx, auth.user_id, &month).await?;
    let version = if existed.is_none() { m.version } else { b.version };
    budgets::replace(&mut tx, auth.user_id, &m, version, &b.lines, &b.items).await?;
    tx.commit().await?;
    audit(
        &s,
        auth.user_id,
        "budget_month",
        Some(m.id),
        "update",
        serde_json::json!({ "month": month }),
    )
    .await;
    Ok(Json(budget_view(&s, auth.user_id, &month).await?))
}

/// Plan de crédito (meta, pasos, reglas, tarjetas excluidas, % máximo de uso). El 30% es una regla personal inicial,
/// no una garantía financiera.
#[utoipa::path(get, path = "/finance/credit-plan", tag = "finance", responses((status = 200, body = CreditPlan)))]
async fn get_plan(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<CreditPlan>> {
    Ok(Json(repo::get_plan(&s.pool, auth.user_id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct PlanBody {
    pub version: i32,
    #[serde(default)]
    pub goal_text: String,
    pub target_date: Option<NaiveDate>,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    pub steps: serde_json::Value,
    #[serde(default)]
    #[schema(value_type = Vec<Object>)]
    pub rules: serde_json::Value,
    pub max_usage_pct: i16,
    #[serde(default)]
    pub excluded_account_ids: Vec<Uuid>,
    #[serde(default = "yes")]
    pub forbid_cards_with_debt: bool,
}

fn yes() -> bool {
    true
}

fn list_of_text(v: &serde_json::Value, field: &str, with_done: bool) -> ApiResult<serde_json::Value> {
    let arr = match v {
        serde_json::Value::Null => return Ok(serde_json::json!([])),
        serde_json::Value::Array(a) => a,
        _ => return Err(ApiError::validation(field, "debe ser una lista")),
    };
    let mut out = Vec::new();
    for it in arr {
        let text = it.get("text").and_then(|t| t.as_str()).unwrap_or_default().trim();
        if text.is_empty() {
            continue;
        }
        let text: String = text.chars().take(300).collect();
        out.push(if with_done {
            serde_json::json!({ "text": text, "done": it.get("done").and_then(|d| d.as_bool()).unwrap_or(false) })
        } else {
            serde_json::json!({ "text": text })
        });
    }
    Ok(serde_json::Value::Array(out))
}

#[utoipa::path(put, path = "/finance/credit-plan", tag = "finance", request_body = PlanBody,
    responses((status = 200, body = CreditPlan), (status = 409, body = crate::error::ErrorBody)))]
async fn put_plan(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<PlanBody>,
) -> ApiResult<Json<CreditPlan>> {
    if !(1..=100).contains(&b.max_usage_pct) {
        return Err(ApiError::validation("max_usage_pct", "entre 1 y 100"));
    }
    for id in &b.excluded_account_ids {
        repo::get_account(&s.pool, auth.user_id, *id)
            .await
            .map_err(|_| ApiError::validation("excluded_account_ids", "tarjeta inexistente"))?;
    }
    let _ = ctx(&s, auth.user_id).await?;
    let p = CreditPlan {
        goal_text: b.goal_text.trim().chars().take(500).collect(),
        target_date: b.target_date,
        steps: list_of_text(&b.steps, "steps", true)?,
        rules: list_of_text(&b.rules, "rules", false)?,
        max_usage_pct: b.max_usage_pct,
        excluded_account_ids: b.excluded_account_ids,
        forbid_cards_with_debt: b.forbid_cards_with_debt,
        version: b.version,
    };
    let saved = repo::save_plan(&s.pool, auth.user_id, &p).await?;
    audit(
        &s,
        auth.user_id,
        "credit_plan",
        None,
        "update",
        serde_json::json!({ "max_usage_pct": saved.max_usage_pct }),
    )
    .await;
    Ok(Json(saved))
}
