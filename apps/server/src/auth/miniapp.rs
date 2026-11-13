//! Validación de `initData` de Telegram Mini Apps (HMAC-SHA256).
//!
//! secret = HMAC_SHA256(key = "WebAppData", msg = bot_token)
//! hash   = hex(HMAC_SHA256(key = secret, msg = data_check_string))
//! data_check_string = campos recibidos excepto `hash`, ordenados por clave, `k=v` unidos por `\n`.
//! Nunca se usa `initDataUnsafe` ni un user_id enviado por el cliente.

use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniAppUser {
    pub id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub auth_date: DateTime<Utc>,
    pub start_param: Option<String>,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum InitDataError {
    #[error("initData vacío o mal formado")]
    Malformed,
    #[error("firma inválida")]
    BadSignature,
    #[error("initData vencido")]
    Expired,
    #[error("initData con fecha futura")]
    FromFuture,
    #[error("initData sin usuario")]
    NoUser,
}

#[derive(Deserialize)]
struct TgUser {
    id: i64,
    username: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
}

pub fn validate_init_data(
    init_data: &str,
    bot_token: &str,
    now: DateTime<Utc>,
    max_age_secs: i64,
) -> Result<MiniAppUser, InitDataError> {
    if init_data.is_empty() || init_data.len() > 8192 {
        return Err(InitDataError::Malformed);
    }
    let mut pairs: Vec<(String, String)> = url::form_urlencoded::parse(init_data.as_bytes())
        .into_owned()
        .collect();
    let hash_pos = pairs
        .iter()
        .position(|(k, _)| k == "hash")
        .ok_or(InitDataError::Malformed)?;
    let (_, hash_hex) = pairs.remove(hash_pos);
    if pairs.iter().any(|(k, _)| k == "hash") {
        return Err(InitDataError::Malformed);
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let check_string = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut secret = <HmacSha256 as KeyInit>::new_from_slice(b"WebAppData").expect("hmac key");
    secret.update(bot_token.as_bytes());
    let secret_key = secret.finalize().into_bytes();
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(&secret_key).expect("hmac key");
    mac.update(check_string.as_bytes());
    let expected = hex::decode(&hash_hex).map_err(|_| InitDataError::BadSignature)?;
    mac.verify_slice(&expected)
        .map_err(|_| InitDataError::BadSignature)?;

    let get = |k: &str| pairs.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.as_str());
    let auth_ts: i64 = get("auth_date")
        .and_then(|v| v.parse().ok())
        .ok_or(InitDataError::Malformed)?;
    let auth_date = DateTime::from_timestamp(auth_ts, 0).ok_or(InitDataError::Malformed)?;
    let age = (now - auth_date).num_seconds();
    if age > max_age_secs {
        return Err(InitDataError::Expired);
    }
    if age < -60 {
        return Err(InitDataError::FromFuture);
    }
    let user: TgUser = serde_json::from_str(get("user").ok_or(InitDataError::NoUser)?)
        .map_err(|_| InitDataError::Malformed)?;
    Ok(MiniAppUser {
        id: user.id,
        username: user.username,
        first_name: user.first_name,
        last_name: user.last_name,
        auth_date,
        start_param: get("start_param").map(str::to_string),
    })
}

/// Firma `initData` como lo haría Telegram (solo pruebas y fixtures).
pub fn sign_init_data_for_tests(fields: &[(&str, &str)], bot_token: &str) -> String {
    let mut sorted: Vec<_> = fields.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let check = sorted
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut secret = <HmacSha256 as KeyInit>::new_from_slice(b"WebAppData").unwrap();
    secret.update(bot_token.as_bytes());
    let key = secret.finalize().into_bytes();
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(&key).unwrap();
    mac.update(check.as_bytes());
    let hash = hex::encode(mac.finalize().into_bytes());
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in fields {
        ser.append_pair(k, v);
    }
    ser.append_pair("hash", &hash);
    ser.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "123456:ABC-DEF";

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn signed(auth_date: i64, token: &str) -> String {
        sign_init_data_for_tests(
            &[
                ("auth_date", &auth_date.to_string()),
                ("query_id", "AAE"),
                ("user", r#"{"id":111,"first_name":"Neil","username":"neil"}"#),
                ("signature", "abc"),
            ],
            token,
        )
    }

    #[test]
    fn accepts_valid_fresh_init_data() {
        let u = validate_init_data(&signed(1_800_000_000 - 30, TOKEN), TOKEN, now(), 3600).unwrap();
        assert_eq!(u.id, 111);
        assert_eq!(u.username.as_deref(), Some("neil"));
    }

    #[test]
    fn rejects_wrong_bot_token() {
        let data = signed(1_800_000_000, "999:OTHER");
        assert_eq!(
            validate_init_data(&data, TOKEN, now(), 3600),
            Err(InitDataError::BadSignature)
        );
    }

    #[test]
    fn rejects_tampered_user() {
        let data = signed(1_800_000_000, TOKEN).replace("%22id%22%3A111", "%22id%22%3A222");
        assert_eq!(
            validate_init_data(&data, TOKEN, now(), 3600),
            Err(InitDataError::BadSignature)
        );
    }

    #[test]
    fn rejects_stale_and_future() {
        assert_eq!(
            validate_init_data(&signed(1_800_000_000 - 7200, TOKEN), TOKEN, now(), 3600),
            Err(InitDataError::Expired)
        );
        assert_eq!(
            validate_init_data(&signed(1_800_000_000 + 3600, TOKEN), TOKEN, now(), 3600),
            Err(InitDataError::FromFuture)
        );
    }

    #[test]
    fn rejects_missing_hash_and_garbage() {
        assert_eq!(
            validate_init_data("user=x&auth_date=1", TOKEN, now(), 3600),
            Err(InitDataError::Malformed)
        );
        assert_eq!(
            validate_init_data("", TOKEN, now(), 3600),
            Err(InitDataError::Malformed)
        );
        assert_eq!(
            validate_init_data("hash=zz", TOKEN, now(), 3600),
            Err(InitDataError::BadSignature)
        );
    }
}
