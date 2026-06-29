//! Nutzer-bezogene DTOs: Session, Einwilligung, Profil und Leaderboard.

use serde::{Deserialize, Serialize};

/// Die aufgelöste Session eines eingeloggten Nutzers (Antwort von `/api/me`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub discord_id: String,
    #[serde(default)]
    pub discord_name: Option<String>,
    #[serde(default)]
    pub discord_avatar: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub is_mod: bool,
}

/// Eingabe einer Datenschutz-Einwilligung.
#[derive(Debug, Clone, Deserialize)]
pub struct ConsentCreate {
    #[serde(default = "default_consent_version")]
    pub consent_version: i64,
}

fn default_consent_version() -> i64 {
    2
}

/// Einwilligungsstatus eines Nutzers.
#[derive(Debug, Clone, Serialize)]
pub struct ConsentStatus {
    pub has_consent: bool,
    pub consented_at: Option<String>,
    pub consent_version: Option<i64>,
}

/// Partielle Profil-Änderung (jedes Feld optional).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserProfileUpdate {
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub invite_auto_accept: Option<bool>,
    #[serde(default)]
    pub notify_discord_dm: Option<bool>,
    #[serde(default)]
    pub notify_browser: Option<bool>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_filename: Option<String>,
    #[serde(default)]
    pub notify_match_start: Option<bool>,
    #[serde(default)]
    pub notify_checkin: Option<bool>,
    #[serde(default)]
    pub notify_team_invite: Option<bool>,
    #[serde(default)]
    pub notify_tournament_news: Option<bool>,
    #[serde(default)]
    pub notify_registration_reminder: Option<bool>,
}

/// Vollständiges Nutzerprofil mit Benachrichtigungs-Präferenzen.
#[derive(Debug, Clone, Serialize)]
pub struct UserProfile {
    pub discord_id: String,
    pub bio: Option<String>,
    pub invite_auto_accept: bool,
    pub notify_discord_dm: bool,
    pub notify_browser: bool,
    pub display_name: Option<String>,
    pub avatar_filename: Option<String>,
    pub notify_match_start: bool,
    pub notify_checkin: bool,
    pub notify_team_invite: bool,
    pub notify_tournament_news: bool,
    pub notify_registration_reminder: bool,
    pub updated_at: Option<String>,
}

/// Ein Eintrag der Turnier-Historie eines Spielers.
#[derive(Debug, Clone, Serialize)]
pub struct TournamentHistoryEntry {
    pub tournament_name: String,
    pub placement: Option<i64>,
    pub team_name: Option<String>,
}

/// Öffentliches Spielerprofil inkl. aggregierter Statistiken.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerProfile {
    pub discord_name: String,
    pub display_name: Option<String>,
    pub discord_avatar: Option<String>,
    pub avatar_filename: Option<String>,
    pub bio: Option<String>,
    pub rank: Option<String>,
    pub rank_score: i64,
    pub tournaments_played: i64,
    pub matches_played: i64,
    pub matches_won: i64,
    pub best_placement: Option<i64>,
    pub total_points: i64,
    pub tournament_history: Vec<TournamentHistoryEntry>,
}

/// Ein Eintrag der Leaderboard-Tabelle.
#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardEntry {
    pub rank_position: i64,
    pub discord_name: String,
    pub rank: Option<String>,
    pub total_points: i64,
    pub tournaments_played: i64,
    pub matches_played: i64,
    pub matches_won: i64,
    pub best_placement: Option<i64>,
}
