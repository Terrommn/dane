//! Propietario: agente B-modules. Personas, notas cronológicas, interacciones y sugerencias.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Person {
    pub id: Uuid,
    pub name: String,
    pub aliases: Vec<String>,
    pub area_id: Option<Uuid>,
    /// `jefe | subordinado | compañero | cliente | familia | pareja | amigo | otro`
    pub relationship: Option<String>,
    pub org: Option<String>,
    pub role: Option<String>,
    pub contact: Option<String>,
    pub email: Option<String>,
    pub context_md: String,
    pub birthday_month: Option<i16>,
    pub birthday_day: Option<i16>,
    pub birthday_year: Option<i16>,
    pub last_contact_on: Option<NaiveDate>,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str =
    "id, name, aliases, area_id, relationship, org, role, contact, email, context_md, birthday_month,
    birthday_day, birthday_year, last_contact_on, archived_at, version, updated_at";

pub struct PersonInput<'a> {
    pub name: &'a str,
    pub aliases: &'a [String],
    pub area_id: Option<Uuid>,
    pub relationship: Option<&'a str>,
    pub org: Option<&'a str>,
    pub role: Option<&'a str>,
    pub contact: Option<&'a str>,
    pub email: Option<&'a str>,
    pub context_md: &'a str,
    pub birthday: (Option<i16>, Option<i16>, Option<i16>),
    pub archived: bool,
}

pub async fn list(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    q: Option<&str>,
    area_id: Option<Uuid>,
    archived: bool,
) -> RepoResult<Vec<Person>> {
    Ok(sqlx::query_as::<_, Person>(crate::sqlf!(
        "SELECT {COLS} FROM people WHERE user_id = $1 AND ($3 OR archived_at IS NULL) AND ($4::uuid IS NULL OR area_id = $4)
         AND ($2::text IS NULL OR immutable_unaccent(lower(name)) LIKE '%' || immutable_unaccent(lower($2)) || '%'
              OR EXISTS (SELECT 1 FROM unnest(aliases) a WHERE immutable_unaccent(lower(a)) LIKE '%' || immutable_unaccent(lower($2)) || '%'))
         ORDER BY name"
    ))
    .bind(user_id)
    .bind(q)
    .bind(archived)
    .bind(area_id)
    .fetch_all(db)
    .await?)
}

/// Resolución exacta por nombre o alias (sin acentos ni mayúsculas): "Juancho" → Juan Pérez.
pub async fn resolve(db: impl PgExecutor<'_>, user_id: Uuid, name: &str) -> RepoResult<Vec<Person>> {
    Ok(sqlx::query_as::<_, Person>(crate::sqlf!(
        "SELECT {COLS} FROM people WHERE user_id = $1 AND archived_at IS NULL AND (
            immutable_unaccent(lower(name)) = immutable_unaccent(lower($2))
            OR EXISTS (SELECT 1 FROM unnest(aliases) a WHERE immutable_unaccent(lower(a)) = immutable_unaccent(lower($2)))
            OR immutable_unaccent(lower(split_part(name, ' ', 1))) = immutable_unaccent(lower($2)))
         ORDER BY name"
    ))
    .bind(user_id)
    .bind(name.trim())
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Person> {
    sqlx::query_as::<_, Person>(crate::sqlf!(
        "SELECT {COLS} FROM people WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn create(db: impl PgExecutor<'_>, user_id: Uuid, p: &PersonInput<'_>) -> RepoResult<Person> {
    Ok(sqlx::query_as::<_, Person>(crate::sqlf!(
        "INSERT INTO people (user_id, name, aliases, area_id, relationship, org, role, contact, email, context_md,
            birthday_month, birthday_day, birthday_year)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(p.name)
    .bind(p.aliases)
    .bind(p.area_id)
    .bind(p.relationship)
    .bind(p.org)
    .bind(p.role)
    .bind(p.contact)
    .bind(p.email)
    .bind(p.context_md)
    .bind(p.birthday.0)
    .bind(p.birthday.1)
    .bind(p.birthday.2)
    .fetch_one(db)
    .await?)
}

pub async fn update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    p: &PersonInput<'_>,
) -> RepoResult<Person> {
    let row = sqlx::query_as::<_, Person>(crate::sqlf!(
        "UPDATE people SET name = $4, aliases = $5, area_id = $6, relationship = $7, org = $8, role = $9, contact = $10,
            email = $11, context_md = $12, birthday_month = $13, birthday_day = $14, birthday_year = $15,
            archived_at = CASE WHEN $16 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(p.name)
    .bind(p.aliases)
    .bind(p.area_id)
    .bind(p.relationship)
    .bind(p.org)
    .bind(p.role)
    .bind(p.contact)
    .bind(p.email)
    .bind(p.context_md)
    .bind(p.birthday.0)
    .bind(p.birthday.1)
    .bind(p.birthday.2)
    .bind(p.archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct PersonNote {
    pub id: Uuid,
    pub person_id: Uuid,
    pub on_date: NaiveDate,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

pub async fn notes(db: impl PgExecutor<'_>, user_id: Uuid, person_id: Uuid) -> RepoResult<Vec<PersonNote>> {
    Ok(sqlx::query_as::<_, PersonNote>(
        "SELECT id, person_id, on_date, text, created_at FROM person_notes WHERE user_id = $1 AND person_id = $2
         ORDER BY on_date DESC, created_at DESC",
    )
    .bind(user_id)
    .bind(person_id)
    .fetch_all(db)
    .await?)
}

pub async fn add_note(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    person_id: Uuid,
    on_date: NaiveDate,
    text: &str,
) -> RepoResult<PersonNote> {
    Ok(sqlx::query_as::<_, PersonNote>(
        "INSERT INTO person_notes (user_id, person_id, on_date, text) VALUES ($1, $2, $3, $4)
         RETURNING id, person_id, on_date, text, created_at",
    )
    .bind(user_id)
    .bind(person_id)
    .bind(on_date)
    .bind(text)
    .fetch_one(db)
    .await?)
}

pub async fn delete_note(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    person_id: Uuid,
    id: Uuid,
) -> RepoResult<()> {
    let n = sqlx::query("DELETE FROM person_notes WHERE user_id = $1 AND person_id = $2 AND id = $3")
        .bind(user_id)
        .bind(person_id)
        .bind(id)
        .execute(db)
        .await?
        .rows_affected();
    if n == 0 { Err(RepoError::NotFound) } else { Ok(()) }
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Interaction {
    pub id: Uuid,
    pub person_id: Uuid,
    pub person_name: String,
    pub on_date: NaiveDate,
    pub topic: String,
    /// `manual | calendar | gmail | bot | import`
    pub source: String,
    pub source_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

const ICOLS: &str =
    "i.id, i.person_id, p.name AS person_name, i.on_date, i.topic, i.source, i.source_ref, i.created_at";

pub async fn interactions(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    person_id: Option<Uuid>,
    date: Option<NaiveDate>,
    limit: i64,
) -> RepoResult<Vec<Interaction>> {
    Ok(sqlx::query_as::<_, Interaction>(crate::sqlf!(
        "SELECT {ICOLS} FROM interactions i JOIN people p ON p.id = i.person_id
         WHERE i.user_id = $1 AND ($2::uuid IS NULL OR i.person_id = $2) AND ($3::date IS NULL OR i.on_date = $3)
         ORDER BY i.on_date DESC, i.created_at DESC LIMIT $4"
    ))
    .bind(user_id)
    .bind(person_id)
    .bind(date)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

/// Registra una interacción (idempotente por fuente+referencia) y recalcula `last_contact_on` como el máximo.
pub async fn add_interaction(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    person_id: Uuid,
    on_date: NaiveDate,
    topic: &str,
    source: &str,
    source_ref: Option<&str>,
) -> RepoResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO interactions (user_id, person_id, on_date, topic, source, source_ref) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (user_id, person_id, source, source_ref) DO UPDATE SET topic = EXCLUDED.topic, on_date = EXCLUDED.on_date
         RETURNING id",
    )
    .bind(user_id)
    .bind(person_id)
    .bind(on_date)
    .bind(topic)
    .bind(source)
    .bind(source_ref)
    .fetch_one(&mut **tx)
    .await?;
    refresh_last_contact(tx, user_id, person_id).await?;
    Ok(id)
}

pub async fn refresh_last_contact(tx: &mut Tx<'_>, user_id: Uuid, person_id: Uuid) -> RepoResult<()> {
    sqlx::query(
        "UPDATE people SET last_contact_on = (SELECT max(on_date) FROM interactions WHERE user_id = $1 AND person_id = $2)
         WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(person_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn delete_interaction(pool: &PgPool, user_id: Uuid, id: Uuid) -> RepoResult<()> {
    let mut tx = pool.begin().await?;
    let person: Option<Uuid> =
        sqlx::query_scalar("DELETE FROM interactions WHERE user_id = $1 AND id = $2 RETURNING person_id")
            .bind(user_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let person = person.ok_or(RepoError::NotFound)?;
    refresh_last_contact(&mut tx, user_id, person).await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct PersonSuggestion {
    pub id: Uuid,
    pub name: String,
    pub email: Option<String>,
    /// `calendar | gmail | dictation | import`
    pub source: String,
    pub source_ref: String,
    pub on_date: Option<NaiveDate>,
    pub context: String,
    /// `pending | accepted | dismissed`
    pub status: String,
    pub person_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

const SCOLS: &str = "id, name, email, source, source_ref, on_date, context, status, person_id, created_at";

pub async fn suggestions(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    status: Option<&str>,
) -> RepoResult<Vec<PersonSuggestion>> {
    Ok(sqlx::query_as::<_, PersonSuggestion>(crate::sqlf!(
        "SELECT {SCOLS} FROM person_suggestions WHERE user_id = $1 AND ($2::text IS NULL OR status = $2) ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .bind(status)
    .fetch_all(db)
    .await?)
}

pub async fn get_suggestion(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    id: Uuid,
) -> RepoResult<PersonSuggestion> {
    sqlx::query_as::<_, PersonSuggestion>(crate::sqlf!(
        "SELECT {SCOLS} FROM person_suggestions WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

/// Crea una sugerencia (idempotente por fuente+referencia). Nunca crea la persona.
#[allow(clippy::too_many_arguments)]
pub async fn suggest(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    name: &str,
    email: Option<&str>,
    source: &str,
    source_ref: &str,
    on_date: Option<NaiveDate>,
    context: &str,
) -> RepoResult<PersonSuggestion> {
    Ok(sqlx::query_as::<_, PersonSuggestion>(crate::sqlf!(
        "INSERT INTO person_suggestions (user_id, name, email, source, source_ref, on_date, context)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (user_id, source, source_ref) DO UPDATE SET context = EXCLUDED.context
         RETURNING {SCOLS}"
    ))
    .bind(user_id)
    .bind(name)
    .bind(email)
    .bind(source)
    .bind(source_ref)
    .bind(on_date)
    .bind(context)
    .fetch_one(db)
    .await?)
}

pub async fn resolve_suggestion(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Uuid,
    status: &str,
    person_id: Option<Uuid>,
) -> RepoResult<()> {
    let n = sqlx::query(
        "UPDATE person_suggestions SET status = $3, person_id = $4 WHERE user_id = $1 AND id = $2 AND status = 'pending'",
    )
    .bind(user_id)
    .bind(id)
    .bind(status)
    .bind(person_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if n == 0 {
        Err(RepoError::Conflict(
            serde_json::json!({ "status": "already_resolved" }),
        ))
    } else {
        Ok(())
    }
}
