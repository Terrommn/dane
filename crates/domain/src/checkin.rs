//! Reglas del check-in diario.
//!
//! * Sin dato ≠ cero ≠ "no": los campos ausentes permanecen `NULL` (sin registrar).
//! * Peso opcional en kg (0 < kg < 500, máx. 2 decimales), energía 1–5.

use rust_decimal::Decimal;

use crate::DomainError;

pub fn validate_energy(e: i16) -> Result<(), DomainError> {
    if (1..=5).contains(&e) {
        Ok(())
    } else {
        Err(DomainError::invalid("energy", "la energía va de 1 a 5"))
    }
}

pub fn validate_weight(kg: Decimal) -> Result<(), DomainError> {
    if kg <= Decimal::ZERO || kg >= Decimal::from(500) {
        return Err(DomainError::invalid("weight_kg", "peso fuera de rango"));
    }
    if kg.scale() > 2 {
        return Err(DomainError::invalid("weight_kg", "máximo dos decimales"));
    }
    Ok(())
}

pub fn validate_peace(p: i16) -> Result<(), DomainError> {
    if (1..=5).contains(&p) {
        Ok(())
    } else {
        Err(DomainError::invalid("peace", "la paz va de 1 a 5"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn ranges() {
        assert!(validate_energy(0).is_err() && validate_energy(6).is_err() && validate_energy(3).is_ok());
        assert!(validate_weight(Decimal::from_str("72.45").unwrap()).is_ok());
        assert!(validate_weight(Decimal::from_str("72.451").unwrap()).is_err());
        assert!(validate_weight(Decimal::ZERO).is_err());
        assert!(validate_peace(5).is_ok() && validate_peace(0).is_err());
    }
}
