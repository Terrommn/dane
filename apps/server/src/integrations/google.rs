//! Google Calendar y Gmail (solo lectura) con OAuth Authorization Code + PKCE y refresh token cifrado.
//!
//! Scopes mínimos: `calendar.readonly` y `gmail.readonly` (Drive solo se usa como enlaces; no se pide su scope).
//! * Calendar: calendarios seleccionables; se excluyen festivos/observancias (`#holiday`), contactos, números de semana
//!   y eventos de todo el día (la agenda del vault solo lista eventos con hora). Upsert por
//!   `(calendar_id, provider_event_id)` en una ventana; cancelados o desaparecidos se quitan. Asistentes → sugerencias.
//! * Gmail: `in:sent` acotado por fecha; se ignoran automáticos/newsletters; destinatarios → sugerencias.

use std::collections::HashSet;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use persistence::repos::integrations as repo;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Endpoints, ProviderError, SyncReport, decrypt_json, encrypt_json, net, read_json};
use crate::config::OAuthApp;
use crate::state::AppState;

pub const SCOPES: [&str; 4] = [
    "openid",
    "email",
    "https://www.googleapis.com/auth/calendar.readonly",
    "https://www.googleapis.com/auth/gmail.readonly",
];

const P: &str = "google";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    scope: Option<String>,
    id_token: Option<String>,
}

fn app(state: &AppState) -> Result<&OAuthApp, ProviderError> {
    state.config.google.as_ref().ok_or_else(|| {
        ProviderError::Config("Google no está configurado en el servidor (GOOGLE_CLIENT_ID)".into())
    })
}

/// URL de autorización. Guarda state + PKCE en `oauth_flows` (10 min, un solo uso).
pub async fn authorize_url(
    state: &AppState,
    user_id: Uuid,
    endpoints: &Endpoints,
    redirect_to: &str,
) -> Result<String, ProviderError> {
    let app = app(state)?;
    let (verifier, challenge) = super::pkce_pair();
    let st = crate::crypto::random_token();
    sqlx::query(
        "INSERT INTO oauth_flows (state, purpose, user_id, pkce_verifier, nonce, redirect_to, expires_at)
         VALUES ($1, 'connect_google', $2, $3, '', $4, $5)",
    )
    .bind(&st)
    .bind(user_id)
    .bind(&verifier)
    .bind(redirect_to)
    .bind(state.now() + Duration::minutes(10))
    .execute(&state.pool)
    .await?;
    let mut url =
        url::Url::parse(&endpoints.google_auth).map_err(|e| ProviderError::Config(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("client_id", &app.client_id)
        .append_pair("redirect_uri", &state.config.google_redirect_uri())
        .append_pair("response_type", "code")
        .append_pair("scope", &SCOPES.join(" "))
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent")
        .append_pair("include_granted_scopes", "true")
        .append_pair("state", &st)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

/// Completa el callback: valida el state (un solo uso, del mismo flujo), intercambia el código y guarda tokens cifrados.
/// Devuelve `(user_id, redirect_to)`.
pub async fn complete(
    state: &AppState,
    endpoints: &Endpoints,
    st: &str,
    code: &str,
) -> Result<(Uuid, String), ProviderError> {
    let app = app(state)?;
    let flow: Option<(Option<Uuid>, String, Option<String>)> = sqlx::query_as(
        "DELETE FROM oauth_flows WHERE state = $1 AND purpose = 'connect_google' AND expires_at > $2
         RETURNING user_id, pkce_verifier, redirect_to",
    )
    .bind(st)
    .bind(state.now())
    .fetch_optional(&state.pool)
    .await?;
    let Some((Some(user_id), verifier, redirect_to)) = flow else {
        return Err(ProviderError::Config("state inválido o vencido".into()));
    };
    let resp = state
        .http
        .post(&endpoints.google_token)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &state.config.google_redirect_uri()),
            ("client_id", &app.client_id),
            ("client_secret", &app.client_secret),
            ("code_verifier", &verifier),
        ])
        .send()
        .await
        .map_err(net(P))?;
    let tr: TokenResponse = read_json(P, resp).await.map_err(|e| match e {
        ProviderError::Http { status: 400, .. } => ProviderError::Unauthorized(P),
        other => other,
    })?;
    let scopes: Vec<String> = tr
        .scope
        .clone()
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let tokens = Tokens {
        access_token: tr.access_token,
        refresh_token: tr.refresh_token,
        expires_at: state.now() + Duration::seconds(tr.expires_in.unwrap_or(3600) - 60),
        scope: tr.scope,
    };
    if tokens.refresh_token.is_none() {
        return Err(ProviderError::Config(
            "Google no entregó refresh token; revoca el acceso en tu cuenta y vuelve a conectar".into(),
        ));
    }
    let label = tr.id_token.as_deref().and_then(email_from_id_token);
    let enc = encrypt_json(state, &tokens)?;
    repo::upsert_connected(
        &state.pool,
        user_id,
        P,
        &scopes,
        label.as_deref(),
        &enc,
        &serde_json::json!({}),
    )
    .await?;
    Ok((user_id, redirect_to.unwrap_or_else(|| "/ajustes".into())))
}

/// Solo para mostrar la cuenta conectada; el token viene directo del endpoint de token por TLS (no se usa para autorizar).
fn email_from_id_token(t: &str) -> Option<String> {
    use base64::Engine;
    let payload = t.split('.').nth(1)?;
    let json: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload.trim_end_matches('='))
            .ok()?,
    )
    .ok()?;
    json.get("email")?.as_str().map(str::to_string)
}

/// Token de acceso vigente; refresca si venció y guarda el nuevo cifrado.
pub async fn access_token(
    state: &AppState,
    user_id: Uuid,
    endpoints: &Endpoints,
) -> Result<String, ProviderError> {
    let conn = repo::get(&state.pool, user_id, P)
        .await?
        .ok_or(ProviderError::NotConnected("Google"))?;
    let enc = conn
        .credentials_enc
        .ok_or(ProviderError::NotConnected("Google"))?;
    let mut tokens: Tokens = decrypt_json(state, &enc)?;
    if tokens.expires_at > state.now() {
        return Ok(tokens.access_token);
    }
    let app = app(state)?;
    let refresh = tokens
        .refresh_token
        .clone()
        .ok_or(ProviderError::Unauthorized(P))?;
    let resp = state
        .http
        .post(&endpoints.google_token)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh),
            ("client_id", &app.client_id),
            ("client_secret", &app.client_secret),
        ])
        .send()
        .await
        .map_err(net(P))?;
    let tr: TokenResponse = read_json(P, resp).await.map_err(|e| match e {
        // invalid_grant: refresh token revocado o vencido.
        ProviderError::Http { status: 400, .. } => ProviderError::Unauthorized(P),
        other => other,
    })?;
    tokens.access_token = tr.access_token;
    tokens.expires_at = state.now() + Duration::seconds(tr.expires_in.unwrap_or(3600) - 60);
    if tr.refresh_token.is_some() {
        tokens.refresh_token = tr.refresh_token;
    }
    repo::save_credentials(&state.pool, user_id, P, &encrypt_json(state, &tokens)?).await?;
    Ok(tokens.access_token)
}

/// Revoca en Google (mejor esfuerzo) y borra credenciales locales.
pub async fn revoke(state: &AppState, user_id: Uuid, endpoints: &Endpoints) -> Result<(), ProviderError> {
    if let Some(conn) = repo::get(&state.pool, user_id, P).await? {
        if let Some(enc) = conn.credentials_enc
            && let Ok(tokens) = decrypt_json::<Tokens>(state, &enc)
        {
            let token = tokens.refresh_token.unwrap_or(tokens.access_token);
            if let Err(e) = state
                .http
                .post(&endpoints.google_revoke)
                .form(&[("token", token.as_str())])
                .send()
                .await
            {
                tracing::warn!(error = %e.without_url(), "revocación en Google falló; credenciales locales borradas igualmente");
            }
        }
        repo::disconnect(&state.pool, user_id, P, "revoked").await?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ Calendar

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CalendarInfo {
    pub id: String,
    pub summary: String,
    pub primary: bool,
    /// Calendarios de festivos/observancias/contactos/semana: siempre excluidos.
    pub excluded_by_rule: bool,
    pub selected: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarListPage {
    #[serde(default)]
    items: Vec<CalendarListEntry>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarListEntry {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    primary: bool,
    summary_override: Option<String>,
}

/// Festivos, observancias, cumpleaños de contactos y números de semana nunca entran a la agenda.
pub fn excluded_calendar(id: &str) -> bool {
    let l = id.to_lowercase();
    l.contains("#holiday@")
        || l.contains("#contacts@")
        || l.contains("#weeknum@")
        || l.contains("#other@")
        || l.contains("#weather")
}

fn configured_calendars(config: &serde_json::Value) -> Option<HashSet<String>> {
    config
        .get("calendars")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
}

pub async fn list_calendars(
    state: &AppState,
    user_id: Uuid,
    endpoints: &Endpoints,
) -> Result<Vec<CalendarInfo>, ProviderError> {
    let token = access_token(state, user_id, endpoints).await?;
    let conn = repo::get(&state.pool, user_id, P)
        .await?
        .ok_or(ProviderError::NotConnected("Google"))?;
    let selected = configured_calendars(&conn.config);
    let mut out = Vec::new();
    let mut page: Option<String> = None;
    for _ in 0..20 {
        let mut req = state
            .http
            .get(format!("{}/users/me/calendarList", endpoints.google_calendar))
            .bearer_auth(&token)
            .query(&[("maxResults", "250")]);
        if let Some(p) = &page {
            req = req.query(&[("pageToken", p)]);
        }
        let pg: CalendarListPage = read_json(P, req.send().await.map_err(net(P))?).await?;
        for c in pg.items {
            let excluded = excluded_calendar(&c.id);
            let sel = !excluded && selected.as_ref().map(|s| s.contains(&c.id)).unwrap_or(c.primary);
            out.push(CalendarInfo {
                summary: c.summary_override.unwrap_or(c.summary),
                selected: sel,
                excluded_by_rule: excluded,
                primary: c.primary,
                id: c.id,
            });
        }
        page = pg.next_page_token;
        if page.is_none() {
            break;
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventsPage {
    #[serde(default)]
    items: Vec<Event>,
    next_page_token: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Event {
    id: String,
    status: String,
    summary: Option<String>,
    location: Option<String>,
    html_link: Option<String>,
    start: Option<EventTime>,
    end: Option<EventTime>,
    attendees: Vec<Attendee>,
    event_type: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct EventTime {
    date_time: Option<DateTime<Utc>>,
    date: Option<NaiveDate>,
}

#[derive(Deserialize, Serialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
struct Attendee {
    email: Option<String>,
    display_name: Option<String>,
    #[serde(rename = "self")]
    is_self: bool,
    resource: bool,
    response_status: Option<String>,
}

/// Sincroniza la agenda de `[from, to]` (fechas locales) de los calendarios seleccionados.
pub async fn sync_calendar(
    state: &AppState,
    user_id: Uuid,
    endpoints: &Endpoints,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<SyncReport, ProviderError> {
    let mut rep = SyncReport {
        provider: P.into(),
        kind: "calendar".into(),
        ..Default::default()
    };
    let conn = repo::get(&state.pool, user_id, P)
        .await?
        .ok_or(ProviderError::NotConnected("Google"))?;
    if conn.config.get("calendar_enabled").and_then(|v| v.as_bool()) == Some(false) {
        return Ok(rep);
    }
    let tz = super::user_tz(state, user_id).await;
    let calendars: Vec<String> = match configured_calendars(&conn.config) {
        Some(s) => s.into_iter().filter(|c| !excluded_calendar(c)).collect(),
        None => list_calendars(state, user_id, endpoints)
            .await?
            .into_iter()
            .filter(|c| c.selected)
            .map(|c| c.id)
            .collect(),
    };
    let token = access_token(state, user_id, endpoints).await?;
    let time_min = domain::time::start_of_local_day(from, tz);
    let time_max = domain::time::start_of_local_day(to + Duration::days(1), tz);
    let now = state.now();
    for cal in calendars {
        let mut seen: HashSet<String> = HashSet::new();
        let mut page: Option<String> = None;
        for _ in 0..50 {
            let mut req = state
                .http
                .get(format!(
                    "{}/calendars/{}/events",
                    endpoints.google_calendar,
                    urlencode(&cal)
                ))
                .bearer_auth(&token)
                .query(&[
                    ("timeMin", time_min.to_rfc3339()),
                    ("timeMax", time_max.to_rfc3339()),
                    ("singleEvents", "true".into()),
                    ("showDeleted", "true".into()),
                    ("maxResults", "250".into()),
                ]);
            if let Some(p) = &page {
                req = req.query(&[("pageToken", p)]);
            }
            let pg: EventsPage = read_json(P, req.send().await.map_err(net(P))?).await?;
            for ev in pg.items {
                let start = ev.start.as_ref().and_then(|s| s.date_time);
                if ev.status == "cancelled"
                    || start.is_none()
                    || matches!(
                        ev.event_type.as_deref(),
                        Some("birthday") | Some("workingLocation")
                    )
                {
                    // Cancelado, de todo el día o no-agenda: se quita si existía.
                    rep.removed += sqlx::query("DELETE FROM calendar_events WHERE user_id = $1 AND source = 'google' AND calendar_id = $2 AND provider_event_id = $3")
                        .bind(user_id)
                        .bind(&cal)
                        .bind(&ev.id)
                        .execute(&state.pool)
                        .await?
                        .rows_affected() as i32;
                    if start.is_none() && ev.status != "cancelled" {
                        rep.skipped += 1;
                    }
                    continue;
                }
                let start = start.unwrap();
                let end = ev.end.as_ref().and_then(|e| e.date_time);
                let on_date = domain::time::local_date(start, tz);
                let title = ev.summary.clone().unwrap_or_else(|| "(sin título)".into());
                let attendees: Vec<Attendee> = ev.attendees.iter().filter(|a| !a.resource).cloned().collect();
                sqlx::query(
                    "INSERT INTO calendar_events (user_id, source, calendar_id, provider_event_id, on_date, starts_at, ends_at, all_day, title, location, attendees, status, html_link, synced_at)
                     VALUES ($1, 'google', $2, $3, $4, $5, $6, false, $7, $8, $9, $10, $11, $12)
                     ON CONFLICT (user_id, source, calendar_id, provider_event_id) DO UPDATE SET
                        on_date = EXCLUDED.on_date, starts_at = EXCLUDED.starts_at, ends_at = EXCLUDED.ends_at, title = EXCLUDED.title,
                        location = EXCLUDED.location, attendees = EXCLUDED.attendees, status = EXCLUDED.status, html_link = EXCLUDED.html_link,
                        synced_at = EXCLUDED.synced_at",
                )
                .bind(user_id)
                .bind(&cal)
                .bind(&ev.id)
                .bind(on_date)
                .bind(start)
                .bind(end)
                .bind(&title)
                .bind(&ev.location)
                .bind(serde_json::to_value(&attendees).unwrap_or_default())
                .bind(&ev.status)
                .bind(&ev.html_link)
                .bind(now)
                .execute(&state.pool)
                .await?;
                rep.upserted += 1;
                seen.insert(ev.id.clone());
                // Un evento no prueba asistencia ni conversación: solo se proponen, y solo de eventos ya iniciados.
                if start <= now {
                    for a in attendees
                        .iter()
                        .filter(|a| !a.is_self && a.response_status.as_deref() != Some("declined"))
                    {
                        if let Some(email) = &a.email
                            && super::suggest_contact(
                                state,
                                user_id,
                                email,
                                a.display_name.as_deref(),
                                on_date,
                                &title,
                                "calendar",
                                &format!("{}:{}", ev.id, on_date),
                            )
                            .await?
                        {
                            rep.suggestions += 1;
                        }
                    }
                }
            }
            page = pg.next_page_token;
            if page.is_none() {
                break;
            }
        }
        // Eventos que desaparecieron de la ventana (borrados o movidos fuera).
        let seen_vec: Vec<String> = seen.into_iter().collect();
        rep.removed += sqlx::query(
            "DELETE FROM calendar_events WHERE user_id = $1 AND source = 'google' AND calendar_id = $2
             AND on_date BETWEEN $3 AND $4 AND NOT (provider_event_id = ANY($5))",
        )
        .bind(user_id)
        .bind(&cal)
        .bind(from)
        .bind(to)
        .bind(&seen_vec)
        .execute(&state.pool)
        .await?
        .rows_affected() as i32;
    }
    Ok(rep)
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ------------------------------------------------------------------------------------------------ Gmail

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageList {
    #[serde(default)]
    messages: Vec<MessageRef>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct MessageRef {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Message {
    id: String,
    internal_date: Option<String>,
    payload: Option<Payload>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(default)]
    headers: Vec<Header>,
}

#[derive(Deserialize)]
struct Header {
    name: String,
    value: String,
}

/// Correo automático o masivo que no representa una conversación personal.
pub fn is_automated(headers: &[(String, String)]) -> bool {
    let get = |n: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(n))
            .map(|(_, v)| v.to_lowercase())
    };
    if get("List-Unsubscribe").is_some() || get("List-Id").is_some() {
        return true;
    }
    if get("Auto-Submitted").is_some_and(|v| v.trim() != "no") {
        return true;
    }
    if get("Precedence").is_some_and(|v| matches!(v.trim(), "bulk" | "list" | "junk")) {
        return true;
    }
    false
}

pub fn is_automated_address(email: &str) -> bool {
    let l = email.to_lowercase();
    [
        "noreply",
        "no-reply",
        "donotreply",
        "do-not-reply",
        "notifications@",
        "notification@",
        "mailer-daemon",
        "postmaster@",
        "bounce",
    ]
    .iter()
    .any(|p| l.contains(p))
}

/// `Ana López <ana@x.com>, b@y.com` → [(Some("Ana López"), "ana@x.com"), (None, "b@y.com")]
pub fn parse_addresses(s: &str) -> Vec<(Option<String>, String)> {
    s.split(',')
        .filter_map(|part| {
            let p = part.trim();
            if let (Some(i), Some(j)) = (p.find('<'), p.rfind('>')) {
                let name = p[..i].trim().trim_matches('"').trim();
                let email = p[i + 1..j].trim().to_lowercase();
                email
                    .contains('@')
                    .then(|| (Some(name.to_string()).filter(|n| !n.is_empty()), email))
            } else {
                p.contains('@').then(|| (None, p.to_lowercase()))
            }
        })
        .collect()
}

pub async fn sync_gmail(
    state: &AppState,
    user_id: Uuid,
    endpoints: &Endpoints,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<SyncReport, ProviderError> {
    let mut rep = SyncReport {
        provider: P.into(),
        kind: "gmail".into(),
        ..Default::default()
    };
    let conn = repo::get(&state.pool, user_id, P)
        .await?
        .ok_or(ProviderError::NotConnected("Google"))?;
    if conn.config.get("gmail_enabled").and_then(|v| v.as_bool()) == Some(false) {
        return Ok(rep);
    }
    let tz = super::user_tz(state, user_id).await;
    let token = access_token(state, user_id, endpoints).await?;
    let q = format!(
        "in:sent after:{} before:{}",
        from.format("%Y/%m/%d"),
        (to + Duration::days(1)).format("%Y/%m/%d")
    );
    let mut page: Option<String> = None;
    let mut ids = Vec::new();
    for _ in 0..10 {
        let mut req = state
            .http
            .get(format!("{}/users/me/messages", endpoints.gmail))
            .bearer_auth(&token)
            .query(&[("q", q.as_str()), ("maxResults", "100")]);
        if let Some(p) = &page {
            req = req.query(&[("pageToken", p)]);
        }
        let pg: MessageList = read_json(P, req.send().await.map_err(net(P))?).await?;
        ids.extend(pg.messages.into_iter().map(|m| m.id));
        page = pg.next_page_token;
        if page.is_none() || ids.len() >= 500 {
            break;
        }
    }
    for id in ids {
        let msg: Message = read_json(
            P,
            state
                .http
                .get(format!("{}/users/me/messages/{id}", endpoints.gmail))
                .bearer_auth(&token)
                .query(&[
                    ("format", "metadata"),
                    ("metadataHeaders", "To"),
                    ("metadataHeaders", "Cc"),
                    ("metadataHeaders", "Subject"),
                    ("metadataHeaders", "List-Unsubscribe"),
                    ("metadataHeaders", "List-Id"),
                    ("metadataHeaders", "Auto-Submitted"),
                    ("metadataHeaders", "Precedence"),
                ])
                .send()
                .await
                .map_err(net(P))?,
        )
        .await?;
        let headers: Vec<(String, String)> = msg
            .payload
            .map(|p| p.headers.into_iter().map(|h| (h.name, h.value)).collect())
            .unwrap_or_default();
        if is_automated(&headers) {
            rep.skipped += 1;
            continue;
        }
        let date = msg
            .internal_date
            .and_then(|ms| ms.parse::<i64>().ok())
            .and_then(DateTime::from_timestamp_millis)
            .map(|d| domain::time::local_date(d, tz))
            .unwrap_or(to);
        let subject = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Subject"))
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let recipients: Vec<(Option<String>, String)> = headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("To") || k.eq_ignore_ascii_case("Cc"))
            .flat_map(|(_, v)| parse_addresses(v))
            .collect();
        for (name, email) in recipients {
            if is_automated_address(&email) {
                continue;
            }
            if super::suggest_contact(
                state,
                user_id,
                &email,
                name.as_deref(),
                date,
                &subject,
                "gmail",
                &msg.id,
            )
            .await?
            {
                rep.suggestions += 1;
            }
        }
        rep.upserted += 0;
    }
    Ok(rep)
}

/// Sincroniza Calendar (ayer → +7 días) y Gmail (últimos 2 días) y registra el resultado.
pub async fn sync_all(
    state: &AppState,
    user_id: Uuid,
    window: Option<(NaiveDate, NaiveDate)>,
) -> Vec<SyncReport> {
    let conn = repo::get(&state.pool, user_id, P).await.ok().flatten();
    let endpoints = Endpoints::resolve(state, conn.as_ref().map(|c| &c.config));
    let today = super::today(state, user_id).await;
    let (from, to) = window.unwrap_or((today - Duration::days(1), today + Duration::days(7)));
    let mut out = Vec::new();
    let started = state.now();
    let cal = sync_calendar(state, user_id, &endpoints, from, to).await;
    super::record_outcome(state, user_id, P, "calendar", started, &cal).await;
    let fatal = matches!(&cal, Err(e) if e.is_fatal());
    out.push(cal.unwrap_or_else(|e| SyncReport {
        provider: P.into(),
        kind: "calendar".into(),
        error: Some(e.to_string()),
        ..Default::default()
    }));
    if !fatal {
        let started = state.now();
        let (gfrom, gto) = window.unwrap_or((today - Duration::days(2), today));
        let gm = sync_gmail(state, user_id, &endpoints, gfrom, gto).await;
        super::record_outcome(state, user_id, P, "gmail", started, &gm).await;
        out.push(gm.unwrap_or_else(|e| SyncReport {
            provider: P.into(),
            kind: "gmail".into(),
            error: Some(e.to_string()),
            ..Default::default()
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holiday_calendars_excluded() {
        assert!(excluded_calendar(
            "en.mexican#holiday@group.v.calendar.google.com"
        ));
        assert!(excluded_calendar(
            "addressbook#contacts@group.v.calendar.google.com"
        ));
        assert!(!excluded_calendar("neil@gmail.com"));
    }

    #[test]
    fn automated_mail_detection() {
        assert!(is_automated(&[("List-Unsubscribe".into(), "<mailto:x>".into())]));
        assert!(is_automated(&[(
            "Auto-Submitted".into(),
            "auto-generated".into()
        )]));
        assert!(is_automated(&[("Precedence".into(), "bulk".into())]));
        assert!(!is_automated(&[
            ("Auto-Submitted".into(), "no".into()),
            ("Subject".into(), "Hola".into())
        ]));
        assert!(is_automated_address("no-reply@github.com"));
        assert_eq!(
            parse_addresses("\"Ana López\" <Ana@X.com>, b@y.com"),
            vec![
                (Some("Ana López".into()), "ana@x.com".into()),
                (None, "b@y.com".into())
            ]
        );
    }
}
