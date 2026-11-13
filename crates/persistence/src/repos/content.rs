//! Propietario: agente B-modules. Piezas de contenido (pipeline) y referencias Swipe.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct ContentPiece {
    pub id: Uuid,
    pub title: String,
    pub idea: String,
    /// `idea | script | recorded | edited | published`
    pub status: String,
    pub format: String,
    pub platforms: Vec<String>,
    pub pillar: Option<String>,
    /// Hasta 10 hooks; `chosen_hook` es el índice elegido.
    pub hooks: Vec<String>,
    pub chosen_hook: Option<i16>,
    pub script_hook: String,
    pub script_body: String,
    pub script_cta: String,
    /// Lista de `{ "n", "shot", "text", "motion" }`.
    #[schema(value_type = Vec<Object>)]
    pub shot_list: serde_json::Value,
    pub edit_notes: String,
    pub result_worked: String,
    pub result_change: String,
    pub publish_on: Option<NaiveDate>,
    pub published_on: Option<NaiveDate>,
    pub raw_url: Option<String>,
    pub render_url: Option<String>,
    pub views: Option<i64>,
    #[schema(value_type = Option<String>)]
    pub retention_pct: Option<Decimal>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str = "id, title, idea, status, format, platforms, pillar, hooks, chosen_hook, script_hook, script_body, script_cta,
    shot_list, edit_notes, result_worked, result_change, publish_on, published_on, raw_url, render_url, views, retention_pct,
    area_id, project_id, archived_at, version, updated_at";

pub struct ContentInput<'a> {
    pub title: &'a str,
    pub idea: &'a str,
    pub status: &'a str,
    pub format: &'a str,
    pub platforms: &'a [String],
    pub pillar: Option<&'a str>,
    pub hooks: &'a [String],
    pub chosen_hook: Option<i16>,
    pub script_hook: &'a str,
    pub script_body: &'a str,
    pub script_cta: &'a str,
    pub shot_list: &'a serde_json::Value,
    pub edit_notes: &'a str,
    pub result_worked: &'a str,
    pub result_change: &'a str,
    pub publish_on: Option<NaiveDate>,
    pub published_on: Option<NaiveDate>,
    pub raw_url: Option<&'a str>,
    pub render_url: Option<&'a str>,
    pub views: Option<i64>,
    pub retention_pct: Option<Decimal>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub archived: bool,
}

pub async fn list(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    status: Option<&str>,
    q: Option<&str>,
    archived: bool,
) -> RepoResult<Vec<ContentPiece>> {
    Ok(sqlx::query_as::<_, ContentPiece>(crate::sqlf!(
        "SELECT {COLS} FROM content_pieces WHERE user_id = $1 AND ($2::text IS NULL OR status = $2)
         AND ($3::text IS NULL OR immutable_unaccent(lower(title || ' ' || idea)) LIKE '%' || immutable_unaccent(lower($3)) || '%')
         AND ($4 OR archived_at IS NULL)
         ORDER BY publish_on NULLS LAST, updated_at DESC"
    ))
    .bind(user_id)
    .bind(status)
    .bind(q)
    .bind(archived)
    .fetch_all(db)
    .await?)
}

/// Publicadas ordenadas por métrica (`views` o `retention`).
pub async fn published(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    by_retention: bool,
    limit: i64,
) -> RepoResult<Vec<ContentPiece>> {
    let order = if by_retention {
        "retention_pct DESC NULLS LAST"
    } else {
        "views DESC NULLS LAST"
    };
    Ok(sqlx::query_as::<_, ContentPiece>(crate::sqlf!(
        "SELECT {COLS} FROM content_pieces WHERE user_id = $1 AND status = 'published' AND archived_at IS NULL
         ORDER BY {order}, published_on DESC NULLS LAST LIMIT $2"
    ))
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

/// Las N publicadas más recientes (contexto obligatorio antes de proponer un guion con IA).
pub async fn recent_published(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    limit: i64,
) -> RepoResult<Vec<ContentPiece>> {
    Ok(sqlx::query_as::<_, ContentPiece>(crate::sqlf!(
        "SELECT {COLS} FROM content_pieces WHERE user_id = $1 AND status = 'published' AND archived_at IS NULL
         ORDER BY coalesce(published_on, publish_on) DESC NULLS LAST, updated_at DESC LIMIT $2"
    ))
    .bind(user_id)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<ContentPiece> {
    sqlx::query_as::<_, ContentPiece>(crate::sqlf!(
        "SELECT {COLS} FROM content_pieces WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn create(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    c: &ContentInput<'_>,
) -> RepoResult<ContentPiece> {
    Ok(sqlx::query_as::<_, ContentPiece>(crate::sqlf!(
        "INSERT INTO content_pieces (user_id, title, idea, status, format, platforms, pillar, hooks, chosen_hook, script_hook,
            script_body, script_cta, shot_list, edit_notes, result_worked, result_change, publish_on, published_on, raw_url,
            render_url, views, retention_pct, area_id, project_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)
         RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(c.title)
    .bind(c.idea)
    .bind(c.status)
    .bind(c.format)
    .bind(c.platforms)
    .bind(c.pillar)
    .bind(c.hooks)
    .bind(c.chosen_hook)
    .bind(c.script_hook)
    .bind(c.script_body)
    .bind(c.script_cta)
    .bind(c.shot_list)
    .bind(c.edit_notes)
    .bind(c.result_worked)
    .bind(c.result_change)
    .bind(c.publish_on)
    .bind(c.published_on)
    .bind(c.raw_url)
    .bind(c.render_url)
    .bind(c.views)
    .bind(c.retention_pct)
    .bind(c.area_id)
    .bind(c.project_id)
    .fetch_one(db)
    .await?)
}

pub async fn update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    c: &ContentInput<'_>,
) -> RepoResult<ContentPiece> {
    let row = sqlx::query_as::<_, ContentPiece>(crate::sqlf!(
        "UPDATE content_pieces SET title = $4, idea = $5, status = $6, format = $7, platforms = $8, pillar = $9, hooks = $10,
            chosen_hook = $11, script_hook = $12, script_body = $13, script_cta = $14, shot_list = $15, edit_notes = $16,
            result_worked = $17, result_change = $18, publish_on = $19, published_on = $20, raw_url = $21, render_url = $22,
            views = $23, retention_pct = $24, area_id = $25, project_id = $26,
            archived_at = CASE WHEN $27 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(c.title)
    .bind(c.idea)
    .bind(c.status)
    .bind(c.format)
    .bind(c.platforms)
    .bind(c.pillar)
    .bind(c.hooks)
    .bind(c.chosen_hook)
    .bind(c.script_hook)
    .bind(c.script_body)
    .bind(c.script_cta)
    .bind(c.shot_list)
    .bind(c.edit_notes)
    .bind(c.result_worked)
    .bind(c.result_change)
    .bind(c.publish_on)
    .bind(c.published_on)
    .bind(c.raw_url)
    .bind(c.render_url)
    .bind(c.views)
    .bind(c.retention_pct)
    .bind(c.area_id)
    .bind(c.project_id)
    .bind(c.archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}

// ---------- Swipe ----------

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct SwipeRef {
    pub id: Uuid,
    pub title: String,
    pub url: String,
    pub creator: Option<String>,
    pub platform: Option<String>,
    pub why_it_works: String,
    /// Hook transcrito.
    pub hook: String,
    pub tags: Vec<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const SW: &str =
    "id, title, url, creator, platform, why_it_works, hook, tags, archived_at, version, updated_at";

pub struct SwipeInput<'a> {
    pub title: &'a str,
    pub url: &'a str,
    pub creator: Option<&'a str>,
    pub platform: Option<&'a str>,
    pub why_it_works: &'a str,
    pub hook: &'a str,
    pub tags: &'a [String],
    pub archived: bool,
}

pub async fn list_swipe(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    tag: Option<&str>,
    q: Option<&str>,
    archived: bool,
) -> RepoResult<Vec<SwipeRef>> {
    Ok(sqlx::query_as::<_, SwipeRef>(crate::sqlf!(
        "SELECT {SW} FROM swipe_refs WHERE user_id = $1 AND ($2::text IS NULL OR $2 = ANY(tags))
         AND ($3::text IS NULL OR immutable_unaccent(lower(title || ' ' || hook || ' ' || why_it_works || ' ' || coalesce(creator, '')))
              LIKE '%' || immutable_unaccent(lower($3)) || '%')
         AND ($4 OR archived_at IS NULL) ORDER BY updated_at DESC"
    ))
    .bind(user_id)
    .bind(tag)
    .bind(q)
    .bind(archived)
    .fetch_all(db)
    .await?)
}

pub async fn get_swipe(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<SwipeRef> {
    sqlx::query_as::<_, SwipeRef>(crate::sqlf!(
        "SELECT {SW} FROM swipe_refs WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn create_swipe(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    w: &SwipeInput<'_>,
) -> RepoResult<SwipeRef> {
    Ok(sqlx::query_as::<_, SwipeRef>(crate::sqlf!(
        "INSERT INTO swipe_refs (user_id, title, url, creator, platform, why_it_works, hook, tags)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {SW}"
    ))
    .bind(user_id)
    .bind(w.title)
    .bind(w.url)
    .bind(w.creator)
    .bind(w.platform)
    .bind(w.why_it_works)
    .bind(w.hook)
    .bind(w.tags)
    .fetch_one(db)
    .await?)
}

pub async fn update_swipe(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    w: &SwipeInput<'_>,
) -> RepoResult<SwipeRef> {
    let row = sqlx::query_as::<_, SwipeRef>(crate::sqlf!(
        "UPDATE swipe_refs SET title = $4, url = $5, creator = $6, platform = $7, why_it_works = $8, hook = $9, tags = $10,
            archived_at = CASE WHEN $11 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {SW}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(w.title)
    .bind(w.url)
    .bind(w.creator)
    .bind(w.platform)
    .bind(w.why_it_works)
    .bind(w.hook)
    .bind(w.tags)
    .bind(w.archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_swipe(pool, user_id, id).await)),
    }
}
