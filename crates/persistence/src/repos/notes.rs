//! Propietario: agente B-modules. Notas (note/runbook/til/snippet), enlaces entre entidades, backlinks, favoritos,
//! archivado/recuperación genéricos y búsqueda global sobre `search_index`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult};

/// Tipo de entidad → tabla (lista cerrada: nunca se interpola texto del usuario en SQL).
pub fn table_of(entity_type: &str) -> Option<&'static str> {
    Some(match entity_type {
        "area" => "areas",
        "project" => "projects",
        "goal" => "goals",
        "habit" => "habits",
        "checkin" => "daily_checkins",
        "person" => "people",
        "task" => "tasks",
        "inbox" => "inbox_items",
        "weekly_review" => "weekly_reviews",
        "devotional" => "devotionals",
        "reading" => "readings",
        "content" => "content_pieces",
        "swipe" => "swipe_refs",
        "subject" => "subjects",
        "class_note" => "class_notes",
        "devlog_session" => "devlog_sessions",
        "adr" => "adrs",
        "note" => "notes",
        "drawing" => "drawings",
        "transaction" => "transactions",
        _ => return None,
    })
}

/// Tablas con `archived_at` (archivar/recuperar genérico).
pub fn archivable_table(entity_type: &str) -> Option<&'static str> {
    match entity_type {
        "area" | "project" | "goal" | "habit" | "person" | "task" | "reading" | "content" | "swipe"
        | "subject" | "note" | "drawing" => table_of(entity_type),
        _ => None,
    }
}

/// ¿Existe la entidad y es del usuario?
pub async fn entity_exists(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    entity_type: &str,
    id: Uuid,
) -> RepoResult<bool> {
    let Some(table) = table_of(entity_type) else {
        return Ok(false);
    };
    Ok(sqlx::query_scalar(crate::sqlf!(
        "SELECT EXISTS (SELECT 1 FROM {table} WHERE user_id = $1 AND id = $2)"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_one(db)
    .await?)
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Note {
    pub id: Uuid,
    /// `note | runbook | til | snippet`
    pub kind: String,
    pub title: String,
    pub body_md: String,
    /// Lenguaje del snippet (texto; nunca se ejecuta).
    pub language: Option<String>,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str = "id, kind, title, body_md, language, aliases, tags, area_id, project_id, metadata, archived_at, version, created_at, updated_at";

pub struct NoteInput<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub body_md: &'a str,
    pub language: Option<&'a str>,
    pub aliases: &'a [String],
    pub tags: &'a [String],
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub metadata: &'a serde_json::Value,
    pub archived: bool,
}

#[derive(Debug, Default)]
pub struct NoteFilter {
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub area_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub q: Option<String>,
    pub archived: bool,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list(db: &PgPool, user_id: Uuid, f: &NoteFilter) -> RepoResult<(Vec<Note>, i64)> {
    let w = "user_id = $1 AND ($2::text IS NULL OR kind = $2) AND ($3::text IS NULL OR $3 = ANY(tags))
        AND ($4::uuid IS NULL OR area_id = $4) AND ($5::uuid IS NULL OR project_id = $5)
        AND ($6::text IS NULL OR immutable_unaccent(lower(title || ' ' || body_md)) LIKE '%' || immutable_unaccent(lower($6)) || '%')
        AND (CASE WHEN $7 THEN archived_at IS NOT NULL ELSE archived_at IS NULL END)";
    let items = sqlx::query_as::<_, Note>(crate::sqlf!(
        "SELECT {COLS} FROM notes WHERE {w} ORDER BY updated_at DESC LIMIT $8 OFFSET $9"
    ))
    .bind(user_id)
    .bind(&f.kind)
    .bind(&f.tag)
    .bind(f.area_id)
    .bind(f.project_id)
    .bind(&f.q)
    .bind(f.archived)
    .bind(f.limit)
    .bind(f.offset)
    .fetch_all(db)
    .await?;
    let total = sqlx::query_scalar(crate::sqlf!("SELECT count(*) FROM notes WHERE {w}"))
        .bind(user_id)
        .bind(&f.kind)
        .bind(&f.tag)
        .bind(f.area_id)
        .bind(f.project_id)
        .bind(&f.q)
        .bind(f.archived)
        .fetch_one(db)
        .await?;
    Ok((items, total))
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Note> {
    sqlx::query_as::<_, Note>(crate::sqlf!(
        "SELECT {COLS} FROM notes WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn create(db: impl PgExecutor<'_>, user_id: Uuid, n: &NoteInput<'_>) -> RepoResult<Note> {
    Ok(sqlx::query_as::<_, Note>(crate::sqlf!(
        "INSERT INTO notes (user_id, kind, title, body_md, language, aliases, tags, area_id, project_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(n.kind)
    .bind(n.title)
    .bind(n.body_md)
    .bind(n.language)
    .bind(n.aliases)
    .bind(n.tags)
    .bind(n.area_id)
    .bind(n.project_id)
    .bind(n.metadata)
    .fetch_one(db)
    .await?)
}

pub async fn update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    n: &NoteInput<'_>,
) -> RepoResult<Note> {
    let row = sqlx::query_as::<_, Note>(crate::sqlf!(
        "UPDATE notes SET kind = $4, title = $5, body_md = $6, language = $7, aliases = $8, tags = $9, area_id = $10,
            project_id = $11, metadata = $12, archived_at = CASE WHEN $13 THEN coalesce(archived_at, now()) ELSE NULL END,
            version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(n.kind)
    .bind(n.title)
    .bind(n.body_md)
    .bind(n.language)
    .bind(n.aliases)
    .bind(n.tags)
    .bind(n.area_id)
    .bind(n.project_id)
    .bind(n.metadata)
    .bind(n.archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct TagCount {
    pub tag: String,
    pub count: i64,
}

/// Tags en uso (notas, swipe y otros con `tags` en el índice de búsqueda).
pub async fn tags(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<TagCount>> {
    Ok(sqlx::query_as::<_, TagCount>(
        "SELECT t AS tag, count(*) AS count FROM search_index, unnest(tags) t WHERE user_id = $1 AND NOT archived
         GROUP BY t ORDER BY count DESC, t",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

// ---------- Enlaces y backlinks ----------

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct LinkView {
    pub src_type: String,
    pub src_id: Uuid,
    pub dst_type: String,
    pub dst_id: Uuid,
    pub relation: String,
    /// Título del otro extremo (desde el índice de búsqueda).
    pub other_title: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub async fn add_link(
    pool: &PgPool,
    user_id: Uuid,
    src: (&str, Uuid),
    dst: (&str, Uuid),
    relation: &str,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO entity_links (user_id, src_type, src_id, dst_type, dst_id, relation) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(src.0)
    .bind(src.1)
    .bind(dst.0)
    .bind(dst.1)
    .bind(relation)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_link(
    pool: &PgPool,
    user_id: Uuid,
    src: (&str, Uuid),
    dst: (&str, Uuid),
    relation: &str,
) -> RepoResult<u64> {
    Ok(sqlx::query(
        "DELETE FROM entity_links WHERE user_id = $1 AND src_type = $2 AND src_id = $3 AND dst_type = $4 AND dst_id = $5 AND relation = $6",
    )
    .bind(user_id)
    .bind(src.0)
    .bind(src.1)
    .bind(dst.0)
    .bind(dst.1)
    .bind(relation)
    .execute(pool)
    .await?
    .rows_affected())
}

pub async fn outgoing(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    t: &str,
    id: Uuid,
) -> RepoResult<Vec<LinkView>> {
    Ok(sqlx::query_as::<_, LinkView>(
        "SELECT l.src_type, l.src_id, l.dst_type, l.dst_id, l.relation, s.title AS other_title, l.created_at
         FROM entity_links l LEFT JOIN search_index s ON s.entity_type = l.dst_type AND s.entity_id = l.dst_id AND s.user_id = l.user_id
         WHERE l.user_id = $1 AND l.src_type = $2 AND l.src_id = $3 ORDER BY l.created_at",
    )
    .bind(user_id)
    .bind(t)
    .bind(id)
    .fetch_all(db)
    .await?)
}

pub async fn backlinks(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    t: &str,
    id: Uuid,
) -> RepoResult<Vec<LinkView>> {
    Ok(sqlx::query_as::<_, LinkView>(
        "SELECT l.src_type, l.src_id, l.dst_type, l.dst_id, l.relation, s.title AS other_title, l.created_at
         FROM entity_links l LEFT JOIN search_index s ON s.entity_type = l.src_type AND s.entity_id = l.src_id AND s.user_id = l.user_id
         WHERE l.user_id = $1 AND l.dst_type = $2 AND l.dst_id = $3 ORDER BY l.created_at",
    )
    .bind(user_id)
    .bind(t)
    .bind(id)
    .fetch_all(db)
    .await?)
}

// ---------- Favoritos y archivados ----------

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct EntityRef {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub title: Option<String>,
    pub on_date: Option<NaiveDate>,
    pub status: Option<String>,
    pub at: DateTime<Utc>,
}

pub async fn favorites(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<EntityRef>> {
    Ok(sqlx::query_as::<_, EntityRef>(
        "SELECT f.entity_type, f.entity_id, s.title, s.on_date, s.status, f.created_at AS at
         FROM favorites f LEFT JOIN search_index s ON s.entity_type = f.entity_type AND s.entity_id = f.entity_id AND s.user_id = f.user_id
         WHERE f.user_id = $1 ORDER BY f.created_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn set_favorite(pool: &PgPool, user_id: Uuid, t: &str, id: Uuid, on: bool) -> RepoResult<()> {
    if on {
        sqlx::query("INSERT INTO favorites (user_id, entity_type, entity_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(t)
            .bind(id)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("DELETE FROM favorites WHERE user_id = $1 AND entity_type = $2 AND entity_id = $3")
            .bind(user_id)
            .bind(t)
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn archived(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<EntityRef>> {
    Ok(sqlx::query_as::<_, EntityRef>(
        "SELECT entity_type, entity_id, title, on_date, status, updated_at AS at FROM search_index
         WHERE user_id = $1 AND archived ORDER BY updated_at DESC LIMIT 500",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// Archiva (`true`) o recupera (`false`) cualquier entidad archivable del usuario.
pub async fn set_archived(pool: &PgPool, user_id: Uuid, t: &str, id: Uuid, archive: bool) -> RepoResult<()> {
    let table = archivable_table(t).ok_or(RepoError::NotFound)?;
    let n = sqlx::query(crate::sqlf!(
        "UPDATE {table} SET archived_at = CASE WHEN $3 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .bind(archive)
    .execute(pool)
    .await?
    .rows_affected();
    if n == 0 { Err(RepoError::NotFound) } else { Ok(()) }
}

// ---------- Búsqueda global ----------

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct SearchHit {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub title: String,
    /// Fragmento del cuerpo (texto plano).
    pub snippet: String,
    pub tags: Vec<String>,
    pub area_id: Option<Uuid>,
    pub on_date: Option<NaiveDate>,
    pub status: Option<String>,
    pub archived: bool,
    pub rank: f32,
}

#[derive(Debug, Default)]
pub struct SearchFilter {
    /// Tokens ya normalizados (minúsculas, sin acentos, solo alfanuméricos).
    pub tokens: Vec<String>,
    pub raw: String,
    pub types: Vec<String>,
    pub area_id: Option<Uuid>,
    pub status: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub include_archived: bool,
    pub limit: i64,
}

pub async fn search(db: impl PgExecutor<'_>, user_id: Uuid, f: &SearchFilter) -> RepoResult<Vec<SearchHit>> {
    // tsquery con prefijos: tokens alfanuméricos ya saneados → "tok1:* & tok2:*".
    let tsq = f
        .tokens
        .iter()
        .map(|t| format!("{t}:*"))
        .collect::<Vec<_>>()
        .join(" & ");
    Ok(sqlx::query_as::<_, SearchHit>(
        "SELECT entity_type, entity_id, title, left(body, 240) AS snippet, tags, area_id, on_date, status, archived,
                (ts_rank(doc, q) + similarity(immutable_unaccent(lower(title)), immutable_unaccent(lower($3))))::real AS rank
         FROM search_index, to_tsquery('simple', $2) q
         WHERE user_id = $1
           AND (doc @@ q OR immutable_unaccent(lower(title)) % immutable_unaccent(lower($3))
                OR $3 = ANY(tags))
           AND (cardinality($4::text[]) = 0 OR entity_type = ANY($4))
           AND ($5::uuid IS NULL OR area_id = $5) AND ($6::text IS NULL OR status = $6)
           AND ($7::date IS NULL OR on_date >= $7) AND ($8::date IS NULL OR on_date <= $8)
           AND ($9 OR NOT archived)
         ORDER BY rank DESC, updated_at DESC LIMIT $10",
    )
    .bind(user_id)
    .bind(tsq)
    .bind(&f.raw)
    .bind(&f.types)
    .bind(f.area_id)
    .bind(&f.status)
    .bind(f.from)
    .bind(f.to)
    .bind(f.include_archived)
    .bind(f.limit)
    .fetch_all(db)
    .await?)
}
