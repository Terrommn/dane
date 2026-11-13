//! Construcción de mensajes breves y botones hacia la app.

use chrono::{Datelike, NaiveDate};
use contracts::{Button, OutboundMessage};

use crate::state::AppState;

/// Botón a una ruta de la app. Telegram exige HTTPS para `web_app`; con un origen local (desarrollo) se usa un
/// enlace normal que abre el navegador.
pub fn app_button(state: &AppState, text: &str, path: &str) -> Button {
    let url = state
        .config
        .public_base_url
        .join(path)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| path.to_string());
    if state.config.public_base_url.scheme() == "https" {
        Button::WebApp {
            text: text.into(),
            url,
        }
    } else {
        Button::Url {
            text: text.into(),
            url,
        }
    }
}

pub fn message(chat_id: i64, text: impl Into<String>, buttons: Vec<Vec<Button>>) -> OutboundMessage {
    OutboundMessage {
        chat_id,
        text: text.into(),
        buttons,
    }
}

const MONTHS: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];
const WEEKDAYS: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];

/// "jueves 12 de noviembre".
pub fn human_date(d: NaiveDate) -> String {
    format!(
        "{} {} de {}",
        WEEKDAYS[d.weekday().num_days_from_monday() as usize],
        d.day(),
        MONTHS[d.month0() as usize]
    )
}

pub fn month_name(d: NaiveDate) -> &'static str {
    MONTHS[d.month0() as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_dates() {
        assert_eq!(
            human_date(NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()),
            "jueves 12 de noviembre"
        );
    }
}
