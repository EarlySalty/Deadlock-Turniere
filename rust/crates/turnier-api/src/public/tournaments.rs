//! Öffentliche Lese-Routen + eigener Status: Turnierliste/-detail, `/me`,
//! Bracket, Gruppen, Check-in-Status.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use sqlx::Row;

use turnier_core::{BracketMatch, Group, Tournament, TournamentDetailPublic};

use crate::error::{WebError, WebResult};
use crate::extract::AuthUser;
use crate::state::AppState;

use super::helpers;

/// Router der öffentlichen Lese-Routen + `/me`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tournaments", get(list_tournaments))
        .route("/api/tournaments/{tournament_id}", get(get_tournament))
        .route("/api/tournaments/{tournament_id}/me", get(get_my_tournament_status))
        .route("/api/tournaments/{tournament_id}/bracket", get(get_bracket))
        .route("/api/tournaments/{tournament_id}/groups", get(get_groups))
        .route("/api/tournaments/{tournament_id}/checkin-status", get(get_checkin_status))
}

/// `GET /api/tournaments` — alle nicht-draft-Turniere, absteigend nach created_at.
async fn list_tournaments(State(state): State<AppState>) -> WebResult<Json<Vec<Tournament>>> {
    let tournaments =
        helpers::list_tournament_dtos(&state.pool, "WHERE status != 'draft' ORDER BY created_at DESC")
            .await?;
    Ok(Json(tournaments))
}

/// `GET /api/tournaments/{tournament_id}` — öffentliches Turnier-Detail.
/// 404 auch bei `status = 'draft'`.
async fn get_tournament(
    State(state): State<AppState>,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<TournamentDetailPublic>> {
    let pool = &state.pool;
    let row = sqlx::query("SELECT * FROM tournaments WHERE id = ?")
        .bind(tournament_id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    };
    let status: String = row.try_get("status")?;
    if status == "draft" {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    }

    let t = helpers::TournamentRowData::from_row(&row)?;
    let teams = helpers::load_teams_public(pool, tournament_id).await?;
    let groups = helpers::load_groups_for_tournament(pool, tournament_id).await?;
    let bracket_matches = helpers::load_bracket_matches(pool, tournament_id).await?;
    let mini_groups = helpers::load_mini_groups_for_tournament(pool, tournament_id).await?;
    let signups = helpers::load_signups_public(pool, tournament_id).await?;

    Ok(Json(helpers::build_detail_public(
        t,
        teams,
        groups,
        bracket_matches,
        mini_groups,
        signups,
    )))
}

/// `GET /api/tournaments/{tournament_id}/me` — eigener Anmeldestatus
/// (kein `discord_id` im Response).
async fn get_my_tournament_status(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;

    let member: Option<(i64, String)> = sqlx::query_as(
        "SELECT tm.team_id, t.captain_discord_id FROM team_members tm \
         JOIN teams t ON tm.team_id = t.id \
         WHERE t.tournament_id = ? AND tm.discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(pool)
    .await?;

    let signup: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM tournament_signups \
         WHERE tournament_id = ? AND discord_id = ? AND (team_id IS NULL OR team_id = 0)",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(pool)
    .await?;

    let checkin: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM tournament_checkins WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(pool)
    .await?;

    let team_id = member.as_ref().map(|(tid, _)| *tid);
    let is_captain = member
        .as_ref()
        .map(|(_, captain)| captain == &user.discord_id)
        .unwrap_or(false);
    let signup_id = signup.map(|(id,)| id);

    Ok(Json(json!({
        "team_id": team_id,
        "signup_id": signup_id,
        "is_captain": is_captain,
        "is_checked_in": checkin.is_some(),
    })))
}

/// `GET /api/tournaments/{tournament_id}/bracket` — Bracket-Matches.
/// 404 nur bei nicht-existentem Turnier (KEIN draft-404, 1:1 zum Original).
async fn get_bracket(
    State(state): State<AppState>,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Vec<BracketMatch>>> {
    let pool = &state.pool;
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM tournaments WHERE id = ?")
        .bind(tournament_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    }
    Ok(Json(helpers::load_bracket_matches(pool, tournament_id).await?))
}

/// `GET /api/tournaments/{tournament_id}/groups` — Gruppen-Standings.
/// 404 nur bei nicht-existentem Turnier (KEIN draft-404, 1:1 zum Original).
async fn get_groups(
    State(state): State<AppState>,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Vec<Group>>> {
    let pool = &state.pool;
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM tournaments WHERE id = ?")
        .bind(tournament_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    }
    Ok(Json(helpers::load_groups_for_tournament(pool, tournament_id).await?))
}

/// `GET /api/tournaments/{tournament_id}/checkin-status` — Check-in-Übersicht.
/// `total_registered` = Union aus Signups + Team-Mitgliedern.
async fn get_checkin_status(
    State(state): State<AppState>,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM tournaments WHERE id = ?")
        .bind(tournament_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    }

    let signup_ids: Vec<String> =
        sqlx::query_scalar("SELECT discord_id FROM tournament_signups WHERE tournament_id = ?")
            .bind(tournament_id)
            .fetch_all(pool)
            .await?;
    let member_ids: Vec<String> = sqlx::query_scalar(
        "SELECT tm.discord_id FROM team_members tm \
         JOIN teams t ON t.id = tm.team_id WHERE t.tournament_id = ?",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let mut registered: std::collections::HashSet<String> = signup_ids.into_iter().collect();
    registered.extend(member_ids);

    let checked_in_names: Vec<String> = sqlx::query_scalar(
        "SELECT COALESCE(NULLIF(ts.discord_name, ''), NULLIF(s.discord_name, ''), \
                NULLIF(tm.discord_name, ''), 'Unbekannt') AS discord_name \
         FROM tournament_checkins tc \
         LEFT JOIN tournament_signups ts \
             ON ts.tournament_id = tc.tournament_id AND ts.discord_id = tc.discord_id \
         LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM sessions \
             WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) s \
             ON s.discord_id = tc.discord_id \
         LEFT JOIN (SELECT discord_id, MAX(discord_name) AS discord_name FROM team_members \
             WHERE discord_name IS NOT NULL AND discord_name != '' GROUP BY discord_id) tm \
             ON tm.discord_id = tc.discord_id \
         WHERE tc.tournament_id = ? ORDER BY tc.checked_in_at, tc.id",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    Ok(Json(json!({
        "total_registered": registered.len(),
        "total_checked_in": checked_in_names.len(),
        "checked_in_names": checked_in_names,
    })))
}
