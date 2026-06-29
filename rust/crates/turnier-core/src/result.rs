//! Ergebnis-DTOs: kanonisches Match-Ergebnis, Selbstmeldungen (Reports) und
//! Check-Ins.
//!
//! `match_type`/`status` der Reports bleiben bewusst als String, weil die
//! Live-DB hier ein weicheres Vokabular führt als ein Enum sauber abbilden
//! würde (siehe `docs/known-issues.md`); die Konsolidierung ist ein Opt-in-
//! Folgefix, kein stiller Eingriff.

use serde::{Deserialize, Serialize};

use crate::enums::ResultSource;

/// Kanonisches, gespeichertes Match-Ergebnis (Tabelle `match_results`).
#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    pub id: i64,
    pub bracket_match_id: Option<i64>,
    pub group_match_id: Option<i64>,
    pub winning_team: Option<i64>,
    pub duration_s: Option<i64>,
    pub player_stats: Option<String>,
    pub source: ResultSource,
    pub created_at: String,
}

/// Eingabe einer Ergebnis-Selbstmeldung durch ein Team.
#[derive(Debug, Clone, Deserialize)]
pub struct MatchResultReportCreate {
    #[serde(default)]
    pub winner_team_id: Option<i64>,
    #[serde(default)]
    pub deadlock_match_id: Option<String>,
    #[serde(default)]
    pub is_no_show: bool,
    #[serde(default)]
    pub no_show_team_id: Option<i64>,
}

/// Gespeicherte Ergebnis-Selbstmeldung inkl. Auflösungs-Metadaten.
#[derive(Debug, Clone, Serialize)]
pub struct MatchResultReport {
    pub id: i64,
    pub match_type: String,
    pub match_id: i64,
    pub tournament_id: i64,
    pub reported_by: String,
    pub winner_team_id: Option<i64>,
    pub deadlock_match_id: Option<String>,
    pub is_no_show: bool,
    pub no_show_team_id: Option<i64>,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
}

/// Ein Check-In eines Spielers zu einem Match.
#[derive(Debug, Clone, Serialize)]
pub struct CheckIn {
    pub id: i64,
    pub match_type: String,
    pub match_id: i64,
    pub team_id: i64,
    pub discord_id: String,
    pub checked_in_at: String,
}
