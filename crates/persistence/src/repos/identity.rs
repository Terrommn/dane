//! Usuarios, identidades verificadas, sesiones y preferencias.

use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgExecutor, PgPool};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{RepoError, RepoResult};

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub display_name: String,
    pub is_demo: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, ToSchema)]
pub struct Identity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub subject: String,
    pub telegram_user_id: Option<i64>,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct UserSettings {
    pub user_id: Uuid,
    pub locale: String,
    pub timezone: String,
    pub currency: String,
    pub week_start: i16,
    #[schema(value_type = Object)]
    pub modules: serde_json::Value,
    pub devotional_mode: String,
    #[schema(value_type = Vec<Object>)]
    pub devotional_template: serde_json::Value,
    pub max_devotional_failures: Option<i32>,
    #[schema(value_type = Option<String>)]
    pub weight_goal_kg: Option<rust_decimal::Decimal>,
    pub weight_direction: Option<String>,
    pub habit_warn_month: i32,
    pub habit_warn_total: i32,
    pub track_family_loans: bool,
    /// Mensaje de la mañana (check-in guiado); `daily_*` es el cierre de la noche.
    pub morning_enabled: bool,
    #[schema(value_type = String, example = "07:00:00")]
    pub morning_time: NaiveTime,
    /// Minutos de anticipación para avisar de un evento con hora (0 = sin avisos).
    pub event_reminder_minutes: i16,
    pub daily_enabled: bool,
    #[schema(value_type = String, example = "21:00:00")]
    pub daily_time: NaiveTime,
    pub weekly_enabled: bool,
    pub weekly_weekday: i16,
    #[schema(value_type = String)]
    pub weekly_time: NaiveTime,
    pub monthly_enabled: bool,
    #[schema(value_type = String)]
    pub monthly_time: NaiveTime,
    pub reminders_paused: bool,
    pub paused_until: Option<DateTime<Utc>>,
    pub onboarding_completed_at: Option<DateTime<Utc>>,
    pub version: i32,
}

/// Datos de una identidad verificada (Mini App u OIDC) para crear/vincular usuario.
#[derive(Debug, Clone)]
pub struct VerifiedIdentity {
    pub provider: &'static str,
    pub subject: String,
    pub telegram_user_id: Option<i64>,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

pub const DEFAULT_AREAS: [(&str, &str); 4] = [
    ("Personal", "personal"),
    ("Trabajo", "trabajo"),
    ("Negocio", "negocio"),
    ("Escuela", "escuela"),
];

/// Catálogo inicial editable (vault: AGENTS.md / gasto.md / ingreso.md).
pub const DEFAULT_EXPENSE_CATEGORIES: [(&str, &str); 15] = [
    ("Comida", "consumption"),
    ("Gasolina", "consumption"),
    ("Casa", "consumption"),
    ("Salud/gym", "consumption"),
    ("Deporte", "consumption"),
    ("Suscripciones", "consumption"),
    ("Personal/ocio", "consumption"),
    ("Salidas con amigos", "consumption"),
    ("Pareja", "consumption"),
    ("Trabajo", "consumption"),
    ("Inversión", "investment"),
    ("Negocio", "consumption"),
    ("Nómina", "payroll"),
    ("Pago de deuda", "debt_payment"),
    ("Otro", "consumption"),
];
pub const DEFAULT_INCOME_CATEGORIES: [(&str, &str); 5] = [
    ("Sueldo", "earned"),
    ("Negocio", "earned"),
    ("Freelance", "earned"),
    ("Préstamo", "financing"),
    ("Otro", "other_income"),
];

/// Busca el usuario de una identidad verificada; si no existe, lo vincula por ID Telegram verificado o lo crea.
/// La vinculación es por `telegram_user_id` verificado, nunca por username ni por `sub`.
pub async fn upsert_verified_identity(pool: &PgPool, v: &VerifiedIdentity) -> RepoResult<Uuid> {
    let mut tx = pool.begin().await?;
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM identities WHERE provider = $1 AND subject = $2")
            .bind(v.provider)
            .bind(&v.subject)
            .fetch_optional(&mut *tx)
            .await?;
    let user_id = match existing {
        Some(u) => u,
        None => {
            let linked: Option<Uuid> = match v.telegram_user_id {
                Some(tg) => sqlx::query_scalar(
                    "SELECT user_id FROM identities WHERE telegram_user_id = $1 ORDER BY created_at LIMIT 1",
                )
                .bind(tg)
                .fetch_optional(&mut *tx)
                .await?,
                None => None,
            };
            let user_id = match linked {
                Some(u) => u,
                None => {
                    let name = v
                        .first_name
                        .clone()
                        .or_else(|| v.username.clone())
                        .unwrap_or_default();
                    let u: Uuid =
                        sqlx::query_scalar("INSERT INTO users (display_name) VALUES ($1) RETURNING id")
                            .bind(name)
                            .fetch_one(&mut *tx)
                            .await?;
                    bootstrap_user(&mut tx, u).await?;
                    u
                }
            };
            sqlx::query(
                "INSERT INTO identities (user_id, provider, subject, telegram_user_id, username, first_name, last_name)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(user_id)
            .bind(v.provider)
            .bind(&v.subject)
            .bind(v.telegram_user_id)
            .bind(&v.username)
            .bind(&v.first_name)
            .bind(&v.last_name)
            .execute(&mut *tx)
            .await?;
            user_id
        }
    };
    sqlx::query(
        "UPDATE identities SET last_login_at = now(), username = coalesce($3, username),
         first_name = coalesce($4, first_name), last_name = coalesce($5, last_name)
         WHERE provider = $1 AND subject = $2",
    )
    .bind(v.provider)
    .bind(&v.subject)
    .bind(&v.username)
    .bind(&v.first_name)
    .bind(&v.last_name)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(user_id)
}

/// Valores iniciales explícitos del requisito (D-026).
pub async fn bootstrap_user(tx: &mut crate::Tx<'_>, user_id: Uuid) -> RepoResult<()> {
    sqlx::query("INSERT INTO user_settings (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    for (i, (name, slug)) in DEFAULT_AREAS.iter().enumerate() {
        let area_id: Uuid = sqlx::query_scalar(
            "INSERT INTO areas (user_id, name, slug, position) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, slug) DO UPDATE SET name = areas.name RETURNING id",
        )
        .bind(user_id)
        .bind(name)
        .bind(slug)
        .bind(i as i32)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO projects (user_id, area_id, name, slug) VALUES ($1, $2, $3, $4) ON CONFLICT (user_id, slug) DO NOTHING",
        )
        .bind(user_id)
        .bind(area_id)
        .bind(slug)
        .bind(slug)
        .execute(&mut **tx)
        .await?;
    }
    for (i, (name, role)) in DEFAULT_EXPENSE_CATEGORIES.iter().enumerate() {
        sqlx::query(
            "INSERT INTO fin_categories (user_id, name, kind, role, position) VALUES ($1, $2, 'expense', $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(name)
        .bind(role)
        .bind(i as i32)
        .execute(&mut **tx)
        .await?;
    }
    for (i, (name, role)) in DEFAULT_INCOME_CATEGORIES.iter().enumerate() {
        sqlx::query(
            "INSERT INTO fin_categories (user_id, name, kind, role, position) VALUES ($1, $2, 'income', $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(name)
        .bind(role)
        .bind(i as i32)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query("INSERT INTO credit_plans (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn get_user(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<User> {
    sqlx::query_as::<_, User>(
        "SELECT id, display_name, is_demo, created_at FROM users WHERE id = $1 AND disabled_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn identities_of(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<Identity>> {
    Ok(sqlx::query_as::<_, Identity>(
        "SELECT id, user_id, provider, subject, telegram_user_id, username, first_name, last_name
         FROM identities WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// IDs de Telegram verificados del usuario (para comprobar allowlist en cada sesión).
pub async fn telegram_ids_of(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "SELECT telegram_user_id FROM identities WHERE user_id = $1 AND telegram_user_id IS NOT NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn user_by_telegram_id(db: impl PgExecutor<'_>, tg: i64) -> RepoResult<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT user_id FROM identities WHERE telegram_user_id = $1 ORDER BY created_at LIMIT 1",
    )
    .bind(tg)
    .fetch_optional(db)
    .await?)
}

// ---------- Sesiones ----------

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub transport: String,
    pub csrf_hash: Option<Vec<u8>>,
    pub expires_at: DateTime<Utc>,
}

pub async fn create_session(
    db: impl PgExecutor<'_>,
    user_id: Uuid,
    token_hash: &[u8],
    csrf_hash: Option<&[u8]>,
    transport: &str,
    expires_at: DateTime<Utc>,
    user_agent: Option<&str>,
) -> RepoResult<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO sessions (user_id, token_hash, csrf_hash, transport, expires_at, user_agent)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(csrf_hash)
    .bind(transport)
    .bind(expires_at)
    .bind(user_agent)
    .fetch_one(db)
    .await?)
}

/// Sesión vigente (no revocada, no vencida, usuario activo) por hash del token.
pub async fn find_session(
    db: impl PgExecutor<'_>,
    token_hash: &[u8],
    now: DateTime<Utc>,
) -> RepoResult<Option<SessionRow>> {
    Ok(sqlx::query_as::<_, SessionRow>(
        "SELECT s.id, s.user_id, s.transport, s.csrf_hash, s.expires_at
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > $2 AND u.disabled_at IS NULL",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(db)
    .await?)
}

pub async fn touch_session(db: impl PgExecutor<'_>, id: Uuid) -> RepoResult<()> {
    sqlx::query("UPDATE sessions SET last_seen_at = now() WHERE id = $1 AND last_seen_at < now() - interval '1 minute'")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn revoke_session(db: impl PgExecutor<'_>, user_id: Uuid, id: Uuid) -> RepoResult<()> {
    sqlx::query(
        "UPDATE sessions SET revoked_at = now() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn revoke_all_sessions(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<u64> {
    Ok(
        sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
            .bind(user_id)
            .execute(db)
            .await?
            .rows_affected(),
    )
}

// ---------- Preferencias ----------

pub async fn get_settings(db: impl PgExecutor<'_>, user_id: Uuid) -> RepoResult<UserSettings> {
    sqlx::query_as::<_, UserSettings>(
        "SELECT user_id, locale, timezone, currency::text AS currency, week_start, modules, devotional_mode,
                devotional_template, max_devotional_failures, weight_goal_kg, weight_direction, habit_warn_month,
                habit_warn_total, track_family_loans, morning_enabled, morning_time, event_reminder_minutes,
                daily_enabled, daily_time, weekly_enabled, weekly_weekday,
                weekly_time, monthly_enabled, monthly_time, reminders_paused, paused_until, onboarding_completed_at,
                version
         FROM user_settings WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or(RepoError::NotFound)
}

pub async fn save_settings(pool: &PgPool, s: &UserSettings) -> RepoResult<UserSettings> {
    let n = sqlx::query(
        "UPDATE user_settings SET locale = $3, timezone = $4, currency = $5, week_start = $6, modules = $7,
            devotional_mode = $8, devotional_template = $9, max_devotional_failures = $10, weight_goal_kg = $11,
            weight_direction = $12, habit_warn_month = $13, habit_warn_total = $14, track_family_loans = $15,
            daily_enabled = $16, daily_time = $17, weekly_enabled = $18, weekly_weekday = $19, weekly_time = $20,
            monthly_enabled = $21, monthly_time = $22, reminders_paused = $23, paused_until = $24,
            onboarding_completed_at = $25, morning_enabled = $26, morning_time = $27,
            event_reminder_minutes = $28, version = version + 1
         WHERE user_id = $1 AND version = $2",
    )
    .bind(s.user_id)
    .bind(s.version)
    .bind(&s.locale)
    .bind(&s.timezone)
    .bind(&s.currency)
    .bind(s.week_start)
    .bind(&s.modules)
    .bind(&s.devotional_mode)
    .bind(&s.devotional_template)
    .bind(s.max_devotional_failures)
    .bind(s.weight_goal_kg)
    .bind(&s.weight_direction)
    .bind(s.habit_warn_month)
    .bind(s.habit_warn_total)
    .bind(s.track_family_loans)
    .bind(s.daily_enabled)
    .bind(s.daily_time)
    .bind(s.weekly_enabled)
    .bind(s.weekly_weekday)
    .bind(s.weekly_time)
    .bind(s.monthly_enabled)
    .bind(s.monthly_time)
    .bind(s.reminders_paused)
    .bind(s.paused_until)
    .bind(s.onboarding_completed_at)
    .bind(s.morning_enabled)
    .bind(s.morning_time)
    .bind(s.event_reminder_minutes)
    .execute(pool)
    .await?
    .rows_affected();
    let current = get_settings(pool, s.user_id).await?;
    if n == 0 {
        return Err(RepoError::Conflict(
            serde_json::to_value(&current).unwrap_or_default(),
        ));
    }
    Ok(current)
}
