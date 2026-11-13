//! Propietario: agente B-modules. Materias y notas de clase (las entregas son tareas con `class_note_id`).

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Subject {
    pub id: Uuid,
    pub name: String,
    pub area_id: Option<Uuid>,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub note_count: i64,
}

const SUBJ: &str = "s.id, s.name, s.area_id, s.archived_at, s.version,
    (SELECT count(*) FROM class_notes c WHERE c.subject_id = s.id) AS note_count";

pub async fn list_subjects(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    archived: bool,
) -> RepoResult<Vec<Subject>> {
    Ok(sqlx::query_as::<_, Subject>(crate::sqlf!(
        "SELECT {SUBJ} FROM subjects s WHERE s.user_id = $1 AND ($2 OR s.archived_at IS NULL) ORDER BY s.name"
    ))
    .bind(user_id)
    .bind(archived)
    .fetch_all(db)
    .await?)
}

pub async fn get_subject(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Subject> {
    sqlx::query_as::<_, Subject>(crate::sqlf!(
        "SELECT {SUBJ} FROM subjects s WHERE s.user_id = $1 AND s.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn create_subject(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
    area_id: Option<Uuid>,
) -> RepoResult<Subject> {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO subjects (user_id, name, area_id) VALUES ($1, $2, $3) RETURNING id")
            .bind(user_id)
            .bind(name)
            .bind(area_id)
            .fetch_one(pool)
            .await?;
    get_subject(pool, user_id, id).await
}

pub async fn update_subject(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    name: &str,
    area_id: Option<Uuid>,
    archived: bool,
) -> RepoResult<Subject> {
    let n = sqlx::query(
        "UPDATE subjects SET name = $4, area_id = $5, archived_at = CASE WHEN $6 THEN coalesce(archived_at, now()) ELSE NULL END,
            version = version + 1 WHERE user_id = $1 AND id = $2 AND version = $3",
    )
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(name)
    .bind(area_id)
    .bind(archived)
    .execute(pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(conflict_or_missing(get_subject(pool, user_id, id).await));
    }
    get_subject(pool, user_id, id).await
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct ClassNote {
    pub id: Uuid,
    pub subject_id: Uuid,
    pub subject_name: String,
    pub on_date: NaiveDate,
    pub topic: String,
    pub topics_md: String,
    pub questions_md: String,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const CN: &str = "c.id, c.subject_id, s.name AS subject_name, c.on_date, c.topic, c.topics_md, c.questions_md, c.version, c.updated_at";

pub async fn list_notes(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    subject_id: Option<Uuid>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> RepoResult<Vec<ClassNote>> {
    Ok(sqlx::query_as::<_, ClassNote>(crate::sqlf!(
        "SELECT {CN} FROM class_notes c JOIN subjects s ON s.id = c.subject_id
         WHERE c.user_id = $1 AND ($2::uuid IS NULL OR c.subject_id = $2) AND ($3::date IS NULL OR c.on_date >= $3)
         AND ($4::date IS NULL OR c.on_date <= $4) ORDER BY c.on_date DESC, c.created_at DESC"
    ))
    .bind(user_id)
    .bind(subject_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

pub async fn get_note(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<ClassNote> {
    sqlx::query_as::<_, ClassNote>(crate::sqlf!(
        "SELECT {CN} FROM class_notes c JOIN subjects s ON s.id = c.subject_id WHERE c.user_id = $1 AND c.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct ClassNoteInput<'a> {
    pub subject_id: Uuid,
    pub on_date: NaiveDate,
    pub topic: &'a str,
    pub topics_md: &'a str,
    pub questions_md: &'a str,
}

pub async fn create_note(pool: &PgPool, user_id: Uuid, c: &ClassNoteInput<'_>) -> RepoResult<ClassNote> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO class_notes (user_id, subject_id, on_date, topic, topics_md, questions_md) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(user_id)
    .bind(c.subject_id)
    .bind(c.on_date)
    .bind(c.topic)
    .bind(c.topics_md)
    .bind(c.questions_md)
    .fetch_one(pool)
    .await?;
    get_note(pool, user_id, id).await
}

pub async fn update_note(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    c: &ClassNoteInput<'_>,
) -> RepoResult<ClassNote> {
    let n = sqlx::query(
        "UPDATE class_notes SET subject_id = $4, on_date = $5, topic = $6, topics_md = $7, questions_md = $8, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3",
    )
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(c.subject_id)
    .bind(c.on_date)
    .bind(c.topic)
    .bind(c.topics_md)
    .bind(c.questions_md)
    .execute(pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(conflict_or_missing(get_note(pool, user_id, id).await));
    }
    get_note(pool, user_id, id).await
}
