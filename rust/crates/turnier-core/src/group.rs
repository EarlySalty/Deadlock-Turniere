//! Gruppenphasen-DTOs: Gruppe, Gruppen-Tabellenzeile, Gruppen-Match.

use serde::Serialize;
use serde_json::Value;

use crate::enums::MatchStatus;

/// Eine Tabellenzeile innerhalb einer Gruppe (Team-Statistik).
#[derive(Debug, Clone, Serialize)]
pub struct GroupTeam {
    pub id: i64,
    pub group_id: i64,
    pub team_id: i64,
    pub team_name: String,
    pub wins: i64,
    pub losses: i64,
    pub points: i64,
}

/// Ein Match innerhalb der Gruppenphase.
#[derive(Debug, Clone, Serialize)]
pub struct GroupMatch {
    pub id: i64,
    pub group_id: i64,
    pub team1_id: i64,
    pub team2_id: i64,
    pub winner_id: Option<i64>,
    pub status: MatchStatus,
    pub steam_party_id: Option<String>,
    pub party_code: Option<String>,
    pub deadlock_match_id: Option<String>,
    pub match_duration_s: Option<i64>,
    pub match_stats: Option<String>,
    pub hero_assignments: Option<Value>,
    pub scheduled_at: Option<String>,
    pub played_at: Option<String>,
}

/// Eine Gruppe inkl. Tabellen und Matches.
#[derive(Debug, Clone, Serialize)]
pub struct Group {
    pub id: i64,
    pub tournament_id: i64,
    pub name: String,
    pub seeding_order: i64,
    pub teams: Vec<GroupTeam>,
    pub matches: Vec<GroupMatch>,
}
