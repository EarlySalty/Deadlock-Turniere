//! Snowflake-/ID-Helfer: Parsen einmal beim Eintritt, Duplikate entfernen,
//! Idempotency-Keys erzeugen.
//!
//! Snowflakes werden durchgängig als [`u64`] geführt. IDs werden EINMAL beim
//! Eintritt geparst (`parse::<u64>`); ungültige IDs lösen KEINE Panic aus,
//! sondern landen deterministisch in der `failed`-Bucket der jeweiligen
//! Operation (siehe [`crate::notifier`]). Das Python-Original rief `int(id)`
//! ohne `try/except` und ließ einen `ValueError` durchschlagen (Befund
//! discord_notifier.py:208/225/456/499 — "behavior-change"); der Port erhält
//! das *beobachtbare* Verhalten (kein Versand bei ungültiger ID), ohne den
//! Prozess mit einer Panic zu gefährden.

use rand::Rng;

/// Eine Discord-Snowflake als String, wie sie aus DB/Aufrufer kommt.
pub type DiscordId = String;

/// Entfernt Duplikate aus einer ID-Liste unter Erhalt der Reihenfolge und
/// trimmt jeden Wert; leere Strings werden verworfen. Entspricht 1:1
/// `_unique_preserve_order` aus dem Python-Original.
pub fn unique_preserve_order(values: &[String]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result: Vec<String> = Vec::new();
    for raw in values {
        let value = raw.trim().to_string();
        if value.is_empty() || seen.contains(&value) {
            continue;
        }
        seen.insert(value.clone());
        result.push(value);
    }
    result
}

/// Parst eine String-ID als Snowflake. `None`, wenn nicht rein numerisch.
///
/// Bewusst strikt wie `int(...)` in Python: führende/folgende Whitespaces
/// werden — wie schon in [`unique_preserve_order`] — getrimmt, ein nicht
/// numerischer Rest ergibt `None` (statt Panic).
pub fn parse_snowflake(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok()
}

/// Erzeugt einen Idempotency-Key der Form `<prefix>-<8 hex>`. Ersetzt
/// `secrets.token_hex(4)` des Originals (4 Byte = 8 Hex-Zeichen).
pub fn idempotency_key(prefix: &str) -> String {
    let bytes: [u8; 4] = rand::thread_rng().gen();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{prefix}-{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_behaelt_reihenfolge_und_trimmt() {
        let input = vec![
            " 100 ".to_string(),
            "200".to_string(),
            "100".to_string(),
            "".to_string(),
            "  ".to_string(),
            "300".to_string(),
        ];
        assert_eq!(unique_preserve_order(&input), vec!["100", "200", "300"]);
    }

    #[test]
    fn snowflake_parsing() {
        assert_eq!(parse_snowflake("123456789"), Some(123_456_789));
        assert_eq!(parse_snowflake(" 42 "), Some(42));
        assert_eq!(parse_snowflake(""), None);
        assert_eq!(parse_snowflake("nope"), None);
        assert_eq!(parse_snowflake("12.3"), None);
        assert_eq!(parse_snowflake("-5"), None);
    }

    #[test]
    fn idempotency_key_format() {
        let k = idempotency_key("move-1-2");
        assert!(k.starts_with("move-1-2-"));
        let suffix = k.rsplit('-').next().unwrap();
        assert_eq!(suffix.len(), 8);
        assert!(suffix.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
