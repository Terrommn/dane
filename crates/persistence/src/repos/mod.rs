//! Repositorios por dominio. Convención: funciones libres que reciben `impl PgExecutor` (pool o transacción) y
//! `user_id` explícito; las actualizaciones usan `version` (control optimista) y devuelvan `RepoError::Conflict` con la
//! fila actual cuando no coincide.

pub mod areas;
pub mod identity;
