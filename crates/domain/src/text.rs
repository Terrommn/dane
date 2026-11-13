//! Utilidades de texto: normalización sin acentos, slugs y validación de URLs externas.

use unicode_normalization::UnicodeNormalization;

use crate::DomainError;

/// Minúsculas sin diacríticos (para comparar alias, nombres y búsquedas).
pub fn fold(s: &str) -> String {
    s.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

/// Slug ASCII `a-z0-9-` a partir de un nombre.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in fold(s).chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { "item".into() } else { out }
}

/// Acepta solo URLs http(s) absolutas (enlaces a medios, repos, referencias). Nunca `javascript:` ni `data:`.
pub fn validate_external_url(field: &str, s: &str) -> Result<String, DomainError> {
    let t = s.trim();
    let lower = t.to_ascii_lowercase();
    let ok = (lower.starts_with("https://") || lower.starts_with("http://"))
        && t.len() <= 2048
        && !t.chars().any(|c| c.is_whitespace() || c.is_control());
    if ok {
        Ok(t.to_string())
    } else {
        Err(DomainError::invalid(
            field,
            "URL inválida: debe empezar con http:// o https://",
        ))
    }
}

/// Recorta y valida longitud de un texto obligatorio.
pub fn required(field: &str, s: &str, max: usize) -> Result<String, DomainError> {
    let t = s.trim();
    if t.is_empty() {
        return Err(DomainError::invalid(field, "es obligatorio"));
    }
    if t.chars().count() > max {
        return Err(DomainError::invalid(field, format!("máximo {max} caracteres")));
    }
    Ok(t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_accents() {
        assert_eq!(fold("Canción ÁRBOL ñ"), "cancion arbol n");
        assert_eq!(slugify("Salud/gym y Deporte"), "salud-gym-y-deporte");
        assert_eq!(slugify("  "), "item");
    }

    #[test]
    fn rejects_dangerous_urls() {
        assert!(validate_external_url("u", "javascript:alert(1)").is_err());
        assert!(validate_external_url("u", "data:text/html,x").is_err());
        assert!(validate_external_url("u", "https://drive.google.com/x y").is_err());
        assert_eq!(
            validate_external_url("u", " https://a.b/c ").unwrap(),
            "https://a.b/c"
        );
    }
}
