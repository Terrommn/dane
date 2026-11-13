//! Propietario: agente B-modules. Devlog diario con sesiones y ADRs numerados por proyecto.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::repos::areas::conflict_or_missing;
use crate::{RepoError, RepoResult, Tx};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Devlog {
    pub id: Uuid,
    pub on_date: NaiveDate,
    pub summary_md: String,
    pub version: i32,
    pub session_count: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct DevSession {
    pub id: Uuid,
    pub devlog_id: Uuid,
    pub repo: String,
    pub branch: String,
    #[schema(value_type = Option<String>, example = "18:30:00")]
    pub at_time: Option<NaiveTime>,
    pub changes_md: String,
    pub bugs_md: String,
    pub decisions_md: String,
    pub next_steps_md: String,
    pub raw_text: Option<String>,
    pub position: i32,
    pub version: i32,
    pub project_ids: Vec<Uuid>,
    pub updated_at: DateTime<Utc>,
}

const SCOLS: &str = "s.id, s.devlog_id, s.repo, s.branch, s.at_time, s.changes_md, s.bugs_md, s.decisions_md, s.next_steps_md,
    s.raw_text, s.position, s.version,
    coalesce((SELECT array_agg(p.project_id) FROM devlog_session_projects p WHERE p.session_id = s.id), '{}') AS project_ids,
    s.updated_at";

pub async fn list_devlogs(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> RepoResult<Vec<Devlog>> {
    Ok(sqlx::query_as::<_, Devlog>(
        "SELECT d.id, d.on_date, d.summary_md, d.version,
                (SELECT count(*) FROM devlog_sessions s WHERE s.devlog_id = d.id) AS session_count, d.updated_at
         FROM devlogs d WHERE d.user_id = $1 AND ($2::date IS NULL OR d.on_date >= $2) AND ($3::date IS NULL OR d.on_date <= $3)
         ORDER BY d.on_date DESC LIMIT 400",
    )
    .bind(user_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?)
}

/// Devlog del día, creándolo si no existe (un registro por usuario y fecha).
pub async fn ensure_devlog(tx: &mut Tx<'_>, user_id: Uuid, date: NaiveDate) -> RepoResult<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO devlogs (user_id, on_date) VALUES ($1, $2)
         ON CONFLICT (user_id, on_date) DO UPDATE SET on_date = EXCLUDED.on_date RETURNING id",
    )
    .bind(user_id)
    .bind(date)
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn get_devlog(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    date: NaiveDate,
) -> RepoResult<Option<Devlog>> {
    Ok(sqlx::query_as::<_, Devlog>(
        "SELECT d.id, d.on_date, d.summary_md, d.version,
                (SELECT count(*) FROM devlog_sessions s WHERE s.devlog_id = d.id) AS session_count, d.updated_at
         FROM devlogs d WHERE d.user_id = $1 AND d.on_date = $2",
    )
    .bind(user_id)
    .bind(date)
    .fetch_optional(db)
    .await?)
}

pub async fn set_summary(
    pool: &PgPool,
    user_id: Uuid,
    date: NaiveDate,
    version: Option<i32>,
    summary: &str,
) -> RepoResult<Devlog> {
    let mut tx = pool.begin().await?;
    let id = ensure_devlog(&mut tx, user_id, date).await?;
    let n = sqlx::query("UPDATE devlogs SET summary_md = $3, version = version + 1 WHERE user_id = $1 AND id = $2 AND ($4::int IS NULL OR version = $4)")
        .bind(user_id)
        .bind(id)
        .bind(summary)
        .bind(version)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(conflict_or_missing(
            get_devlog(&mut *tx, user_id, date)
                .await
                .and_then(|d| d.ok_or(RepoError::NotFound)),
        ));
    }
    tx.commit().await?;
    get_devlog(pool, user_id, date).await?.ok_or(RepoError::NotFound)
}

pub async fn sessions(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    devlog_id: Uuid,
) -> RepoResult<Vec<DevSession>> {
    Ok(sqlx::query_as::<_, DevSession>(crate::sqlf!(
        "SELECT {SCOLS} FROM devlog_sessions s WHERE s.user_id = $1 AND s.devlog_id = $2 ORDER BY s.at_time NULLS LAST, s.position, s.created_at"
    ))
    .bind(user_id)
    .bind(devlog_id)
    .fetch_all(db)
    .await?)
}

pub async fn get_session(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<DevSession> {
    sqlx::query_as::<_, DevSession>(crate::sqlf!(
        "SELECT {SCOLS} FROM devlog_sessions s WHERE s.user_id = $1 AND s.id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct SessionInput<'a> {
    pub repo: &'a str,
    pub branch: &'a str,
    pub at_time: Option<NaiveTime>,
    pub changes_md: &'a str,
    pub bugs_md: &'a str,
    pub decisions_md: &'a str,
    pub next_steps_md: &'a str,
    pub raw_text: Option<&'a str>,
    pub project_ids: &'a [Uuid],
}

async fn set_projects(tx: &mut Tx<'_>, user_id: Uuid, session: Uuid, projects: &[Uuid]) -> RepoResult<()> {
    sqlx::query("DELETE FROM devlog_session_projects WHERE user_id = $1 AND session_id = $2")
        .bind(user_id)
        .bind(session)
        .execute(&mut **tx)
        .await?;
    for p in projects {
        sqlx::query("INSERT INTO devlog_session_projects (user_id, session_id, project_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(session)
            .bind(p)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn add_session(
    tx: &mut Tx<'_>,
    user_id: Uuid,
    date: NaiveDate,
    s: &SessionInput<'_>,
) -> RepoResult<Uuid> {
    let devlog = ensure_devlog(tx, user_id, date).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO devlog_sessions (user_id, devlog_id, repo, branch, at_time, changes_md, bugs_md, decisions_md, next_steps_md, raw_text, position)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, (SELECT coalesce(max(position) + 1, 0) FROM devlog_sessions WHERE devlog_id = $2))
         RETURNING id",
    )
    .bind(user_id)
    .bind(devlog)
    .bind(s.repo)
    .bind(s.branch)
    .bind(s.at_time)
    .bind(s.changes_md)
    .bind(s.bugs_md)
    .bind(s.decisions_md)
    .bind(s.next_steps_md)
    .bind(s.raw_text)
    .fetch_one(&mut **tx)
    .await?;
    set_projects(tx, user_id, id, s.project_ids).await?;
    Ok(id)
}

pub async fn update_session(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    s: &SessionInput<'_>,
) -> RepoResult<DevSession> {
    let mut tx = pool.begin().await?;
    let n = sqlx::query(
        "UPDATE devlog_sessions SET repo = $4, branch = $5, at_time = $6, changes_md = $7, bugs_md = $8, decisions_md = $9,
            next_steps_md = $10, version = version + 1 WHERE user_id = $1 AND id = $2 AND version = $3",
    )
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(s.repo)
    .bind(s.branch)
    .bind(s.at_time)
    .bind(s.changes_md)
    .bind(s.bugs_md)
    .bind(s.decisions_md)
    .bind(s.next_steps_md)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(conflict_or_missing(get_session(&mut *tx, user_id, id).await));
    }
    set_projects(&mut tx, user_id, id, s.project_ids).await?;
    tx.commit().await?;
    get_session(pool, user_id, id).await
}

pub async fn delete_session(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<()> {
    let n = sqlx::query("DELETE FROM devlog_sessions WHERE user_id = $1 AND id = $2")
        .bind(user_id)
        .bind(id)
        .execute(db)
        .await?
        .rows_affected();
    if n == 0 { Err(RepoError::NotFound) } else { Ok(()) }
}

// ---------- ADRs ----------

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Adr {
    pub id: Uuid,
    pub project_id: Uuid,
    /// Número por proyecto (ADR-0001…).
    pub number: i32,
    pub title: String,
    /// `proposed | accepted | superseded`
    pub status: String,
    pub superseded_by_id: Option<Uuid>,
    pub decided_on: NaiveDate,
    pub context_md: String,
    pub decision_md: String,
    pub alternatives_md: String,
    pub consequences_md: String,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

const ACOLS: &str = "id, project_id, number, title, status, superseded_by_id, decided_on, context_md, decision_md, alternatives_md,
    consequences_md, version, updated_at";

pub async fn list_adrs(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    project_id: Option<Uuid>,
    status: Option<&str>,
) -> RepoResult<Vec<Adr>> {
    Ok(sqlx::query_as::<_, Adr>(crate::sqlf!(
        "SELECT {ACOLS} FROM adrs WHERE user_id = $1 AND ($2::uuid IS NULL OR project_id = $2) AND ($3::text IS NULL OR status = $3)
         ORDER BY project_id, number"
    ))
    .bind(user_id)
    .bind(project_id)
    .bind(status)
    .fetch_all(db)
    .await?)
}

pub async fn get_adr(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<Adr> {
    sqlx::query_as::<_, Adr>(crate::sqlf!(
        "SELECT {ACOLS} FROM adrs WHERE user_id = $1 AND id = $2"
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub struct AdrInput<'a> {
    pub title: &'a str,
    pub decided_on: NaiveDate,
    pub context_md: &'a str,
    pub decision_md: &'a str,
    pub alternatives_md: &'a str,
    pub consequences_md: &'a str,
}

/// Crea un ADR con el siguiente número libre del proyecto. Se serializa con un lock por proyecto para evitar
/// números duplicados en creaciones concurrentes (además del UNIQUE).
pub async fn create_adr(pool: &PgPool, user_id: Uuid, project_id: Uuid, a: &AdrInput<'_>) -> RepoResult<Adr> {
    let mut tx = pool.begin().await?;
    // Valida propiedad del proyecto y bloquea su fila.
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM projects WHERE user_id = $1 AND id = $2 FOR UPDATE")
            .bind(user_id)
            .bind(project_id)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        return Err(RepoError::NotFound);
    }
    let row = sqlx::query_as::<_, Adr>(crate::sqlf!(
        "INSERT INTO adrs (user_id, project_id, number, title, decided_on, context_md, decision_md, alternatives_md, consequences_md)
         VALUES ($1, $2, (SELECT coalesce(max(number), 0) + 1 FROM adrs WHERE user_id = $1 AND project_id = $2),
                 $3, $4, $5, $6, $7, $8) RETURNING {ACOLS}"
    ))
    .bind(user_id)
    .bind(project_id)
    .bind(a.title)
    .bind(a.decided_on)
    .bind(a.context_md)
    .bind(a.decision_md)
    .bind(a.alternatives_md)
    .bind(a.consequences_md)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn update_adr(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    version: i32,
    a: &AdrInput<'_>,
    status: &str,
    superseded_by: Option<Uuid>,
) -> RepoResult<Adr> {
    let row = sqlx::query_as::<_, Adr>(crate::sqlf!(
        "UPDATE adrs SET title = $4, decided_on = $5, context_md = $6, decision_md = $7, alternatives_md = $8,
            consequences_md = $9, status = $10, superseded_by_id = $11, version = version + 1
         WHERE user_id = $1 AND id = $2 AND version = $3 RETURNING {ACOLS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(version)
    .bind(a.title)
    .bind(a.decided_on)
    .bind(a.context_md)
    .bind(a.decision_md)
    .bind(a.alternatives_md)
    .bind(a.consequences_md)
    .bind(status)
    .bind(superseded_by)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => Ok(r),
        None => Err(conflict_or_missing(get_adr(pool, user_id, id).await)),
    }
}
