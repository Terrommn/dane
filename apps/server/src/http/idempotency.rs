//! Idempotencia de mutaciones (T21). Con header `Idempotency-Key`:
//! * primera vez: se reserva la clave, se ejecuta y se guarda la respuesta (salvo 5xx, que libera la clave);
//! * repetición con el mismo cuerpo: se devuelve la respuesta guardada con `Idempotent-Replayed: true`;
//! * misma clave con otro cuerpo: 422 `idempotency_mismatch`;
//! * repetición mientras la primera sigue en curso: 409 `in_progress`.

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::auth::AuthUser;
use crate::crypto::sha256;
use crate::error::ApiError;
use crate::state::AppState;

pub const HEADER: &str = "idempotency-key";
const MAX_BODY: usize = 60 * 1024 * 1024;
const MAX_STORED: usize = 1024 * 1024;

pub async fn layer(State(state): State<AppState>, req: Request, next: Next) -> Result<Response, ApiError> {
    let is_mutation = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let key = req
        .headers()
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let user = req.extensions().get::<AuthUser>().copied();
    let (Some(key), Some(user), true) = (key, user, is_mutation) else {
        return Ok(next.run(req).await);
    };
    if !(8..=200).contains(&key.len()) {
        return Err(ApiError::bad_request(
            "Idempotency-Key debe tener entre 8 y 200 caracteres",
        ));
    }
    let method = req.method().to_string();
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let (parts, body) = req.into_parts();
    let bytes = to_bytes(body, MAX_BODY).await.map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "Cuerpo demasiado grande",
        )
    })?;
    let hash = sha256(&[method.as_bytes(), b"\n", path.as_bytes(), b"\n", &bytes].concat());

    let inserted = sqlx::query(
        "INSERT INTO idempotency_keys (user_id, key, method, path, request_hash) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_id, key) DO NOTHING",
    )
    .bind(user.user_id)
    .bind(&key)
    .bind(&method)
    .bind(&path)
    .bind(&hash)
    .execute(&state.pool)
    .await?
    .rows_affected()
        == 1;

    if !inserted {
        let row: Option<(Vec<u8>, Option<i32>, Option<serde_json::Value>)> = sqlx::query_as(
            "SELECT request_hash, status_code, response_body FROM idempotency_keys WHERE user_id = $1 AND key = $2",
        )
        .bind(user.user_id)
        .bind(&key)
        .fetch_optional(&state.pool)
        .await?;
        let Some((stored_hash, status, body)) = row else {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "in_progress",
                "Reintenta en un momento",
            ));
        };
        if stored_hash != hash {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "idempotency_mismatch",
                "La clave de idempotencia ya se usó con otro contenido",
            ));
        }
        let Some(status) = status else {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "in_progress",
                "La operación sigue en curso; reintenta en un momento",
            ));
        };
        let status = StatusCode::from_u16(status as u16).unwrap_or(StatusCode::OK);
        let mut resp = match body {
            Some(json) => (status, axum::Json(json)).into_response(),
            None => status.into_response(),
        };
        resp.headers_mut()
            .insert("idempotent-replayed", HeaderValue::from_static("true"));
        return Ok(resp);
    }

    let resp = next.run(Request::from_parts(parts, Body::from(bytes))).await;
    let status = resp.status();
    if status.is_server_error() {
        sqlx::query("DELETE FROM idempotency_keys WHERE user_id = $1 AND key = $2")
            .bind(user.user_id)
            .bind(&key)
            .execute(&state.pool)
            .await?;
        return Ok(resp);
    }
    let (rparts, rbody) = resp.into_parts();
    let rbytes = to_bytes(rbody, MAX_STORED * 8)
        .await
        .map_err(|_| ApiError::internal())?;
    let json: Option<serde_json::Value> = if rbytes.is_empty() || rbytes.len() > MAX_STORED {
        None
    } else {
        serde_json::from_slice(&rbytes).ok()
    };
    sqlx::query(
        "UPDATE idempotency_keys SET status_code = $3, response_body = $4 WHERE user_id = $1 AND key = $2",
    )
    .bind(user.user_id)
    .bind(&key)
    .bind(status.as_u16() as i32)
    .bind(json)
    .execute(&state.pool)
    .await?;
    Ok(Response::from_parts(rparts, Body::from(rbytes)))
}
