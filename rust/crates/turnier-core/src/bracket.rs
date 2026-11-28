use serde::Serialize;
use serde_json::Value;

use crate::enums::{BracketType, MatchStatus};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LobbyAccess {
    pub steam_party_id: Option<String>,
    pub party_code: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct NoLobbyAccess {}

#[derive(Debug, Clone, Serialize)]
pub struct MatchGame<L = LobbyAccess> {
    pub id: i64,
    pub bracket_match_id: i64,
    pub game_number: i64,
    pub status: String,
    #[serde(flatten)]
    pub lobby: L,
    pub deadlock_match_id: Option<String>,
    pub winner_team: Option<i64>,
    pub duration_s: Option<i64>,
    pub match_stats: Option<Value>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BracketMatch<L = LobbyAccess> {
    pub id: i64,
    pub tournament_id: i64,
    pub round: i64,
    pub position: i64,
    pub bracket_type: BracketType,
    pub mini_group_id: Option<i64>,
    pub team1_id: Option<i64>,
    pub team2_id: Option<i64>,
    pub winner_id: Option<i64>,
    pub status: MatchStatus,
    pub source_match1_id: Option<i64>,
    pub source_match2_id: Option<i64>,
    pub loser_to_match_id: Option<i64>,
    pub loser_to_slot: Option<i64>,
    #[serde(flatten)]
    pub lobby: L,
    pub deadlock_match_id: Option<String>,
    pub match_duration_s: Option<i64>,
    pub match_stats: Option<String>,
    pub hero_assignments: Option<Value>,
    pub series_wins_team1: i64,
    pub series_wins_team2: i64,
    pub games: Vec<MatchGame<L>>,
    pub scheduled_at: Option<String>,
    pub on_stream: bool,
    pub played_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BracketMiniGroup {
    pub id: i64,
    pub tournament_id: i64,
    pub round: i64,
    pub position: i64,
    pub advances_to_match_id: Option<i64>,
    pub advances_to_slot: Option<i64>,
    pub team_ids: Vec<i64>,
    pub match_ids: Vec<i64>,
}

pub type PublicMatchGame = MatchGame<NoLobbyAccess>;
pub type PublicBracketMatch = BracketMatch<NoLobbyAccess>;
