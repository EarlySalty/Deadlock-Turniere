//! Zeit-Parsing, Fälligkeit und Reminder-Offsets — reine Logik (DB-frei).
//!
//! ## TZ-Parität (BEWUSST 1:1 erhalten, needs-decision)
//! Das Python-Original (`_parse_timestamp`, `datetime.now()`) vergleicht gegen
//! die **lokale, naive** Server-Zeit: tz-aware Strings werden per
//! `astimezone().replace(tzinfo=None)` in lokale naive Zeit umgerechnet, naive
//! Strings bleiben unverändert. Stimmen die DB-Zeitstempel nicht mit der lokalen
//! Server-TZ überein (z. B. UTC in der DB, Server-TZ ≠ UTC), verschieben sich
//! Phasenwechsel und alle Reminder um den TZ-Offset; über DST-Wechsel driftet das
//! zusätzlich.
//!
//! Dieser Port repliziert das EXAKT mit [`chrono::Local`]: tz-aware Strings via
//! `DateTime::parse_from_rfc3339` → `.with_timezone(&Local).naive_local()`, naive
//! Strings werden als lokal interpretiert; verglichen wird gegen
//! `Local::now().naive_local()`. Eine Umstellung auf UTC würde ändern, WANN
//! Reminder feuern (Parität!) — daher NICHT geändert, sondern als `bugs_preserved`
//! dokumentiert.

use chrono::{DateTime, Duration, NaiveDateTime};

/// Default-Reminder-Offsets (Minuten), abwärts sortiert — wie im Original.
pub const DEFAULT_REMINDER_OFFSETS: [i64; 3] = [1440, 120, 15];

/// Breite des Reminder-Toleranzfensters (Minuten). Im Original fest 5 Minuten
/// (`reminder_at + timedelta(minutes=5)`). Siehe `bugs_preserved`: zur 60-s-Loop-
/// Kadenz überdimensioniert, holt verpasste Fenster nicht nach — bewusst 1:1.
pub const REMINDER_WINDOW_MINUTES: i64 = 5;

/// Ist `now` im Reminder-Fenster `reminder_at <= now <= reminder_at + 5min`?
/// Portiert die inline-Bedingung der drei Reminder-Tasks (Z.316/409).
pub fn is_within_window(reminder_at: NaiveDateTime, now: NaiveDateTime) -> bool {
    reminder_at <= now && now <= reminder_at + Duration::minutes(REMINDER_WINDOW_MINUTES)
}

/// Parst einen DB-/Frontend-Zeitstempel robust für lokale Vergleiche.
///
/// Portiert `_parse_timestamp` (Z.29-42): leer/`None` → `None`; `Z` wird zu
/// `+00:00`; tz-aware Werte werden in **lokale naive** Zeit umgerechnet, naive
/// Werte bleiben unverändert. Nicht parsebare Werte → `None`.
pub fn parse_timestamp(value: Option<&str>) -> Option<NaiveDateTime> {
    let raw = value?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.replace('Z', "+00:00");

    // Zuerst als tz-aware (RFC3339) versuchen → wie Python `astimezone()` in
    // lokale naive Zeit umrechnen.
    if let Ok(aware) = DateTime::parse_from_rfc3339(&normalized) {
        return Some(aware.with_timezone(&chrono::Local).naive_local());
    }

    // Sonst als naiver ISO-Zeitstempel — wie Python: unverändert (als lokal
    // interpretiert) übernommen. Mit und ohne Sekundenbruchteile, mit Leerzeichen
    // oder 'T' als Trenner.
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&normalized, fmt) {
            return Some(naive);
        }
    }
    None
}

/// Ist der Wert fällig (`parsed <= now`)? Portiert `_is_due` (Z.45-47).
pub fn is_due(value: Option<&str>, now: NaiveDateTime) -> bool {
    parse_timestamp(value).is_some_and(|parsed| parsed <= now)
}

/// Parst die Reminder-Offset-Liste robust. Portiert `_parse_reminder_offsets`
/// (Z.50-65): akzeptiert ein JSON-Array (als String) oder einen bereits
/// geparsten Wert; dedupliziert, behält nur `>= 0`, sortiert absteigend. Leere
/// oder ungültige Eingabe → [`DEFAULT_REMINDER_OFFSETS`].
///
/// Robustheit gegen Müll-Elemente (nicht-numerisch) ist hier ein safe-Fix:
/// einzelne nicht-konvertierbare Elemente werden übersprungen statt die ganze
/// Schleife abzubrechen (das Original hätte bei `int("x")` geworfen).
pub fn parse_reminder_offsets(value: Option<&str>) -> Vec<i64> {
    let parsed: Option<serde_json::Value> = match value {
        Some(s) if !s.trim().is_empty() => serde_json::from_str(s.trim()).ok(),
        _ => None,
    };

    let Some(serde_json::Value::Array(items)) = parsed else {
        return DEFAULT_REMINDER_OFFSETS.to_vec();
    };

    let mut offsets: Vec<i64> = items
        .iter()
        .filter_map(json_to_i64)
        .filter(|&n| n >= 0)
        .collect();

    if offsets.is_empty() {
        return DEFAULT_REMINDER_OFFSETS.to_vec();
    }

    // Dedupliziert, absteigend sortiert — wie `sorted({...}, reverse=True)`.
    offsets.sort_unstable();
    offsets.dedup();
    offsets.reverse();
    offsets
}

/// Wandelt einen JSON-Wert in `i64` um — wie Pythons `int(offset)`: ganze Zahlen
/// direkt, Floats abgeschnitten (Richtung Null), numerische Strings geparst.
fn json_to_i64(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f.trunc() as i64)),
        serde_json::Value::String(s) => {
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
    use chrono::TimeZone;

    fn naive(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn parse_naiv_unveraendert() {
        assert_eq!(
            parse_timestamp(Some("2026-06-14T12:00:00")),
            Some(naive("2026-06-14T12:00:00"))
        );
        // Leerzeichen-Trenner ebenfalls.
        assert_eq!(
            parse_timestamp(Some("2026-06-14 12:00:00")),
            Some(naive("2026-06-14T12:00:00"))
        );
        // Mit Bruchteilen.
        assert!(parse_timestamp(Some("2026-06-14T12:00:00.123")).is_some());
    }

    #[test]
    fn parse_leer_und_muell_ist_none() {
        assert_eq!(parse_timestamp(None), None);
        assert_eq!(parse_timestamp(Some("")), None);
        assert_eq!(parse_timestamp(Some("   ")), None);
        assert_eq!(parse_timestamp(Some("kein-datum")), None);
    }

    #[test]
    fn tz_aware_wird_in_lokale_naive_zeit_umgerechnet() {
        // Konstruiere einen tz-aware UTC-Wert und vergleiche mit der erwarteten
        // lokalen naiven Repräsentation desselben Instants.
        let utc = "2026-06-14T12:00:00+00:00";
        let got = parse_timestamp(Some(utc)).unwrap();
        let expected = chrono::Utc
            .with_ymd_and_hms(2026, 6, 14, 12, 0, 0)
            .unwrap()
            .with_timezone(&chrono::Local)
            .naive_local();
        assert_eq!(got, expected);
        // Z-Suffix verhält sich identisch zu +00:00.
        assert_eq!(parse_timestamp(Some("2026-06-14T12:00:00Z")).unwrap(), expected);
    }

    #[test]
    fn is_due_vergleicht_kleiner_gleich() {
        let now = naive("2026-06-14T12:00:00");
        assert!(is_due(Some("2026-06-14T11:59:59"), now));
        assert!(is_due(Some("2026-06-14T12:00:00"), now)); // exakt gleich = fällig
        assert!(!is_due(Some("2026-06-14T12:00:01"), now));
        assert!(!is_due(None, now));
    }

    #[test]
    fn offsets_default_bei_leer_und_muell() {
        assert_eq!(parse_reminder_offsets(None), vec![1440, 120, 15]);
        assert_eq!(parse_reminder_offsets(Some("")), vec![1440, 120, 15]);
        assert_eq!(parse_reminder_offsets(Some("[]")), vec![1440, 120, 15]);
        assert_eq!(parse_reminder_offsets(Some("kaputt")), vec![1440, 120, 15]);
        // Nur negative → Default.
        assert_eq!(parse_reminder_offsets(Some("[-5, -1]")), vec![1440, 120, 15]);
    }

    #[test]
    fn offsets_dedupliziert_und_absteigend() {
        assert_eq!(
            parse_reminder_offsets(Some("[15, 1440, 15, 120, 0]")),
            vec![1440, 120, 15, 0]
        );
        // Negative werden gefiltert, der Rest bleibt.
        assert_eq!(parse_reminder_offsets(Some("[60, -10, 30]")), vec![60, 30]);
    }

    #[test]
    fn offsets_muell_elemente_uebersprungen() {
        // Nicht-numerische Elemente werden ignoriert statt zu werfen (safe-Fix).
        assert_eq!(parse_reminder_offsets(Some("[60, \"x\", 30]")), vec![60, 30]);
        // Float wird abgeschnitten Richtung Null.
        assert_eq!(parse_reminder_offsets(Some("[60.9, 30.1]")), vec![60, 30]);
    }

    #[test]
    fn reminder_fenster_5_minuten() {
        let reminder_at = naive("2026-06-14T12:00:00");
        // Genau am Start des Fensters.
        assert!(is_within_window(reminder_at, naive("2026-06-14T12:00:00")));
        // Innerhalb (4min59s).
        assert!(is_within_window(reminder_at, naive("2026-06-14T12:04:59")));
        // Exakt am Ende des Fensters (5min) noch drin.
        assert!(is_within_window(reminder_at, naive("2026-06-14T12:05:00")));
        // Eine Sekunde nach dem Fenster → raus.
        assert!(!is_within_window(reminder_at, naive("2026-06-14T12:05:01")));
        // Vor dem Fenster → raus.
        assert!(!is_within_window(reminder_at, naive("2026-06-14T11:59:59")));
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
