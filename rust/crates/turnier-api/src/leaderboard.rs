//! Leaderboard-Router: öffentliche Rangliste + Spielerprofil (portiert
//! `tournament/leaderboard_routes.py`). Beide Endpunkte sind ohne Login erreichbar.

use std::cmp::Ordering;
use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

use turnier_core::{LeaderboardEntry, PlayerProfile, TournamentHistoryEntry};

use turnier_engine::{player_standings, PlayerAggregate};

use crate::db;
use crate::error::{WebError, WebResult};
use crate::state::AppState;

/// Router der Leaderboard-/Profil-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/leaderboard", get(get_leaderboard))
        .route("/api/players/{discord_name}", get(get_player_profile))
}

#[derive(sqlx::FromRow)]
struct PlayerLabelRow {
    discord_id: i64,
    discord_name: Option<String>,
    rank: Option<String>,
}

async fn get_leaderboard(State(state): State<AppState>) -> WebResult<Json<Vec<LeaderboardEntry>>> {
    let mut standings: Vec<(i64, PlayerAggregate)> =
        player_standings(&state.pool).await?.into_iter().collect();
    standings.sort_by(|(a_id, a), (b_id, b)| {
        b.total_points
            .cmp(&a.total_points)
            .then_with(|| placement_order(a.best_placement, b.best_placement))
            .then_with(|| a_id.cmp(b_id))
    });
    let ids: Vec<i64> = standings.iter().map(|(id, _)| *id).collect();
    let labels: HashMap<i64, PlayerLabelRow> = sqlx::query_as::<_, PlayerLabelRow>(
        r#"SELECT ids.discord_id, s.discord_name, rc.rank
         FROM UNNEST($1::bigint[]) AS ids(discord_id)
         LEFT JOIN (
             SELECT discord_id, MAX(discord_name) AS discord_name
             FROM turnier."sessions"
             WHERE discord_name IS NOT NULL AND discord_name != ''
             GROUP BY discord_id
         ) s ON s.discord_id = ids.discord_id
         LEFT JOIN turnier."rank_cache" rc ON rc.discord_id = ids.discord_id"#,
    )
    .bind(&ids)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|row| (row.discord_id, row))
    .collect();

    let entries = standings
        .into_iter()
        .enumerate()
        .map(|(index, (discord_id, agg))| {
            let label = labels.get(&discord_id);
            LeaderboardEntry {
                rank_position: index as i64 + 1,
                discord_name: label
                    .and_then(|l| l.discord_name.clone())
                    .unwrap_or_else(|| db::discord_id_to_string(discord_id)),
                rank: label.and_then(|l| l.rank.clone()),
                total_points: agg.total_points,
                tournaments_played: agg.tournaments_played,
                matches_played: agg.matches_played,
                matches_won: agg.matches_won,
                best_placement: agg.best_placement,
                best_placement_shared: agg.best_placement_shared,
            }
        })
        .collect();
    Ok(Json(entries))
}

fn placement_order(a: Option<i64>, b: Option<i64>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Punkte-Zeile eines Spielers.
#[derive(sqlx::FromRow, Default)]
struct PointsRow {
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

    let discord_avatar: Option<(Option<String>,)> = sqlx::query_as(
        r#"SELECT discord_avatar FROM turnier."sessions"
         WHERE discord_id = $1 AND discord_avatar IS NOT NULL LIMIT 1"#,
    )
    .bind(discord_id)
    .fetch_optional(pool)
    .await?;
    let discord_avatar = discord_avatar.and_then(|r| r.0);

    let profile: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT display_name, bio, avatar_filename FROM turnier."user_profiles" WHERE discord_id = $1"#,
    )
    .bind(discord_id)
    .fetch_optional(pool)
    .await?;
    let (display_name, bio, avatar_filename) = profile.unwrap_or((None, None, None));

    let rank: Option<(Option<String>,)> =
        sqlx::query_as(r#"SELECT rank FROM turnier."rank_cache" WHERE discord_id = $1"#)
            .bind(discord_id)
            .fetch_optional(pool)
            .await?;
    let rank = rank.and_then(|r| r.0);

    let points: Option<PointsRow> = sqlx::query_as(
        r#"SELECT total_points, tournaments_played, matches_played, matches_won, best_placement
         FROM turnier."player_points" WHERE discord_id = $1"#,
    )
    .bind(discord_id)
    .fetch_optional(pool)
    .await?;
    let points = points.unwrap_or_default();

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
            placement_shared: false,
            team_name: h.team_name,
        })
        .collect();

    Ok(Json(PlayerProfile {
        discord_name: discord_name.clone(),
        display_name: Some(display_name.unwrap_or(discord_name)),
        discord_avatar,
        avatar_filename,
        bio,
        rank,
        // 1:1 erhaltener Original-Bug: rank_score bekommt matches_won (siehe
        // docs/known-issues.md KI-W01), nicht den echten Rang-Score.
        rank_score: points.matches_won,
        tournaments_played: points.tournaments_played,
        matches_played: points.matches_played,
        matches_won: points.matches_won,
        best_placement: points.best_placement,
        total_points: points.total_points,
        tournament_history,
    }))
}
