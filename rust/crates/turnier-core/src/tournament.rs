//! Turnier-DTOs: getrennte Familien für Anlegen (`Create`), Ändern (`Update`),
//! Lesen (`Tournament`/`TournamentDetail`) und die öffentliche (anonymisierte)
//! Sicht. Zeitstempel bleiben als ISO-String, um die Wire-Form 1:1 zum
//! Python-Backend zu erhalten.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::marker::PhantomData;

use crate::bracket::{BracketMatch, BracketMiniGroup};
use crate::enums::*;
use crate::group::Group;
use crate::json::*;
use crate::team::{Team, TeamPublic};

fn d_bracket_format() -> BracketFormat {
    BracketFormat::SingleElimination
}
fn d_invite_mode() -> InviteMode {
    InviteMode::Always
}
fn d_preset() -> LobbySettingsPreset {
    LobbySettingsPreset::Standard
}
fn d_game_mode() -> TournamentGameMode {
    TournamentGameMode::Standard
}
fn d_objective() -> String {
    "auto".to_string()
}

/// Dreiwertige Patch-Semantik für nullable Update-Felder.
///
/// `Missing` bedeutet: JSON-Feld wurde weggelassen und die Spalte bleibt
/// unverändert. `Null` bedeutet: Feld wurde explizit als JSON-`null` gesendet und
/// die Spalte wird geleert. `Value` setzt einen neuen Wert.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Patch<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

impl<'de, T> Deserialize<'de> for Patch<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PatchVisitor<T>(PhantomData<T>);

        impl<'de, T> serde::de::Visitor<'de> for PatchVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = Patch<T>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("missing, null, or a concrete patch value")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Patch::Null)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Patch::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                T::deserialize(deserializer).map(Patch::Value)
            }
        }

        deserializer.deserialize_option(PatchVisitor(PhantomData))
    }
}

/// Eingabe zum Anlegen eines Turniers.
#[derive(Debug, Clone, Deserialize)]
pub struct TournamentCreate {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_team_size")]
    pub team_size: i64,
    #[serde(default = "d_bracket_format")]
    pub bracket_format: BracketFormat,
    #[serde(default = "default_series_format")]
    pub series_format: i64,
    #[serde(default)]
    pub final_series_format: Option<i64>,
    #[serde(default)]
    pub registration_start: Option<String>,
    #[serde(default)]
    pub registration_end: Option<String>,
    #[serde(default)]
    pub checkin_start: Option<String>,
    #[serde(default)]
    pub group_phase_start: Option<String>,
    #[serde(default)]
    pub bracket_start: Option<String>,
    #[serde(default = "d_invite_mode")]
    pub invite_mode: InviteMode,
    #[serde(default)]
    pub invite_window_start: Option<String>,
    #[serde(default)]
    pub invite_window_end: Option<String>,
    #[serde(default = "d_preset")]
    pub lobby_settings_preset: LobbySettingsPreset,
    #[serde(default)]
    pub lobby_settings: Option<Value>,
    /// Admin-Override: erzwingt `group_stage` oder `bracket_only`.
    #[serde(default)]
    pub force_tournament_mode: Option<TournamentMode>,
    #[serde(default = "d_game_mode")]
    pub tournament_game_mode: TournamentGameMode,
    #[serde(default = "default_true")]
    pub auto_lobby_enabled: bool,
    #[serde(default)]
    pub exclude_from_leaderboard: bool,
    #[serde(default = "default_reminder_offsets")]
    pub reminder_offsets: Vec<i64>,
    #[serde(default = "default_start_reminder_offsets")]
    pub start_reminder_offsets: Vec<i64>,
    #[serde(default = "d_objective")]
    pub match_objective: String,
    #[serde(default = "default_no_show_grace_minutes")]
    pub no_show_grace_minutes: i64,
    #[serde(default)]
    pub rules: Option<String>,
    #[serde(default)]
    pub is_test: bool,
}

impl TournamentCreate {
    /// Validiert wie die Pydantic-Validatoren und bereinigt die Offset-Listen.
    /// Gibt eine Fehlermeldung zurück, wenn `series_format`/`final_series_format`
    /// nicht in `{1, 3, 5}` liegt.
    pub fn validated(mut self) -> Result<Self, String> {
        check_series_format(self.series_format)?;
        if let Some(f) = self.final_series_format {
            check_final_series_format(f)?;
        }
        self.reminder_offsets = clean_offsets(&self.reminder_offsets, &default_reminder_offsets());
        self.start_reminder_offsets = clean_offsets(
            &self.start_reminder_offsets,
            &default_start_reminder_offsets(),
        );
        Ok(self)
    }
}

/// Partielle Änderung eines Turniers — jedes Feld optional (`None` = unverändert).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TournamentUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Patch<String>,
    #[serde(default)]
    pub status: Option<TournamentStatus>,
    #[serde(default)]
    pub team_size: Option<i64>,
    #[serde(default)]
    pub bracket_format: Option<BracketFormat>,
    #[serde(default)]
    pub series_format: Option<i64>,
    #[serde(default)]
    pub final_series_format: Patch<i64>,
    #[serde(default)]
    pub registration_start: Patch<String>,
    #[serde(default)]
    pub registration_end: Patch<String>,
    #[serde(default)]
    pub checkin_start: Patch<String>,
    #[serde(default)]
    pub group_phase_start: Patch<String>,
    #[serde(default)]
    pub bracket_start: Patch<String>,
    #[serde(default)]
    pub invite_mode: Option<InviteMode>,
    #[serde(default)]
    pub invite_window_start: Patch<String>,
    #[serde(default)]
    pub invite_window_end: Patch<String>,
    #[serde(default)]
    pub lobby_settings_preset: Option<LobbySettingsPreset>,
    #[serde(default)]
    pub lobby_settings: Patch<Value>,
    #[serde(default)]
    pub force_tournament_mode: Option<TournamentMode>,
    #[serde(default)]
    pub tournament_game_mode: Option<TournamentGameMode>,
    #[serde(default)]
    pub auto_lobby_enabled: Option<bool>,
    #[serde(default)]
    pub exclude_from_leaderboard: Option<bool>,
    #[serde(default)]
    pub is_test: Option<bool>,
    #[serde(default)]
    pub reminder_offsets: Patch<Vec<i64>>,
    #[serde(default)]
    pub start_reminder_offsets: Patch<Vec<i64>>,
    #[serde(default)]
    pub match_objective: Option<String>,
    #[serde(default)]
    pub no_show_grace_minutes: Option<i64>,
    #[serde(default)]
    pub rules: Patch<String>,
}

impl TournamentUpdate {
    /// Validiert die gesetzten Felder und bereinigt vorhandene Offset-Listen.
    pub fn validated(mut self) -> Result<Self, String> {
        if let Some(v) = self.series_format {
            check_series_format(v)?;
        }
        if let Patch::Value(v) = &self.final_series_format {
            check_final_series_format(*v)?;
        }
        if let Patch::Value(ref offs) = self.reminder_offsets {
            self.reminder_offsets = Patch::Value(clean_offsets(offs, &default_reminder_offsets()));
        }
        if let Patch::Value(ref offs) = self.start_reminder_offsets {
            self.start_reminder_offsets =
                Patch::Value(clean_offsets(offs, &default_start_reminder_offsets()));
        }
        Ok(self)
    }
}

/// Vollständige Lese-Sicht eines Turniers (für Admin/eingeloggte Nutzer).
#[derive(Debug, Clone, Serialize)]
pub struct Tournament {
    pub id: i64,
    pub name: String,
    pub status: TournamentStatus,
    pub description: Option<String>,
    pub team_size: i64,
    pub series_format: i64,
    pub final_series_format: Option<i64>,
    pub registration_start: Option<String>,
    pub registration_end: Option<String>,
    pub checkin_start: Option<String>,
    pub group_phase_start: Option<String>,
    pub bracket_start: Option<String>,
    pub bracket_format: String,
    pub tournament_mode: TournamentMode,
    pub tournament_game_mode: TournamentGameMode,
    pub auto_lobby_enabled: bool,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub invite_mode: InviteMode,
    pub invite_window_start: Option<String>,
    pub invite_window_end: Option<String>,
    pub lobby_settings: Option<String>,
    pub exclude_from_leaderboard: bool,
    pub reminder_offsets: Vec<i64>,
    pub start_reminder_offsets: Vec<i64>,
    pub match_objective: String,
    pub no_show_grace_minutes: i64,
    pub rules: Option<String>,
    pub is_test: bool,
}

/// Anmeldung eines Spielers zu einem Turnier (volle Sicht inkl. `discord_id`).
#[derive(Debug, Clone, Serialize)]
pub struct TournamentSignup {
    pub id: i64,
    pub tournament_id: i64,
    pub discord_id: String,
    pub discord_name: Option<String>,
    pub steam_id: Option<String>,
    pub rank: Option<String>,
    pub rank_score: i64,
    pub team_id: Option<i64>,
    pub signed_up_at: String,
}

/// Öffentliche (anonymisierte) Anmeldungs-Sicht — ohne `discord_id`/`steam_id`.
#[derive(Debug, Clone, Serialize)]
pub struct TournamentSignupPublic {
    pub id: i64,
    pub tournament_id: i64,
    pub discord_name: Option<String>,
    pub rank: Option<String>,
    pub rank_score: i64,
    pub team_id: Option<i64>,
    pub signed_up_at: String,
}

/// Detail-Sicht eines Turniers inkl. Teams, Gruppen, Bracket und Anmeldungen.
#[derive(Debug, Clone, Serialize)]
pub struct TournamentDetail {
    #[serde(flatten)]
    pub tournament: Tournament,
    pub teams: Vec<Team>,
    pub groups: Vec<Group>,
    pub bracket_matches: Vec<BracketMatch>,
    pub mini_groups: Vec<BracketMiniGroup>,
    pub signups: Vec<TournamentSignup>,
}

/// Öffentliche Detail-Sicht (Teams/Anmeldungen anonymisiert).
#[derive(Debug, Clone, Serialize)]
pub struct TournamentDetailPublic {
    pub id: i64,
    pub name: String,
    pub status: TournamentStatus,
    pub description: Option<String>,
    pub team_size: i64,
    pub series_format: i64,
    pub final_series_format: Option<i64>,
    pub registration_start: Option<String>,
    pub registration_end: Option<String>,
    pub group_phase_start: Option<String>,
    pub bracket_start: Option<String>,
    pub bracket_format: String,
    pub tournament_mode: TournamentMode,
    pub tournament_game_mode: TournamentGameMode,
    pub auto_lobby_enabled: bool,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub invite_mode: InviteMode,
    pub invite_window_start: Option<String>,
    pub invite_window_end: Option<String>,
    pub lobby_settings: Option<String>,
    pub rules: Option<String>,
    pub is_test: bool,
    pub teams: Vec<TeamPublic>,
    pub groups: Vec<Group>,
    pub bracket_matches: Vec<BracketMatch>,
    pub mini_groups: Vec<BracketMiniGroup>,
    pub signups: Vec<TournamentSignupPublic>,
}

fn check_series_format(v: i64) -> Result<(), String> {
    if matches!(v, 1 | 3 | 5) {
        Ok(())
    } else {
        Err("series_format muss 1, 3 oder 5 sein".to_string())
    }
}

fn check_final_series_format(v: i64) -> Result<(), String> {
    if matches!(v, 1 | 3 | 5) {
        Ok(())
    } else {
        Err("final_series_format muss 1, 3 oder 5 sein".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_from_json(body: &str) -> TournamentUpdate {
        serde_json::from_str(body).expect("TournamentUpdate JSON parses")
    }

    macro_rules! assert_missing {
        ($body:expr, $($field:ident),+ $(,)?) => {
            $(assert!(matches!($body.$field, Patch::Missing), "{} should be missing", stringify!($field));)+
        };
    }

    macro_rules! assert_null {
        ($body:expr, $($field:ident),+ $(,)?) => {
            $(assert!(matches!($body.$field, Patch::Null), "{} should be null", stringify!($field));)+
        };
    }

    macro_rules! assert_value {
        ($body:expr, $field:ident, $expected:expr) => {
            match &$body.$field {
                Patch::Value(value) => assert_eq!(value, &$expected),
                other => panic!("{} should be value, got {:?}", stringify!($field), other),
            }
        };
    }

    #[test]
    fn create_validation_rejects_bad_series_format() {
        let raw = r#"{"name":"T","series_format":2}"#;
        let c: TournamentCreate = serde_json::from_str(raw).unwrap();
        assert!(c.validated().is_err());
    }

    #[test]
    fn create_defaults_match_python() {
        let c: TournamentCreate = serde_json::from_str(r#"{"name":"T"}"#).unwrap();
        let c = c.validated().unwrap();
        assert_eq!(c.team_size, 6);
        assert_eq!(c.series_format, 1);
        assert!(c.auto_lobby_enabled);
        assert_eq!(c.reminder_offsets, vec![1440, 120, 15]);
        assert_eq!(c.match_objective, "auto");
    }

    #[test]
    fn nullable_update_fields_treat_missing_as_unchanged() {
        let body = update_from_json("{}");

        assert_missing!(
            body,
            description,
            registration_start,
            registration_end,
            checkin_start,
            group_phase_start,
            bracket_start,
            invite_window_start,
            invite_window_end,
            lobby_settings,
            final_series_format,
            rules,
            reminder_offsets,
            start_reminder_offsets,
        );
    }

    #[test]
    fn nullable_update_fields_treat_explicit_null_as_clear() {
        let body = update_from_json(
            r#"{
                "description": null,
                "registration_start": null,
                "registration_end": null,
                "checkin_start": null,
                "group_phase_start": null,
                "bracket_start": null,
                "invite_window_start": null,
                "invite_window_end": null,
                "lobby_settings": null,
                "final_series_format": null,
                "rules": null,
                "reminder_offsets": null,
                "start_reminder_offsets": null
            }"#,
        );

        assert_null!(
            body,
            description,
            registration_start,
            registration_end,
            checkin_start,
            group_phase_start,
            bracket_start,
            invite_window_start,
            invite_window_end,
            lobby_settings,
            final_series_format,
            rules,
            reminder_offsets,
            start_reminder_offsets,
        );
    }

    #[test]
    fn nullable_update_fields_treat_values_as_set() {
        let body = update_from_json(
            r#"{
                "description": "Beschreibung",
                "registration_start": "2026-07-01T10:00:00",
                "registration_end": "2026-07-01T11:00:00",
                "checkin_start": "2026-07-01T12:00:00",
                "group_phase_start": "2026-07-01T13:00:00",
                "bracket_start": "2026-07-01T14:00:00",
                "invite_window_start": "2026-07-01T15:00:00",
                "invite_window_end": "2026-07-01T16:00:00",
                "lobby_settings": {"preset": "custom"},
                "final_series_format": 3,
                "rules": "Regeln",
                "reminder_offsets": [15, 1440, 120],
                "start_reminder_offsets": [60, 1440]
            }"#,
        );

        assert_value!(body, description, "Beschreibung".to_string());
        assert_value!(body, registration_start, "2026-07-01T10:00:00".to_string());
        assert_value!(body, registration_end, "2026-07-01T11:00:00".to_string());
        assert_value!(body, checkin_start, "2026-07-01T12:00:00".to_string());
        assert_value!(body, group_phase_start, "2026-07-01T13:00:00".to_string());
        assert_value!(body, bracket_start, "2026-07-01T14:00:00".to_string());
        assert_value!(body, invite_window_start, "2026-07-01T15:00:00".to_string());
        assert_value!(body, invite_window_end, "2026-07-01T16:00:00".to_string());
        assert_value!(
            body,
            lobby_settings,
            serde_json::json!({"preset": "custom"})
        );
        assert_value!(body, final_series_format, 3);
        assert_value!(body, rules, "Regeln".to_string());
        assert_value!(body, reminder_offsets, vec![15, 1440, 120]);
        assert_value!(body, start_reminder_offsets, vec![60, 1440]);
    }

    #[test]
    fn nullable_offset_values_are_cleaned_only_when_set() {
        let body = update_from_json(
            r#"{
                "reminder_offsets": [15, -5, 120, 15],
                "start_reminder_offsets": []
            }"#,
        )
        .validated()
        .expect("valid offset update");

        assert_value!(body, reminder_offsets, vec![120, 15]);
        assert_value!(body, start_reminder_offsets, vec![1440, 60]);

        let body = update_from_json(
            r#"{
                "reminder_offsets": null,
                "start_reminder_offsets": null
            }"#,
        )
        .validated()
        .expect("null offset update");

        assert_null!(body, reminder_offsets, start_reminder_offsets);
    }
}
