//! Acceso a PostgreSQL: pool, migraciones embebidas y repositorios. Toda consulta filtra por `user_id`.

pub mod hooks;
pub mod repos;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};
use std::str::FromStr;
use std::time::Duration;

pub use sqlx;

pub type Tx<'a> = Transaction<'a, Postgres>;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect(url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    let opts = PgConnectOptions::from_str(url)?.application_name("dane-server");
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await
}

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

/// Errores de repositorio. `Conflict` lleva el estado actual para que el cliente resuelva (no se sobrescribe).
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("no encontrado")]
    NotFound,
    #[error("conflicto de versión")]
    Conflict(serde_json::Value),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub type RepoResult<T> = Result<T, RepoError>;

/// SQL con fragmentos constantes interpolados (listas de columnas). Nunca interpolar datos del usuario: usar `bind`.
#[macro_export]
macro_rules! sqlf {
    ($($t:tt)*) => { $crate::sqlx::AssertSqlSafe(format!($($t)*)) };
}
