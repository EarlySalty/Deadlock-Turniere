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
use serde::Deserialize;
use serde_json::Value;
use sqlx::{Postgres, Row};

use turnier_core::bench::{self, Lineup, SeatRequest};
use turnier_core::{
    BracketMiniGroup, GroupTeam, InviteMode, NoLobbyAccess, PublicBracketMatch, PublicGroup,
    PublicGroupMatch, RankProfile, RecruitmentStatus, Team, TeamMember, TeamMemberPublic,
    TeamPublic, TeamRole, Tournament, TournamentDetailPublic, TournamentSignup,
    TournamentSignupPublic, UserSession,
};
use turnier_db::Pool;

use crate::db;
use crate::error::{map_unique_conflict, WebError, WebResult};
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
    let mut conn = pool.acquire().await?;
    resolve_discord_name_in(&mut conn, discord_id, preferred_name).await
}

pub async fn resolve_discord_name_in(
    conn: &mut sqlx::PgConnection,
    discord_id: &str,
    preferred_name: Option<&str>,
) -> WebResult<String> {
    if let Some(name) = sanitize_discord_name(preferred_name, Some(discord_id)) {
        return Ok(name);
    }

    let session_name: Option<(Option<String>,)> = sqlx::query_as(
        r#"SELECT discord_name FROM turnier."sessions"
         WHERE discord_id = $1 AND discord_name IS NOT NULL AND discord_name != ''
         ORDER BY token DESC LIMIT 1"#,
    )
    .bind(db::parse_participant_id(discord_id)?)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((Some(name),)) = session_name {
        if let Some(sanitized) = sanitize_discord_name(Some(&name), Some(discord_id)) {
            return Ok(sanitized);
        }
    }

    let member_name: Option<(Option<String>,)> = sqlx::query_as(
        r#"SELECT discord_name FROM turnier."team_members"
         WHERE discord_id = $1 AND discord_name IS NOT NULL AND discord_name != ''
         ORDER BY joined_at DESC LIMIT 1"#,
    )
    .bind(db::parse_participant_id(discord_id)?)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((Some(name),)) = member_name {
        if let Some(sanitized) = sanitize_discord_name(Some(&name), Some(discord_id)) {
            return Ok(sanitized);
        }
    }

    let profile_name: Option<(Option<String>,)> = sqlx::query_as(
        r#"SELECT display_name FROM turnier."user_profiles"
         WHERE discord_id = $1 AND display_name IS NOT NULL AND display_name != ''
         ORDER BY updated_at DESC LIMIT 1"#,
    )
    .bind(db::parse_participant_id(discord_id)?)
    .fetch_optional(&mut *conn)
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
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&normalized, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc));
        }
    }
    None
}

/// Ein roher Member-/Signup-Datensatz vor der Rang-Anreicherung.
struct RankFields {
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: i64,
}

/// Wendet ein Discord-first-Profil auf die Rangfelder an, exakt wie
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
    tm.steam_id, tm.rank, tm.rank_score, tm.role, tm.is_bench, tm.joined_at \
    FROM turnier.\"team_members\" tm \
    LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM turnier.\"sessions\" \
        WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s \
        ON s.discord_id = tm.discord_id \
    LEFT JOIN turnier.\"user_profiles\" p ON p.discord_id = tm.discord_id \
    WHERE tm.team_id = $1 ORDER BY tm.is_bench, tm.joined_at";

/// Eine angereicherte Member-Zeile (Namens-Kandidaten + Rangfelder).
struct MemberRow {
    id: i64,
    team_id: i64,
    discord_id: i64,
    team_member_discord_name: Option<String>,
    session_discord_name: Option<String>,
    profile_display_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: i64,
    role: turnier_core::TeamRole,
    is_bench: bool,
    joined_at: DateTime<Utc>,
}

impl MemberRow {
    fn preferred_name(&self) -> Option<String> {
        preferred_discord_name(
            &[
                self.profile_display_name.as_deref(),
                self.session_discord_name.as_deref(),
                self.team_member_discord_name.as_deref(),
            ],
            Some(&db::discord_id_to_string(self.discord_id)),
        )
    }
}

/// Lädt die Member-Zeilen eines Teams (rohe Namens-Kandidaten + Rangfelder).
async fn load_member_rows(pool: &Pool, team_id: i64) -> WebResult<Vec<MemberRow>> {
    let rows = sqlx::query(MEMBER_SELECT)
        .bind(team_id)
        .fetch_all(pool)
        .await?;
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
            is_bench: row.try_get("is_bench")?,
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
    captain_discord_id: i64,
    created_at: DateTime<Utc>,
    recruitment_status: RecruitmentStatus,
}

/// Baut einen `TeamMember` aus Zeile + Anreicherungs-Map.
fn member_from_row(row: MemberRow, ranks: &HashMap<String, RankProfile>) -> TeamMember {
    let preferred = row.preferred_name();
    let discord_id = db::discord_id_to_string(row.discord_id);
    let mut fields = RankFields {
        steam_id: row.steam_id,
        rank: row.rank,
        rank_score: row.rank_score,
    };
    apply_profile(&mut fields, ranks.get(&discord_id));
    TeamMember {
        id: row.id,
        team_id: row.team_id,
        discord_id,
        discord_name: preferred,
        steam_id: fields.steam_id,
        rank: fields.rank,
        rank_score: fields.rank_score,
        role: row.role,
        is_bench: row.is_bench,
        joined_at: db::ts_to_string(row.joined_at),
    }
}

/// Lädt alle Teams eines Turniers inkl. Mitglieder (interne Sicht, rang-angereichert).
/// Entspricht `_load_teams_for_tournament` (routes.py:119).
pub async fn load_teams_for_tournament(
    state: &AppState,
    tournament_id: i64,
) -> WebResult<Vec<Team>> {
    let pool = &state.pool;
    let team_rows: Vec<TeamRow> =
        sqlx::query_as(r#"SELECT * FROM turnier."teams" WHERE tournament_id = $1"#)
            .bind(tournament_id)
            .fetch_all(pool)
            .await?;

    let mut teams = Vec::with_capacity(team_rows.len());
    for t in team_rows {
        let member_rows = load_member_rows(pool, t.id).await?;
        let ranks = enrich_for_ids(
            state,
            member_rows
                .iter()
                .map(|m| db::discord_id_to_string(m.discord_id))
                .collect(),
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
            captain_discord_id: db::discord_id_to_string(t.captain_discord_id),
            created_at: db::ts_to_string(t.created_at),
            recruitment_status: t.recruitment_status,
            members,
        });
    }
    Ok(teams)
}

/// Lädt Teams in der öffentlichen Sicht (ohne discord_id/captain in Members,
/// mit `has_pending_applications`-Flag). Entspricht `_load_teams_public`
/// (routes.py:159). Bewusst OHNE Rang-Anreicherung, 1:1 zum Original.
pub async fn load_teams_public(pool: &Pool, tournament_id: i64) -> WebResult<Vec<TeamPublic>> {
    let team_rows: Vec<TeamRow> =
        sqlx::query_as(r#"SELECT * FROM turnier."teams" WHERE tournament_id = $1"#)
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
                    is_bench: m.is_bench,
                    joined_at: db::ts_to_string(m.joined_at),
                }
            })
            .collect();

        let app_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM turnier."team_applications" WHERE team_id = $1 AND status = 'pending'"#,
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
            created_at: db::ts_to_string(t.created_at),
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
    let t: TeamRow = sqlx::query_as(r#"SELECT * FROM turnier."teams" WHERE id = $1"#)
        .bind(team_id)
        .fetch_one(pool)
        .await?;
    let member_rows = load_member_rows(pool, team_id).await?;
    let ranks = enrich_for_ids(
        state,
        member_rows
            .iter()
            .map(|m| db::discord_id_to_string(m.discord_id))
            .collect(),
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
        captain_discord_id: db::discord_id_to_string(t.captain_discord_id),
        created_at: db::ts_to_string(t.created_at),
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
            tracing::warn!(error = %err, "Rang-Anreicherung fehlgeschlagen, ohne Profil weiter");
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
    discord_id: i64,
    signup_discord_name: Option<String>,
    session_discord_name: Option<String>,
    team_member_discord_name: Option<String>,
    profile_display_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: i64,
    team_id: Option<i64>,
    signed_up_at: DateTime<Utc>,
}

impl SignupRow {
    fn preferred_name(&self) -> Option<String> {
        let discord_id = db::discord_id_to_string(self.discord_id);
        preferred_discord_name(
            &[
                self.profile_display_name.as_deref(),
                self.session_discord_name.as_deref(),
                self.signup_discord_name.as_deref(),
                self.team_member_discord_name.as_deref(),
            ],
            Some(&discord_id),
        )
    }
}

/// SELECT für tournament_signups inkl. Namens-Kandidaten aus sessions/
/// team_members/user_profiles. Konsolidiert die doppelte Query.
const SIGNUP_SELECT: &str = "SELECT ts.id, ts.tournament_id, ts.discord_id, \
    ts.discord_name AS signup_discord_name, s.discord_name AS session_discord_name, \
    tm.discord_name AS team_member_discord_name, p.display_name AS profile_display_name, \
    ts.steam_id, ts.rank, ts.rank_score, ts.team_id, ts.signed_up_at \
    FROM turnier.\"tournament_signups\" ts \
    LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM turnier.\"sessions\" \
        WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s \
        ON s.discord_id = ts.discord_id \
    LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM turnier.\"team_members\" \
        WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) tm \
        ON tm.discord_id = ts.discord_id \
    LEFT JOIN turnier.\"user_profiles\" p ON p.discord_id = ts.discord_id \
    WHERE ts.tournament_id = $1 ORDER BY ts.signed_up_at DESC";

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
        rows.iter()
            .map(|r| db::discord_id_to_string(r.discord_id))
            .collect(),
    )
    .await?;
    let signups = rows
        .into_iter()
        .map(|r| {
            let preferred = r.preferred_name();
            let discord_id = db::discord_id_to_string(r.discord_id);
            let mut fields = RankFields {
                steam_id: r.steam_id,
                rank: r.rank,
                rank_score: r.rank_score,
            };
            apply_profile(&mut fields, ranks.get(&discord_id));
            TournamentSignup {
                id: r.id,
                tournament_id: r.tournament_id,
                discord_id,
                discord_name: preferred,
                steam_id: fields.steam_id,
                rank: fields.rank,
                rank_score: fields.rank_score,
                team_id: r.team_id,
                signed_up_at: db::ts_to_string(r.signed_up_at),
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
                signed_up_at: db::ts_to_string(r.signed_up_at),
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
pub async fn load_groups_for_tournament(
    pool: &Pool,
    tournament_id: i64,
) -> WebResult<Vec<PublicGroup>> {
    #[derive(sqlx::FromRow)]
    struct GroupStammRow {
        id: i64,
        tournament_id: i64,
        name: String,
        seeding_order: i64,
    }

    let group_rows: Vec<GroupStammRow> = sqlx::query_as(
        r#"SELECT * FROM turnier."groups" WHERE tournament_id = $1 ORDER BY seeding_order"#,
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let mut groups = Vec::with_capacity(group_rows.len());
    for g in group_rows {
        let team_rows = sqlx::query(
            r#"SELECT gt.id, gt.group_id, gt.team_id, t.name AS team_name,
                    gt.wins, gt.losses, gt.points
             FROM turnier."group_teams" gt JOIN turnier."teams" t ON gt.team_id = t.id WHERE gt.group_id = $1 ORDER BY gt.id"#,
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
                seed_tiebreak: false,
            });
        }

        let match_rows = sqlx::query(
            r#"SELECT id, group_id, team1_id, team2_id, winner_id, status,
                    deadlock_match_id, match_duration_s, match_stats,
                    hero_assignments, scheduled_at, played_at
             FROM turnier."group_matches" WHERE group_id = $1"#,
        )
        .bind(g.id)
        .fetch_all(pool)
        .await?;
        let mut matches = Vec::with_capacity(match_rows.len());
        for row in match_rows {
            matches.push(PublicGroupMatch {
                id: row.try_get("id")?,
                group_id: row.try_get("group_id")?,
                team1_id: row.try_get("team1_id")?,
                team2_id: row.try_get("team2_id")?,
                winner_id: row.try_get("winner_id")?,
                status: row.try_get("status")?,
                lobby: NoLobbyAccess {},
                deadlock_match_id: row.try_get("deadlock_match_id")?,
                match_duration_s: row.try_get("match_duration_s")?,
                match_stats: db::json_to_wire(row.try_get("match_stats")?),
                hero_assignments: db::json_object(row.try_get("hero_assignments")?),
                scheduled_at: db::opt_ts_to_string(row.try_get("scheduled_at")?),
                played_at: db::opt_ts_to_string(row.try_get("played_at")?),
                round: None,
            });
        }

        groups.push(PublicGroup {
            id: g.id,
            tournament_id: g.tournament_id,
            name: g.name,
            seeding_order: g.seeding_order,
            teams,
            matches,
        });
    }
    crate::league_view::apply_league_view(pool, tournament_id, &mut groups).await?;
    Ok(groups)
}

/// Lädt alle Bracket-Matches eines Turniers. Entspricht `_load_bracket_matches`
/// (routes.py:239). `series_wins_*`/`games` defaulten wie im Pydantic-Modell.
pub async fn load_bracket_matches(
    pool: &Pool,
    tournament_id: i64,
) -> WebResult<Vec<PublicBracketMatch>> {
    let rows = sqlx::query(
        r#"SELECT id, tournament_id, round, position, bracket_type, mini_group_id, team1_id,
                team2_id, winner_id, status, source_match1_id, source_match2_id,
                loser_to_match_id, loser_to_slot, deadlock_match_id, match_duration_s,
                match_stats, hero_assignments, scheduled_at, on_stream, played_at
         FROM turnier."bracket_matches" WHERE tournament_id = $1 ORDER BY round, position"#,
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(PublicBracketMatch {
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
            lobby: NoLobbyAccess {},
            deadlock_match_id: row.try_get("deadlock_match_id")?,
            match_duration_s: row.try_get("match_duration_s")?,
            match_stats: db::json_to_wire(row.try_get("match_stats")?),
            hero_assignments: db::json_object(row.try_get("hero_assignments")?),
            series_wins_team1: 0,
            series_wins_team2: 0,
            games: Vec::new(),
            scheduled_at: db::opt_ts_to_string(row.try_get("scheduled_at")?),
            on_stream: row.try_get("on_stream")?,
            played_at: db::opt_ts_to_string(row.try_get("played_at")?),
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
        r#"SELECT id, tournament_id, round, position, advances_to_match_id, advances_to_slot
         FROM turnier."bracket_mini_groups" WHERE tournament_id = $1 ORDER BY round, position, id"#,
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let team_ids: Vec<i64> = sqlx::query_scalar(
            r#"SELECT team_id FROM turnier."bracket_mini_group_teams"
             WHERE mini_group_id = $1 AND team_id IS NOT NULL ORDER BY seed_order, id"#,
        )
        .bind(row.id)
        .fetch_all(pool)
        .await?;
        let match_ids: Vec<i64> = sqlx::query_scalar(
            r#"SELECT id FROM turnier."bracket_matches" WHERE mini_group_id = $1 ORDER BY round, position, id"#,
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

pub fn tournament_select() -> String {
    format!(
        r#"SELECT *, {} AS played_bracket_format FROM turnier."tournaments""#,
        turnier_engine::PLAYED_BRACKET_FORMAT_SQL
    )
}

pub fn tournament_from_row(row: &sqlx::postgres::PgRow) -> WebResult<Tournament> {
    let status: turnier_core::TournamentStatus = row.try_get("status")?;
    let reminder_offsets = db::json_to_wire(row.try_get("reminder_offsets")?);
    let start_reminder_offsets = db::json_to_wire(row.try_get("start_reminder_offsets")?);
    Ok(Tournament {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        status,
        registration_open: status.registration_window_open(),
        replacements_open: status.is_match_phase(),
        description: row.try_get("description")?,
        team_size: row.try_get("team_size")?,
        bench_slots: row.try_get("bench_slots")?,
        series_format: row.try_get("series_format")?,
        final_series_format: row.try_get("final_series_format")?,
        registration_start: db::opt_ts_to_string(row.try_get("registration_start")?),
        registration_end: db::opt_ts_to_string(row.try_get("registration_end")?),
        checkin_start: db::opt_ts_to_string(row.try_get("checkin_start")?),
        group_phase_start: db::opt_ts_to_string(row.try_get("group_phase_start")?),
        bracket_start: db::opt_ts_to_string(row.try_get("bracket_start")?),
        bracket_format: row.try_get("bracket_format")?,
        played_bracket_format: row.try_get("played_bracket_format")?,
        tournament_mode: row.try_get("tournament_mode")?,
        tournament_mode_auto: row.try_get("tournament_mode_auto")?,
        tournament_game_mode: row.try_get("tournament_game_mode")?,
        auto_lobby_enabled: row.try_get("auto_lobby_enabled")?,
        created_by: db::discord_id_to_string(row.try_get("created_by")?),
        created_at: db::ts_to_string(row.try_get("created_at")?),
        updated_at: db::ts_to_string(row.try_get("updated_at")?),
        invite_mode: row.try_get("invite_mode")?,
        invite_window_start: db::opt_ts_to_string(row.try_get("invite_window_start")?),
        invite_window_end: db::opt_ts_to_string(row.try_get("invite_window_end")?),
        lobby_settings: db::json_to_wire(row.try_get("lobby_settings")?),
        exclude_from_leaderboard: row.try_get("exclude_from_leaderboard")?,
        reminder_offsets: turnier_core::json::parse_offsets(
            reminder_offsets.as_deref(),
            &turnier_core::json::default_reminder_offsets(),
        ),
        start_reminder_offsets: turnier_core::json::parse_offsets(
            start_reminder_offsets.as_deref(),
            &turnier_core::json::default_start_reminder_offsets(),
        ),
        match_objective: row.try_get("match_objective")?,
        no_show_grace_minutes: row.try_get("no_show_grace_minutes")?,
        rules: row.try_get("rules")?,
        is_test: row.try_get("is_test")?,
    })
}

/// Liest alle Turniere (DTO-Form) gemäß einer Filterbedingung.
pub async fn list_tournament_dtos(pool: &Pool, where_order: &str) -> WebResult<Vec<Tournament>> {
    let rows = sqlx::query(&format!("{} {where_order}", tournament_select()))
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(tournament_from_row(&row)?);
    }
    Ok(out)
}

/// Baut die öffentliche Turnier-Detail-Sicht aus Rohdaten + geladenen Listen.
#[allow(clippy::too_many_arguments)]
pub fn build_detail_public(
    t: Tournament,
    teams: Vec<TeamPublic>,
    groups: Vec<PublicGroup>,
    bracket_matches: Vec<PublicBracketMatch>,
    mini_groups: Vec<BracketMiniGroup>,
    signups: Vec<TournamentSignupPublic>,
) -> TournamentDetailPublic {
    TournamentDetailPublic {
        id: t.id,
        name: t.name,
        status: t.status,
        registration_open: t.registration_open,
        replacements_open: t.replacements_open,
        description: t.description,
        team_size: t.team_size,
        bench_slots: t.bench_slots,
        series_format: t.series_format,
        final_series_format: t.final_series_format,
        registration_start: t.registration_start,
        registration_end: t.registration_end,
        checkin_start: t.checkin_start,
        group_phase_start: t.group_phase_start,
        bracket_start: t.bracket_start,
        bracket_format: t.bracket_format,
        played_bracket_format: t.played_bracket_format,
        tournament_mode: t.tournament_mode,
        tournament_mode_auto: t.tournament_mode_auto,
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
    pub status: turnier_core::TournamentStatus,
    pub team_size: i64,
    pub bench_slots: i64,
    pub invite_mode: Option<InviteMode>,
    pub invite_window_start: Option<String>,
    pub invite_window_end: Option<String>,
    pub is_test: bool,
}

/// Lädt die Guard-Felder eines Turniers oder liefert 404.
/// Entspricht `_load_tournament_or_404` (routes.py:413).
pub async fn load_tournament_or_404(pool: &Pool, tournament_id: i64) -> WebResult<TournamentGuard> {
    let row = sqlx::query(&format!("{TOURNAMENT_GUARD_SELECT} WHERE id = $1"))
        .bind(tournament_id)
        .fetch_optional(pool)
        .await?;
    tournament_guard_from_row(row)
}

pub async fn lock_tournament_row(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<sqlx::postgres::PgRow> {
    turnier_engine::lock_tournament_row(tx, tournament_id)
        .await?
        .ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))
}

pub async fn lock_tournament_or_404(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<TournamentGuard> {
    tournament_guard_from_row(Some(lock_tournament_row(tx, tournament_id).await?))
}

const TOURNAMENT_GUARD_SELECT: &str = r#"SELECT id, name, status, team_size, bench_slots, invite_mode,
    invite_window_start, invite_window_end, is_test FROM turnier."tournaments""#;

fn tournament_guard_from_row(row: Option<sqlx::postgres::PgRow>) -> WebResult<TournamentGuard> {
    let Some(row) = row else {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    };
    Ok(TournamentGuard {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        status: row.try_get("status")?,
        team_size: row.try_get("team_size")?,
        bench_slots: row.try_get("bench_slots")?,
        invite_mode: row.try_get("invite_mode")?,
        invite_window_start: db::opt_ts_to_string(row.try_get("invite_window_start")?),
        invite_window_end: db::opt_ts_to_string(row.try_get("invite_window_end")?),
        is_test: row.try_get("is_test")?,
    })
}

/// Eine Team-Stammzeile für Guards.
pub struct TeamGuard {
    pub id: i64,
    pub tournament_id: i64,
    pub name: String,
    pub captain_discord_id: String,
    pub recruitment_status: RecruitmentStatus,
}

/// Lädt ein Team oder liefert 404. Entspricht `_load_team_or_404` (routes.py:427).
pub async fn load_team_or_404<'e, E>(
    executor: E,
    tournament_id: i64,
    team_id: i64,
) -> WebResult<TeamGuard>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let row = sqlx::query(
        r#"SELECT id, tournament_id, name, captain_discord_id, recruitment_status
         FROM turnier."teams" WHERE id = $1 AND tournament_id = $2"#,
    )
    .bind(team_id)
    .bind(tournament_id)
    .fetch_optional(executor)
    .await?;
    let Some(row) = row else {
        return Err(WebError::not_found("Team nicht gefunden"));
    };
    Ok(TeamGuard {
        id: row.try_get("id")?,
        tournament_id: row.try_get("tournament_id")?,
        name: row.try_get("name")?,
        captain_discord_id: db::discord_id_to_string(row.try_get("captain_discord_id")?),
        recruitment_status: row.try_get("recruitment_status")?,
    })
}

pub const GUEST_NAME_TAKEN: &str = "Dieser Name ist in diesem Turnier schon angemeldet. Bitte wähle einen anderen Namen oder melde dich beim Orga-Team.";

pub async fn ensure_guest_name_free(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    participant_id: i64,
    name: Option<&str>,
) -> WebResult<()> {
    if !turnier_core::is_guest_participant(participant_id) {
        return Ok(());
    }
    let key = participant_name_key(name.unwrap_or(""));
    let names = participant_names(&mut **tx, tournament_id).await?;
    if holds_name(&names, participant_id, &key) {
        return Ok(());
    }
    if name_taken_by_other(&names, participant_id, &key) {
        return Err(WebError::conflict(GUEST_NAME_TAKEN));
    }
    Ok(())
}

fn holds_name(names: &[(i64, String)], participant_id: i64, key: &str) -> bool {
    names
        .iter()
        .any(|(id, name)| *id == participant_id && participant_name_key(name) == key)
}

fn name_taken_by_other(names: &[(i64, String)], own_id: i64, key: &str) -> bool {
    names
        .iter()
        .any(|(id, name)| *id != own_id && participant_name_key(name) == key)
}

pub async fn lock_tournament_status(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<turnier_core::TournamentStatus> {
    Ok(lock_tournament_or_404(tx, tournament_id).await?.status)
}

pub fn participant_name_key(name: &str) -> String {
    turnier_engine::name_key(name.trim())
}

pub async fn participant_names<'e, E>(
    executor: E,
    tournament_id: i64,
) -> WebResult<Vec<(i64, String)>>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        r#"SELECT discord_id, discord_name FROM turnier."tournament_signups"
         WHERE tournament_id = $1 AND discord_name IS NOT NULL
         UNION
         SELECT tm.discord_id, tm.discord_name FROM turnier."team_members" tm
         JOIN turnier."teams" t ON t.id = tm.team_id
         WHERE t.tournament_id = $1 AND tm.discord_name IS NOT NULL"#,
    )
    .bind(tournament_id)
    .fetch_all(executor)
    .await?)
}

pub fn duplicate_name_ids(names: &[(i64, String)]) -> Vec<String> {
    let mut by_key: HashMap<String, std::collections::BTreeSet<i64>> = HashMap::new();
    for (discord_id, name) in names {
        let key = participant_name_key(name);
        if !key.is_empty() {
            by_key.entry(key).or_default().insert(*discord_id);
        }
    }
    let mut ids: Vec<i64> = by_key
        .into_values()
        .filter(|ids| ids.len() > 1)
        .flatten()
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter().map(db::discord_id_to_string).collect()
}

pub async fn ensure_participant_name_free(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    name: &str,
    own_id: i64,
    taken_message: &str,
) -> WebResult<()> {
    let key = participant_name_key(name);
    let names = participant_names(&mut **tx, tournament_id).await?;
    if name_taken_by_other(&names, own_id, &key) {
        return Err(WebError::conflict(taken_message));
    }
    Ok(())
}

pub async fn remove_signup_and_checkin(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    discord_id: i64,
) -> WebResult<()> {
    sqlx::query(
        r#"DELETE FROM turnier."tournament_signups" WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(discord_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"DELETE FROM turnier."tournament_checkins" WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(discord_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub const PARTICIPANT_NAME_MAX_CHARS: usize = 32;

fn is_hidden_char(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{115F}'
                | '\u{1160}'
                | '\u{17B4}'
                | '\u{17B5}'
                | '\u{180B}'..='\u{180F}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{2800}'
                | '\u{3164}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{FFA0}'
                | '\u{FFF0}'..='\u{FFFB}'
                | '\u{1BCA0}'..='\u{1BCA3}'
                | '\u{1D173}'..='\u{1D17A}'
                | '\u{E0000}'..='\u{E0FFF}'
        )
}

pub fn has_hidden_chars(name: &str) -> bool {
    name.chars().any(is_hidden_char)
}

pub fn participant_name(raw: &str) -> WebResult<&str> {
    let name = raw.trim();
    let invalid = name.is_empty()
        || name.chars().count() > PARTICIPANT_NAME_MAX_CHARS
        || has_hidden_chars(name);
    if invalid {
        return Err(WebError::bad_request(format!(
            "Bitte gib einen Namen mit 1 bis {PARTICIPANT_NAME_MAX_CHARS} Zeichen ohne unsichtbare Steuerzeichen ein"
        )));
    }
    if looks_like_discord_id(Some(name)) {
        return Err(WebError::bad_request(
            "Bitte gib einen Namen ein, der nicht wie eine Discord-ID aussieht",
        ));
    }
    Ok(name)
}

pub async fn allocate_guest_participant_id<'e, E>(executor: E) -> WebResult<i64>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    Ok(sqlx::query_scalar(
        "SELECT -nextval(pg_get_serial_sequence('turnier.tournament_signups', 'id'))",
    )
    .fetch_one(executor)
    .await?)
}

/// Prüft, ob die Anmeldung offen ist (status in registration|checkin), 400 sonst.
/// Entspricht `_ensure_registration_open` (routes.py:441).
pub fn ensure_registration_open(status: turnier_core::TournamentStatus) -> WebResult<()> {
    if !status.registration_window_open() {
        return Err(WebError::bad_request("Anmeldung ist nicht geöffnet"));
    }
    Ok(())
}

/// Mod- oder Admin-Rechte. Entspricht `_is_mod_user` (routes.py:449).
pub fn is_mod_user(user: &UserSession) -> bool {
    user.is_mod || user.is_admin
}

/// Erzwingt Captain-Rechte (403 sonst). Entspricht `_ensure_captain` (routes.py:453).
pub const TEAM_ROSTER_LOCKED_MESSAGE: &str =
    "Teams lassen sich nur während der Anmeldung ändern, nicht mehr im Check-in";

pub fn ensure_team_roster_open(status: turnier_core::TournamentStatus) -> WebResult<()> {
    if status != turnier_core::TournamentStatus::Registration {
        return Err(WebError::bad_request(TEAM_ROSTER_LOCKED_MESSAGE));
    }
    Ok(())
}

pub async fn precheck_player_team_registration(
    pool: &Pool,
    tournament_id: i64,
) -> WebResult<TournamentGuard> {
    let t = load_tournament_or_404(pool, tournament_id).await?;
    ensure_player_teams_allowed(&t)?;
    ensure_registration_open(t.status)?;
    Ok(t)
}

pub fn ensure_captain(user: &UserSession, captain_discord_id: &str) -> WebResult<()> {
    if user.discord_id != captain_discord_id {
        return Err(WebError::forbidden(
            "Nur der Captain darf diese Aktion ausführen",
        ));
    }
    Ok(())
}

/// Erzwingt Captain- ODER Mod-Rechte (403 sonst).
/// Entspricht `_ensure_captain_or_mod` (routes.py:461).
pub fn ensure_captain_or_mod(user: &UserSession, captain_discord_id: &str) -> WebResult<()> {
    if user.discord_id == captain_discord_id || is_mod_user(user) {
        return Ok(());
    }
    Err(WebError::forbidden(
        "Nur Captain oder Mod dürfen diese Aktion ausführen",
    ))
}

pub fn ensure_player_teams_allowed(t: &TournamentGuard) -> WebResult<()> {
    if t.invite_mode == Some(InviteMode::SoloOnly) {
        return Err(WebError::forbidden(
            "In diesem Turnier stellt das Orga-Team die Teams zusammen. Melde dich bitte solo an.",
        ));
    }
    Ok(())
}

/// Prüft, ob Einladungen erlaubt sind, und liefert ggf. das Ablaufdatum
/// (Window-Ende). 403 bei `never`, `solo_only` oder außerhalb des Fensters;
/// NULL gilt als `always`.
pub fn ensure_invites_enabled(t: &TournamentGuard) -> WebResult<Option<String>> {
    ensure_player_teams_allowed(t)?;
    let mode = t.invite_mode.unwrap_or(InviteMode::Always);
    if mode == InviteMode::Never {
        return Err(WebError::forbidden(
            "Einladungen sind für dieses Turnier deaktiviert",
        ));
    }
    if mode == InviteMode::Window {
        let now = Utc::now();
        let start = parse_timestamp(t.invite_window_start.as_deref());
        let end = parse_timestamp(t.invite_window_end.as_deref());
        match (start, end) {
            (Some(start), Some(end)) if now >= start && now <= end => {
                return Ok(t.invite_window_end.clone());
            }
            _ => {
                return Err(WebError::forbidden(
                    "Einladungen sind aktuell nicht erlaubt",
                ));
            }
        }
    }
    Ok(None)
}

/// Zählt die Mitglieder eines Teams. Entspricht `_count_team_members` (routes.py:492).
pub async fn count_team_members<'e, E>(executor: E, team_id: i64) -> WebResult<i64>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM turnier."team_members" WHERE team_id = $1"#)
            .bind(team_id)
            .fetch_one(executor)
            .await?;
    Ok(count)
}

pub async fn team_lineup<'e, E>(executor: E, team_id: i64) -> WebResult<Lineup>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let (starters, bench): (i64, i64) = sqlx::query_as(
        r#"SELECT COUNT(*) FILTER (WHERE NOT is_bench), COUNT(*) FILTER (WHERE is_bench)
         FROM turnier."team_members" WHERE team_id = $1"#,
    )
    .bind(team_id)
    .fetch_one(executor)
    .await?;
    Ok(Lineup { starters, bench })
}

pub async fn reserve_seat<'e, E>(
    executor: E,
    team_id: i64,
    team_size: i64,
    bench_slots: i64,
    request: SeatRequest,
) -> WebResult<bool>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let lineup = team_lineup(executor, team_id).await?;
    bench::place(lineup, team_size, bench_slots, request)
        .map_err(|err| WebError::bad_request(err.message()))
}

pub async fn set_member_lineup(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    team_id: i64,
    discord_id: i64,
    team_size: i64,
    bench_slots: i64,
    to_bench: bool,
) -> WebResult<()> {
    let member: Option<(bool, i64)> = sqlx::query_as(
        r#"SELECT tm.is_bench, t.captain_discord_id FROM turnier."team_members" tm
         JOIN turnier."teams" t ON t.id = tm.team_id
         WHERE tm.team_id = $1 AND tm.discord_id = $2 FOR UPDATE OF tm"#,
    )
    .bind(team_id)
    .bind(discord_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((is_bench, captain_discord_id)) = member else {
        return Err(WebError::not_found("Mitglied nicht gefunden"));
    };
    if is_bench == to_bench {
        return Ok(());
    }
    let lineup = team_lineup(&mut **tx, team_id).await?;
    bench::move_seat(
        lineup,
        team_size,
        bench_slots,
        captain_discord_id == discord_id,
        to_bench,
    )
    .map_err(|err| WebError::bad_request(err.message()))?;
    sqlx::query(
        r#"UPDATE turnier."team_members" SET is_bench = $1 WHERE team_id = $2 AND discord_id = $3"#,
    )
    .bind(to_bench)
    .bind(team_id)
    .bind(discord_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Default, Deserialize)]
struct BenchWish {
    #[serde(default)]
    as_bench: bool,
}

pub fn bench_wish(body: &[u8]) -> WebResult<bool> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(false);
    }
    serde_json::from_slice::<BenchWish>(body)
        .map(|wish| wish.as_bench)
        .map_err(|_| WebError::bad_request("Ungültige Anfrage"))
}

pub fn seat_request(as_bench: bool) -> SeatRequest {
    if as_bench {
        SeatRequest::Bench
    } else {
        SeatRequest::Starter
    }
}

/// Stellt sicher, dass der Spieler in keinem Team des Turniers ist (409 sonst).
/// Entspricht `_ensure_user_not_in_tournament_team` (routes.py:509).
pub async fn ensure_user_not_in_tournament_team<'e, E>(
    executor: E,
    tournament_id: i64,
    discord_id: &str,
) -> WebResult<()>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let discord_id = db::parse_participant_id(discord_id)?;
    let existing: Option<(i64,)> = sqlx::query_as(
        r#"SELECT tm.id FROM turnier."team_members" tm JOIN turnier."teams" t ON tm.team_id = t.id
         WHERE t.tournament_id = $1 AND tm.discord_id = $2"#,
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
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let discord_id = db::parse_participant_id(discord_id)?;
    let row: Option<(Option<i64>,)> = sqlx::query_as(
        r#"SELECT consent_version FROM turnier."user_consents" WHERE discord_id = $1"#,
    )
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
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let user_id = user_id.map(db::parse_actor_id).transpose()?;
    sqlx::query(
        r#"INSERT INTO turnier."audit_log" (action, user_id, details, created_at)
           VALUES ($1, $2, $3, now())"#,
    )
    .bind(action)
    .bind(user_id)
    .bind(details)
    .execute(executor)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Signup-Synchronisation
// ---------------------------------------------------------------------------

#[derive(Default)]
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
        _ => RankInput {
            steam_id: None,
            rank: None,
            rank_score: 0,
        },
    }
}

#[derive(sqlx::FromRow)]
struct CheckinMembershipRow {
    discord_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: Option<i64>,
    team_id: i64,
}

pub async fn ensure_checkin_signup(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    participant_id: i64,
    preferred_name: Option<&str>,
) -> WebResult<bool> {
    let signup_exists: Option<i64> = sqlx::query_scalar(
        r#"SELECT id FROM turnier."tournament_signups" WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(participant_id)
    .fetch_optional(&mut **tx)
    .await?;
    if signup_exists.is_some() {
        return Ok(true);
    }
    let membership: Option<CheckinMembershipRow> = sqlx::query_as(
        r#"SELECT tm.discord_name, tm.steam_id, tm.rank, tm.rank_score, tm.team_id
         FROM turnier."team_members" tm JOIN turnier."teams" t ON t.id = tm.team_id
         WHERE t.tournament_id = $1 AND tm.discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(participant_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(member) = membership else {
        return Ok(false);
    };
    let discord_name = preferred_name
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or(member.discord_name);
    let rank = RankInput {
        steam_id: member.steam_id,
        rank: member.rank,
        rank_score: member.rank_score.unwrap_or(0),
    };
    upsert_signup(
        tx,
        tournament_id,
        &db::discord_id_to_string(participant_id),
        discord_name.as_deref(),
        &rank,
        Some(member.team_id),
    )
    .await?;
    Ok(true)
}

pub fn ensure_checkin_open(status: turnier_core::TournamentStatus) -> WebResult<()> {
    if status != turnier_core::TournamentStatus::Checkin {
        return Err(WebError::bad_request("Check-in ist aktuell nicht geöffnet"));
    }
    Ok(())
}

pub async fn insert_checkin(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    participant_id: i64,
) -> WebResult<bool> {
    let inserted = sqlx::query(
        r#"INSERT INTO turnier."tournament_checkins" (tournament_id, discord_id, checked_in_at)
         VALUES ($1, $2, now())
         ON CONFLICT DO NOTHING"#,
    )
    .bind(tournament_id)
    .bind(participant_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(inserted > 0)
}

/// Upsert in `tournament_signups`: aktualisiert den vorhandenen Eintrag oder legt
/// einen neuen an. Entspricht `_upsert_signup` (routes.py:366).
pub async fn upsert_signup(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    discord_id: &str,
    discord_name: Option<&str>,
    rank: &RankInput,
    team_id: Option<i64>,
) -> WebResult<()> {
    let discord_id = db::parse_participant_id(discord_id)?;
    let existing: Option<(i64,)> = sqlx::query_as(
        r#"SELECT id FROM turnier."tournament_signups" WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(discord_id)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some((id,)) = existing {
        sqlx::query(
            r#"UPDATE turnier."tournament_signups" SET discord_name = $1, steam_id = $2, rank = $3,
             rank_score = $4, team_id = $5 WHERE id = $6"#,
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
        insert_signup(tx, tournament_id, discord_id, discord_name, rank, team_id).await?;
    }
    Ok(())
}

/// Fügt einen Spieler einem Team hinzu (Member-Insert + Signup-Sync mit
/// aufgelöstem Namen). Entspricht `_add_user_to_team` (routes.py:581).
pub async fn add_user_to_team(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    team_id: i64,
    discord_id: &str,
    discord_name: Option<&str>,
    rank: &RankInput,
    is_bench: bool,
) -> WebResult<()> {
    let discord_id_i64 = db::parse_participant_id(discord_id)?;
    let resolved_name = resolve_discord_name_in(tx, discord_id, discord_name).await?;
    ensure_guest_name_free(tx, tournament_id, discord_id_i64, Some(&resolved_name)).await?;
    insert_team_member(
        tx,
        team_id,
        discord_id_i64,
        Some(&resolved_name),
        rank,
        TeamRole::Member,
        is_bench,
    )
    .await?;
    upsert_signup(
        tx,
        tournament_id,
        discord_id,
        Some(&resolved_name),
        rank,
        Some(team_id),
    )
    .await?;
    Ok(())
}

pub async fn insert_signup(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    discord_id: i64,
    discord_name: Option<&str>,
    rank: &RankInput,
    team_id: Option<i64>,
) -> WebResult<(i64, DateTime<Utc>)> {
    turnier_engine::insert_signup(
        tx,
        &turnier_engine::NewSignup {
            tournament_id,
            discord_id,
            discord_name,
            steam_id: rank.steam_id.as_deref(),
            rank: rank.rank.as_deref(),
            rank_score: rank.rank_score,
            team_id,
        },
    )
    .await
    .map_err(|err| map_unique_conflict(err, "Du bist bereits für dieses Turnier angemeldet"))
}

pub async fn insert_team_member(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    team_id: i64,
    discord_id: i64,
    discord_name: Option<&str>,
    rank: &RankInput,
    role: TeamRole,
    is_bench: bool,
) -> WebResult<i64> {
    turnier_engine::insert_team_member(
        tx,
        &turnier_engine::NewTeamMember {
            team_id,
            discord_id,
            discord_name,
            steam_id: rank.steam_id.as_deref(),
            rank: rank.rank.as_deref(),
            rank_score: rank.rank_score,
            role,
            is_bench,
        },
    )
    .await
    .map_err(|err| map_unique_conflict(err, "Spieler ist bereits Mitglied in diesem Team"))
}

pub async fn insert_team(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    name: &str,
    captain_discord_id: i64,
    recruitment_status: RecruitmentStatus,
) -> WebResult<i64> {
    turnier_engine::insert_team(
        tx,
        tournament_id,
        name,
        captain_discord_id,
        recruitment_status,
    )
    .await
    .map_err(|err| map_unique_conflict(err, "Ein Team mit diesem Namen existiert bereits"))
}

pub fn parse_recruitment_status(raw: &str) -> WebResult<RecruitmentStatus> {
    serde_json::from_value(serde_json::Value::String(raw.to_string()))
        .map_err(|_| WebError::bad_request("Ungültiger Recruiting-Status"))
}

#[cfg(test)]
mod tests {
    use super::{
        build_detail_public, duplicate_name_ids, holds_name, name_taken_by_other,
        participant_name_key,
    };
    use serde_json::Value;
    use turnier_core::{
        BracketFormat, BracketMatch, BracketType, GroupMatch, InviteMode, LobbyAccess, MatchGame,
        MatchStatus, NoLobbyAccess, PublicGroup, Tournament, TournamentGameMode, TournamentMode,
        TournamentStatus,
    };

    const LOBBY_KEYS: [&str; 2] = ["party_code", "steam_party_id"];

    fn tournament() -> Tournament {
        Tournament {
            id: 8,
            name: "Cup".into(),
            status: TournamentStatus::Bracket,
            registration_open: false,
            replacements_open: true,
            description: None,
            team_size: 6,
            series_format: 3,
            final_series_format: None,
            registration_start: None,
            registration_end: None,
            checkin_start: None,
            group_phase_start: None,
            bracket_start: None,
            bracket_format: BracketFormat::SingleElimination,
            played_bracket_format: None,
            tournament_mode: TournamentMode::GroupStage,
            tournament_mode_auto: true,
            tournament_game_mode: TournamentGameMode::Standard,
            auto_lobby_enabled: true,
            created_by: "1".into(),
            created_at: "2026-11-28T00:00:00Z".into(),
            updated_at: "2026-11-28T00:00:00Z".into(),
            invite_mode: InviteMode::Always,
            invite_window_start: None,
            invite_window_end: None,
            lobby_settings: None,
            exclude_from_leaderboard: false,
            reminder_offsets: Vec::new(),
            start_reminder_offsets: Vec::new(),
            match_objective: "auto".into(),
            no_show_grace_minutes: 10,
            rules: None,
            is_test: false,
        }
    }

    fn game<L>(lobby: L) -> MatchGame<L> {
        MatchGame {
            id: 1,
            bracket_match_id: 1,
            game_number: 1,
            status: "lobby_created".into(),
            lobby,
            deadlock_match_id: None,
            winner_team: None,
            duration_s: None,
            match_stats: None,
            created_at: "2026-11-28T00:00:00Z".into(),
            completed_at: None,
        }
    }

    fn bracket_match<L: Clone>(lobby: L) -> BracketMatch<L> {
        BracketMatch {
            id: 1,
            tournament_id: 8,
            round: 1,
            position: 0,
            bracket_type: BracketType::Winners,
            mini_group_id: None,
            team1_id: Some(1),
            team2_id: Some(2),
            winner_id: None,
            status: MatchStatus::LobbyCreated,
            source_match1_id: None,
            source_match2_id: None,
            loser_to_match_id: None,
            loser_to_slot: None,
            lobby: lobby.clone(),
            deadlock_match_id: None,
            match_duration_s: None,
            match_stats: None,
            hero_assignments: None,
            series_wins_team1: 0,
            series_wins_team2: 0,
            games: vec![game(lobby)],
            scheduled_at: None,
            on_stream: false,
            played_at: None,
        }
    }

    fn group_match<L>(lobby: L) -> GroupMatch<L> {
        GroupMatch {
            id: 2,
            group_id: 1,
            team1_id: 1,
            team2_id: 2,
            winner_id: None,
            status: MatchStatus::LobbyCreated,
            lobby,
            deadlock_match_id: None,
            match_duration_s: None,
            match_stats: None,
            hero_assignments: None,
            scheduled_at: None,
            played_at: None,
            round: None,
        }
    }

    fn lobby_keys(value: &Value, found: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if LOBBY_KEYS.contains(&key.as_str()) {
                        found.push(key.clone());
                    }
                    lobby_keys(child, found);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| lobby_keys(item, found)),
            _ => {}
        }
    }

    #[test]
    fn public_detail_never_serializes_lobby_codes() {
        let groups = vec![PublicGroup {
            id: 1,
            tournament_id: 8,
            name: "A".into(),
            seeding_order: 0,
            teams: Vec::new(),
            matches: vec![group_match(NoLobbyAccess {})],
        }];
        let detail = build_detail_public(
            tournament(),
            Vec::new(),
            groups,
            vec![bracket_match(NoLobbyAccess {})],
            Vec::new(),
            Vec::new(),
        );
        let json = serde_json::to_value(&detail).expect("public detail serializes");
        assert_eq!(json["bracket_matches"][0]["games"][0]["game_number"], 1);
        assert_eq!(json["groups"][0]["matches"][0]["id"], 2);
        let mut found = Vec::new();
        lobby_keys(&json, &mut found);
        assert!(found.is_empty(), "public detail leaks {found:?}");
    }

    #[test]
    fn admin_matches_keep_lobby_codes() {
        let lobby = LobbyAccess {
            steam_party_id: Some("123".into()),
            party_code: Some("ABCDE".into()),
        };
        let bracket = serde_json::to_value(bracket_match(lobby.clone())).expect("serializes");
        let group = serde_json::to_value(group_match(lobby)).expect("serializes");
        for value in [&bracket, &bracket["games"][0], &group] {
            assert_eq!(value["party_code"], "ABCDE");
            assert_eq!(value["steam_party_id"], "123");
        }
    }

    #[test]
    fn registered_guest_keeps_name_after_same_named_discord_signup() {
        let names = vec![(-5, "Alex".to_string()), (42, "alex ".to_string())];
        let key = participant_name_key("Alex");
        assert!(holds_name(&names, -5, &key));
        assert!(name_taken_by_other(&names, -5, &key));
        assert!(!holds_name(&names, -6, &key));
        assert!(!holds_name(&names, -5, &participant_name_key("Bob")));
    }

    #[test]
    fn duplicate_name_ids_trim_unicode_space_and_fold_case() {
        let names = vec![
            (1, "Alex\u{00A0}".to_string()),
            (-5, "alex".to_string()),
            (2, "Bob".to_string()),
            (3, "Straße".to_string()),
            (-6, "STRASSE ".to_string()),
            (4, "  ".to_string()),
            (-7, "\t".to_string()),
        ];
        assert_eq!(duplicate_name_ids(&names), vec!["-6", "-5", "1", "3"]);
    }
}
