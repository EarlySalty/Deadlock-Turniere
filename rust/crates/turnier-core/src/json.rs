//! Kleine serde-Helfer für die JSON-in-TEXT-Felder und wiederkehrende Defaults.
//!
//! Im Python-Original lagen `reminder_offsets`/`start_reminder_offsets` als
//! JSON-String in TEXT-Spalten und wurden über `mode="before"`-Validatoren
//! tolerant zu `list[int]` geparst. Hier kapseln wir dieselbe Toleranz an einer
//! Stelle, statt sie über die Modelle zu streuen.

/// Standard-Erinnerungs-Offsets in Minuten vor einem Turnier (`[1440, 120, 15]`).
pub fn default_reminder_offsets() -> Vec<i64> {
    vec![1440, 120, 15]
}

/// Standard-Offsets für die Start-Erinnerung (`[1440, 60]`).
pub fn default_start_reminder_offsets() -> Vec<i64> {
    vec![1440, 60]
}

/// `true` als serde-Default (für Felder, die im Original auf `True` defaulten).
pub fn default_true() -> bool {
    true
}

/// Standard-Teamgröße (6 Spieler).
pub fn default_team_size() -> i64 {
    6
}

/// Standard-`series_format` (Best-of-1).
pub fn default_series_format() -> i64 {
    1
}

/// Standard-Schonfrist bei No-Show in Minuten.
pub fn default_no_show_grace_minutes() -> i64 {
    10
}

/// Bereinigt eine Offset-Liste wie der Python-Validator: Werte < 0 raus,
/// dedupliziert, absteigend sortiert. Leeres Ergebnis fällt auf `fallback` zurück.
pub fn clean_offsets(values: &[i64], fallback: &[i64]) -> Vec<i64> {
    let mut cleaned: Vec<i64> = values.iter().copied().filter(|o| *o >= 0).collect();
    cleaned.sort_unstable_by(|a, b| b.cmp(a));
    cleaned.dedup();
    if cleaned.is_empty() {
        fallback.to_vec()
    } else {
        cleaned
    }
}

/// Parst den DB-TEXT einer Offset-Spalte tolerant zu `Vec<i64>`.
/// `null`, leerer String oder ungültiges JSON → `fallback`.
pub fn parse_offsets(raw: Option<&str>, fallback: &[i64]) -> Vec<i64> {
    let Some(text) = raw else {
        return fallback.to_vec();
    };
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return fallback.to_vec();
    }
    match serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        Ok(list) => {
            let parsed: Vec<i64> = list.iter().filter_map(value_as_i64).collect();
            if parsed.is_empty() {
                fallback.to_vec()
            } else {
                parsed
            }
        }
        Err(_) => fallback.to_vec(),
    }
}

/// Parst ein `dict`-Feld (z. B. `hero_assignments`) tolerant zu einem JSON-Objekt.
/// Alles, was kein Objekt ist (`null`, `""`, Liste, ungültig) → `None`.
pub fn parse_object(raw: Option<&str>) -> Option<serde_json::Value> {
    let text = raw?.trim();
    if text.is_empty() || text == "null" {
        return None;
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v @ serde_json::Value::Object(_)) => Some(v),
        _ => None,
    }
}

fn value_as_i64(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        serde_json::Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_offsets_dedupes_and_sorts_desc() {
        assert_eq!(clean_offsets(&[15, 120, 15, -5, 1440], &[1]), vec![1440, 120, 15]);
        assert_eq!(clean_offsets(&[-1, -2], &[1440, 60]), vec![1440, 60]);
    }

    #[test]
    fn parse_offsets_handles_json_and_garbage() {
        assert_eq!(parse_offsets(Some("[1440, 120, 15]"), &[1]), vec![1440, 120, 15]);
        assert_eq!(parse_offsets(Some("kaputt"), &[1440, 60]), vec![1440, 60]);
        assert_eq!(parse_offsets(None, &[1440, 60]), vec![1440, 60]);
        assert_eq!(parse_offsets(Some(""), &[1440, 60]), vec![1440, 60]);
    }

    #[test]
    fn parse_object_only_accepts_objects() {
        assert!(parse_object(Some("{\"1\":\"a\"}")).is_some());
        assert!(parse_object(Some("[1,2]")).is_none());
        assert!(parse_object(Some("null")).is_none());
        assert!(parse_object(None).is_none());
    }
}
