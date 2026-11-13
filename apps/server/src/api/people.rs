//! Propietario: agente B-modules. T27 Personas, alias, cumpleaños, interacciones y sugerencias.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use domain::tasks::{days_since, next_birthday};
use domain::text::required;
use persistence::repos::people::{
    self as repo, Interaction, Person, PersonInput, PersonNote, PersonSuggestion,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::audit;
use crate::api::tasks::user_today;
use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_people, create_person))
        .routes(routes!(people_by_area))
        .routes(routes!(resolve_people))
        .routes(routes!(upcoming_birthdays))
        .routes(routes!(get_person, update_person))
        .routes(routes!(add_note))
        .routes(routes!(delete_note))
        .routes(routes!(add_interaction))
        .routes(routes!(list_interactions))
        .routes(routes!(delete_interaction))
        .routes(routes!(list_suggestions, create_suggestion))
        .routes(routes!(accept_suggestion))
        .routes(routes!(dismiss_suggestion))
}

const RELATIONS: [&str; 8] = [
    "jefe",
    "subordinado",
    "compañero",
    "cliente",
    "familia",
    "pareja",
    "amigo",
    "otro",
];

#[derive(Serialize, ToSchema)]
pub struct PersonView {
    #[serde(flatten)]
    pub person: Person,
    /// Días sin hablar (fórmula de Personas.base).
    pub days_since_contact: Option<i64>,
    pub next_birthday: Option<NaiveDate>,
    pub days_to_birthday: Option<i64>,
    /// Años que cumple, si se conoce el año.
    pub turning: Option<i32>,
}

fn view(p: Person, today: NaiveDate) -> PersonView {
    let nb = match (p.birthday_month, p.birthday_day) {
        (Some(m), Some(d)) => next_birthday(m as u32, d as u32, p.birthday_year.map(i32::from), today),
        _ => None,
    };
    PersonView {
        days_since_contact: days_since(p.last_contact_on, today),
        next_birthday: nb.map(|x| x.0),
        days_to_birthday: nb.map(|x| (x.0 - today).num_days()),
        turning: nb.and_then(|x| x.1),
        person: p,
    }
}

#[derive(Deserialize, IntoParams)]
pub struct PeopleQuery {
    /// Nombre o alias (sin acentos).
    pub q: Option<String>,
    pub area_id: Option<Uuid>,
    pub archived: Option<bool>,
}

#[utoipa::path(get, path = "/people", tag = "people", params(PeopleQuery), responses((status = 200, body = Vec<PersonView>)))]
async fn list_people(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PeopleQuery>,
) -> ApiResult<Json<Vec<PersonView>>> {
    let (_, today) = user_today(&s, auth.user_id).await?;
    let qs = q.q.as_deref().map(str::trim).filter(|x| !x.is_empty());
    let rows = repo::list(&s.pool, auth.user_id, qs, q.area_id, q.archived.unwrap_or(false)).await?;
    Ok(Json(rows.into_iter().map(|p| view(p, today)).collect()))
}

#[derive(Serialize, ToSchema)]
pub struct PeopleGroup {
    pub area_id: Option<Uuid>,
    pub area_name: String,
    pub people: Vec<PersonView>,
}

/// Vista Personas.base: agrupado por área, con días sin hablar y cumpleaños.
#[utoipa::path(get, path = "/people/by-area", tag = "people", responses((status = 200, body = Vec<PeopleGroup>)))]
async fn people_by_area(State(s): State<AppState>, auth: AuthUser) -> ApiResult<Json<Vec<PeopleGroup>>> {
    let (_, today) = user_today(&s, auth.user_id).await?;
    let areas = persistence::repos::areas::list_areas(&s.pool, auth.user_id, true).await?;
    let mut groups: Vec<PeopleGroup> = areas
        .iter()
        .map(|a| PeopleGroup {
            area_id: Some(a.id),
            area_name: a.name.clone(),
            people: vec![],
        })
        .collect();
    let mut none = PeopleGroup {
        area_id: None,
        area_name: "Sin área".into(),
        people: vec![],
    };
    for p in repo::list(&s.pool, auth.user_id, None, None, false).await? {
        match groups
            .iter_mut()
            .find(|g| g.area_id == p.area_id && p.area_id.is_some())
        {
            Some(g) => g.people.push(view(p, today)),
            None => none.people.push(view(p, today)),
        }
    }
    groups.push(none);
    groups.retain(|g| !g.people.is_empty());
    Ok(Json(groups))
}

#[derive(Deserialize, IntoParams)]
pub struct ResolveQuery {
    /// Nombre, nombre de pila o alias exacto (ej. "Juancho").
    pub name: String,
}

/// Busca a quién se refiere un nombre o apodo, sin crear nada.
#[utoipa::path(get, path = "/people/resolve", tag = "people", params(ResolveQuery), responses((status = 200, body = Vec<Person>)))]
async fn resolve_people(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ResolveQuery>,
) -> ApiResult<Json<Vec<Person>>> {
    Ok(Json(repo::resolve(&s.pool, auth.user_id, &q.name).await?))
}

#[derive(Deserialize, IntoParams)]
pub struct BirthdayQuery {
    /// Ventana en días (por defecto 30).
    pub days: Option<i64>,
}

#[utoipa::path(get, path = "/people/birthdays", tag = "people", params(BirthdayQuery), responses((status = 200, body = Vec<PersonView>)))]
async fn upcoming_birthdays(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<BirthdayQuery>,
) -> ApiResult<Json<Vec<PersonView>>> {
    let (_, today) = user_today(&s, auth.user_id).await?;
    let days = q.days.unwrap_or(30).clamp(0, 366);
    let mut out: Vec<PersonView> = repo::list(&s.pool, auth.user_id, None, None, false)
        .await?
        .into_iter()
        .map(|p| view(p, today))
        .filter(|v| v.days_to_birthday.is_some_and(|d| d <= days))
        .collect();
    out.sort_by_key(|v| v.days_to_birthday);
    Ok(Json(out))
}

#[derive(Deserialize, ToSchema)]
pub struct PersonBody {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub area_id: Option<Uuid>,
    pub relationship: Option<String>,
    pub org: Option<String>,
    pub role: Option<String>,
    pub contact: Option<String>,
    pub email: Option<String>,
    #[serde(default)]
    pub context_md: String,
    /// `YYYY-MM-DD` o `MM-DD` (sin año).
    pub birthday: Option<String>,
    #[serde(default)]
    pub archived: bool,
    pub version: Option<i32>,
}

pub fn parse_birthday(s: Option<&str>) -> ApiResult<(Option<i16>, Option<i16>, Option<i16>)> {
    let Some(s) = s.map(str::trim).filter(|x| !x.is_empty()) else {
        return Ok((None, None, None));
    };
    let err = || ApiError::validation("birthday", "usa YYYY-MM-DD o MM-DD");
    let parts: Vec<&str> = s.split('-').collect();
    let (y, m, d) = match parts.as_slice() {
        [y, m, d] => (Some(y.parse::<i16>().map_err(|_| err())?), m, d),
        [m, d] => (None, m, d),
        _ => return Err(err()),
    };
    let m: i16 = m.parse().map_err(|_| err())?;
    let d: i16 = d.parse().map_err(|_| err())?;
    // Valida contra un año bisiesto para aceptar 02-29 sin año.
    NaiveDate::from_ymd_opt(y.map(i32::from).unwrap_or(2000), m as u32, d as u32).ok_or_else(err)?;
    Ok((Some(m), Some(d), y))
}

struct ValidPerson {
    name: String,
    aliases: Vec<String>,
    birthday: (Option<i16>, Option<i16>, Option<i16>),
}

fn validate(b: &PersonBody) -> ApiResult<ValidPerson> {
    let name = required("name", &b.name, 120)?;
    if let Some(r) = &b.relationship
        && !RELATIONS.contains(&r.as_str())
    {
        return Err(ApiError::validation("relationship", "relación no válida"));
    }
    let mut aliases: Vec<String> = b
        .aliases
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect();
    aliases.dedup();
    if aliases.len() > 20 {
        return Err(ApiError::validation("aliases", "máximo 20 alias"));
    }
    Ok(ValidPerson {
        name,
        aliases,
        birthday: parse_birthday(b.birthday.as_deref())?,
    })
}

fn input<'a>(b: &'a PersonBody, v: &'a ValidPerson) -> PersonInput<'a> {
    let opt = |x: &'a Option<String>| x.as_deref().map(str::trim).filter(|s| !s.is_empty());
    PersonInput {
        name: &v.name,
        aliases: &v.aliases,
        area_id: b.area_id,
        relationship: b.relationship.as_deref(),
        org: opt(&b.org),
        role: opt(&b.role),
        contact: opt(&b.contact),
        email: opt(&b.email),
        context_md: &b.context_md,
        birthday: v.birthday,
        archived: b.archived,
    }
}

#[utoipa::path(post, path = "/people", tag = "people", request_body = PersonBody, responses((status = 201, body = Person)))]
async fn create_person(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<PersonBody>,
) -> ApiResult<(StatusCode, Json<Person>)> {
    let v = validate(&b)?;
    let p = repo::create(&s.pool, auth.user_id, &input(&b, &v)).await?;
    audit(
        &s,
        auth.user_id,
        "person",
        Some(p.id),
        "create",
        serde_json::json!({ "name": v.name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(p)))
}

#[derive(Serialize, ToSchema)]
pub struct PersonDetail {
    pub person: PersonView,
    pub notes: Vec<PersonNote>,
    pub interactions: Vec<Interaction>,
}

#[utoipa::path(get, path = "/people/{id}", tag = "people", responses((status = 200, body = PersonDetail)))]
async fn get_person(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PersonDetail>> {
    let (_, today) = user_today(&s, auth.user_id).await?;
    let p = repo::get(&s.pool, auth.user_id, id).await?;
    Ok(Json(PersonDetail {
        person: view(p, today),
        notes: repo::notes(&s.pool, auth.user_id, id).await?,
        interactions: repo::interactions(&s.pool, auth.user_id, Some(id), None, 100).await?,
    }))
}

/// Editar no borra el cumpleaños existente salvo que se envíe vacío explícitamente.
#[utoipa::path(put, path = "/people/{id}", tag = "people", request_body = PersonBody, responses((status = 200, body = Person), (status = 409, body = crate::error::ErrorBody)))]
async fn update_person(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<PersonBody>,
) -> ApiResult<Json<Person>> {
    let mut v = validate(&b)?;
    let version = b
        .version
        .ok_or_else(|| ApiError::validation("version", "es obligatoria al actualizar"))?;
    if b.birthday.is_none() {
        let cur = repo::get(&s.pool, auth.user_id, id).await?;
        v.birthday = (cur.birthday_month, cur.birthday_day, cur.birthday_year);
    }
    let p = repo::update(&s.pool, auth.user_id, id, version, &input(&b, &v)).await?;
    audit(
        &s,
        auth.user_id,
        "person",
        Some(id),
        "update",
        serde_json::json!({ "name": v.name }),
    )
    .await;
    Ok(Json(p))
}

#[derive(Deserialize, ToSchema)]
pub struct NoteBody {
    /// Por defecto hoy.
    pub on_date: Option<NaiveDate>,
    pub text: String,
}

#[utoipa::path(post, path = "/people/{id}/notes", tag = "people", request_body = NoteBody, responses((status = 201, body = PersonNote)))]
async fn add_note(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<NoteBody>,
) -> ApiResult<(StatusCode, Json<PersonNote>)> {
    let text = required("text", &b.text, 5000)?;
    let date = match b.on_date {
        Some(d) => d,
        None => user_today(&s, auth.user_id).await?.1,
    };
    repo::get(&s.pool, auth.user_id, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(repo::add_note(&s.pool, auth.user_id, id, date, &text).await?),
    ))
}

#[utoipa::path(delete, path = "/people/{id}/notes/{note_id}", tag = "people", responses((status = 204)))]
async fn delete_note(
    State(s): State<AppState>,
    auth: AuthUser,
    Path((id, note_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    repo::delete_note(&s.pool, auth.user_id, id, note_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct InteractionBody {
    pub on_date: Option<NaiveDate>,
    #[serde(default)]
    pub topic: String,
    /// `manual` (por defecto), `calendar`, `gmail`, `bot`, `import`.
    pub source: Option<String>,
    /// Referencia externa (id de evento/correo) para no duplicar.
    pub source_ref: Option<String>,
}

/// Registra con quién habló el usuario ese día; actualiza "último contacto".
#[utoipa::path(post, path = "/people/{id}/interactions", tag = "people", request_body = InteractionBody, responses((status = 201, body = Interaction)))]
async fn add_interaction(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<InteractionBody>,
) -> ApiResult<(StatusCode, Json<Interaction>)> {
    let source = b.source.as_deref().unwrap_or("manual");
    if !["manual", "calendar", "gmail", "bot", "import"].contains(&source) {
        return Err(ApiError::validation("source", "fuente no válida"));
    }
    let date = match b.on_date {
        Some(d) => d,
        None => user_today(&s, auth.user_id).await?.1,
    };
    let mut tx = s.pool.begin().await?;
    let iid = repo::add_interaction(
        &mut tx,
        auth.user_id,
        id,
        date,
        b.topic.trim(),
        source,
        b.source_ref.as_deref(),
    )
    .await?;
    tx.commit().await?;
    let row = repo::interactions(&s.pool, auth.user_id, Some(id), Some(date), 500)
        .await?
        .into_iter()
        .find(|i| i.id == iid)
        .ok_or_else(ApiError::internal)?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Deserialize, IntoParams)]
pub struct InteractionQuery {
    /// Fecha (sección "Personas" de la daily).
    pub date: Option<NaiveDate>,
    pub person_id: Option<Uuid>,
}

#[utoipa::path(get, path = "/interactions", tag = "people", params(InteractionQuery), responses((status = 200, body = Vec<Interaction>)))]
async fn list_interactions(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<InteractionQuery>,
) -> ApiResult<Json<Vec<Interaction>>> {
    Ok(Json(
        repo::interactions(&s.pool, auth.user_id, q.person_id, q.date, 500).await?,
    ))
}

#[utoipa::path(delete, path = "/interactions/{id}", tag = "people", responses((status = 204)))]
async fn delete_interaction(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    repo::delete_interaction(&s.pool, auth.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, IntoParams)]
pub struct SuggestionQuery {
    /// `pending` (por defecto), `accepted`, `dismissed`.
    pub status: Option<String>,
}

#[utoipa::path(get, path = "/person-suggestions", tag = "people", params(SuggestionQuery), responses((status = 200, body = Vec<PersonSuggestion>)))]
async fn list_suggestions(
    State(s): State<AppState>,
    auth: AuthUser,
    Query(q): Query<SuggestionQuery>,
) -> ApiResult<Json<Vec<PersonSuggestion>>> {
    Ok(Json(
        repo::suggestions(
            &s.pool,
            auth.user_id,
            Some(q.status.as_deref().unwrap_or("pending")),
        )
        .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct SuggestionBody {
    pub name: String,
    pub email: Option<String>,
    /// `calendar | gmail | dictation | import`
    pub source: String,
    pub source_ref: String,
    pub on_date: Option<NaiveDate>,
    #[serde(default)]
    pub context: String,
}

/// Sugiere una persona detectada (no la crea).
#[utoipa::path(post, path = "/person-suggestions", tag = "people", request_body = SuggestionBody, responses((status = 201, body = PersonSuggestion)))]
async fn create_suggestion(
    State(s): State<AppState>,
    auth: AuthUser,
    Json(b): Json<SuggestionBody>,
) -> ApiResult<(StatusCode, Json<PersonSuggestion>)> {
    let name = required("name", &b.name, 120)?;
    if !["calendar", "gmail", "dictation", "import"].contains(&b.source.as_str()) {
        return Err(ApiError::validation("source", "fuente no válida"));
    }
    let r = repo::suggest(
        &s.pool,
        auth.user_id,
        &name,
        b.email.as_deref(),
        &b.source,
        required("source_ref", &b.source_ref, 500)?.as_str(),
        b.on_date,
        &b.context,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(r)))
}

#[derive(Deserialize, ToSchema)]
pub struct AcceptSuggestion {
    /// Vincular a una persona existente…
    pub person_id: Option<Uuid>,
    /// …o crear una nueva con estos datos (decisión explícita del usuario).
    pub person: Option<PersonBody>,
    /// Registrar además la interacción del día sugerido.
    #[serde(default)]
    pub record_interaction: bool,
}

#[derive(Serialize, ToSchema)]
pub struct AcceptResult {
    pub person: Person,
    pub created: bool,
}

#[utoipa::path(post, path = "/person-suggestions/{id}/accept", tag = "people", request_body = AcceptSuggestion, responses((status = 200, body = AcceptResult)))]
async fn accept_suggestion(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<AcceptSuggestion>,
) -> ApiResult<Json<AcceptResult>> {
    let sug = repo::get_suggestion(&s.pool, auth.user_id, id).await?;
    let mut tx = s.pool.begin().await?;
    let (person, created) = match (b.person_id, &b.person) {
        (Some(pid), None) => (repo::get(&mut *tx, auth.user_id, pid).await?, false),
        (None, Some(pb)) => {
            let v = validate(pb)?;
            (repo::create(&mut *tx, auth.user_id, &input(pb, &v)).await?, true)
        }
        _ => {
            return Err(ApiError::validation(
                "person",
                "indica person_id o los datos de la nueva persona",
            ));
        }
    };
    repo::resolve_suggestion(&mut tx, auth.user_id, id, "accepted", Some(person.id)).await?;
    if b.record_interaction
        && let Some(d) = sug.on_date
    {
        let src = if sug.source == "dictation" {
            "manual"
        } else {
            sug.source.as_str()
        };
        repo::add_interaction(
            &mut tx,
            auth.user_id,
            person.id,
            d,
            &sug.context,
            src,
            Some(&sug.source_ref),
        )
        .await?;
    }
    tx.commit().await?;
    let person = repo::get(&s.pool, auth.user_id, person.id).await?;
    Ok(Json(AcceptResult { person, created }))
}

#[utoipa::path(post, path = "/person-suggestions/{id}/dismiss", tag = "people", responses((status = 204)))]
async fn dismiss_suggestion(
    State(s): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let mut tx = s.pool.begin().await?;
    repo::get_suggestion(&mut *tx, auth.user_id, id).await?;
    repo::resolve_suggestion(&mut tx, auth.user_id, id, "dismissed", None).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
