//! Endpoints internos para el bot (montados bajo `/internal/v1`, protegidos por `INTERNAL_API_TOKEN`).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Duration;
use contracts::{
    AckOutcome, AckRequest, AckResponse, ClaimRequest, ClaimResponse, ClaimedJob, OutboundMessage,
    UpdateAccepted, UpdateIngest,
};
use persistence::repos::{jobs, telegram};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/telegram/updates", post(ingest_update))
        .route("/jobs/claim", post(claim))
        .route("/jobs/{id}/ack", post(ack))
}

/// Aceptación durable: responde 200 solo después del commit. Duplicados no alteran el estado existente.
async fn ingest_update(
    State(s): State<AppState>,
    Json(b): Json<UpdateIngest>,
) -> ApiResult<Json<UpdateAccepted>> {
    if b.update_id < 0 || !b.payload.is_object() {
        return Err(ApiError::bad_request("update inválido"));
    }
    if b.payload.get("update_id").and_then(|v| v.as_i64()) != Some(b.update_id) {
        return Err(ApiError::bad_request("update_id no coincide con el payload"));
    }
    let duplicate = telegram::ingest(&s.pool, b.update_id, &b.payload, s.now()).await?;
    Ok(Json(UpdateAccepted {
        update_id: b.update_id,
        duplicate,
    }))
}

async fn claim(State(s): State<AppState>, Json(b): Json<ClaimRequest>) -> ApiResult<Json<ClaimResponse>> {
    let limit = b.limit.clamp(1, 50) as i64;
    let lease = Duration::seconds(b.lease_secs.clamp(10, 600) as i64);
    let worker: String = b.worker_id.chars().take(100).collect();
    let now = s.now();
    let rows = jobs::claim(&s.pool, &worker, limit, lease, now).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        match serde_json::from_value::<OutboundMessage>(r.payload.clone()) {
            Ok(message) => out.push(ClaimedJob {
                id: r.id,
                lease_token: r.lease_token,
                kind: r.kind,
                attempt: r.attempts,
                message,
            }),
            Err(e) => {
                tracing::error!(job_id = %r.id, error = %e, "payload de job inválido; se marca fallido");
                let _ = jobs::ack(
                    &s.pool,
                    r.id,
                    r.lease_token,
                    jobs::AckKind::Failed {
                        error: "payload inválido".into(),
                        chat_blocked: false,
                    },
                    now,
                )
                .await;
            }
        }
    }
    Ok(Json(ClaimResponse { jobs: out }))
}

async fn ack(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    Json(b): Json<AckRequest>,
) -> ApiResult<Json<AckResponse>> {
    let kind = match b.outcome {
        AckOutcome::Sent { telegram_message_id } => jobs::AckKind::Sent { telegram_message_id },
        AckOutcome::Retry {
            retry_after_secs,
            error,
        } => jobs::AckKind::Retry {
            retry_after: retry_after_secs.map(|s| Duration::seconds(s.min(86_400) as i64)),
            error,
        },
        AckOutcome::Failed { error, chat_blocked } => jobs::AckKind::Failed { error, chat_blocked },
    };
    match jobs::ack(&s.pool, id, b.lease_token, kind, s.now()).await {
        Ok(o) => Ok(Json(AckResponse {
            status: o.status,
            next_attempt_at: o.next_attempt_at,
            replayed: o.replayed,
        })),
        Err(jobs::AckError::NotFound) => Err(ApiError::not_found()),
        Err(jobs::AckError::StaleLease) => Err(ApiError::new(
            StatusCode::CONFLICT,
            "stale_lease",
            "El lease ya no pertenece a este worker",
        )),
        Err(jobs::AckError::Db(e)) => Err(e.into()),
    }
}
