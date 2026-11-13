//! Propietario: agente B-modules. Agenda: eventos manuales por fecha y lectura de los sincronizados (Google, agente E).
//! Un evento no prueba asistencia ni conversación: los invitados solo alimentan propuestas de personas.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{NaiveDate, NaiveTime};
use domain::text::required;
use domain::time::local_to_utc;
use persistence::repos::agenda::{self as repo, CalendarEvent, EventInput};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::tasks::user_today;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_range))
        .routes(routes!(day_agenda))
        .routes(routes!(create_event))
        .routes(routes!(update_event, delete_event))
}

#[derive(Deserialize, IntoParams)]
pub struct RangeQuery {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[utoipa::path(get, path = "/agenda", tag = "agenda", params(RangeQuery), responses((status = 200, body = Vec<CalendarEvent>)))]
async fn list_range(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<CalendarEvent>>> {
    if q.to < q.from || (q.to - q.from).num_days() > 62 {
        return Err(ApiError::validation("to", "rango de hasta 62 días"));
    }
    Ok(Json(repo::for_range(&s.pool, auth.user_id, q.from, q.to).await?))
}

/// Agenda de un día (manual + sincronizada), ordenada por hora.
#[utoipa::path(get, path = "/agenda/day/{date}", tag = "agenda", responses((status = 200, body = Vec<CalendarEvent>)))]
async fn day_agenda(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(date): Path<NaiveDate>,
) -> ApiResult<Json<Vec<CalendarEvent>>> {
    Ok(Json(repo::for_range(&s.pool, auth.user_id, date, date).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct EventBody {
    pub on_date: NaiveDate,
    pub title: String,
    /// Hora local `HH:MM` (zona del usuario).
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    #[serde(default)]
    pub all_day: bool,
    pub location: Option<String>,
}

fn parse_t(field: &str, v: Option<&str>) -> ApiResult<Option<NaiveTime>> {
    match v.map(str::trim).filter(|x| !x.is_empty()) {
        None => Ok(None),
        Some(x) => NaiveTime::parse_from_str(x, "%H:%M")
            .map(Some)
            .map_err(|_| ApiError::validation(field, "usa HH:MM")),
    }
}

async fn to_input<'a>(
    s: &AppState,
    user_id: Uuid,
    b: &'a EventBody,
    title: &'a str,
) -> ApiResult<EventInput<'a>> {
    let (tz, _) = user_today(s, user_id).await?;
    let start = parse_t("start_time", b.start_time.as_deref())?;
    let end = parse_t("end_time", b.end_time.as_deref())?;
    if !b.all_day && start.is_none() {
        return Err(ApiError::validation(
            "start_time",
            "indica la hora o marca todo el día",
        ));
    }
    if let (Some(a), Some(z)) = (start, end)
        && z < a
    {
        return Err(ApiError::validation("end_time", "termina antes de empezar"));
    }
    Ok(EventInput {
        on_date: b.on_date,
        starts_at: if b.all_day {
            None
        } else {
            start.map(|t| local_to_utc(b.on_date, t, tz))
        },
        ends_at: if b.all_day {
            None
        } else {
            end.map(|t| local_to_utc(b.on_date, t, tz))
        },
        all_day: b.all_day,
        title,
        location: b.location.as_deref().filter(|x| !x.trim().is_empty()),
    })
}

#[utoipa::path(post, path = "/agenda", tag = "agenda", request_body = EventBody, responses((status = 201, body = CalendarEvent)))]
async fn create_event(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<EventBody>,
) -> ApiResult<(StatusCode, Json<CalendarEvent>)> {
    let title = required("title", &b.title, 300)?;
    let input = to_input(&s, auth.user_id, &b, &title).await?;
    Ok((
        StatusCode::CREATED,
        Json(repo::create_manual(&s.pool, auth.user_id, &input).await?),
    ))
}

#[utoipa::path(put, path = "/agenda/{id}", tag = "agenda", request_body = EventBody, responses((status = 200, body = CalendarEvent), (status = 404, body = crate::error::ErrorBody)))]
async fn update_event(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<EventBody>,
) -> ApiResult<Json<CalendarEvent>> {
    let title = required("title", &b.title, 300)?;
    let input = to_input(&s, auth.user_id, &b, &title).await?;
    Ok(Json(
        repo::update_manual(&s.pool, auth.user_id, id, &input).await?,
    ))
}

#[utoipa::path(delete, path = "/agenda/{id}", tag = "agenda", responses((status = 204)))]
async fn delete_event(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    repo::delete_manual(&s.pool, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
