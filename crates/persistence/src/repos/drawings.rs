//! Propietario: agente B-modules. Notas visuales (escena Excalidraw en JSON) enlazables a notas.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Drawing {
    pub id: Uuid,
    pub title: String,
    /// Escena Excalidraw (`elements`, `appState`, `files`).
    #[schema(value_type = Object)]
    pub scene: serde_json::Value,
    pub note_id: Option<Uuid>,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct DrawingSummary {
    pub id: Uuid,
    pub title: String,
    pub note_id: Option<Uuid>,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const COLS: &str = "id, title, scene, note_id, archived_at, version, updated_at";

pub async fn list(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    note_id: Option<Uuid>,
    archived: bool,
) -> RepoResult<Vec<DrawingSummary>> {
    Ok(sqlx::query_as::<_, DrawingSummary>(
        "SELECT id, title, note_id, archived_at, version, updated_at FROM drawings
         WHERE user_id = $1 AND ($2::uuid IS NULL OR note_id = $2) AND ($3 OR archived_at IS NULL) ORDER BY updated_at DESC",
    )
    .bind(user_id)
    .bind(note_id)
    .bind(archived)
    .fetch_all(db)
    .await?)
}

pub async fn get(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Drawing> {
    sqlx::query_as::<_, Drawing>(crate::sqlf!(
        "SELECT {COLS} FROM drawings WHERE user_id = $1 AND id = $2"
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
    title: &str,
    scene: &serde_json::Value,
    note_id: Option<Uuid>,
) -> RepoResult<Drawing> {
    Ok(sqlx::query_as::<_, Drawing>(crate::sqlf!(
        "INSERT INTO drawings (user_id, title, scene, note_id) VALUES ($1, $2, $3, $4) RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(title)
    .bind(scene)
    .bind(note_id)
    .fetch_one(db)
    .await?)
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    title: &str,
    scene: &serde_json::Value,
    note_id: Option<Uuid>,
    archived: bool,
) -> RepoResult<Drawing> {
    let row = sqlx::query_as::<_, Drawing>(crate::sqlf!(
        "UPDATE drawings SET title = $4, scene = $5, note_id = $6,
            archived_at = CASE WHEN $7 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(title)
    .bind(scene)
    .bind(note_id)
    .bind(archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get(pool, user_id, id).await)),
    }
}
