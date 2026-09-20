//! Globale Regiegrenzen; Matchmodus und manuelle Kamera-Auswahl bleiben Laufzeitdaten.
use crate::ConfigError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObserverDirectorConfig {
    pub min_hold_milliseconds: i64,
    pub stale_after_milliseconds: i64,
    pub normal_switch_delta: f64,
    pub emergency_switch_delta: f64,
    pub minimum_interesting_score: f64,
    pub player_view_score: f64,
}

impl Default for ObserverDirectorConfig {
    fn default() -> Self {
        Self {
            min_hold_milliseconds: 4500,
            stale_after_milliseconds: 2500,
            normal_switch_delta: 12.0,
            emergency_switch_delta: 25.0,
            minimum_interesting_score: 18.0,
            player_view_score: 58.0,
        }
    }
}

impl ObserverDirectorConfig {
    pub(crate) fn validate(&self, evaluate_milliseconds: u64) -> Result<(), ConfigError> {
        if !(1..=60000).contains(&self.min_hold_milliseconds)
            || !(1..=60000).contains(&self.stale_after_milliseconds)
            || self.stale_after_milliseconds < evaluate_milliseconds as i64
        {
            return Err(ConfigError::Invalid("observer_director: Zeiten 1..60000 ms, Stale-Frist mindestens ein Auswertungsintervall"));
        }
        if [
            self.normal_switch_delta,
            self.emergency_switch_delta,
            self.minimum_interesting_score,
            self.player_view_score,
        ]
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=10000.0).contains(value))
            || self.normal_switch_delta > self.emergency_switch_delta
            || self.minimum_interesting_score > self.player_view_score
        {
            return Err(ConfigError::Invalid("observer_director: endliche Score-Grenzen 0..10000 in aufsteigender Reihenfolge erforderlich"));
        }
        Ok(())
    }
}
