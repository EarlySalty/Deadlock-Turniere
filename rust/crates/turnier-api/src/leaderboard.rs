//! Leaderboard-Router: öffentliche Rangliste + Spielerprofil (portiert
//! `tournament/leaderboard_routes.py`). Beide Endpunkte sind ohne Login erreichbar.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

use turnier_core::{LeaderboardEntry, PlayerProfile, TournamentHistoryEntry};

use crate::db;
use crate::error::{WebError, WebResult};
use crate::state::AppState;

/// Router der Leaderboard-/Profil-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/leaderboard", get(get_leaderboard))
        .route("/api/players/{discord_name}", get(get_player_profile))
}

/// Eine Zeile der Leaderboard-Query.
#[derive(sqlx::FromRow)]
struct LeaderboardRow {
    discord_id: i64,
    total_points: i64,
    tournaments_played: i64,
    matches_played: i64,
    matches_won: i64,
    best_placement: Option<i64>,
    discord_name: Option<String>,
    rank: Option<String>,
}

/// `GET /api/leaderboard`: globale Rangliste nach Punkten.
async fn get_leaderboard(State(state): State<AppState>) -> WebResult<Json<Vec<LeaderboardEntry>>> {
    let rows: Vec<LeaderboardRow> = sqlx::query_as(
        r#"SELECT pp.discord_id, pp.total_points, pp.tournaments_played,
                pp.matches_played, pp.matches_won, pp.best_placement,
                COALESCE(NULLIF(s.discord_name, ''), NULL) AS discord_name,
                rc.rank
         FROM turnier."player_points" pp
         LEFT JOIN (
             SELECT discord_id, MAX(discord_name) AS discord_name
             FROM turnier."sessions"
             WHERE discord_name IS NOT NULL AND discord_name != ''
             GROUP BY discord_id
         ) s ON s.discord_id = pp.discord_id
         LEFT JOIN turnier."rank_cache" rc ON rc.discord_id = pp.discord_id
         ORDER BY pp.total_points DESC, pp.best_placement ASC NULLS LAST"#,
    )
    .fetch_all(&state.pool)
    .await?;

    let entries = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| LeaderboardEntry {
            rank_position: index as i64 + 1,
            discord_name: row
                .discord_name
                .unwrap_or_else(|| db::discord_id_to_string(row.discord_id)),
            rank: row.rank,
            total_points: row.total_points,
            tournaments_played: row.tournaments_played,
            matches_played: row.matches_played,
            matches_won: row.matches_won,
            best_placement: row.best_placement,
        })
        .collect();
    Ok(Json(entries))
}

#[derive(sqlx::FromRow)]
struct ProfileHeadRow {
    discord_avatar: Option<String>,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_filename: Option<String>,
    rank: Option<String>,
    total_points: i64,
    tournaments_played: i64,
    matches_played: i64,
    matches_won: i64,
    best_placement: Option<i64>,
}

/// Ein Eintrag der Turnier-Historie.
#[derive(sqlx::FromRow)]
struct HistoryRow {
    tournament_name: String,
    team_name: Option<String>,
}

/// `GET /api/players/{discord_name}`: öffentliches Spielerprofil.
async fn get_player_profile(
    State(state): State<AppState>,
    Path(discord_name): Path<String>,
) -> WebResult<Json<PlayerProfile>> {
    let pool = &state.pool;

    let discord_id = crate::db::discord_account_id_by_name(pool, &discord_name).await?;
    let Some(discord_id) = discord_id else {
        return Err(WebError::not_found("Spieler nicht gefunden"));
    };

    let head: ProfileHeadRow = sqlx::query_as(
        r#"SELECT avatar.discord_avatar,
                  profile.display_name, profile.bio, profile.avatar_filename,
                  ranks.rank,
                  COALESCE(points.total_points, 0)::bigint AS total_points,
                  COALESCE(points.tournaments_played, 0)::bigint AS tournaments_played,
                  COALESCE(points.matches_played, 0)::bigint AS matches_played,
                  COALESCE(points.matches_won, 0)::bigint AS matches_won,
                  points.best_placement
           FROM (SELECT $1::bigint AS discord_id) identity
           LEFT JOIN LATERAL (
                SELECT discord_avatar FROM turnier."sessions"
                WHERE discord_id = identity.discord_id
                  AND discord_avatar IS NOT NULL LIMIT 1
           ) avatar ON TRUE
           LEFT JOIN turnier."user_profiles" profile
             ON profile.discord_id = identity.discord_id
           LEFT JOIN turnier."rank_cache" ranks
             ON ranks.discord_id = identity.discord_id
           LEFT JOIN turnier."player_points" points
             ON points.discord_id = identity.discord_id"#,
    )
    .bind(discord_id)
    .fetch_one(pool)
    .await?;

    let history: Vec<HistoryRow> = sqlx::query_as(
        r#"SELECT t.name AS tournament_name, teams.name AS team_name
         FROM turnier."team_members" tm
         JOIN turnier."teams" teams ON tm.team_id = teams.id
         JOIN turnier."tournaments" t ON teams.tournament_id = t.id
         WHERE tm.discord_id = $1 ORDER BY t.created_at DESC"#,
    )
    .bind(discord_id)
    .fetch_all(pool)
    .await?;
    let tournament_history = history
        .into_iter()
        .map(|h| TournamentHistoryEntry {
            tournament_name: h.tournament_name,
            placement: None,
            team_name: h.team_name,
        })
        .collect();

    Ok(Json(PlayerProfile {
        discord_name: discord_name.clone(),
        display_name: Some(head.display_name.unwrap_or(discord_name)),
        discord_avatar: head.discord_avatar,
        avatar_filename: head.avatar_filename,
        bio: head.bio,
        rank: head.rank,
        // 1:1 erhaltener Original-Bug: rank_score bekommt matches_won (siehe
        // docs/known-issues.md KI-W01), nicht den echten Rang-Score.
        rank_score: head.matches_won,
        tournaments_played: head.tournaments_played,
        matches_played: head.matches_played,
        matches_won: head.matches_won,
        best_placement: head.best_placement,
        total_points: head.total_points,
        tournament_history,
    }))
}
