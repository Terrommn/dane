//! Áreas y proyectos. Patrón de referencia para el resto de repositorios:
//! filtrar por `user_id`, actualizar con `version` y devolver `Conflict` con la fila actual.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Area {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub color: Option<String>,
    pub position: i32,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Project {
    pub id: Uuid,
    pub area_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub status: String,
    pub repo_url: Option<String>,
    #[schema(value_type = Vec<Object>)]
    pub links: serde_json::Value,
    pub notes_md: String,
    pub archived_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const AREA_COLS: &str = "id, name, slug, description, color, position, archived_at, version, updated_at";
const PROJECT_COLS: &str = "id, area_id, name, slug, description, status, repo_url, links, notes_md, archived_at, version, updated_at";

pub async fn list_areas(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    include_archived: bool,
) -> RepoResult<Vec<Area>> {
    Ok(sqlx::query_as::<_, Area>(crate::sqlf!(
        "SELECT {AREA_COLS} FROM areas WHERE user_id = $1 AND ($2 OR archived_at IS NULL) ORDER BY position, name"
    ))
    .bind(user_id)
    .bind(include_archived)
    .fetch_all(db)
    .await?)
}

pub async fn get_area(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Area> {
    sqlx::query_as::<_, Area>(crate::sqlf!(
        "SELECT {AREA_COLS} FROM areas WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct AreaInput<'a> {
    pub name: &'a str,
    pub slug: &'a str,
    pub description: &'a str,
    pub color: Option<&'a str>,
    pub position: i32,
    pub archived: bool,
}

pub async fn create_area(db: impl PgExecutor<'_>, user_id: Uuid, a: &AreaInput<'_>) -> RepoResult<Area> {
    Ok(sqlx::query_as::<_, Area>(crate::sqlf!(
        "INSERT INTO areas (user_id, name, slug, description, color, position) VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING {AREA_COLS}"
    ))
    .bind(user_id)
    .bind(a.name)
    .bind(a.slug)
    .bind(a.description)
    .bind(a.color)
    .bind(a.position)
    .fetch_one(db)
    .await?)
}

pub async fn update_area(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    a: &AreaInput<'_>,
) -> RepoResult<Area> {
    let row = sqlx::query_as::<_, Area>(crate::sqlf!(
        "UPDATE areas SET name = $4, slug = $5, description = $6, color = $7, position = $8,
            archived_at = CASE WHEN $9 THEN coalesce(archived_at, now()) ELSE NULL END, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {AREA_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(a.name)
    .bind(a.slug)
    .bind(a.description)
    .bind(a.color)
    .bind(a.position)
    .bind(a.archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_area(pool, user_id, id).await)),
    }
}

pub async fn list_projects(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    area_id: Option<Uuid>,
    include_archived: bool,
) -> RepoResult<Vec<Project>> {
    Ok(sqlx::query_as::<_, Project>(crate::sqlf!(
        "SELECT {PROJECT_COLS} FROM projects WHERE user_id = $1 AND ($2::uuid IS NULL OR area_id = $2)
         AND ($3 OR archived_at IS NULL) ORDER BY name"
    ))
    .bind(user_id)
    .bind(area_id)
    .bind(include_archived)
    .fetch_all(db)
    .await?)
}

pub async fn get_project(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Project> {
    sqlx::query_as::<_, Project>(crate::sqlf!(
        "SELECT {PROJECT_COLS} FROM projects WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct ProjectInput<'a> {
    pub area_id: Option<Uuid>,
    pub name: &'a str,
    pub slug: &'a str,
    pub description: &'a str,
    pub status: &'a str,
    pub repo_url: Option<&'a str>,
    pub links: &'a serde_json::Value,
    pub notes_md: &'a str,
    pub archived: bool,
}

pub async fn create_project(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    p: &ProjectInput<'_>,
) -> RepoResult<Project> {
    Ok(sqlx::query_as::<_, Project>(crate::sqlf!(
        "INSERT INTO projects (user_id, area_id, name, slug, description, status, repo_url, links, notes_md)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {PROJECT_COLS}"
    ))
    .bind(user_id)
    .bind(p.area_id)
    .bind(p.name)
    .bind(p.slug)
    .bind(p.description)
    .bind(p.status)
    .bind(p.repo_url)
    .bind(p.links)
    .bind(p.notes_md)
    .fetch_one(db)
    .await?)
}

pub async fn update_project(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    p: &ProjectInput<'_>,
) -> RepoResult<Project> {
    let row = sqlx::query_as::<_, Project>(crate::sqlf!(
        "UPDATE projects SET area_id = $4, name = $5, slug = $6, description = $7, status = $8, repo_url = $9,
            links = $10, notes_md = $11, archived_at = CASE WHEN $12 THEN coalesce(archived_at, now()) ELSE NULL END,
            version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {PROJECT_COLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(p.area_id)
    .bind(p.name)
    .bind(p.slug)
    .bind(p.description)
    .bind(p.status)
    .bind(p.repo_url)
    .bind(p.links)
    .bind(p.notes_md)
    .bind(p.archived)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_project(pool, user_id, id).await)),
    }
}

/// Elemento de actividad reciente (vista "Dev reciente" de Proyectos.base, sin clases/objetivos/finanzas).
#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct ActivityItem {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub title: String,
    pub on_date: Option<NaiveDate>,
    pub status: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Actividad reciente de un proyecto (o de todos si `project_id` es None) a partir de entidades relacionadas.
pub async fn recent_activity(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    project_id: Option<Uuid>,
    limit: i64,
) -> RepoResult<Vec<ActivityItem>> {
    Ok(sqlx::query_as::<_, ActivityItem>(
        "SELECT * FROM (
            SELECT 'task' AS entity_type, id AS entity_id, title, planned_on AS on_date, status, updated_at
              FROM tasks WHERE user_id = $1 AND ($2::uuid IS NULL OR project_id = $2)
            UNION ALL
            SELECT 'note', id, title, NULL::date, kind, updated_at
              FROM notes WHERE user_id = $1 AND ($2::uuid IS NULL OR project_id = $2)
            UNION ALL
            SELECT 'adr', id, title, decided_on, status, updated_at
              FROM adrs WHERE user_id = $1 AND ($2::uuid IS NULL OR project_id = $2)
            UNION ALL
            SELECT 'devlog_session', s.id, coalesce(nullif(s.repo, ''), 'Sesión'), d.on_date, NULL, s.updated_at
              FROM devlog_sessions s JOIN devlogs d ON d.id = s.devlog_id
              WHERE s.user_id = $1 AND ($2::uuid IS NULL OR EXISTS (
                SELECT 1 FROM devlog_session_projects p WHERE p.session_id = s.id AND p.project_id = $2))
            UNION ALL
            SELECT 'content', id, title, publish_on, status, updated_at
              FROM content_pieces WHERE user_id = $1 AND ($2::uuid IS NULL OR project_id = $2)
         ) a ORDER BY updated_at DESC LIMIT $3",
    )
    .bind(user_id)
    .bind(project_id)
    .bind(limit)
    .fetch_all(db)
    .await?)
}

/// Convierte el resultado de releer una fila tras un UPDATE sin efecto en Conflict (existe) o NotFound.
pub fn conflict_or_missing<T: Serialize>(current: RepoResult<T>) -> RepoError {
    match current {
        Ok(row) => RepoError::Conflict(serde_json::to_value(row).unwrap_or_default()),
        Err(e) => e,
    }
}
