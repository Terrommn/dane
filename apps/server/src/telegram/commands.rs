//! Interpretación de updates y comandos (reglas de negocio en el server, D-012). Todo ocurre dentro de la
//! transacción del procesador: efectos + respuesta encolada + `processed` se confirman juntos.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use contracts::OutboundMessage;
use persistence::Tx;
use persistence::repos::{jobs, telegram};
use serde_json::Value;
use uuid::Uuid;

use super::messages::{app_button, human_date, message};
use crate::state::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub chat_id: i64,
    pub chat_type: String,
    pub from_id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub text: Option<String>,
}

/// Extrae el mensaje relevante (`message`). Otros tipos de update (ediciones, canales…) se ignoran.
pub fn parse_incoming(payload: &Value) -> Option<Incoming> {
    let m = payload.get("message")?;
    let chat = m.get("chat")?;
    let from = m.get("from")?;
    if from.get("is_bot").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    Some(Incoming {
        chat_id: chat.get("id")?.as_i64()?,
        chat_type: s(chat, "type")?,
        from_id: from.get("id")?.as_i64()?,
        username: s(from, "username"),
        first_name: s(from, "first_name"),
        last_name: s(from, "last_name"),
        text: s(m, "text"),
    })
}

/// `/Weekly-Review@dane_bot  texto` → (`weekly_review`, `texto`). Acepta los nombres con guion del vault.
pub fn parse_command(text: &str) -> Option<(String, String)> {
    let t = text.trim_start();
    let rest = t.strip_prefix('/')?;
    let (head, args) = match rest.find(char::is_whitespace) {
        Some(i) => (&rest[..i], rest[i..].trim()),
        None => (rest, ""),
    };
    let name = head
        .split('@')
        .next()
        .unwrap_or_default()
        .to_lowercase()
        .replace('-', "_");
    if name.is_empty() {
        return None;
    }
    Some((name, args.to_string()))
}

/// `hoy`, `ayer`, `mañana` o `AAAA-MM-DD`.
pub fn parse_day(arg: &str, today: NaiveDate) -> Option<NaiveDate> {
    match domain::text::fold(arg.trim()).as_str() {
        "" | "hoy" => Some(today),
        "ayer" => Some(today - Duration::days(1)),
        "manana" => Some(today + Duration::days(1)),
        other => NaiveDate::parse_from_str(other, "%Y-%m-%d").ok(),
    }
}

const CAPTURE_INTENTS: [(&str, &str); 7] = [
    ("daily", "tu día"),
    ("dump", "tus pendientes"),
    ("gasto", "tus gastos"),
    ("ingreso", "tus ingresos"),
    ("objetivos", "tus objetivos"),
    ("weekly_review", "tu revisión semanal"),
    ("log_session", "tu sesión de código"),
];

pub async fn handle_update(
    state: &AppState,
    tx: &mut Tx<'_>,
    update_id: i64,
    payload: &Value,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let Some(m) = parse_incoming(payload) else {
        return Ok(());
    };
    if m.chat_type != "private" {
        // Solo chats privados; en grupos no se responde nada.
        return Ok(());
    }
    let key = format!("upd:{update_id}");
    let command = m.text.as_deref().and_then(parse_command);
    if !state.config.is_allowed(m.from_id) {
        tracing::info!(update_id, "update de usuario fuera de allowlist ignorado");
        if matches!(&command, Some((c, _)) if c == "start") {
            let msg = message(m.chat_id, "Este bot es privado.", vec![]);
            jobs::enqueue(
                &mut **tx,
                None,
                "reply",
                &key,
                &serde_json::to_value(msg)?,
                now,
                Some(now + Duration::hours(1)),
                3,
            )
            .await?;
        }
        return Ok(());
    }
    let user_id = ensure_user(tx, &m).await?;
    telegram::upsert_chat(tx, user_id, m.chat_id).await?;
    let tz_name: String = sqlx::query_scalar("SELECT timezone FROM user_settings WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&mut **tx)
        .await?;
    let tz = domain::time::parse_tz(&tz_name).unwrap_or(chrono_tz::America::Monterrey);
    let today = domain::time::local_date(now, tz);
    tracing::info!(
        update_id,
        command = command.as_ref().map(|c| c.0.as_str()).unwrap_or("-"),
        "procesando update"
    );

    let reply = match command {
        Some((name, args)) => command_reply(state, tx, user_id, &m, &name, &args, today, now).await?,
        None => match m.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(text) => {
                // Texto libre = captura rápida al Inbox (se procesa en la revisión).
                sqlx::query(
                    "INSERT INTO inbox_items (user_id, text, source, client_id, captured_at) VALUES ($1, $2, 'telegram', $3, $4)
                     ON CONFLICT (user_id, client_id) DO NOTHING",
                )
                .bind(user_id)
                .bind(text.chars().take(4000).collect::<String>())
                .bind(&key)
                .bind(now)
                .execute(&mut **tx)
                .await?;
                message(
                    m.chat_id,
                    "📥 Guardado en tu Inbox.",
                    vec![vec![app_button(state, "Ver Inbox", "/inbox")]],
                )
            }
            None => message(m.chat_id, "Por ahora solo entiendo texto. Usa /help.", vec![]),
        },
    };
    jobs::enqueue(
        &mut **tx,
        Some(user_id),
        "reply",
        &key,
        &serde_json::to_value(reply)?,
        now,
        Some(now + Duration::hours(1)),
        jobs::DEFAULT_MAX_ATTEMPTS,
    )
    .await?;
    Ok(())
}

/// Busca el usuario de la identidad Telegram (verificada por el canal del bot) o lo crea con los valores iniciales.
async fn ensure_user(tx: &mut Tx<'_>, m: &Incoming) -> anyhow::Result<Uuid> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('tg-user:' || $1::text))")
        .bind(m.from_id)
        .execute(&mut **tx)
        .await?;
    if let Some(u) = persistence::repos::identity::user_by_telegram_id(&mut **tx, m.from_id).await? {
        return Ok(u);
    }
    let name = m
        .first_name
        .clone()
        .or_else(|| m.username.clone())
        .unwrap_or_default();
    let user_id: Uuid = sqlx::query_scalar("INSERT INTO users (display_name) VALUES ($1) RETURNING id")
        .bind(name)
        .fetch_one(&mut **tx)
        .await?;
    persistence::repos::identity::bootstrap_user(tx, user_id).await?;
    sqlx::query(
        "INSERT INTO identities (user_id, provider, subject, telegram_user_id, username, first_name, last_name)
         VALUES ($1, 'telegram', $2, $3, $4, $5, $6) ON CONFLICT (provider, subject) DO NOTHING",
    )
    .bind(user_id)
    .bind(m.from_id.to_string())
    .bind(m.from_id)
    .bind(&m.username)
    .bind(&m.first_name)
    .bind(&m.last_name)
    .execute(&mut **tx)
    .await?;
    Ok(user_id)
}

#[allow(clippy::too_many_arguments)]
async fn command_reply(
    state: &AppState,
    tx: &mut Tx<'_>,
    user_id: Uuid,
    m: &Incoming,
    name: &str,
    args: &str,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> anyhow::Result<OutboundMessage> {
    let chat = m.chat_id;
    let checkin = |d: NaiveDate| {
        message(
            chat,
            format!("📝 Check-in del {}.", human_date(d)),
            vec![vec![app_button(
                state,
                "Abrir check-in",
                &format!("/hoy?date={d}"),
            )]],
        )
    };
    let has_text = !args.trim().is_empty();
    Ok(match name {
        "start" => message(
            chat,
            format!(
                "¡Hola{}! Aquí recibirás tus recordatorios. Los detalles se editan en la app.",
                m.first_name
                    .as_deref()
                    .map(|n| format!(", {n}"))
                    .unwrap_or_default()
            ),
            vec![
                vec![app_button(
                    state,
                    "Abrir check-in de hoy",
                    &format!("/hoy?date={today}"),
                )],
                vec![app_button(state, "Dashboard", "/dashboard")],
            ],
        ),
        "checkin" => match parse_day(args, today) {
            Some(d) => checkin(d),
            None => message(
                chat,
                "Fecha no válida. Usa /checkin, /checkin ayer o /checkin 2026-11-12.",
                vec![],
            ),
        },
        "daily" if !has_text => checkin(today),
        "daily" if parse_day(args, today).is_some() => checkin(parse_day(args, today).unwrap_or(today)),
        "dashboard" => message(
            chat,
            "📊 Tu dashboard:",
            vec![vec![app_button(state, "Abrir dashboard", "/dashboard")]],
        ),
        "objetivos" if !has_text => message(
            chat,
            "🎯 Objetivos del mes y de la semana.",
            vec![vec![
                app_button(state, "Ver objetivos", "/objetivos"),
                app_button(state, "Planificar", "/objetivos/planificar"),
            ]],
        ),
        "settings" => {
            let (paused, until, daily_on, daily_time): (bool, Option<DateTime<Utc>>, bool, chrono::NaiveTime) = sqlx::query_as(
                "SELECT reminders_paused, paused_until, daily_enabled, daily_time FROM user_settings WHERE user_id = $1",
            )
            .bind(user_id)
            .fetch_one(&mut **tx)
            .await?;
            let estado = if paused || until.is_some_and(|u| u > now) {
                "pausados".to_string()
            } else if daily_on {
                format!("activos, diario a las {}", daily_time.format("%H:%M"))
            } else {
                "diario desactivado".to_string()
            };
            message(
                chat,
                format!("⚙️ Recordatorios: {estado}."),
                vec![vec![app_button(state, "Abrir ajustes", "/ajustes")]],
            )
        }
        "help" => {
            let mut text = String::from("Comandos:\n");
            for (cmd, desc, vault) in contracts::COMMANDS {
                text.push_str(&format!("/{cmd} — {desc}"));
                if !vault.is_empty() && vault != cmd {
                    text.push_str(&format!(" (= {vault})"));
                }
                text.push('\n');
            }
            text.push_str("Escribe cualquier texto para guardarlo en tu Inbox.");
            message(chat, text, vec![])
        }
        "pause" => {
            let days = args.trim().parse::<i64>().ok().filter(|d| (1..=365).contains(d));
            match days {
                Some(d) => {
                    sqlx::query("UPDATE user_settings SET paused_until = $2, version = version + 1 WHERE user_id = $1")
                        .bind(user_id)
                        .bind(now + Duration::days(d))
                        .execute(&mut **tx)
                        .await?
                }
                None => {
                    sqlx::query("UPDATE user_settings SET reminders_paused = true, version = version + 1 WHERE user_id = $1")
                        .bind(user_id)
                        .execute(&mut **tx)
                        .await?
                }
            };
            jobs::cancel_pending_reminders(&mut **tx, user_id, "recordatorios pausados").await?;
            let text = match days {
                Some(d) => {
                    format!("⏸ Recordatorios pausados {d} día(s). Usa /resume para reanudarlos antes.")
                }
                None => "⏸ Recordatorios pausados. Usa /resume para reanudarlos.".into(),
            };
            message(chat, text, vec![])
        }
        "resume" => {
            sqlx::query("UPDATE user_settings SET reminders_paused = false, paused_until = NULL, version = version + 1 WHERE user_id = $1")
                .bind(user_id)
                .execute(&mut **tx)
                .await?;
            message(chat, "▶️ Recordatorios reanudados.", vec![])
        }
        intent if CAPTURE_INTENTS.iter().any(|(i, _)| *i == intent) => {
            let label = CAPTURE_INTENTS
                .iter()
                .find(|(i, _)| *i == intent)
                .map(|(_, l)| *l)
                .unwrap_or("tu captura");
            if has_text {
                let id: Uuid = sqlx::query_scalar(
                    "INSERT INTO capture_proposals (user_id, input_text, source, intent, reference_date)
                     VALUES ($1, $2, 'telegram', $3, $4) RETURNING id",
                )
                .bind(user_id)
                .bind(args.chars().take(8000).collect::<String>())
                .bind(intent)
                .bind(today)
                .fetch_one(&mut **tx)
                .await?;
                message(
                    chat,
                    format!("Recibí {label}. Revisa y confirma en la app antes de guardar."),
                    vec![vec![app_button(state, "Revisar", &format!("/captura/{id}"))]],
                )
            } else {
                let (text, path) = form_for(intent, today);
                message(chat, text, vec![vec![app_button(state, "Abrir", &path)]])
            }
        }
        _ => message(chat, "No conozco ese comando. Usa /help.", vec![]),
    })
}

/// Formulario completo de cada comando cuando no se manda texto.
fn form_for(intent: &str, today: NaiveDate) -> (String, String) {
    match intent {
        "dump" => (
            "Escribe tus pendientes después del comando, o captúralos en la app:".into(),
            "/inbox".into(),
        ),
        "gasto" => (
            "Registra el gasto en la app (o escribe: /gasto 250 gasolina con débito):".into(),
            "/finanzas/movimientos?nuevo=gasto".into(),
        ),
        "ingreso" => (
            "Registra el ingreso en la app (o escribe: /ingreso hoy 15000 sueldo):".into(),
            "/finanzas/movimientos?nuevo=ingreso".into(),
        ),
        "weekly_review" => (
            "🗓️ Revisión semanal:".into(),
            format!("/semanal?week={}", domain::time::iso_week_key(today)),
        ),
        "log_session" => (
            "Pega el resumen de tu sesión después del comando, o regístrala en la app:".into(),
            "/dev".into(),
        ),
        "objetivos" => ("🎯 Objetivos:".into(), "/objetivos/planificar".into()),
        _ => (
            format!("📝 Check-in del {}:", human_date(today)),
            format!("/hoy?date={today}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_with_bot_suffix_and_vault_hyphens() {
        assert_eq!(parse_command("/start"), Some(("start".into(), "".into())));
        assert_eq!(
            parse_command("/Gasto@dane_bot 250 gasolina"),
            Some(("gasto".into(), "250 gasolina".into()))
        );
        assert_eq!(
            parse_command("/weekly-review"),
            Some(("weekly_review".into(), "".into()))
        );
        assert_eq!(
            parse_command("/log-session repo dane"),
            Some(("log_session".into(), "repo dane".into()))
        );
        assert_eq!(parse_command("hola"), None);
        assert_eq!(parse_command("/"), None);
    }

    #[test]
    fn parses_relative_days() {
        let t = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        assert_eq!(parse_day("ayer", t), NaiveDate::from_ymd_opt(2026, 11, 11));
        assert_eq!(parse_day("Mañana", t), NaiveDate::from_ymd_opt(2026, 11, 13));
        assert_eq!(parse_day("2026-11-01", t), NaiveDate::from_ymd_opt(2026, 11, 1));
        assert_eq!(parse_day("", t), Some(t));
        assert_eq!(parse_day("pronto", t), None);
    }

    #[test]
    fn ignores_bots_and_non_messages() {
        assert!(parse_incoming(&serde_json::json!({"update_id": 1, "edited_message": {}})).is_none());
        let bot = serde_json::json!({"message": {"chat": {"id": 1, "type": "private"}, "from": {"id": 1, "is_bot": true}, "text": "/start"}});
        assert!(parse_incoming(&bot).is_none());
    }
}
