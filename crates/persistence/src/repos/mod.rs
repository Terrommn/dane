//! Repositorios por dominio. Convención: funciones libres que reciben `impl PgExecutor` (pool o transacción) y
//! `user_id` explícito; las actualizaciones usan `version` (control optimista) y devuelvan `RepoError::Conflict` con la
//! fila actual cuando no coincide.

pub mod agenda;
pub mod areas;
pub mod captures;
pub mod checkins;
pub mod content;
pub mod dashboard;
pub mod dev;
pub mod drawings;
pub mod finance;
pub mod goals;
pub mod habits;
pub mod identity;
pub mod imports;
pub mod inbox;
pub mod integrations;
pub mod jobs;
pub mod notes;
pub mod people;
pub mod readings;
pub mod school;
pub mod tasks;
pub mod telegram;
pub mod weekly;
