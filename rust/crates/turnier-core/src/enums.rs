//! Alle Domänen-Enums als ein Satz typsicherer Werte.
//!
//! Jedes Enum bildet exakt das Vokabular des Python-Originals ab und wird sowohl
//! über `serde` (Wire-Format) als auch über `sqlx::Type` (TEXT-Spalten) als
//! kleingeschriebener String repräsentiert. Damit ersetzt EIN Typ die früheren
//! Magic-Strings, die verstreut in Routen und SQL standen.

use serde::{Deserialize, Serialize};

/// Makro für die wiederkehrende Ableitungs-Salve eines String-Enums.
macro_rules! str_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $( $variant:ident ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
        #[serde(rename_all = "snake_case")]
        #[sqlx(rename_all = "snake_case")]
        pub enum $name {
            $( $variant ),+
        }
    };
}

str_enum! {
    /// Lebenszyklus eines Turniers.
    TournamentStatus { Draft, Registration, Checkin, GroupPhase, Bracket, Completed, Archived }
}

str_enum! {
    /// Zustand eines einzelnen Matches (Gruppe oder Bracket).
    MatchStatus { Pending, Checkin, LobbyCreated, InProgress, Completed, Forfeit, Cancelled }
}

str_enum! {
    /// Bracket-Seite eines Matches.
    BracketType { Winners, Losers, GrandFinal }
}

str_enum! {
    /// Bracket-Verfahren.
    BracketFormat { SingleElimination, DoubleElimination }
}

str_enum! {
    /// Aus der Teamanzahl abgeleiteter (oder admin-erzwungener) Turnier-Modus.
    TournamentMode { GroupStage, BracketOnly }
}

str_enum! {
    /// Spielmodus, der das Hero-Assignment und die Lobby beeinflusst.
    TournamentGameMode { Standard, Mirror, AllSame, RandomHeroes, SingleLane }
}

str_enum! {
    /// Vordefinierte Lobby-Einstellungs-Presets.
    LobbySettingsPreset {
        Standard, FastMode, HighDamage, LowGravity, SpeedMode, GlassCannon,
        RichStart, ChaosMode, AllSameHero, Immortal, Custom,
    }
}

str_enum! {
    /// Rolle eines Mitglieds innerhalb eines Teams.
    TeamRole { Captain, Member }
}

str_enum! {
    /// Aufnahmestatus eines Teams (offen, Bewerbung nötig, geschlossen).
    RecruitmentStatus { Open, Application, Closed }
}

str_enum! {
    /// Einladungspolitik eines Turniers.
    InviteMode { Always, Window, Never }
}

str_enum! {
    /// Status einer Team-Einladung.
    InvitationStatus { Pending, Accepted, Rejected, Expired }
}

str_enum! {
    /// Status einer Team-Bewerbung.
    ApplicationStatus { Pending, Accepted, Rejected }
}

str_enum! {
    /// Herkunft eines Match-Ergebnisses.
    ResultSource { Manual, Automatic }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialisiert_als_snake_case_string() {
        assert_eq!(
            serde_json::to_string(&TournamentStatus::GroupPhase).unwrap(),
            "\"group_phase\""
        );
        assert_eq!(
            serde_json::to_string(&MatchStatus::LobbyCreated).unwrap(),
            "\"lobby_created\""
        );
        assert_eq!(
            serde_json::to_string(&BracketType::GrandFinal).unwrap(),
            "\"grand_final\""
        );
        assert_eq!(
            serde_json::to_string(&TournamentGameMode::RandomHeroes).unwrap(),
            "\"random_heroes\""
        );
    }

    #[test]
    fn deserialisiert_python_werte() {
        let s: TournamentMode = serde_json::from_str("\"bracket_only\"").unwrap();
        assert_eq!(s, TournamentMode::BracketOnly);
        let p: LobbySettingsPreset = serde_json::from_str("\"all_same_hero\"").unwrap();
        assert_eq!(p, LobbySettingsPreset::AllSameHero);
    }
}
