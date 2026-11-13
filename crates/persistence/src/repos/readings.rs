//! Lecturas (libros y artículos). Propietario: agente A-core.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Reading {
    pub id: Uuid,
    /// `book | article`
    pub kind: String,
    pub title: String,
    pub author: String,
    pub url: Option<String>,
    /// `want | reading | finished | abandoned`
    pub status: String,
    pub rating: Option<i16>,
    pub started_on: Option<NaiveDate>,
    pub finished_on: Option<NaiveDate>,
    pub ideas_md: String,
    pub quotes_md: String,
    pub application_md: String,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str = "id, kind, title, author, url, status, rating, started_on, finished_on, ideas_md, quotes_md, application_md,
    archived_at, version, updated_at";

pub async fn list(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    status: Option<&str>,
    q: Option<&str>,
    include_archived: bool,
) -> RepoResult<Vec<Reading>> {
    Ok(sqlx::query_as::<_, Reading>(crate::sqlf!(
        "SELECT {COLS} FROM readings WHERE user_id = $1 AND ($2::text IS NULL OR status = $2)
           AND ($3::text IS NULL OR immutable_unaccent(lower(title || ' ' || author)) LIKE '%' || immutable_unaccent(lower($3)) || '%')
           AND ($4 OR archived_at IS NULL)
         ORDER BY CASE status WHEN 'reading' THEN 0 WHEN 'want' THEN 1 WHEN 'finished' THEN 2 ELSE 3 END,
                  finished_on DESC NULLS LAST, rating DESC NULLS LAST, title"
    ))
    .bind(user_id)
    .bind(status)
    .bind(q)
    .bind(include_archived)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Reading> {
    sqlx::query_as::<_, Reading>(crate::sqlf!(
        "SELECT {COLS} FROM readings WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct ReadingInput<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub author: &'a str,
    pub url: Option<&'a str>,
    pub status: &'a str,
    pub rating: Option<i16>,
    pub started_on: Option<NaiveDate>,
    pub finished_on: Option<NaiveDate>,
    pub ideas_md: &'a str,
    pub quotes_md: &'a str,
    pub application_md: &'a str,
    pub archived: bool,
}

pub async fn create(tx: &mut Tx<'_>, user_id: Uuid, r: &ReadingInput<'_>) -> RepoResult<Reading> {
    Ok(sqlx::query_as::<_, Reading>(crate::sqlf!(
        "INSERT INTO readings (user_id, kind, title, author, url, status, rating, started_on, finished_on, ideas_md, quotes_md, application_md)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(r.kind)
    .bind(r.title)
    .bind(r.author)
    .bind(r.url)
    .bind(r.status)
    .bind(r.rating)
    .bind(r.started_on)
    .bind(r.finished_on)
    .bind(r.ideas_md)
    .bind(r.quotes_md)
    .bind(r.application_md)
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn update(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    r: &ReadingInput<'_>,
) -> RepoResult<Reading> {
    let row = sqlx::query_as::<_, Reading>(crate::sqlf!(
        "UPDATE readings SET kind = $4, title = $5, author = $6, url = $7, status = $8, rating = $9, started_on = $10,
            finished_on = $11, ideas_md = $12, quotes_md = $13, application_md = $14,
            archived_at = CASE WHEN $15 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(r.kind)
    .bind(r.title)
    .bind(r.author)
    .bind(r.url)
    .bind(r.status)
    .bind(r.rating)
    .bind(r.started_on)
    .bind(r.finished_on)
    .bind(r.ideas_md)
    .bind(r.quotes_md)
    .bind(r.application_md)
    .bind(r.archived)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(&mut **tx, user_id, id).await)),
    }
}

pub async fn delete(tx: &mut Tx<'_>, user_id: Uuid, id: Uuid) -> RepoResult<()> {
    let n = sqlx::query("DELETE FROM readings WHERE user_id = $1 AND id = $2")
        .bind(user_id)
        .bind(id)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    if n == 0 { Err(RepoError::NotFound) } else { Ok(()) }
}
