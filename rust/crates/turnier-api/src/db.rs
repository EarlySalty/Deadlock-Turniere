use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::error::{WebError, WebResult};

pub fn parse_discord_id(value: &str) -> WebResult<i64> {
    turnier_core::parse_discord_id(value)
        .map_err(|_| WebError::bad_request("Ungueltige Discord-ID"))
}

pub fn parse_actor_id(value: &str) -> WebResult<i64> {
    turnier_core::parse_discord_id(value).map_err(|_| WebError::internal("Ungueltige Actor-ID"))
}

/// Zentrale PG-Spalten sind BIGINT. Fuer die historische Admin-Funktion
/// "leeres Team ohne Captain" muss `teams.captain_discord_id` wegen NOT NULL
/// intern als `0` abgelegt werden; an der HTTP-Grenze bleibt das der alte
/// leere String. Echte Discord-Snowflakes werden weiter strikt geparst.
pub fn parse_captain_id(value: &str) -> WebResult<i64> {
    if value.trim().is_empty() {
        Ok(0)
    } else {
        parse_discord_id(value)
    }
}

pub fn discord_id_to_string(value: i64) -> String {
    if value == 0 {
        String::new()
    } else {
        turnier_core::discord_id_to_string(value)
    }
}

pub fn ts_to_string(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

pub fn opt_ts_to_string(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(ts_to_string)
}

pub fn json_to_wire(value: Option<Value>) -> Option<String> {
    turnier_core::json::normalize_nullable_jsonb(value).map(|value| value.to_string())
}

pub fn json_object(value: Option<Value>) -> Option<Value> {
    turnier_core::json::normalize_nullable_jsonb(value).and_then(|value| match value {
        Value::Object(_) => Some(value),
        _ => None,
    })
}
