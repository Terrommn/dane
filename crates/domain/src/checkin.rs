//! Reglas del check-in diario y del devocional.
//!
//! * Sin dato ≠ cero ≠ "no": los campos ausentes permanecen `NULL` (sin registrar).
//! * Peso opcional en kg (0 < kg < 500, máx. 2 decimales), energía 1–5.
//! * Un devocional **con contenido real** (pasaje o alguna sección o fila versículo/acción no vacía) puede completar el
//!   hábito vinculado; uno vacío no.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerseAction {
    #[serde(default)]
    pub verse: String,
    #[serde(default)]
    pub action: String,
}

/// Valida la tabla versículo/acción (máx. 20 filas, 500 caracteres por celda) y descarta filas vacías.
pub fn normalize_verse_actions(rows: Vec<VerseAction>) -> Result<Vec<VerseAction>, DomainError> {
    if rows.len() > 20 {
        return Err(DomainError::invalid("verse_actions", "máximo 20 filas"));
    }
    let out: Vec<VerseAction> = rows
        .into_iter()
        .map(|r| VerseAction {
            verse: r.verse.trim().to_string(),
            action: r.action.trim().to_string(),
        })
        .filter(|r| !r.verse.is_empty() || !r.action.is_empty())
        .collect();
    if out
        .iter()
        .any(|r| r.verse.chars().count() > 500 || r.action.chars().count() > 500)
    {
        return Err(DomainError::invalid(
            "verse_actions",
            "máximo 500 caracteres por celda",
        ));
    }
    Ok(out)
}

pub struct DevotionalContent<'a> {
    pub passage: &'a str,
    pub verse_actions: &'a [VerseAction],
    pub sections: &'a [&'a str],
}

pub fn devotional_has_content(d: &DevotionalContent<'_>) -> bool {
    !d.passage.trim().is_empty()
        || d.verse_actions
            .iter()
            .any(|v| !v.verse.trim().is_empty() || !v.action.trim().is_empty())
        || d.sections.iter().any(|s| !s.trim().is_empty())
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

    #[test]
    fn devotional_content_detection() {
        let empty = DevotionalContent {
            passage: " ",
            verse_actions: &[VerseAction::default()],
            sections: &["", "  "],
        };
        assert!(!devotional_has_content(&empty));
        let va = [VerseAction {
            verse: "Juan 15:5".into(),
            action: String::new(),
        }];
        assert!(devotional_has_content(&DevotionalContent {
            passage: "",
            verse_actions: &va,
            sections: &[]
        }));
        assert!(devotional_has_content(&DevotionalContent {
            passage: "",
            verse_actions: &[],
            sections: &["Aprendí a confiar"]
        }));
        let rows = normalize_verse_actions(vec![
            VerseAction::default(),
            VerseAction {
                verse: " a ".into(),
                action: "b".into(),
            },
        ])
        .unwrap();
        assert_eq!(
            rows,
            vec![VerseAction {
                verse: "a".into(),
                action: "b".into()
            }]
        );
    }
}
