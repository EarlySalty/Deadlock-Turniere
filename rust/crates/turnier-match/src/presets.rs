//! Live-Event-ConVar-Presets und ConVar-Normalisierung.
//!
//! Portiert `MATCH_EVENT_PRESETS` und `_normalize_convar_payload` aus
//! `match/manager.py`. Reine Logik, DB-frei — die Anwendung auf eine Lobby läuft
//! über [`crate::lobby`].

use serde_json::{json, Value};

use crate::error::MatchError;

/// Ein vordefiniertes Live-Event-Preset.
#[derive(Debug, Clone)]
pub struct EventPreset {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub requires_cheats: bool,
    /// ConVars beim Aktivieren des Presets.
    pub convars: &'static [(&'static str, Value)],
    /// ConVars beim Deaktivieren (Reset).
    pub reset_convars: &'static [(&'static str, Value)],
}

impl EventPreset {
    /// Die anzuwendenden ConVars je nach Aktivierungszustand
    /// (`convars` bei `enabled`, sonst `reset_convars`).
    pub fn convars_for(&self, enabled: bool) -> serde_json::Map<String, Value> {
        let pairs = if enabled {
            self.convars
        } else {
            self.reset_convars
        };
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    /// Wire-Form fürs Admin-Panel (entspricht einem Eintrag aus
    /// `list_match_event_presets`).
    pub fn to_value(&self) -> Value {
        let convars: serde_json::Map<String, Value> = self
            .convars
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();
        let reset: serde_json::Map<String, Value> = self
            .reset_convars
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();
        json!({
            "key": self.key,
            "label": self.label,
            "description": self.description,
            "requires_cheats": self.requires_cheats,
            "convars": Value::Object(convars),
            "reset_convars": Value::Object(reset),
        })
    }
}

/// Liefert ein Preset anhand seines Schlüssels (getrimmt), oder `None`.
/// Entspricht `MATCH_EVENT_PRESETS.get(str(preset_key).strip())`.
pub fn get_preset(key: &str) -> Option<&'static EventPreset> {
    let key = key.trim();
    event_presets().iter().find(|p| p.key == key)
}

/// Alle Presets in stabiler Reihenfolge (entspricht der Insertion-Order des
/// Python-Dicts). [`list_match_event_presets`] gibt diese als Wire-Werte zurück.
pub fn list_match_event_presets() -> Vec<Value> {
    event_presets().iter().map(EventPreset::to_value).collect()
}

/// Normalisiert ein ConVar-Payload: trimmt Namen (leer → Fehler), wandelt
/// String-Werte (`true`/`on` → 1, `false`/`off` → 0, sonst int/float/Klartext).
/// Leeres Payload oder leerer String-Wert → Fehler. Portiert
/// `_normalize_convar_payload` (Fehler dort `MatchStateError` → hier
/// [`MatchError::State`]).
pub fn normalize_convar_payload(
    convars: &serde_json::Map<String, Value>,
) -> Result<serde_json::Map<String, Value>, MatchError> {
    let mut normalized = serde_json::Map::new();
    for (raw_name, raw_value) in convars {
        let name = raw_name.trim();
        if name.is_empty() {
            return Err(MatchError::state("ConVar-Name darf nicht leer sein"));
        }
        let value = normalize_convar_value(name, raw_value)?;
        normalized.insert(name.to_string(), value);
    }
    if normalized.is_empty() {
        return Err(MatchError::state("Mindestens eine ConVar ist erforderlich"));
    }
    Ok(normalized)
}

/// Wandelt einen einzelnen ConVar-Wert nach den Python-Regeln.
///
/// Nur String-Werte werden umgeformt; andere JSON-Typen (Zahl, Bool, …) bleiben
/// unverändert — exakt wie das Original, das nur `isinstance(value, str)` behandelt.
fn normalize_convar_value(name: &str, value: &Value) -> Result<Value, MatchError> {
    let Value::String(s) = value else {
        return Ok(value.clone());
    };
    let stripped = s.trim();
    if stripped.is_empty() {
        return Err(MatchError::state(format!(
            "ConVar-Wert fuer {name} darf nicht leer sein"
        )));
    }
    let lowered = stripped.to_lowercase();
    if lowered == "true" || lowered == "on" {
        return Ok(json!(1));
    }
    if lowered == "false" || lowered == "off" {
        return Ok(json!(0));
    }
    if let Ok(i) = stripped.parse::<i64>() {
        return Ok(json!(i));
    }
    if let Ok(f) = stripped.parse::<f64>() {
        return Ok(json!(f));
    }
    Ok(json!(stripped))
}

/// Die sieben Presets in Original-Reihenfolge.
fn event_presets() -> &'static [EventPreset] {
    use std::sync::OnceLock;
    static PRESETS: OnceLock<Vec<EventPreset>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        vec![
            EventPreset {
                key: "duplicate_heroes",
                label: "Duplicate Heroes",
                description: "Alle Teams duerfen denselben Hero mehrfach spielen.",
                requires_cheats: false,
                convars: Box::leak(Box::new([("citadel_allow_duplicate_heroes", json!(1))])),
                reset_convars: Box::leak(Box::new([("citadel_allow_duplicate_heroes", json!(0))])),
            },
            EventPreset {
                key: "slowmo",
                label: "Slow Motion",
                description: "Verlangsamt das Match fuer Clutch- oder Showmomente.",
                requires_cheats: true,
                convars: Box::leak(Box::new([("host_timescale", json!(0.7))])),
                reset_convars: Box::leak(Box::new([("host_timescale", json!(1))])),
            },
            EventPreset {
                key: "melee_mayhem",
                label: "Melee Mayhem",
                description: "Nahkampf wird deutlich staerker als gewohnt.",
                requires_cheats: true,
                convars: Box::leak(Box::new([("citadel_melee_damage_scale", json!(2.5))])),
                reset_convars: Box::leak(Box::new([("citadel_melee_damage_scale", json!(1))])),
            },
            EventPreset {
                key: "glass_cannon",
                label: "Glass Cannon",
                description: "Hoher Schaden fuer schnelle, chaotische Teamfights.",
                requires_cheats: true,
                convars: Box::leak(Box::new([
                    ("citadel_dps_multiplier", json!(2)),
                    ("citadel_melee_damage_scale", json!(1.5)),
                ])),
                reset_convars: Box::leak(Box::new([
                    ("citadel_dps_multiplier", json!(1)),
                    ("citadel_melee_damage_scale", json!(1)),
                ])),
            },
            EventPreset {
                key: "walljump_party",
                label: "Walljump Party",
                description: "Mehr Mobilitaet fuer alberne Mobility-Runden.",
                requires_cheats: true,
                convars: Box::leak(Box::new([
                    ("citadel_initial_wall_jump_stamina_cost", json!(0)),
                    ("citadel_air_jumps_enabled", json!(1)),
                ])),
                reset_convars: Box::leak(Box::new([
                    ("citadel_initial_wall_jump_stamina_cost", json!(0)),
                    ("citadel_air_jumps_enabled", json!(1)),
                ])),
            },
            EventPreset {
                key: "orb_madness",
                label: "Orb Madness",
                description: "Orbs werden leichter und chaotischer claimbar.",
                requires_cheats: true,
                convars: Box::leak(Box::new([
                    ("citadel_orb_required_bullets_to_claim_override", json!(1)),
                    ("citadel_orb_expire_percentage", json!(1)),
                ])),
                reset_convars: Box::leak(Box::new([
                    ("citadel_orb_required_bullets_to_claim_override", json!(0)),
                    ("citadel_orb_expire_percentage", json!(1)),
                ])),
            },
            EventPreset {
                key: "zipline_boost",
                label: "Zipline Boost",
                description: "Mehr Druck auf Movement und Rotationen ueber Ziplines.",
                requires_cheats: true,
                convars: Box::leak(Box::new([
                    ("zipline_use_new_latch", json!(2)),
                    ("citadel_debug_zipline_camera_height_add", json!(0)),
                ])),
                reset_convars: Box::leak(Box::new([
                    ("zipline_use_new_latch", json!(2)),
                    ("citadel_debug_zipline_camera_height_add", json!(0)),
                ])),
            },
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_hat_sieben_presets_in_reihenfolge() {
        let list = list_match_event_presets();
        assert_eq!(list.len(), 7);
        assert_eq!(list[0]["key"], json!("duplicate_heroes"));
        assert_eq!(list[6]["key"], json!("zipline_boost"));
        // requires_cheats korrekt typisiert.
        assert_eq!(list[0]["requires_cheats"], json!(false));
        assert_eq!(list[1]["requires_cheats"], json!(true));
    }

    #[test]
    fn get_preset_trimmt_und_findet() {
        assert!(get_preset("  slowmo ").is_some());
        assert!(get_preset("unbekannt").is_none());
    }

    #[test]
    fn convars_for_enabled_vs_reset() {
        let p = get_preset("slowmo").unwrap();
        assert_eq!(p.convars_for(true).get("host_timescale"), Some(&json!(0.7)));
        assert_eq!(p.convars_for(false).get("host_timescale"), Some(&json!(1)));
    }

    #[test]
    fn normalize_bool_strings() {
        let mut input = serde_json::Map::new();
        input.insert("a".into(), json!("true"));
        input.insert("b".into(), json!("OFF"));
        input.insert("c".into(), json!(" 42 "));
        input.insert("d".into(), json!("0.5"));
        input.insert("e".into(), json!("custom"));
        input.insert("f".into(), json!(7)); // Nicht-String bleibt.
        let out = normalize_convar_payload(&input).unwrap();
        assert_eq!(out["a"], json!(1));
        assert_eq!(out["b"], json!(0));
        assert_eq!(out["c"], json!(42));
        assert_eq!(out["d"], json!(0.5));
        assert_eq!(out["e"], json!("custom"));
        assert_eq!(out["f"], json!(7));
    }

    #[test]
    fn normalize_leerer_name_fehler() {
        let mut input = serde_json::Map::new();
        input.insert("   ".into(), json!(1));
        assert!(normalize_convar_payload(&input).is_err());
    }

    #[test]
    fn normalize_leerer_wert_fehler() {
        let mut input = serde_json::Map::new();
        input.insert("x".into(), json!("   "));
        assert!(normalize_convar_payload(&input).is_err());
    }

    #[test]
    fn normalize_leeres_payload_fehler() {
        let input = serde_json::Map::new();
        assert!(normalize_convar_payload(&input).is_err());
    }
}
