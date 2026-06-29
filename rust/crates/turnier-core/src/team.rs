//! Team-bezogene DTOs: Mitglieder, volle und öffentliche Team-Sicht, Bewerbungen
//! und Einladungen.

use serde::{Deserialize, Serialize};

use crate::enums::*;

/// Eingabe zum Anlegen eines Teams.
#[derive(Debug, Clone, Deserialize)]
pub struct TeamCreate {
    pub name: String,
    pub tournament_id: i64,
}

/// Mitglied eines Teams (volle Sicht inkl. `discord_id`).
#[derive(Debug, Clone, Serialize)]
pub struct TeamMember {
    pub id: i64,
    pub team_id: i64,
    pub discord_id: String,
    pub discord_name: Option<String>,
    pub steam_id: Option<String>,
    pub rank: Option<String>,
    pub rank_score: i64,
    pub role: TeamRole,
    pub joined_at: String,
}

/// Öffentliche Mitglieds-Sicht — ohne `discord_id`.
#[derive(Debug, Clone, Serialize)]
pub struct TeamMemberPublic {
    pub id: i64,
    pub team_id: i64,
    pub discord_name: Option<String>,
    pub steam_id: Option<String>,
    pub rank: Option<String>,
    pub rank_score: i64,
    pub role: TeamRole,
    pub joined_at: String,
}

/// Volle Team-Sicht inkl. Mitgliederliste.
#[derive(Debug, Clone, Serialize)]
pub struct Team {
    pub id: i64,
    pub tournament_id: i64,
    pub name: String,
    pub name_key: String,
    pub captain_discord_id: String,
    pub created_at: String,
    pub recruitment_status: RecruitmentStatus,
    pub members: Vec<TeamMember>,
}

/// Öffentliche Team-Sicht (Kapitän anonymisiert, nur Flag für offene Bewerbungen).
#[derive(Debug, Clone, Serialize)]
pub struct TeamPublic {
    pub id: i64,
    pub tournament_id: i64,
    pub name: String,
    pub name_key: String,
    pub members: Vec<TeamMemberPublic>,
    pub created_at: String,
    pub recruitment_status: RecruitmentStatus,
    pub has_pending_applications: bool,
}

/// Bewerbung eines Spielers auf ein Team.
#[derive(Debug, Clone, Serialize)]
pub struct TeamApplication {
    pub id: i64,
    pub team_id: i64,
    pub discord_name: String,
    pub status: ApplicationStatus,
    pub created_at: String,
}

/// Einladung eines Spielers in ein Team.
#[derive(Debug, Clone, Serialize)]
pub struct TeamInvitation {
    pub id: i64,
    pub tournament_id: i64,
    pub team_id: i64,
    pub team_name: Option<String>,
    pub status: InvitationStatus,
    pub created_at: String,
    pub expires_at: Option<String>,
}
