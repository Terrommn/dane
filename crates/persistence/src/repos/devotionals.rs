//! Devocionales / reflexión diaria. Propietario: agente A-core.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Devotional {
    pub id: Uuid,
    pub on_date: NaiveDate,
    /// `devotional | meditation`
    pub mode: String,
    pub passage: String,
    /// `[{verse, action}]` — "¿Cómo puedo agradar a Dios hoy?"
    #[schema(value_type = Vec<Object>)]
    pub verse_actions: serde_json::Value,
    pub learned: String,
    pub teaching: String,
    pub prayer: String,
    pub song: String,
    /// Secciones personalizadas de la plantilla del usuario: `{clave: texto}`.
    #[schema(value_type = Object)]
    pub extra: serde_json::Value,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str =
    "id, on_date, mode, passage, verse_actions, learned, teaching, prayer, song, extra, version, updated_at";

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, on: NaiveDate) -> RepoResult<Option<Devotional>> {
    Ok(sqlx::query_as::<_, Devotional>(crate::sqlf!(
        "SELECT {COLS} FROM devotionals WHERE user_id = $1 AND on_date = $2"
    ))
    .bind(user_id)
    .bind(on)
    .fetch_optional(db)
    .await?)
}

pub async fn list(
    db: &sqlx::PgPool,
    user_id: Uuid,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> RepoResult<(Vec<Devotional>, i64)> {
    let pattern = q.map(|q| format!("%{}%", q.replace('%', "")));
    let filter = "user_id = $1 AND ($2::text IS NULL OR immutable_unaccent(lower(passage || ' ' || learned || ' ' || teaching || ' ' || prayer || ' ' || song || ' ' || verse_actions::text))
        LIKE immutable_unaccent(lower($2)))";
    let rows = sqlx::query_as::<_, Devotional>(crate::sqlf!(
        "SELECT {COLS} FROM devotionals WHERE {filter} ORDER BY on_date DESC LIMIT $3 OFFSET $4"
    ))
    .bind(user_id)
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    let total: i64 = sqlx::query_scalar(crate::sqlf!("SELECT count(*) FROM devotionals WHERE {filter}"))
        .bind(user_id)
        .bind(&pattern)
        .fetch_one(db)
        .await?;
    Ok((rows, total))
}

pub struct DevotionalInput<'a> {
    pub mode: &'a str,
    pub passage: &'a str,
    pub verse_actions: &'a serde_json::Value,
    pub learned: &'a str,
    pub teaching: &'a str,
    pub prayer: &'a str,
    pub song: &'a str,
    pub extra: &'a serde_json::Value,
}

/// Crea o actualiza el devocional del día. `expected_version` 0/None = crear (409 si ya existe con otra versión).
pub async fn upsert(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    on: NaiveDate,
    expected_version: Option<i32>,
    d: &DevotionalInput<'_>,
) -> RepoResult<Devotional> {
    let row = sqlx::query_as::<_, Devotional>(crate::sqlf!(
        "INSERT INTO devotionals (user_id, on_date, mode, passage, verse_actions, learned, teaching, prayer, song, extra)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         ON CONFLICT (user_id, on_date) DO UPDATE SET mode = EXCLUDED.mode, passage = EXCLUDED.passage,
            verse_actions = EXCLUDED.verse_actions, learned = EXCLUDED.learned, teaching = EXCLUDED.teaching,
            prayer = EXCLUDED.prayer, song = EXCLUDED.song, extra = EXCLUDED.extra, version = devotionals.version + 1
         WHERE devotionals.version = coalesce($11, 0)
         RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(on)
    .bind(d.mode)
    .bind(d.passage)
    .bind(d.verse_actions)
    .bind(d.learned)
    .bind(d.teaching)
    .bind(d.prayer)
    .bind(d.song)
    .bind(d.extra)
    .bind(expected_version)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => match get(&mut **tx, user_id, on).await? {
            Some(cur) => Err(RepoError::Conflict(serde_json::to_value(cur).unwrap_or_default())),
            None => Err(RepoError::NotFound),
        },
    }
}

pub async fn delete(tx: &mut Tx<'_>, user_id: Uuid, on: NaiveDate) -> RepoResult<bool> {
    Ok(
        sqlx::query("DELETE FROM devotionals WHERE user_id = $1 AND on_date = $2")
            .bind(user_id)
            .bind(on)
            .execute(&mut **tx)
            .await?
            .rows_affected()
            > 0,
    )
}
