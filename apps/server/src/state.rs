use std::num::NonZeroU32;
use std::sync::Arc;

use domain::time::Clock;
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use sqlx::PgPool;

use crate::config::Config;
use crate::crypto::Cipher;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub clock: Arc<dyn Clock>,
    pub http: reqwest::Client,
    pub cipher: Cipher,
    /// Límite de intentos de autenticación por IP/clave (10 por minuto, ráfaga 10).
    pub auth_limiter: Arc<DefaultKeyedRateLimiter<String>>,
    pub oidc_cache: Arc<crate::auth::oidc::MetadataCache>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config, clock: Arc<dyn Clock>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent("dane-server/0.1")
            .build()
            .expect("cliente HTTP");
        let cipher = Cipher::new(&config.encryption_key);
        Self {
            pool,
            config: Arc::new(config),
            clock,
            http,
            cipher,
            auth_limiter: Arc::new(RateLimiter::keyed(Quota::per_minute(
                NonZeroU32::new(10).unwrap(),
            ))),
            oidc_cache: Arc::new(Default::default()),
        }
    }

    pub fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.clock.now()
    }
}
