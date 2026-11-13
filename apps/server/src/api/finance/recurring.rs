//! T40 suscripciones e ingresos fijos: reglas, ocurrencias idempotentes y estados registrado/confirmado/proyectado.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{Datelike, NaiveDate};
use domain::money::Currency;
use domain::text::required;
use domain::time::DayOfMonth;
use persistence::repos::finance::recurring::{self as rrepo, RecurringRule};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use super::{ctx, parse_month_param, require_version};
use crate::api::audit;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_rules, create_rule))
        .routes(routes!(get_rule, update_rule))
        .routes(routes!(occurrences))
        .routes(routes!(materialize))
}

#[utoipa::path(get, path = "/finance/recurring", tag = "finance", responses((status = 200, body = Vec<RecurringRule>)))]
async fn list_rules(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<RecurringRule>>> {
    Ok(Json(rrepo::list(&s.pool, auth.user_id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct RuleBody {
    /// `subscription | fixed_income` (no cambia al actualizar).
    pub kind: String,
    pub name: String,
    pub amount_cents: i64,
    pub currency: Option<String>,
    /// `day | last | penultimate`; con `day` se indica `day` 1–31 (si el mes es corto, se usa el último día).
    pub day_kind: String,
    pub day: Option<i16>,
    pub category_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    /// `active | cancelled | ended`. Cancelar no altera ocurrencias históricas.
    #[serde(default = "active")]
    pub status: String,
    pub starts_on: Option<NaiveDate>,
    pub ends_on: Option<NaiveDate>,
    #[serde(default)]
    pub notes_md: String,
    pub version: Option<i32>,
}

fn active() -> String {
    "active".into()
}

async fn validate(s: &AppState, user_id: Uuid, b: &RuleBody, base: &str) -> ApiResult<(String, String)> {
    if !["subscription", "fixed_income"].contains(&b.kind.as_str()) {
        return Err(ApiError::validation("kind", "subscription o fixed_income"));
    }
    let name = required("name", &b.name, 100)?;
    if b.amount_cents <= 0 {
        return Err(ApiError::validation(
            "amount_cents",
            "el monto debe ser mayor que cero",
        ));
    }
    DayOfMonth::from_parts(&b.day_kind, b.day)?;
    if !["active", "cancelled", "ended"].contains(&b.status.as_str()) {
        return Err(ApiError::validation("status", "estado inválido"));
    }
    if let (Some(a), Some(e)) = (b.starts_on, b.ends_on)
        && e < a
    {
        return Err(ApiError::validation("ends_on", "termina antes de empezar"));
    }
    if let Some(cat) = b.category_id {
        let kind: Option<String> =
            sqlx::query_scalar("SELECT kind FROM fin_categories WHERE user_id = $1 AND id = $2")
                .bind(user_id)
                .bind(cat)
                .fetch_optional(&s.pool)
                .await?;
        let want = if b.kind == "subscription" {
            "expense"
        } else {
            "income"
        };
        if kind.as_deref() != Some(want) {
            return Err(ApiError::validation(
                "category_id",
                "la categoría no corresponde al tipo de regla",
            ));
        }
    }
    Ok((
        name,
        Currency::parse(b.currency.as_deref().unwrap_or(base))?.to_string(),
    ))
}

fn input<'a>(
    b: &'a RuleBody,
    name: &'a str,
    currency: &'a str,
    starts_on: NaiveDate,
) -> rrepo::RuleInput<'a> {
    rrepo::RuleInput {
        kind: &b.kind,
        name,
        amount_cents: b.amount_cents,
        currency,
        day_kind: &b.day_kind,
        day: b.day,
        category_id: b.category_id,
        account_id: b.account_id,
        area_id: b.area_id,
        project_id: b.project_id,
        status: &b.status,
        starts_on,
        ends_on: b.ends_on,
        notes_md: &b.notes_md,
    }
}

#[utoipa::path(post, path = "/finance/recurring", tag = "finance", request_body = RuleBody, responses((status = 201, body = RecurringRule)))]
async fn create_rule(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<RuleBody>,
) -> ApiResult<(StatusCode, Json<RecurringRule>)> {
    let c = ctx(&s, auth.user_id).await?;
    let (name, currency) = validate(&s, auth.user_id, &b, &c.base).await?;
    let r = rrepo::create(
        &s.pool,
        auth.user_id,
        &input(&b, &name, &currency, b.starts_on.unwrap_or(c.today)),
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "recurring_rule",
        Some(r.id),
        "create",
        serde_json::json!({ "name": name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(r)))
}

#[utoipa::path(get, path = "/finance/recurring/{id}", tag = "finance", responses((status = 200, body = RecurringRule)))]
async fn get_rule(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RecurringRule>> {
    Ok(Json(rrepo::get(&s.pool, auth.user_id, id).await?))
}

/// Cambios futuros: las ocurrencias ya creadas conservan su monto y fecha.
#[utoipa::path(put, path = "/finance/recurring/{id}", tag = "finance", request_body = RuleBody,
    responses((status = 200, body = RecurringRule), (status = 409, body = crate::error::ErrorBody)))]
async fn update_rule(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<RuleBody>,
) -> ApiResult<Json<RecurringRule>> {
    let c = ctx(&s, auth.user_id).await?;
    let (name, currency) = validate(&s, auth.user_id, &b, &c.base).await?;
    let cur = rrepo::get(&s.pool, auth.user_id, id).await?;
    if cur.kind != b.kind {
        return Err(ApiError::validation(
            "kind",
            "el tipo de regla no se puede cambiar",
        ));
    }
    let r = rrepo::update(
        &s.pool,
        auth.user_id,
        id,
        require_version(b.version)?,
        &input(&b, &name, &currency, b.starts_on.unwrap_or(cur.starts_on)),
    )
    .await?;
    audit(
        &s,
        auth.user_id,
        "recurring_rule",
        Some(id),
        "update",
        serde_json::json!({ "status": r.status }),
    )
    .await;
    Ok(Json(r))
}

#[derive(Deserialize, IntoParams)]
pub struct MonthQuery {
    /// `YYYY-MM` (por defecto, mes actual).
    pub month: Option<String>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct OccurrenceView {
    pub rule_id: Uuid,
    pub rule_name: String,
    /// `subscription | fixed_income`
    pub kind: String,
    pub date: NaiveDate,
    pub amount_cents: i64,
    /// `pending` (registrada, pendiente de conciliar) | `confirmed` | `void` | `projected` (aún no registrada).
    pub state: String,
    pub transaction_id: Option<Uuid>,
    pub transaction_version: Option<i32>,
}

/// Ocurrencias de un mes por regla (incluye reglas ya canceladas con ocurrencias registradas).
pub(crate) async fn month_occurrences(
    s: &AppState,
    user_id: Uuid,
    month: &str,
) -> ApiResult<Vec<OccurrenceView>> {
    let p = domain::time::parse_month(month)?;
    let rules = rrepo::list(&s.pool, user_id).await?;
    let occ = rrepo::occurrences(&s.pool, user_id, month).await?;
    let mut out = Vec::new();
    for r in &rules {
        if let Some(o) = occ.iter().find(|o| o.rule_id == r.id) {
            out.push(OccurrenceView {
                rule_id: r.id,
                rule_name: r.name.clone(),
                kind: r.kind.clone(),
                date: o.occurred_on,
                amount_cents: o.amount_cents,
                state: if o.status == "pending_reconciliation" {
                    "pending".into()
                } else {
                    o.status.clone()
                },
                transaction_id: Some(o.transaction_id),
                transaction_version: Some(o.version),
            });
        } else if r.status == "active"
            && let Some(d) = r.occurrence_in(p.start.year(), p.start.month())
        {
            out.push(OccurrenceView {
                rule_id: r.id,
                rule_name: r.name.clone(),
                kind: r.kind.clone(),
                date: d,
                amount_cents: r.amount_cents,
                state: "projected".into(),
                transaction_id: None,
                transaction_version: None,
            });
        }
    }
    out.sort_by_key(|o| o.date);
    Ok(out)
}

#[utoipa::path(get, path = "/finance/recurring/occurrences", tag = "finance", params(MonthQuery),
    responses((status = 200, body = Vec<OccurrenceView>)))]
async fn occurrences(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<MonthQuery>,
) -> ApiResult<Json<Vec<OccurrenceView>>> {
    let c = ctx(&s, auth.user_id).await?;
    let (key, _) = parse_month_param(q.month.as_deref(), c.today)?;
    Ok(Json(month_occurrences(&s, auth.user_id, &key).await?))
}

#[derive(Serialize, ToSchema)]
pub struct MaterializeResult {
    pub created: u64,
}

/// Registra ahora las ocurrencias vencidas del usuario (el scheduler lo hace solo; esto lo adelanta). Idempotente.
#[utoipa::path(post, path = "/finance/recurring/materialize", tag = "finance", responses((status = 200, body = MaterializeResult)))]
async fn materialize(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<MaterializeResult>> {
    let c = ctx(&s, auth.user_id).await?;
    let created = rrepo::materialize_due_for_user(&s.pool, auth.user_id, c.today).await?;
    Ok(Json(MaterializeResult { created }))
}
