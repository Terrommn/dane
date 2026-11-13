//! Servidor principal: API, autenticación, reglas de negocio, scheduler e integraciones.

pub mod api;
pub mod auth;
pub mod config;
pub mod crypto;
pub mod error;
pub mod http;
pub mod internal;
pub mod state;
pub mod workers;

pub use api::app;
pub use state::AppState;
