//! Dinero exacto en centavos (D-009). Nunca `f64` para cálculos monetarios.

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Currency([u8; 3]);

impl Currency {
    pub const MXN: Currency = Currency(*b"MXN");
    pub const USD: Currency = Currency(*b"USD");

    pub fn parse(s: &str) -> Result<Self, DomainError> {
        let t = s.trim().to_ascii_uppercase();
        let b = t.as_bytes();
        if b.len() == 3 && b.iter().all(u8::is_ascii_uppercase) {
            Ok(Currency([b[0], b[1], b[2]]))
        } else {
            Err(DomainError::invalid(
                "currency",
                format!("moneda ISO inválida: {s}"),
            ))
        }
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("ascii")
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Cantidad en centavos con moneda. Suma solo entre la misma moneda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub cents: i64,
    pub currency: Currency,
}

impl Money {
    pub fn new(cents: i64, currency: Currency) -> Self {
        Self { cents, currency }
    }
    pub fn mxn(cents: i64) -> Self {
        Self::new(cents, Currency::MXN)
    }
    pub fn checked_add(self, other: Money) -> Result<Money, DomainError> {
        if self.currency != other.currency {
            return Err(DomainError::invalid("currency", "no se suman monedas distintas"));
        }
        let cents = self
            .cents
            .checked_add(other.cents)
            .ok_or_else(|| DomainError::invalid("amount", "desbordamiento"))?;
        Ok(Money::new(cents, self.currency))
    }
}

/// Convierte texto como `$1,250.50`, `1250.5` o `1 250` a centavos. Rechaza más de 2 decimales y negativos.
pub fn parse_amount_to_cents(s: &str) -> Result<i64, DomainError> {
    let cleaned: String = s
        .trim()
        .chars()
        .filter(|c| !matches!(c, '$' | ',' | ' ' | '\u{a0}'))
        .collect();
    let err = || DomainError::invalid("amount", format!("monto inválido: {s}"));
    if cleaned.is_empty() || cleaned.starts_with('-') {
        return Err(err());
    }
    let dec = Decimal::from_str(&cleaned).map_err(|_| err())?;
    if dec.scale() > 2 {
        return Err(DomainError::invalid("amount", "máximo dos decimales"));
    }
    (dec * Decimal::from(100)).trunc().to_i64().ok_or_else(err)
}

/// Formato es-MX: `$1,250.50` (o `−$…` si es negativo).
pub fn format_cents(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    let (int, frac) = (abs / 100, abs % 100);
    let digits = int.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}${grouped}.{frac:02}", if neg { "−" } else { "" })
}

/// Conversión explícita: `original` × `rate` → centavos de la moneda destino, redondeo bancario a centavo.
pub fn convert_cents(original_cents: i64, rate: Decimal) -> Result<i64, DomainError> {
    if rate <= Decimal::ZERO {
        return Err(DomainError::invalid("fx_rate", "la tasa debe ser positiva"));
    }
    (Decimal::from(original_cents) * rate)
        .round_dp_with_strategy(0, rust_decimal::RoundingStrategy::MidpointNearestEven)
        .to_i64()
        .ok_or_else(|| DomainError::invalid("amount", "desbordamiento"))
}

/// `part / whole` en porcentaje entero redondeado, `None` si `whole` es 0.
pub fn pct(part: i64, whole: i64) -> Option<i64> {
    (whole != 0).then(|| ((part as i128 * 100 + (whole as i128 / 2)) / whole as i128) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_amounts_exactly() {
        assert_eq!(parse_amount_to_cents("$1,250").unwrap(), 125_000);
        assert_eq!(parse_amount_to_cents("0.1").unwrap(), 10);
        assert_eq!(parse_amount_to_cents("19.99").unwrap(), 1_999);
        assert!(parse_amount_to_cents("1.999").is_err());
        assert!(parse_amount_to_cents("-5").is_err());
        assert!(parse_amount_to_cents("abc").is_err());
    }

    #[test]
    fn no_float_drift() {
        // 0.1 + 0.2 en centavos es exacto.
        let a = Money::mxn(parse_amount_to_cents("0.1").unwrap());
        let b = Money::mxn(parse_amount_to_cents("0.2").unwrap());
        assert_eq!(a.checked_add(b).unwrap().cents, 30);
    }

    #[test]
    fn refuses_mixed_currency_sum() {
        assert!(
            Money::mxn(100)
                .checked_add(Money::new(100, Currency::USD))
                .is_err()
        );
    }

    #[test]
    fn formats_es_mx() {
        assert_eq!(format_cents(125_050), "$1,250.50");
        assert_eq!(format_cents(-5), "−$0.05");
        assert_eq!(format_cents(100_000_000), "$1,000,000.00");
    }

    #[test]
    fn converts_with_explicit_rate() {
        let rate = Decimal::from_str("17.2345").unwrap();
        assert_eq!(convert_cents(10_000, rate).unwrap(), 172_345);
        assert!(convert_cents(1, Decimal::ZERO).is_err());
        assert_eq!(pct(1, 3), Some(33));
        assert_eq!(pct(5, 0), None);
    }
}
