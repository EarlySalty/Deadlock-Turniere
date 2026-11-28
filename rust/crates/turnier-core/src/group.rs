use serde::Serialize;
use serde_json::Value;

use crate::bracket::{LobbyAccess, NoLobbyAccess};
use crate::enums::MatchStatus;

#[derive(Debug, Clone, Serialize)]
pub struct GroupTeam {
    pub id: i64,
    pub group_id: i64,
    pub team_id: i64,
    pub team_name: String,
    pub wins: i64,
    pub losses: i64,
    pub points: i64,
    pub seed_tiebreak: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupMatch<L = LobbyAccess> {
    pub id: i64,
    pub group_id: i64,
    pub team1_id: i64,
    pub team2_id: i64,
    pub winner_id: Option<i64>,
    pub status: MatchStatus,
    #[serde(flatten)]
    pub lobby: L,
    pub deadlock_match_id: Option<String>,
    pub match_duration_s: Option<i64>,
    pub match_stats: Option<String>,
    pub hero_assignments: Option<Value>,
    pub scheduled_at: Option<String>,
    pub played_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Group<L = LobbyAccess> {
    pub id: i64,
    pub tournament_id: i64,
    pub name: String,
    pub seeding_order: i64,
    pub teams: Vec<GroupTeam>,
    pub matches: Vec<GroupMatch<L>>,
}

pub type PublicGroupMatch = GroupMatch<NoLobbyAccess>;
pub type PublicGroup = Group<NoLobbyAccess>;
