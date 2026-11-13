//! Cliente de la API interna del server (reqwest). Único canal del bot hacia los datos.

use std::time::Duration;

use contracts::{
    AckRequest, AckResponse, ClaimRequest, ClaimResponse, ClaimedJob, UpdateAccepted, UpdateIngest,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct ServerApi {
    http: reqwest::Client,
    base: url::Url,
    token: String,
    worker_id: String,
}

#[derive(Debug)]
pub enum ApiError {
    /// El lease ya no es de este worker (otro lo reclamó tras vencer).
    StaleLease,
    Status(u16),
    Transport(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::StaleLease => write!(f, "lease obsoleto"),
            ApiError::Status(s) => write!(f, "server respondió {s}"),
            ApiError::Transport(e) => write!(f, "error de transporte: {e}"),
        }
    }
}
impl std::error::Error for ApiError {}

impl ServerApi {
    pub fn new(base: url::Url, token: String, worker_id: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("cliente HTTP");
        Self {
            http,
            base,
            token,
            worker_id,
        }
    }

    async fn post<B: serde::Serialize, R: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, ApiError> {
        let url = self
            .base
            .join(path)
            .map_err(|e| ApiError::Transport(e.to_string()))?;
        let resp = self
            .http
            .post(url)
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| ApiError::Transport(e.without_url().to_string()))?;
        let status = resp.status().as_u16();
        if status == 409 {
            return Err(ApiError::StaleLease);
        }
        if !(200..300).contains(&status) {
            return Err(ApiError::Status(status));
        }
        resp.json::<R>()
            .await
            .map_err(|e| ApiError::Transport(e.without_url().to_string()))
    }

    /// Entrega durable de un update. `Ok` significa que el server ya lo confirmó en su base de datos.
    pub async fn ingest(
        &self,
        update_id: i64,
        payload: serde_json::Value,
    ) -> Result<UpdateAccepted, ApiError> {
        self.post(contracts::PATH_UPDATES, &UpdateIngest { update_id, payload })
            .await
    }

    pub async fn claim(&self, limit: u32, lease_secs: u32) -> Result<Vec<ClaimedJob>, ApiError> {
        let r: ClaimResponse = self
            .post(
                contracts::PATH_CLAIM,
                &ClaimRequest {
                    worker_id: self.worker_id.clone(),
                    limit,
                    lease_secs,
                },
            )
            .await?;
        Ok(r.jobs)
    }

    pub async fn ack(&self, id: Uuid, req: &AckRequest) -> Result<AckResponse, ApiError> {
        self.post(&contracts::path_ack(id), req).await
    }
}
