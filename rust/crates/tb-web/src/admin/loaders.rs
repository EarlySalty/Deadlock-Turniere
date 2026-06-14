//! Detail-Loader für die Admin-Lese-Endpunkte: Teams (inkl. Rang-Anreicherung),
//! Gruppen, Bracket-Matches (inkl. Serien-Spiele), Mini-Groups und Signups.
//!
//! Portiert die `_load_*`-Helfer aus `tournament/routes.py` (dort geteilt mit dem
//! öffentlichen Router). Die Discord-Namens-Bevorzugung
//! (`_preferred_discord_name`) und die Rang-Anreicherung (`_enrich_rank_data`)
//! sind 1:1 übernommen.
//!
//! Hinweis für den Integrator: dieselben Loader baut auch der öffentliche Router
//! (`tournament/routes.py`). Sie sind hier self-contained, damit das Admin-Modul
//! unabhängig kompiliert; eine spätere Konsolidierung in ein geteiltes
//! `crate::loaders`-Modul ist möglich.

use tb_core::{
    BracketMatch, BracketMiniGroup, Group, GroupMatch, GroupTeam, MatchGame, Team, TeamMember,
    TournamentSignup,
};
use tb_db::Pool;

use crate::error::WebResult;
use crate::state::AppState;

/// Prüft, ob ein String wie eine rohe Discord-ID aussieht (16–21 Ziffern).
fn looks_like_discord_id(value: &str) -> bool {
    let stripped = value.trim();
    !stripped.is_empty()
        && stripped.chars().all(|c| c.is_ascii_digit())
        && (16..=21).contains(&stripped.len())
}

/// Bereinigt einen Discord-Namen (leere/ID-gleiche/ID-artige Werte → `None`).
fn sanitize_discord_name(name: Option<&str>, discord_id: Option<&str>) -> Option<String> {
    let stripped = name?.trim();
    if stripped.is_empty() {
        return None;
    }
    if let Some(id) = discord_id {
        if stripped == id.trim() {
            return None;
        }
    }
    if looks_like_discord_id(stripped) {
        return None;
    }
    Some(stripped.to_string())
}

/// Wählt den ersten brauchbaren Namen aus den Kandidaten. Portiert
/// `_preferred_discord_name`.
fn preferred_discord_name(candidates: &[Option<&str>], discord_id: Option<&str>) -> Option<String> {
    for candidate in candidates {
        if let Some(name) = sanitize_discord_name(*candidate, discord_id) {
            return Some(name);
        }
    }
    None
}

/// Wandelt einen DB-String in ein (snake_case-serdes) Domänen-Enum. DB-Spalten
/// werden als `String` dekodiert (Konvention der Codebase) und hier konvertiert.
fn parse_enum<T: serde::de::DeserializeOwned>(value: &str) -> WebResult<T> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| crate::error::WebError::internal("Ungültiger Enum-Wert in der DB"))
}

/// Rohzeile eines Team-Mitglieds mit den Namens-Kandidaten.
#[derive(sqlx::FromRow)]
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
    role: String,
    joined_at: String,
}

const MEMBER_SELECT: &str =
    "SELECT tm.id, tm.team_id, tm.discord_id, tm.discord_name AS team_member_discord_name, \
            s.discord_name AS session_discord_name, p.display_name AS profile_display_name, \
            tm.steam_id, tm.rank, tm.rank_score, tm.role, tm.joined_at \
     FROM team_members tm \
     LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM sessions \
                WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s \
       ON s.discord_id = tm.discord_id \
     LEFT JOIN user_profiles p ON p.discord_id = tm.discord_id \
     WHERE tm.team_id = ? ORDER BY tm.joined_at";

/// Wandelt eine Mitglieds-Rohzeile in das DTO und reichert den Rang an.
async fn member_from_row(state: &AppState, row: MemberRow) -> WebResult<TeamMember> {
    let discord_name = preferred_discord_name(
        &[
            row.profile_display_name.as_deref(),
            row.session_discord_name.as_deref(),
            row.team_member_discord_name.as_deref(),
        ],
        Some(&row.discord_id),
    );

    // Rang-Anreicherung: Discord-first, DB-Fallback (`_enrich_rank_data`).
    let mut steam_id = row.steam_id;
    let mut rank = row.rank;
    let mut rank_score = row.rank_score;
    if let Ok(Some(profile)) = state.rank_resolver.rank_profile(&row.discord_id).await {
        if profile.steam_id.is_some() {
            steam_id = profile.steam_id;
        }
        if profile.rank.is_some() {
            rank = profile.rank;
        }
        rank_score = profile.rank_score;
    }

    Ok(TeamMember {
        id: row.id,
        team_id: row.team_id,
        discord_id: row.discord_id,
        discord_name,
        steam_id,
        rank,
        rank_score,
        role: parse_enum(&row.role)?,
        joined_at: row.joined_at,
    })
}

/// Lädt die Mitglieder eines Teams (sortiert, angereichert).
pub async fn load_team_members(state: &AppState, team_id: i64) -> WebResult<Vec<TeamMember>> {
    let rows: Vec<MemberRow> = sqlx::query_as(MEMBER_SELECT).bind(team_id).fetch_all(&state.pool).await?;
    let mut members = Vec::with_capacity(rows.len());
    for row in rows {
        members.push(member_from_row(state, row).await?);
    }
    Ok(members)
}

/// Rohzeile eines Teams.
#[derive(sqlx::FromRow)]
struct TeamRow {
    id: i64,
    tournament_id: i64,
    name: String,
    name_key: String,
    captain_discord_id: String,
    created_at: String,
    recruitment_status: String,
}

/// Lädt ein einzelnes Team-Detail (für Mutations-Responses). Portiert
/// `_load_team_detail`.
pub async fn load_team_detail(state: &AppState, team_id: i64) -> WebResult<Team> {
    let row: TeamRow = sqlx::query_as(
        "SELECT id, tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status \
         FROM teams WHERE id = ?",
    )
    .bind(team_id)
    .fetch_one(&state.pool)
    .await?;
    let members = load_team_members(state, team_id).await?;
    Ok(Team {
        id: row.id,
        tournament_id: row.tournament_id,
        name: row.name,
        name_key: row.name_key,
        captain_discord_id: row.captain_discord_id,
        created_at: row.created_at,
        recruitment_status: parse_enum(&row.recruitment_status)?,
        members,
    })
}

/// Lädt alle Teams eines Turniers inkl. Mitglieder. Portiert
/// `_load_teams_for_tournament`.
pub async fn load_teams_for_tournament(state: &AppState, tournament_id: i64) -> WebResult<Vec<Team>> {
    let rows: Vec<TeamRow> = sqlx::query_as(
        "SELECT id, tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status \
         FROM teams WHERE tournament_id = ?",
    )
    .bind(tournament_id)
    .fetch_all(&state.pool)
    .await?;
    let mut teams = Vec::with_capacity(rows.len());
    for row in rows {
        let members = load_team_members(state, row.id).await?;
        teams.push(Team {
            id: row.id,
            tournament_id: row.tournament_id,
            name: row.name,
            name_key: row.name_key,
            captain_discord_id: row.captain_discord_id,
            created_at: row.created_at,
            recruitment_status: parse_enum(&row.recruitment_status)?,
            members,
        });
    }
    Ok(teams)
}

/// Rohzeile einer Gruppen-Tabellenzeile.
#[derive(sqlx::FromRow)]
struct GroupTeamRow {
    id: i64,
    group_id: i64,
    team_id: i64,
    team_name: String,
    wins: i64,
    losses: i64,
    points: i64,
}

/// Rohzeile eines Gruppen-Matches (hero_assignments roh als String).
#[derive(sqlx::FromRow)]
struct GroupMatchRow {
    id: i64,
    group_id: i64,
    team1_id: i64,
    team2_id: i64,
    winner_id: Option<i64>,
    status: String,
    steam_party_id: Option<String>,
    party_code: Option<String>,
    deadlock_match_id: Option<String>,
    match_duration_s: Option<i64>,
    match_stats: Option<String>,
    hero_assignments: Option<String>,
    scheduled_at: Option<String>,
    played_at: Option<String>,
}

/// Lädt alle Gruppen eines Turniers inkl. Tabellen und Matches. Portiert
/// `_load_groups_for_tournament`.
pub async fn load_groups_for_tournament(pool: &Pool, tournament_id: i64) -> WebResult<Vec<Group>> {
    #[derive(sqlx::FromRow)]
    struct GroupRow {
        id: i64,
        tournament_id: i64,
        name: String,
        seeding_order: i64,
    }
    let group_rows: Vec<GroupRow> = sqlx::query_as(
        "SELECT id, tournament_id, name, seeding_order FROM groups WHERE tournament_id = ? ORDER BY seeding_order",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let mut groups = Vec::with_capacity(group_rows.len());
    for g in group_rows {
        let team_rows: Vec<GroupTeamRow> = sqlx::query_as(
            "SELECT gt.id, gt.group_id, gt.team_id, t.name AS team_name, gt.wins, gt.losses, gt.points \
             FROM group_teams gt JOIN teams t ON gt.team_id = t.id WHERE gt.group_id = ?",
        )
        .bind(g.id)
        .fetch_all(pool)
        .await?;
        let teams: Vec<GroupTeam> = team_rows
            .into_iter()
            .map(|r| GroupTeam {
                id: r.id,
                group_id: r.group_id,
                team_id: r.team_id,
                team_name: r.team_name,
                wins: r.wins,
                losses: r.losses,
                points: r.points,
            })
            .collect();

        let match_rows: Vec<GroupMatchRow> = sqlx::query_as(
            "SELECT id, group_id, team1_id, team2_id, winner_id, status, steam_party_id, party_code, \
                    deadlock_match_id, match_duration_s, match_stats, hero_assignments, scheduled_at, played_at \
             FROM group_matches WHERE group_id = ?",
        )
        .bind(g.id)
        .fetch_all(pool)
        .await?;
        let mut matches: Vec<GroupMatch> = Vec::with_capacity(match_rows.len());
        for r in match_rows {
            matches.push(GroupMatch {
                id: r.id,
                group_id: r.group_id,
                team1_id: r.team1_id,
                team2_id: r.team2_id,
                winner_id: r.winner_id,
                status: parse_enum(&r.status)?,
                steam_party_id: r.steam_party_id,
                party_code: r.party_code,
                deadlock_match_id: r.deadlock_match_id,
                match_duration_s: r.match_duration_s,
                match_stats: r.match_stats,
                hero_assignments: r.hero_assignments.as_deref().and_then(|s| serde_json::from_str(s).ok()),
                scheduled_at: r.scheduled_at,
                played_at: r.played_at,
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

/// Rohzeile eines Bracket-Matches.
#[derive(sqlx::FromRow)]
struct BracketRow {
    id: i64,
    tournament_id: i64,
    round: i64,
    position: i64,
    bracket_type: String,
    mini_group_id: Option<i64>,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
    winner_id: Option<i64>,
    status: String,
    source_match1_id: Option<i64>,
    source_match2_id: Option<i64>,
    loser_to_match_id: Option<i64>,
    loser_to_slot: Option<i64>,
    steam_party_id: Option<String>,
    party_code: Option<String>,
    deadlock_match_id: Option<String>,
    match_duration_s: Option<i64>,
    match_stats: Option<String>,
    hero_assignments: Option<String>,
    scheduled_at: Option<String>,
    on_stream: i64,
    played_at: Option<String>,
}

/// Lädt die Serien-Spiele eines Bracket-Matches (über den geteilten
/// `tb_match::series`-Loader, der `match_stats` korrekt parst).
async fn load_match_games(pool: &Pool, bracket_match_id: i64) -> WebResult<Vec<MatchGame>> {
    Ok(tb_match::series::get_series_games(pool, bracket_match_id).await?)
}

/// Wandelt eine Bracket-Rohzeile in das DTO inkl. Serien-Aggregation.
async fn bracket_from_row(pool: &Pool, row: BracketRow) -> WebResult<BracketMatch> {
    let games = load_match_games(pool, row.id).await?;
    let series_wins_team1 = games.iter().filter(|g| g.winner_team == Some(1)).count() as i64;
    let series_wins_team2 = games.iter().filter(|g| g.winner_team == Some(2)).count() as i64;
    let hero_assignments = row
        .hero_assignments
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    Ok(BracketMatch {
        id: row.id,
        tournament_id: row.tournament_id,
        round: row.round,
        position: row.position,
        bracket_type: parse_enum(&row.bracket_type)?,
        mini_group_id: row.mini_group_id,
        team1_id: row.team1_id,
        team2_id: row.team2_id,
        winner_id: row.winner_id,
        status: parse_enum(&row.status)?,
        source_match1_id: row.source_match1_id,
        source_match2_id: row.source_match2_id,
        loser_to_match_id: row.loser_to_match_id,
        loser_to_slot: row.loser_to_slot,
        steam_party_id: row.steam_party_id,
        party_code: row.party_code,
        deadlock_match_id: row.deadlock_match_id,
        match_duration_s: row.match_duration_s,
        match_stats: row.match_stats,
        hero_assignments,
        series_wins_team1,
        series_wins_team2,
        games,
        scheduled_at: row.scheduled_at,
        on_stream: row.on_stream != 0,
        played_at: row.played_at,
    })
}

/// Lädt alle Bracket-Matches eines Turniers. Portiert `_load_bracket_matches`.
pub async fn load_bracket_matches(pool: &Pool, tournament_id: i64) -> WebResult<Vec<BracketMatch>> {
    let rows: Vec<BracketRow> = sqlx::query_as(
        "SELECT id, tournament_id, round, position, bracket_type, mini_group_id, team1_id, team2_id, \
                winner_id, status, source_match1_id, source_match2_id, loser_to_match_id, loser_to_slot, \
                steam_party_id, party_code, deadlock_match_id, match_duration_s, match_stats, \
                hero_assignments, scheduled_at, on_stream, played_at \
         FROM bracket_matches WHERE tournament_id = ? ORDER BY round, position",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(bracket_from_row(pool, row).await?);
    }
    Ok(out)
}

/// Lädt alle Mini-Groups eines Turniers inkl. Team-/Match-IDs. Portiert
/// `_load_mini_groups_for_tournament`.
pub async fn load_mini_groups_for_tournament(
    pool: &Pool,
    tournament_id: i64,
) -> WebResult<Vec<BracketMiniGroup>> {
    #[derive(sqlx::FromRow)]
    struct MiniRow {
        id: i64,
        tournament_id: i64,
        round: i64,
        position: i64,
        advances_to_match_id: Option<i64>,
        advances_to_slot: Option<i64>,
    }
    let mini_rows: Vec<MiniRow> = sqlx::query_as(
        "SELECT id, tournament_id, round, position, advances_to_match_id, advances_to_slot \
         FROM bracket_mini_groups WHERE tournament_id = ? ORDER BY round, position, id",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(mini_rows.len());
    for row in mini_rows {
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

/// Rohzeile eines Signups mit Namens-Kandidaten.
#[derive(sqlx::FromRow)]
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

/// Lädt alle Signups eines Turniers (angereichert). Portiert
/// `_load_signups_for_tournament`.
pub async fn load_signups_for_tournament(
    state: &AppState,
    tournament_id: i64,
) -> WebResult<Vec<TournamentSignup>> {
    let rows: Vec<SignupRow> = sqlx::query_as(
        "SELECT ts.id, ts.tournament_id, ts.discord_id, \
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
         WHERE ts.tournament_id = ? ORDER BY ts.signed_up_at DESC",
    )
    .bind(tournament_id)
    .fetch_all(&state.pool)
    .await?;

    let mut signups = Vec::with_capacity(rows.len());
    for row in rows {
        let discord_name = preferred_discord_name(
            &[
                row.profile_display_name.as_deref(),
                row.session_discord_name.as_deref(),
                row.signup_discord_name.as_deref(),
                row.team_member_discord_name.as_deref(),
            ],
            Some(&row.discord_id),
        );

        let mut steam_id = row.steam_id;
        let mut rank = row.rank;
        let mut rank_score = row.rank_score;
        if let Ok(Some(profile)) = state.rank_resolver.rank_profile(&row.discord_id).await {
            if profile.steam_id.is_some() {
                steam_id = profile.steam_id;
            }
            if profile.rank.is_some() {
                rank = profile.rank;
            }
            rank_score = profile.rank_score;
        }

        signups.push(TournamentSignup {
            id: row.id,
            tournament_id: row.tournament_id,
            discord_id: row.discord_id,
            discord_name,
            steam_id,
            rank,
            rank_score,
            team_id: row.team_id,
            signed_up_at: row.signed_up_at,
        });
    }
    Ok(signups)
}
