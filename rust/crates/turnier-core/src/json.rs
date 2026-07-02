//! Kleine serde-Helfer für die JSON-in-TEXT-Felder und wiederkehrende Defaults.
//!
//! Im Python-Original lagen `reminder_offsets`/`start_reminder_offsets` als
//! JSON-String in TEXT-Spalten und wurden über `mode="before"`-Validatoren
//! tolerant zu `list[int]` geparst. Hier kapseln wir dieselbe Toleranz an einer
//! Stelle, statt sie über die Modelle zu streuen.

use serde::Serialize;
use serde_json::Value;

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
    match serde_json::from_str::<Vec<Value>>(trimmed) {
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

/// Normalisiert nullable JSONB-Felder: SQL `NULL` und JSON `null` werden beide
/// als `None` behandelt.
pub fn normalize_nullable_jsonb(value: Option<Value>) -> Option<Value> {
    match value {
        Some(Value::Null) | None => None,
        Some(value) => Some(value),
    }
}

/// Serialisiert ein optionales Feld fuer eine nullable JSONB-Spalte.
pub fn to_nullable_jsonb<T>(value: Option<&T>) -> serde_json::Result<Option<Value>>
where
    T: Serialize + ?Sized,
{
    value
        .map(serde_json::to_value)
        .transpose()
        .map(normalize_nullable_jsonb)
}

/// Mappt nullable JSONB aus PG auf die bisherige Wire-Form `Option<String>`.
pub fn jsonb_to_wire_string(value: Option<Value>) -> Option<String> {
    normalize_nullable_jsonb(value).map(|value| value.to_string())
}

/// Parst eine optionale Wire-JSON-Zeichenkette fuer eine nullable JSONB-Spalte.
pub fn wire_string_to_jsonb(raw: Option<&str>) -> Option<Value> {
    let text = raw?.trim();
    if text.is_empty() || text == "null" {
        return None;
    }
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| normalize_nullable_jsonb(Some(value)))
}

/// Serialisiert Reminder-Offsets als JSONB-Array.
pub fn offsets_to_jsonb(values: &[i64]) -> Value {
    Value::Array(values.iter().copied().map(Value::from).collect())
}

/// Parst Reminder-Offsets direkt aus einer nullable JSONB-Spalte.
pub fn parse_offsets_jsonb(value: Option<&Value>, fallback: &[i64]) -> Vec<i64> {
    let Some(value) = value else {
        return fallback.to_vec();
    };
    match value {
        Value::Array(items) => {
            let parsed: Vec<i64> = items.iter().filter_map(value_as_i64).collect();
            if parsed.is_empty() {
                fallback.to_vec()
            } else {
                parsed
            }
        }
        Value::String(text) => parse_offsets(Some(text), fallback),
        Value::Null => fallback.to_vec(),
        _ => fallback.to_vec(),
    }
}

/// Akzeptiert nur JSONB-Objekte fuer nullable Objektfelder wie `lobby_settings`.
pub fn parse_object_jsonb(value: Option<&Value>) -> Option<Value> {
    match value {
        Some(v @ Value::Object(_)) => Some(v.clone()),
        _ => None,
    }
}

fn value_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_offsets_dedupes_and_sorts_desc() {
        assert_eq!(
            clean_offsets(&[15, 120, 15, -5, 1440], &[1]),
            vec![1440, 120, 15]
        );
        assert_eq!(clean_offsets(&[-1, -2], &[1440, 60]), vec![1440, 60]);
    }

    #[test]
    fn parse_offsets_handles_json_and_garbage() {
        assert_eq!(
            parse_offsets(Some("[1440, 120, 15]"), &[1]),
            vec![1440, 120, 15]
        );
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

    #[test]
    fn nullable_jsonb_maps_sql_and_json_null_to_none() {
        assert_eq!(normalize_nullable_jsonb(None), None);
        assert_eq!(normalize_nullable_jsonb(Some(Value::Null)), None);
        assert_eq!(
            normalize_nullable_jsonb(Some(Value::Bool(true))),
            Some(Value::Bool(true))
        );
    }

    #[test]
    fn to_nullable_jsonb_serializes_values() {
        let value = vec![1440_i64, 60];
        assert_eq!(
            to_nullable_jsonb(Some(&value)).unwrap(),
            Some(offsets_to_jsonb(&value))
        );
        assert_eq!(to_nullable_jsonb::<Vec<i64>>(None).unwrap(), None);
    }

    #[test]
    fn jsonb_wire_string_roundtrip_handles_nullable_fields() {
        let value = serde_json::json!({ "preset": "standard" });
        let wire = jsonb_to_wire_string(Some(value.clone())).unwrap();
        assert_eq!(wire_string_to_jsonb(Some(&wire)), Some(value));
        assert_eq!(wire_string_to_jsonb(Some("null")), None);
        assert_eq!(wire_string_to_jsonb(Some("")), None);
        assert_eq!(wire_string_to_jsonb(Some("kaputt")), None);
    }

    #[test]
    fn parse_offsets_jsonb_accepts_arrays_and_legacy_strings() {
        let fallback = [1440_i64, 60];
        assert_eq!(
            parse_offsets_jsonb(Some(&serde_json::json!([120, "15", -1])), &fallback),
            vec![120, 15, -1]
        );
        assert_eq!(
            parse_offsets_jsonb(Some(&serde_json::json!("[120,15]")), &fallback),
            vec![120, 15]
        );
        assert_eq!(parse_offsets_jsonb(Some(&Value::Null), &fallback), fallback);
    }

    #[test]
    fn parse_object_jsonb_only_accepts_objects() {
        let value = serde_json::json!({"foo": "bar"});
        assert_eq!(parse_object_jsonb(Some(&value)), Some(value));
        assert_eq!(parse_object_jsonb(Some(&serde_json::json!([1, 2]))), None);
        assert_eq!(parse_object_jsonb(None), None);
    }
}
