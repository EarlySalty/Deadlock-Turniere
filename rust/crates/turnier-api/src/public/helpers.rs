//! Geteilte Bausteine des öffentlichen/Teilnehmer-Routers: Name-Resolution,
//! Rang-Anreicherung (gebatcht), Read-Modell-Loader, Guards, Signup-Sync und
//! das Mapping der Turnier-Rohzeile auf die DTOs.
//!
//! Diese Datei konsolidiert die im Python-Original 4–5-fach kopierten
//! Member-/Signup-JOIN-Queries und die inline duplizierten Guards/Consent-Checks
//! (Befunde routes.py „safe": Duplikation, lastrowid, N+1) zu jeweils EINER
//! wiederverwendbaren Funktion.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;

use turnier_core::{
    BracketMatch, BracketMiniGroup, Group, GroupMatch, GroupTeam, InviteMode, RankProfile,
    RecruitmentStatus, Team, TeamMember, TeamMemberPublic, TeamPublic, Tournament,
    TournamentDetailPublic, TournamentSignup, TournamentSignupPublic, UserSession,
};
use turnier_db::Pool;

use crate::error::{WebError, WebResult};
use crate::state::AppState;

/// Aktuelle Consent-Version (Modul-Konstante `_CURRENT_CONSENT_VERSION`).
pub const CURRENT_CONSENT_VERSION: i64 = 2;

// ---------------------------------------------------------------------------
// Name-Resolution
// ---------------------------------------------------------------------------

/// Prüft, ob ein Wert wie eine rohe Discord-ID aussieht (16–21 Ziffern).
/// Entspricht `_looks_like_discord_id` (routes.py:84).
pub fn looks_like_discord_id(value: Option<&str>) -> bool {
    match value {
        Some(v) => {
            let stripped = v.trim();
            !stripped.is_empty()
                && stripped.chars().all(|c| c.is_ascii_digit())
                && (16..=21).contains(&stripped.len())
        }
        None => false,
    }
}

/// Bereinigt einen Discord-Namen: leere Werte, die rohe ID und ID-artige Strings
/// werden aussortiert. Entspricht `_sanitize_discord_name` (routes.py:91).
pub fn sanitize_discord_name(name: Option<&str>, discord_id: Option<&str>) -> Option<String> {
    let stripped = name?.trim();
    if stripped.is_empty() {
        return None;
    }
    if let Some(id) = discord_id {
        if stripped == id.trim() {
            return None;
        }
    }
    if looks_like_discord_id(Some(stripped)) {
        return None;
    }
    Some(stripped.to_string())
}

/// Wählt den ersten brauchbaren Namen aus den Kandidaten (Priorität wie übergeben).
/// Entspricht `_preferred_discord_name` (routes.py:108).
pub fn preferred_discord_name(
    candidates: &[Option<&str>],
    discord_id: Option<&str>,
) -> Option<String> {
    for candidate in candidates {
        if let Some(sanitized) = sanitize_discord_name(*candidate, discord_id) {
            return Some(sanitized);
        }
    }
    None
}

/// Löst den Anzeigenamen einer Discord-ID auf: bevorzugter Name →
/// sessions → team_members → user_profiles → rohe discord_id.
/// Entspricht `_resolve_discord_name` (routes.py:523).
pub async fn resolve_discord_name(
    pool: &Pool,
    discord_id: &str,
    preferred_name: Option<&str>,
) -> WebResult<String> {
    if let Some(name) = sanitize_discord_name(preferred_name, Some(discord_id)) {
        return Ok(name);
    }

    let session_name: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT discord_name FROM sessions \
         WHERE discord_id = ? AND discord_name IS NOT NULL AND discord_name != '' \
         ORDER BY token DESC LIMIT 1",
    )
    .bind(discord_id)
    .fetch_optional(pool)
    .await?;
    if let Some((Some(name),)) = session_name {
        if let Some(sanitized) = sanitize_discord_name(Some(&name), Some(discord_id)) {
            return Ok(sanitized);
        }
    }

    let member_name: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT discord_name FROM team_members \
         WHERE discord_id = ? AND discord_name IS NOT NULL AND discord_name != '' \
         ORDER BY joined_at DESC LIMIT 1",
    )
    .bind(discord_id)
    .fetch_optional(pool)
    .await?;
    if let Some((Some(name),)) = member_name {
        if let Some(sanitized) = sanitize_discord_name(Some(&name), Some(discord_id)) {
            return Ok(sanitized);
        }
    }

    let profile_name: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT display_name FROM user_profiles \
         WHERE discord_id = ? AND display_name IS NOT NULL AND display_name != '' \
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(discord_id)
    .fetch_optional(pool)
    .await?;
    if let Some((Some(name),)) = profile_name {
        if let Some(sanitized) = sanitize_discord_name(Some(&name), Some(discord_id)) {
            return Ok(sanitized);
        }
    }

    Ok(discord_id.to_string())
}

// ---------------------------------------------------------------------------
// Zeitstempel
// ---------------------------------------------------------------------------

/// Parst einen Zeitstempel robust zu UTC. Entspricht `_parse_timestamp`
/// (routes.py:68): leer/ungültig → `None`, naive Werte gelten als UTC.
pub fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    let text = value?.trim();
    if text.is_empty() {
        return None;
    }
    let normalized = text.replace('Z', "+00:00");
    if let Ok(dt) = DateTime::parse_from_rfc3339(&normalized) {
        return Some(dt.with_timezone(&Utc));
    }
    // Naiver ISO-Wert ohne Zone → als UTC interpretieren.
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&normalized, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Rang-Anreicherung (gebatcht — gegen N+1)
// ---------------------------------------------------------------------------

/// Ein roher Member-/Signup-Datensatz vor der Rang-Anreicherung.
struct RankFields {
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: i64,
}

/// Wendet ein Discord-first-Profil auf die Rangfelder an — exakt wie
/// `_enrich_rank_data` (routes.py:45): nur nicht-leere Profilwerte überschreiben.
fn apply_profile(fields: &mut RankFields, profile: Option<&RankProfile>) {
    let Some(profile) = profile else { return };
    if let Some(steam_id) = &profile.steam_id {
        if !steam_id.is_empty() {
            fields.steam_id = Some(steam_id.clone());
        }
    }
    if let Some(rank) = &profile.rank {
        if !rank.is_empty() {
            fields.rank = Some(rank.clone());
        }
    }
    // rank_score wird im Original gesetzt, sobald es != None ist (auch 0).
    // RankProfile.rank_score ist i64 (nie None) → immer übernehmen.
    fields.rank_score = profile.rank_score;
}

// ---------------------------------------------------------------------------
// Member-Name-Resolution per JOIN (EINE wiederverwendbare Query)
// ---------------------------------------------------------------------------

/// SELECT-Liste + JOINs für team_members inkl. Namens-Kandidaten aus
/// sessions/user_profiles. Konsolidiert die 4-fach kopierte Query.
const MEMBER_SELECT: &str = "SELECT tm.id, tm.team_id, tm.discord_id, \
    tm.discord_name AS team_member_discord_name, \
    s.discord_name AS session_discord_name, p.display_name AS profile_display_name, \
    tm.steam_id, tm.rank, tm.rank_score, tm.role, tm.joined_at \
    FROM team_members tm \
    LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM sessions \
        WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s \
        ON s.discord_id = tm.discord_id \
    LEFT JOIN user_profiles p ON p.discord_id = tm.discord_id \
    WHERE tm.team_id = ? ORDER BY tm.joined_at";

/// Eine angereicherte Member-Zeile (Namens-Kandidaten + Rangfelder).
struct MemberRow {
    id: i64,
    team_id: i64,
    discord_id: String,
    team_member_discord_name: Option<String>,
    session_discord_name: Option<String>,
    profile_display_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: i64,
    role: turnier_core::TeamRole,
    joined_at: String,
}

impl MemberRow {
    fn preferred_name(&self) -> Option<String> {
        preferred_discord_name(
            &[
                self.profile_display_name.as_deref(),
                self.session_discord_name.as_deref(),
                self.team_member_discord_name.as_deref(),
            ],
            Some(&self.discord_id),
        )
    }
}

/// Lädt die Member-Zeilen eines Teams (rohe Namens-Kandidaten + Rangfelder).
async fn load_member_rows(pool: &Pool, team_id: i64) -> WebResult<Vec<MemberRow>> {
    let rows = sqlx::query(MEMBER_SELECT).bind(team_id).fetch_all(pool).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(MemberRow {
            id: row.try_get("id")?,
            team_id: row.try_get("team_id")?,
            discord_id: row.try_get("discord_id")?,
            team_member_discord_name: row.try_get("team_member_discord_name")?,
            session_discord_name: row.try_get("session_discord_name")?,
            profile_display_name: row.try_get("profile_display_name")?,
            steam_id: row.try_get("steam_id")?,
            rank: row.try_get("rank")?,
            rank_score: row.try_get::<Option<i64>, _>("rank_score")?.unwrap_or(0),
            role: row.try_get("role")?,
            joined_at: row.try_get("joined_at")?,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Team-Loader (intern, mit Rang-Anreicherung)
// ---------------------------------------------------------------------------

/// Eine Team-Stammzeile.
#[derive(sqlx::FromRow)]
struct TeamRow {
    id: i64,
    tournament_id: i64,
    name: String,
    name_key: String,
    captain_discord_id: String,
    created_at: String,
    recruitment_status: RecruitmentStatus,
}

/// Baut einen `TeamMember` aus Zeile + Anreicherungs-Map.
fn member_from_row(row: MemberRow, ranks: &HashMap<String, RankProfile>) -> TeamMember {
    let preferred = row.preferred_name();
    let mut fields = RankFields { steam_id: row.steam_id, rank: row.rank, rank_score: row.rank_score };
    apply_profile(&mut fields, ranks.get(&row.discord_id));
    TeamMember {
        id: row.id,
        team_id: row.team_id,
        discord_id: row.discord_id,
        discord_name: preferred,
        steam_id: fields.steam_id,
        rank: fields.rank,
        rank_score: fields.rank_score,
        role: row.role,
        joined_at: row.joined_at,
    }
}

/// Lädt alle Teams eines Turniers inkl. Mitglieder (interne Sicht, rang-angereichert).
/// Entspricht `_load_teams_for_tournament` (routes.py:119).
pub async fn load_teams_for_tournament(state: &AppState, tournament_id: i64) -> WebResult<Vec<Team>> {
    let pool = &state.pool;
    let team_rows: Vec<TeamRow> =
        sqlx::query_as("SELECT * FROM teams WHERE tournament_id = ?")
            .bind(tournament_id)
            .fetch_all(pool)
            .await?;

    let mut teams = Vec::with_capacity(team_rows.len());
    for t in team_rows {
        let member_rows = load_member_rows(pool, t.id).await?;
        let ranks = enrich_for_ids(
            state,
            member_rows.iter().map(|m| m.discord_id.clone()).collect(),
        )
        .await?;
        let members = member_rows
            .into_iter()
            .map(|m| member_from_row(m, &ranks))
            .collect();
        teams.push(Team {
            id: t.id,
            tournament_id: t.tournament_id,
            name: t.name,
            name_key: t.name_key,
            captain_discord_id: t.captain_discord_id,
            created_at: t.created_at,
            recruitment_status: t.recruitment_status,
            members,
        });
    }
    Ok(teams)
}

/// Lädt Teams in der öffentlichen Sicht (ohne discord_id/captain in Members,
/// mit `has_pending_applications`-Flag). Entspricht `_load_teams_public`
/// (routes.py:159). Bewusst OHNE Rang-Anreicherung — 1:1 zum Original.
pub async fn load_teams_public(pool: &Pool, tournament_id: i64) -> WebResult<Vec<TeamPublic>> {
    let team_rows: Vec<TeamRow> =
        sqlx::query_as("SELECT * FROM teams WHERE tournament_id = ?")
            .bind(tournament_id)
            .fetch_all(pool)
            .await?;

    let mut teams = Vec::with_capacity(team_rows.len());
    for t in team_rows {
        let member_rows = load_member_rows(pool, t.id).await?;
        let members: Vec<TeamMemberPublic> = member_rows
            .into_iter()
            .map(|m| {
                let preferred = m.preferred_name();
                TeamMemberPublic {
                    id: m.id,
                    team_id: m.team_id,
                    discord_name: preferred,
                    steam_id: m.steam_id,
                    rank: m.rank,
                    rank_score: m.rank_score,
                    role: m.role,
                    joined_at: m.joined_at,
                }
            })
            .collect();

        let app_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM team_applications WHERE team_id = ? AND status = 'pending'",
        )
        .bind(t.id)
        .fetch_one(pool)
        .await?;

        let has_pending_applications =
            t.recruitment_status == RecruitmentStatus::Application && app_count > 0;

        teams.push(TeamPublic {
            id: t.id,
            tournament_id: t.tournament_id,
            name: t.name,
            name_key: t.name_key,
            members,
            created_at: t.created_at,
            recruitment_status: t.recruitment_status,
            has_pending_applications,
        });
    }
    Ok(teams)
}

/// Lädt EIN Team in voller Sicht (rang-angereichert) für Mutations-Responses.
/// Entspricht `_load_team_response` (routes.py:614).
pub async fn load_team_response(state: &AppState, team_id: i64) -> WebResult<Team> {
    let pool = &state.pool;
    let t: TeamRow = sqlx::query_as("SELECT * FROM teams WHERE id = ?")
        .bind(team_id)
        .fetch_one(pool)
        .await?;
    let member_rows = load_member_rows(pool, team_id).await?;
    let ranks = enrich_for_ids(
        state,
        member_rows.iter().map(|m| m.discord_id.clone()).collect(),
    )
    .await?;
    let members = member_rows
        .into_iter()
        .map(|m| member_from_row(m, &ranks))
        .collect();
    Ok(Team {
        id: t.id,
        tournament_id: t.tournament_id,
        name: t.name,
        name_key: t.name_key,
        captain_discord_id: t.captain_discord_id,
        created_at: t.created_at,
        recruitment_status: t.recruitment_status,
        members,
    })
}

/// Gebatchter Rang-Lookup für eine Menge Discord-IDs. Fehler des Resolvers
/// degradieren still zu „kein Profil" (wie im Original `await get_player_rank_profile`,
/// dessen Fehler dort ebenfalls nicht hart durchschlagen).
async fn enrich_for_ids(
    state: &AppState,
    ids: Vec<String>,
) -> WebResult<HashMap<String, RankProfile>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    match state.rank_resolver.rank_profiles(&ids).await {
        Ok(map) => Ok(map),
        Err(err) => {
            tracing::warn!(error = %err, "Rang-Anreicherung fehlgeschlagen — ohne Profil weiter");
            Ok(HashMap::new())
        }
    }
}

// ---------------------------------------------------------------------------
// Signup-Loader
// ---------------------------------------------------------------------------

/// Eine angereicherte Signup-Zeile (Namens-Kandidaten + Rangfelder).
struct SignupRow {
    id: i64,
    tournament_id: i64,
    discord_id: String,
    signup_discord_name: Option<String>,
    session_discord_name: Option<String>,
    team_member_discord_name: Option<String>,
    profile_display_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: i64,
    team_id: Option<i64>,
    signed_up_at: String,
}

impl SignupRow {
    fn preferred_name(&self) -> Option<String> {
        preferred_discord_name(
            &[
                self.profile_display_name.as_deref(),
                self.session_discord_name.as_deref(),
                self.signup_discord_name.as_deref(),
                self.team_member_discord_name.as_deref(),
            ],
            Some(&self.discord_id),
        )
    }
}

/// SELECT für tournament_signups inkl. Namens-Kandidaten aus sessions/
/// team_members/user_profiles. Konsolidiert die doppelte Query.
const SIGNUP_SELECT: &str = "SELECT ts.id, ts.tournament_id, ts.discord_id, \
    ts.discord_name AS signup_discord_name, s.discord_name AS session_discord_name, \
    tm.discord_name AS team_member_discord_name, p.display_name AS profile_display_name, \
    ts.steam_id, ts.rank, ts.rank_score, ts.team_id, ts.signed_up_at \
    FROM tournament_signups ts \
    LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM sessions \
        WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s \
        ON s.discord_id = ts.discord_id \
    LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM team_members \
        WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) tm \
        ON tm.discord_id = ts.discord_id \
    LEFT JOIN user_profiles p ON p.discord_id = ts.discord_id \
    WHERE ts.tournament_id = ? ORDER BY ts.signed_up_at DESC";

async fn load_signup_rows(pool: &Pool, tournament_id: i64) -> WebResult<Vec<SignupRow>> {
    let rows = sqlx::query(SIGNUP_SELECT)
        .bind(tournament_id)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(SignupRow {
            id: row.try_get("id")?,
            tournament_id: row.try_get("tournament_id")?,
            discord_id: row.try_get("discord_id")?,
            signup_discord_name: row.try_get("signup_discord_name")?,
            session_discord_name: row.try_get("session_discord_name")?,
            team_member_discord_name: row.try_get("team_member_discord_name")?,
            profile_display_name: row.try_get("profile_display_name")?,
            steam_id: row.try_get("steam_id")?,
            rank: row.try_get("rank")?,
            rank_score: row.try_get::<Option<i64>, _>("rank_score")?.unwrap_or(0),
            team_id: row.try_get("team_id")?,
            signed_up_at: row.try_get("signed_up_at")?,
        });
    }
    Ok(out)
}

/// Lädt alle Signups eines Turniers in interner Sicht (rang-angereichert).
/// Entspricht `_load_signups_for_tournament` (routes.py:293).
pub async fn load_signups_for_tournament(
    state: &AppState,
    tournament_id: i64,
) -> WebResult<Vec<TournamentSignup>> {
    let rows = load_signup_rows(&state.pool, tournament_id).await?;
    let ranks = enrich_for_ids(
        state,
        rows.iter().map(|r| r.discord_id.clone()).collect(),
    )
    .await?;
    let signups = rows
        .into_iter()
        .map(|r| {
            let preferred = r.preferred_name();
            let mut fields =
                RankFields { steam_id: r.steam_id, rank: r.rank, rank_score: r.rank_score };
            apply_profile(&mut fields, ranks.get(&r.discord_id));
            TournamentSignup {
                id: r.id,
                tournament_id: r.tournament_id,
                discord_id: r.discord_id,
                discord_name: preferred,
                steam_id: fields.steam_id,
                rank: fields.rank,
                rank_score: fields.rank_score,
                team_id: r.team_id,
                signed_up_at: r.signed_up_at,
            }
        })
        .collect();
    Ok(signups)
}

/// Lädt Signups in öffentlicher Sicht (ohne discord_id/steam_id). Entspricht
/// `_load_signups_public` (routes.py:333). Bewusst OHNE Rang-Anreicherung.
pub async fn load_signups_public(
    pool: &Pool,
    tournament_id: i64,
) -> WebResult<Vec<TournamentSignupPublic>> {
    let rows = load_signup_rows(pool, tournament_id).await?;
    let signups = rows
        .into_iter()
        .map(|r| {
            let preferred = r.preferred_name();
            TournamentSignupPublic {
                id: r.id,
                tournament_id: r.tournament_id,
                discord_name: preferred,
                rank: r.rank,
                rank_score: r.rank_score,
                team_id: r.team_id,
                signed_up_at: r.signed_up_at,
            }
        })
        .collect();
    Ok(signups)
}

// ---------------------------------------------------------------------------
// Gruppen-/Bracket-Loader
// ---------------------------------------------------------------------------

/// Lädt alle Gruppen eines Turniers inkl. Tabellen und Matches.
/// Entspricht `_load_groups_for_tournament` (routes.py:207).
pub async fn load_groups_for_tournament(pool: &Pool, tournament_id: i64) -> WebResult<Vec<Group>> {
    #[derive(sqlx::FromRow)]
    struct GroupStammRow {
        id: i64,
        tournament_id: i64,
        name: String,
        seeding_order: i64,
    }

    let group_rows: Vec<GroupStammRow> =
        sqlx::query_as("SELECT * FROM groups WHERE tournament_id = ? ORDER BY seeding_order")
            .bind(tournament_id)
            .fetch_all(pool)
            .await?;

    let mut groups = Vec::with_capacity(group_rows.len());
    for g in group_rows {
        let team_rows = sqlx::query(
            "SELECT gt.id, gt.group_id, gt.team_id, t.name AS team_name, \
                    gt.wins, gt.losses, gt.points \
             FROM group_teams gt JOIN teams t ON gt.team_id = t.id WHERE gt.group_id = ?",
        )
        .bind(g.id)
        .fetch_all(pool)
        .await?;
        let mut teams = Vec::with_capacity(team_rows.len());
        for row in team_rows {
            teams.push(GroupTeam {
                id: row.try_get("id")?,
                group_id: row.try_get("group_id")?,
                team_id: row.try_get("team_id")?,
                team_name: row.try_get("team_name")?,
                wins: row.try_get("wins")?,
                losses: row.try_get("losses")?,
                points: row.try_get("points")?,
            });
        }

        let match_rows = sqlx::query(
            "SELECT id, group_id, team1_id, team2_id, winner_id, status, steam_party_id, \
                    party_code, deadlock_match_id, match_duration_s, match_stats, \
                    hero_assignments, scheduled_at, played_at \
             FROM group_matches WHERE group_id = ?",
        )
        .bind(g.id)
        .fetch_all(pool)
        .await?;
        let mut matches = Vec::with_capacity(match_rows.len());
        for row in match_rows {
            matches.push(GroupMatch {
                id: row.try_get("id")?,
                group_id: row.try_get("group_id")?,
                team1_id: row.try_get("team1_id")?,
                team2_id: row.try_get("team2_id")?,
                winner_id: row.try_get("winner_id")?,
                status: row.try_get("status")?,
                steam_party_id: row.try_get("steam_party_id")?,
                party_code: row.try_get("party_code")?,
                deadlock_match_id: row.try_get("deadlock_match_id")?,
                match_duration_s: row.try_get("match_duration_s")?,
                match_stats: row.try_get("match_stats")?,
                hero_assignments: turnier_core::json::parse_object(
                    row.try_get::<Option<String>, _>("hero_assignments")?.as_deref(),
                ),
                scheduled_at: row.try_get("scheduled_at")?,
                played_at: row.try_get("played_at")?,
            });
        }

        groups.push(Group {
            id: g.id,
            tournament_id: g.tournament_id,
            name: g.name,
            seeding_order: g.seeding_order,
            teams,
            matches,
        });
    }
    Ok(groups)
}

/// Lädt alle Bracket-Matches eines Turniers. Entspricht `_load_bracket_matches`
/// (routes.py:239). `series_wins_*`/`games` defaulten wie im Pydantic-Modell.
pub async fn load_bracket_matches(pool: &Pool, tournament_id: i64) -> WebResult<Vec<BracketMatch>> {
    let rows = sqlx::query(
        "SELECT * FROM bracket_matches WHERE tournament_id = ? ORDER BY round, position",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(BracketMatch {
            id: row.try_get("id")?,
            tournament_id: row.try_get("tournament_id")?,
            round: row.try_get("round")?,
            position: row.try_get("position")?,
            bracket_type: row.try_get("bracket_type")?,
            mini_group_id: row.try_get("mini_group_id")?,
            team1_id: row.try_get("team1_id")?,
            team2_id: row.try_get("team2_id")?,
            winner_id: row.try_get("winner_id")?,
            status: row.try_get("status")?,
            source_match1_id: row.try_get("source_match1_id")?,
            source_match2_id: row.try_get("source_match2_id")?,
            loser_to_match_id: row.try_get("loser_to_match_id")?,
            loser_to_slot: row.try_get("loser_to_slot")?,
            steam_party_id: row.try_get("steam_party_id")?,
            party_code: row.try_get("party_code")?,
            deadlock_match_id: row.try_get("deadlock_match_id")?,
            match_duration_s: row.try_get("match_duration_s")?,
            match_stats: row.try_get("match_stats")?,
            hero_assignments: turnier_core::json::parse_object(
                row.try_get::<Option<String>, _>("hero_assignments")?.as_deref(),
            ),
            series_wins_team1: 0,
            series_wins_team2: 0,
            games: Vec::new(),
            scheduled_at: row.try_get("scheduled_at")?,
            on_stream: row.try_get::<Option<i64>, _>("on_stream")?.unwrap_or(1) != 0,
            played_at: row.try_get("played_at")?,
        });
    }
    Ok(out)
}

/// Lädt die Mini-Groups eines Turniers (nur IDs). Entspricht
/// `_load_mini_groups_for_tournament` (routes.py:249).
pub async fn load_mini_groups_for_tournament(
    pool: &Pool,
    tournament_id: i64,
) -> WebResult<Vec<BracketMiniGroup>> {
    #[derive(sqlx::FromRow)]
    struct MiniGroupRow {
        id: i64,
        tournament_id: i64,
        round: i64,
        position: i64,
        advances_to_match_id: Option<i64>,
        advances_to_slot: Option<i64>,
    }

    let rows: Vec<MiniGroupRow> = sqlx::query_as(
        "SELECT id, tournament_id, round, position, advances_to_match_id, advances_to_slot \
         FROM bracket_mini_groups WHERE tournament_id = ? ORDER BY round, position, id",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let team_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT team_id FROM bracket_mini_group_teams \
             WHERE mini_group_id = ? AND team_id IS NOT NULL ORDER BY seed_order, id",
        )
        .bind(row.id)
        .fetch_all(pool)
        .await?;
        let match_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM bracket_matches WHERE mini_group_id = ? ORDER BY round, position, id",
        )
        .bind(row.id)
        .fetch_all(pool)
        .await?;
        out.push(BracketMiniGroup {
            id: row.id,
            tournament_id: row.tournament_id,
            round: row.round,
            position: row.position,
            advances_to_match_id: row.advances_to_match_id,
            advances_to_slot: row.advances_to_slot,
            team_ids,
            match_ids,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Turnier-Rohzeile → DTOs
// ---------------------------------------------------------------------------

/// Die für die Turnier-DTOs relevanten Spalten der `tournaments`-Tabelle.
/// Wird per `SELECT *` befüllt, daher als manuelles Row-Mapping über `Row`.
pub struct TournamentRowData {
    pub id: i64,
    pub name: String,
    pub status: turnier_core::TournamentStatus,
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
    pub tournament_mode: turnier_core::TournamentMode,
    pub tournament_game_mode: turnier_core::TournamentGameMode,
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

impl TournamentRowData {
    /// Liest die DTO-relevanten Spalten aus einer `tournaments`-Zeile.
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> WebResult<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            status: row.try_get("status")?,
            description: row.try_get("description")?,
            team_size: row.try_get("team_size")?,
            series_format: row.try_get("series_format")?,
            final_series_format: row.try_get("final_series_format")?,
            registration_start: row.try_get("registration_start")?,
            registration_end: row.try_get("registration_end")?,
            checkin_start: row.try_get("checkin_start")?,
            group_phase_start: row.try_get("group_phase_start")?,
            bracket_start: row.try_get("bracket_start")?,
            bracket_format: row.try_get("bracket_format")?,
            tournament_mode: row.try_get("tournament_mode")?,
            tournament_game_mode: row.try_get("tournament_game_mode")?,
            auto_lobby_enabled: row.try_get::<i64, _>("auto_lobby_enabled")? != 0,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            invite_mode: row.try_get("invite_mode")?,
            invite_window_start: row.try_get("invite_window_start")?,
            invite_window_end: row.try_get("invite_window_end")?,
            lobby_settings: row.try_get("lobby_settings")?,
            exclude_from_leaderboard: row.try_get::<i64, _>("exclude_from_leaderboard")? != 0,
            reminder_offsets: turnier_core::json::parse_offsets(
                row.try_get::<Option<String>, _>("reminder_offsets")?.as_deref(),
                &turnier_core::json::default_reminder_offsets(),
            ),
            start_reminder_offsets: turnier_core::json::parse_offsets(
                row.try_get::<Option<String>, _>("start_reminder_offsets")?.as_deref(),
                &turnier_core::json::default_start_reminder_offsets(),
            ),
            match_objective: row.try_get("match_objective")?,
            no_show_grace_minutes: row.try_get("no_show_grace_minutes")?,
            rules: row.try_get("rules")?,
            is_test: row.try_get::<i64, _>("is_test")? != 0,
        })
    }

    /// Baut die volle `Tournament`-Lese-Sicht.
    pub fn into_tournament(self) -> Tournament {
        Tournament {
            id: self.id,
            name: self.name,
            status: self.status,
            description: self.description,
            team_size: self.team_size,
            series_format: self.series_format,
            final_series_format: self.final_series_format,
            registration_start: self.registration_start,
            registration_end: self.registration_end,
            checkin_start: self.checkin_start,
            group_phase_start: self.group_phase_start,
            bracket_start: self.bracket_start,
            bracket_format: self.bracket_format,
            tournament_mode: self.tournament_mode,
            tournament_game_mode: self.tournament_game_mode,
            auto_lobby_enabled: self.auto_lobby_enabled,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            invite_mode: self.invite_mode,
            invite_window_start: self.invite_window_start,
            invite_window_end: self.invite_window_end,
            lobby_settings: self.lobby_settings,
            exclude_from_leaderboard: self.exclude_from_leaderboard,
            reminder_offsets: self.reminder_offsets,
            start_reminder_offsets: self.start_reminder_offsets,
            match_objective: self.match_objective,
            no_show_grace_minutes: self.no_show_grace_minutes,
            rules: self.rules,
            is_test: self.is_test,
        }
    }
}

/// Liest alle Turniere (DTO-Form) gemäß einer Filterbedingung.
pub async fn list_tournament_dtos(pool: &Pool, where_order: &str) -> WebResult<Vec<Tournament>> {
    let rows = sqlx::query(&format!("SELECT * FROM tournaments {where_order}"))
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(TournamentRowData::from_row(&row)?.into_tournament());
    }
    Ok(out)
}

/// Baut die öffentliche Turnier-Detail-Sicht aus Rohdaten + geladenen Listen.
#[allow(clippy::too_many_arguments)]
pub fn build_detail_public(
    t: TournamentRowData,
    teams: Vec<TeamPublic>,
    groups: Vec<Group>,
    bracket_matches: Vec<BracketMatch>,
    mini_groups: Vec<BracketMiniGroup>,
    signups: Vec<TournamentSignupPublic>,
) -> TournamentDetailPublic {
    TournamentDetailPublic {
        id: t.id,
        name: t.name,
        status: t.status,
        description: t.description,
        team_size: t.team_size,
        series_format: t.series_format,
        final_series_format: t.final_series_format,
        registration_start: t.registration_start,
        registration_end: t.registration_end,
        group_phase_start: t.group_phase_start,
        bracket_start: t.bracket_start,
        bracket_format: t.bracket_format,
        tournament_mode: t.tournament_mode,
        tournament_game_mode: t.tournament_game_mode,
        auto_lobby_enabled: t.auto_lobby_enabled,
        created_by: t.created_by,
        created_at: t.created_at,
        updated_at: t.updated_at,
        invite_mode: t.invite_mode,
        invite_window_start: t.invite_window_start,
        invite_window_end: t.invite_window_end,
        lobby_settings: t.lobby_settings,
        rules: t.rules,
        is_test: t.is_test,
        teams,
        groups,
        bracket_matches,
        mini_groups,
        signups,
    }
}

// ---------------------------------------------------------------------------
// Guards & 404-Helfer
// ---------------------------------------------------------------------------

/// Eine vollständige Turnier-Zeile als generische Map für Guards (status,
/// team_size, invite_mode, …). Wir lesen die wenigen gebrauchten Felder gezielt.
pub struct TournamentGuard {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub team_size: i64,
    pub invite_mode: Option<String>,
    pub invite_window_start: Option<String>,
    pub invite_window_end: Option<String>,
    pub is_test: bool,
}

/// Lädt die Guard-Felder eines Turniers oder liefert 404.
/// Entspricht `_load_tournament_or_404` (routes.py:413).
pub async fn load_tournament_or_404(pool: &Pool, tournament_id: i64) -> WebResult<TournamentGuard> {
    let row = sqlx::query(
        "SELECT id, name, status, team_size, invite_mode, invite_window_start, \
                invite_window_end, is_test FROM tournaments WHERE id = ?",
    )
    .bind(tournament_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    };
    Ok(TournamentGuard {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        status: row.try_get("status")?,
        team_size: row.try_get("team_size")?,
        invite_mode: row.try_get("invite_mode")?,
        invite_window_start: row.try_get("invite_window_start")?,
        invite_window_end: row.try_get("invite_window_end")?,
        is_test: row.try_get::<i64, _>("is_test")? != 0,
    })
}

/// Eine Team-Stammzeile für Guards.
#[derive(sqlx::FromRow)]
pub struct TeamGuard {
    pub id: i64,
    pub tournament_id: i64,
    pub name: String,
    pub captain_discord_id: String,
    pub recruitment_status: String,
}

/// Lädt ein Team oder liefert 404. Entspricht `_load_team_or_404` (routes.py:427).
pub async fn load_team_or_404(
    pool: &Pool,
    tournament_id: i64,
    team_id: i64,
) -> WebResult<TeamGuard> {
    sqlx::query_as::<_, TeamGuard>(
        "SELECT id, tournament_id, name, captain_discord_id, recruitment_status \
         FROM teams WHERE id = ? AND tournament_id = ?",
    )
    .bind(team_id)
    .bind(tournament_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| WebError::not_found("Team nicht gefunden"))
}

/// Prüft, ob die Anmeldung offen ist (status in registration|checkin) — 400 sonst.
/// Entspricht `_ensure_registration_open` (routes.py:441).
pub fn ensure_registration_open(status: &str) -> WebResult<()> {
    if status != "registration" && status != "checkin" {
        return Err(WebError::bad_request("Anmeldung ist nicht geöffnet"));
    }
    Ok(())
}

/// Mod- oder Admin-Rechte. Entspricht `_is_mod_user` (routes.py:449).
pub fn is_mod_user(user: &UserSession) -> bool {
    user.is_mod || user.is_admin
}

/// Erzwingt Captain-Rechte (403 sonst). Entspricht `_ensure_captain` (routes.py:453).
pub fn ensure_captain(user: &UserSession, captain_discord_id: &str) -> WebResult<()> {
    if user.discord_id != captain_discord_id {
        return Err(WebError::forbidden("Nur der Captain darf diese Aktion ausführen"));
    }
    Ok(())
}

/// Erzwingt Captain- ODER Mod-Rechte (403 sonst).
/// Entspricht `_ensure_captain_or_mod` (routes.py:461).
pub fn ensure_captain_or_mod(user: &UserSession, captain_discord_id: &str) -> WebResult<()> {
    if user.discord_id == captain_discord_id || is_mod_user(user) {
        return Ok(());
    }
    Err(WebError::forbidden("Nur Captain oder Mod dürfen diese Aktion ausführen"))
}

/// Prüft, ob Einladungen erlaubt sind, und liefert ggf. das Ablaufdatum
/// (Window-Ende). 403 bei `never` oder außerhalb des Fensters.
/// Entspricht `_ensure_invites_enabled` (routes.py:470).
pub fn ensure_invites_enabled(t: &TournamentGuard) -> WebResult<Option<String>> {
    // Vergleich auf den rohen DB-String wie im Original (`mode == InviteMode.x.value`);
    // leerer/NULL-Wert defaultet auf "always".
    let mode = t.invite_mode.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| "always".to_string());
    if mode == "never" {
        return Err(WebError::forbidden("Einladungen sind für dieses Turnier deaktiviert"));
    }
    if mode == "window" {
        let now = Utc::now();
        let start = parse_timestamp(t.invite_window_start.as_deref());
        let end = parse_timestamp(t.invite_window_end.as_deref());
        match (start, end) {
            (Some(start), Some(end)) if now >= start && now <= end => {
                return Ok(t.invite_window_end.clone());
            }
            _ => {
                return Err(WebError::forbidden("Einladungen sind aktuell nicht erlaubt"));
            }
        }
    }
    Ok(None)
}

/// Zählt die Mitglieder eines Teams. Entspricht `_count_team_members` (routes.py:492).
pub async fn count_team_members<'e, E>(executor: E, team_id: i64) -> WebResult<i64>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM team_members WHERE team_id = ?")
            .bind(team_id)
            .fetch_one(executor)
            .await?;
    Ok(count)
}

/// Stellt sicher, dass das Team noch Kapazität hat (400 sonst).
/// Entspricht `_ensure_team_has_capacity` (routes.py:501).
pub async fn ensure_team_has_capacity<'e, E>(
    executor: E,
    team_id: i64,
    team_size: i64,
) -> WebResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    if count_team_members(executor, team_id).await? >= team_size {
        return Err(WebError::bad_request("Team ist bereits voll"));
    }
    Ok(())
}

/// Stellt sicher, dass der Spieler in keinem Team des Turniers ist (409 sonst).
/// Entspricht `_ensure_user_not_in_tournament_team` (routes.py:509).
pub async fn ensure_user_not_in_tournament_team<'e, E>(
    executor: E,
    tournament_id: i64,
    discord_id: &str,
) -> WebResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT tm.id FROM team_members tm JOIN teams t ON tm.team_id = t.id \
         WHERE t.tournament_id = ? AND tm.discord_id = ?",
    )
    .bind(tournament_id)
    .bind(discord_id)
    .fetch_optional(executor)
    .await?;
    if existing.is_some() {
        return Err(WebError::conflict("Spieler ist bereits in einem Team"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Consent
// ---------------------------------------------------------------------------

/// Prüft die Datenschutz-Einwilligung. Robust gegen NULL/Leer-Werte
/// (Original-Bug `int(...)` ValueError → 500 wird vermieden, „safe"):
/// fehlt/zu alt → 403 `CONSENT_REQUIRED`.
pub async fn ensure_consent<'e, E>(executor: E, discord_id: &str) -> WebResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT consent_version FROM user_consents WHERE discord_id = ?")
            .bind(discord_id)
            .fetch_optional(executor)
            .await?;
    let version = row.and_then(|r| r.0).unwrap_or(0);
    if version < CURRENT_CONSENT_VERSION {
        return Err(WebError::forbidden("CONSENT_REQUIRED"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Audit-Log
// ---------------------------------------------------------------------------

/// Schreibt einen Audit-Log-Eintrag (innerhalb der laufenden Verbindung/Tx).
/// Entspricht `_audit` (routes.py:61).
pub async fn audit<'e, E>(
    executor: E,
    action: &str,
    user_id: Option<&str>,
    details: &Value,
) -> WebResult<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query("INSERT INTO audit_log (action, user_id, details) VALUES (?, ?, ?)")
        .bind(action)
        .bind(user_id)
        .bind(details.to_string())
        .execute(executor)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Signup-Synchronisation
// ---------------------------------------------------------------------------

/// Rangdaten für einen Signup-/Member-Eintrag.
pub struct RankInput {
    pub steam_id: Option<String>,
    pub rank: Option<String>,
    pub rank_score: i64,
}

/// Lädt das Rang-Profil eines Spielers und mappt es auf `RankInput`
/// (für die direkten `get_player_rank_profile`-Aufrufe in den Mutations-Routen).
/// Fehler degradieren still zu Default-Werten.
pub async fn load_rank_input(state: &AppState, discord_id: &str) -> RankInput {
    match state.rank_resolver.rank_profile(discord_id).await {
        Ok(Some(profile)) => RankInput {
            steam_id: profile.steam_id,
            rank: profile.rank,
            rank_score: profile.rank_score,
        },
        _ => RankInput { steam_id: None, rank: None, rank_score: 0 },
    }
}

/// Upsert in `tournament_signups`: aktualisiert den vorhandenen Eintrag oder legt
/// einen neuen an. Entspricht `_upsert_signup` (routes.py:366).
pub async fn upsert_signup(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tournament_id: i64,
    discord_id: &str,
    discord_name: Option<&str>,
    rank: &RankInput,
    team_id: Option<i64>,
) -> WebResult<()> {
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(discord_id)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some((id,)) = existing {
        sqlx::query(
            "UPDATE tournament_signups SET discord_name = ?, steam_id = ?, rank = ?, \
             rank_score = ?, team_id = ? WHERE id = ?",
        )
        .bind(discord_name)
        .bind(&rank.steam_id)
        .bind(&rank.rank)
        .bind(rank.rank_score)
        .bind(team_id)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO tournament_signups \
             (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(tournament_id)
        .bind(discord_id)
        .bind(discord_name)
        .bind(&rank.steam_id)
        .bind(&rank.rank)
        .bind(rank.rank_score)
        .bind(team_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Fügt einen Spieler einem Team hinzu (Member-Insert + Signup-Sync mit
/// aufgelöstem Namen). Entspricht `_add_user_to_team` (routes.py:581).
pub async fn add_user_to_team(
    pool: &Pool,
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tournament_id: i64,
    team_id: i64,
    discord_id: &str,
    discord_name: Option<&str>,
    rank: &RankInput,
) -> WebResult<()> {
    // Name-Resolution liest aus mehreren Tabellen — über den Pool (read-only).
    // Entspricht dem Original, das im selben Connection-Kontext liest.
    let resolved_name = resolve_discord_name(pool, discord_id, discord_name).await?;
    sqlx::query(
        "INSERT INTO team_members \
         (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) \
         VALUES (?, ?, ?, ?, ?, ?, 'member')",
    )
    .bind(team_id)
    .bind(discord_id)
    .bind(&resolved_name)
    .bind(&rank.steam_id)
    .bind(&rank.rank)
    .bind(rank.rank_score)
    .execute(&mut **tx)
    .await?;
    upsert_signup(tx, tournament_id, discord_id, Some(&resolved_name), rank, Some(team_id)).await?;
    Ok(())
}
