//! Zeit-Fälligkeit und Reminder-Offsets — reine Logik (DB-frei).
//!
//! Der zentrale PG-Port liest Zeitpunkte aus `TIMESTAMPTZ`-Spalten als
//! [`DateTime<Utc>`] und vergleicht Instants direkt. String-Zeitstempel aus der
//! SQLite-Schicht werden im Scheduler-Persistenzpfad nicht mehr geparst.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;

/// Default-Reminder-Offsets (Minuten), abwärts sortiert — wie im Original.
pub use turnier_config::DEFAULT_REMINDER_OFFSETS;

/// Breite des Reminder-Toleranzfensters (Minuten). Im Original fest 5 Minuten
/// (`reminder_at + timedelta(minutes=5)`). Siehe `bugs_preserved`: zur 60-s-Loop-
/// Kadenz überdimensioniert, holt verpasste Fenster nicht nach — bewusst 1:1.
pub use turnier_config::REMINDER_WINDOW_MINUTES;

/// Ist `now` im Reminder-Fenster `reminder_at <= now <= reminder_at + 5min`?
/// Portiert die inline-Bedingung der drei Reminder-Tasks (Z.316/409).
pub fn is_within_window(reminder_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    is_within_configured_window(reminder_at, now, REMINDER_WINDOW_MINUTES)
}

pub fn is_within_configured_window(
    reminder_at: DateTime<Utc>,
    now: DateTime<Utc>,
    minutes: i64,
) -> bool {
    reminder_at <= now && now <= reminder_at + Duration::minutes(minutes)
}

/// Ist der Wert fällig (`parsed <= now`)? Portiert `_is_due` (Z.45-47).
pub fn is_due(value: Option<&DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    value.is_some_and(|parsed| *parsed <= now)
}

/// Parst die Reminder-Offset-Liste robust aus einer nullable JSONB-Spalte.
/// Portiert `_parse_reminder_offsets` (Z.50-65): akzeptiert ein JSON-Array oder
/// einen Legacy-String mit JSON-Array, dedupliziert, behält nur `>= 0`, sortiert
/// absteigend. Leere oder ungültige Eingabe → [`DEFAULT_REMINDER_OFFSETS`].
///
/// Robustheit gegen Müll-Elemente (nicht-numerisch) ist hier ein safe-Fix:
/// einzelne nicht-konvertierbare Elemente werden übersprungen statt die ganze
/// Schleife abzubrechen (das Original hätte bei `int("x")` geworfen).
pub fn parse_reminder_offsets(value: Option<&Value>) -> Vec<i64> {
    parse_reminder_offsets_with_default(value, &DEFAULT_REMINDER_OFFSETS)
}

pub fn parse_reminder_offsets_with_default(value: Option<&Value>, defaults: &[i64]) -> Vec<i64> {
    let Some(value) = value else {
        return defaults.to_vec();
    };
    let items = match value {
        Value::Array(items) => items.clone(),
        Value::String(s) if !s.trim().is_empty() => match serde_json::from_str::<Value>(s.trim()) {
            Ok(Value::Array(items)) => items,
            _ => return defaults.to_vec(),
        },
        _ => return defaults.to_vec(),
    };

    let mut offsets: Vec<i64> = items
        .iter()
        .filter_map(json_to_i64)
        .filter(|&n| n >= 0)
        .collect();

    if offsets.is_empty() {
        return defaults.to_vec();
    }

    // Dedupliziert, absteigend sortiert — wie `sorted({...}, reverse=True)`.
    offsets.sort_unstable();
    offsets.dedup();
    offsets.reverse();
    offsets
}

/// Wandelt einen JSON-Wert in `i64` um — wie Pythons `int(offset)`: ganze Zahlen
/// direkt, Floats abgeschnitten (Richtung Null), numerische Strings geparst.
fn json_to_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        Value::String(s) => {
            let t = s.trim();
            t.parse::<i64>()
                .ok()
                .or_else(|| t.parse::<f64>().ok().map(|f| f.trunc() as i64))
        }
        _ => None,
    }
}

/// Formatiert einen Offset als „Xh Ymin"-Label. Portiert `_offset_label`
/// (Z.366-373) — die Einzelquelle, die auch die Registrierungs-Reminder nutzt
/// (der duplizierte Inline-Block aus dem Original ist als safe-Fix
/// zusammengeführt).
pub fn offset_label(offset_minutes: i64) -> String {
    let hours = offset_minutes / 60;
    let minutes = offset_minutes % 60;
    if hours != 0 && minutes != 0 {
        format!("{hours}h {minutes}min")
    } else if hours != 0 {
        format!("{hours}h")
    } else {
        format!("{minutes}min")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("{s}Z"))
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn is_due_vergleicht_instant_kleiner_gleich() {
        let now = utc("2026-06-14T12:00:00");
        let before = utc("2026-06-14T11:59:59");
        let same = utc("2026-06-14T12:00:00");
        let after = utc("2026-06-14T12:00:01");
        assert!(is_due(Some(&before), now));
        assert!(is_due(Some(&same), now)); // exakt gleich = fällig
        assert!(!is_due(Some(&after), now));
        assert!(!is_due(None, now));
    }

    #[test]
    fn offsets_default_bei_leer_und_muell() {
        assert_eq!(parse_reminder_offsets(None), vec![1440, 120, 15]);
        assert_eq!(
            parse_reminder_offsets(Some(&Value::Null)),
            vec![1440, 120, 15]
        );
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!(""))),
            vec![1440, 120, 15]
        );
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!([]))),
            vec![1440, 120, 15]
        );
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!("kaputt"))),
            vec![1440, 120, 15]
        );
        // Nur negative → Default.
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!([-5, -1]))),
            vec![1440, 120, 15]
        );
    }

    #[test]
    fn offsets_dedupliziert_und_absteigend() {
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!([15, 1440, 15, 120, 0]))),
            vec![1440, 120, 15, 0]
        );
        // Negative werden gefiltert, der Rest bleibt.
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!([60, -10, 30]))),
            vec![60, 30]
        );
    }

    #[test]
    fn offsets_legacy_json_string_weiter_akzeptiert() {
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!("[60, 30]"))),
            vec![60, 30]
        );
    }

    #[test]
    fn offsets_muell_elemente_uebersprungen() {
        // Nicht-numerische Elemente werden ignoriert statt zu werfen (safe-Fix).
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!([60, "x", 30]))),
            vec![60, 30]
        );
        // Float wird abgeschnitten Richtung Null.
        assert_eq!(
            parse_reminder_offsets(Some(&serde_json::json!([60.9, 30.1]))),
            vec![60, 30]
        );
    }

    #[test]
    fn reminder_fenster_5_minuten() {
        let reminder_at = utc("2026-06-14T12:00:00");
        // Genau am Start des Fensters.
        assert!(is_within_window(reminder_at, utc("2026-06-14T12:00:00")));
        // Innerhalb (4min59s).
        assert!(is_within_window(reminder_at, utc("2026-06-14T12:04:59")));
        // Exakt am Ende des Fensters (5min) noch drin.
        assert!(is_within_window(reminder_at, utc("2026-06-14T12:05:00")));
        // Eine Sekunde nach dem Fenster → raus.
        assert!(!is_within_window(reminder_at, utc("2026-06-14T12:05:01")));
        // Vor dem Fenster → raus.
        assert!(!is_within_window(reminder_at, utc("2026-06-14T11:59:59")));
    }

    #[test]
    fn offset_label_formatierung() {
        assert_eq!(offset_label(1440), "24h");
        assert_eq!(offset_label(90), "1h 30min");
        assert_eq!(offset_label(15), "15min");
        assert_eq!(offset_label(120), "2h");
        assert_eq!(offset_label(0), "0min");
        assert_eq!(offset_label(60), "1h");
    }
}
