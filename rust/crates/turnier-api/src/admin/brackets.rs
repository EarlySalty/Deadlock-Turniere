//! Gruppen-/Bracket-Generierung. Delegiert an [`turnier_engine::generate_groups`],
//! [`turnier_engine::generate_group_matches`] und [`turnier_engine::generate_bracket`].

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use turnier_engine::league::DEFAULT_SLOT_MINUTES;
use turnier_engine::{
    generate_bracket, generate_group_matches, generate_groups, generate_league_plan_in_tx,
    league_slot_minutes_in_tx,
};
use turnier_match::league::tournament_is_league;

use crate::error::{WebError, WebResult};
use crate::extract::ModUser;
use crate::state::AppState;

use super::helpers::{audit, group_phase_has_played_matches};

/// Router der Generierungs-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/tournaments/{tournament_id}/groups/generate",
            post(generate_groups_route),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/bracket/generate",
            post(generate_bracket_route),
        )
}

/// Body von `groups/generate` (`num_groups` optional).
#[derive(Debug, Default, Deserialize)]
struct GroupsBody {
    #[serde(default)]
    num_groups: Option<i64>,
    #[serde(default)]
    slot_minutes: Option<i64>,
}

/// `POST /api/admin/tournaments/{id}/groups/generate` — Gruppen (Snake-Draft) +
/// Gruppen-Matches generieren.
async fn generate_groups_route(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
    body: Option<Json<GroupsBody>>,
) -> WebResult<Json<Value>> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    if tournament_is_league(&state.pool, tournament_id).await? {
        return generate_league_plan(&state, &user.discord_id, tournament_id, body.slot_minutes)
            .await;
    }
    // num_groups auf [2, 8] klemmen (wie das Original); fehlend → None (Auto).
    let num_groups = body.num_groups.map(|n| n.clamp(2, 8) as usize);

    let group_ids = generate_groups(&state.pool, tournament_id, num_groups).await?;
    let match_count = generate_group_matches(&state.pool, tournament_id).await?;

    audit(
        &state.pool,
        "groups_generate",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "groups": group_ids.len(),
            "matches": match_count,
        }),
    )
    .await?;

    Ok(Json(json!({
        "groups_created": group_ids.len(),
        "matches_created": match_count,
    })))
}

async fn generate_league_plan(
    state: &AppState,
    actor: &str,
    tournament_id: i64,
    slot_minutes: Option<i64>,
) -> WebResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    turnier_scheduler::acquire_tournament_schedule_lock(&mut tx).await?;
    let status: String =
        sqlx::query_scalar(r#"SELECT status FROM turnier."tournaments" WHERE id = $1 FOR UPDATE"#)
            .bind(tournament_id)
            .fetch_one(&mut *tx)
            .await?;
    if status != "group_phase" {
        return Err(WebError::bad_request(
            "Der Rundenplan lässt sich nur neu erstellen, solange das Turnier läuft",
        ));
    }
    if group_phase_has_played_matches(&mut *tx, tournament_id).await? {
        return Err(WebError::bad_request(
            "Der Rundenplan lässt sich nicht mehr neu erstellen, sobald ein Spiel begonnen hat",
        ));
    }
    let slot_minutes = match slot_minutes {
        Some(minutes) => minutes,
        None => league_slot_minutes_in_tx(&mut tx, tournament_id)
            .await?
            .unwrap_or(DEFAULT_SLOT_MINUTES),
    };
    let plan = generate_league_plan_in_tx(&mut tx, tournament_id, slot_minutes).await?;
    audit(
        &mut *tx,
        "league_plan_generate",
        actor,
        json!({
            "tournament_id": tournament_id,
            "slot_minutes": slot_minutes,
            "matches": plan.matches,
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({
        "groups_created": 1,
        "matches_created": plan.matches,
    })))
}

/// `POST /api/admin/tournaments/{id}/bracket/generate` — Bracket generieren.
async fn generate_bracket_route(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    if tournament_is_league(&state.pool, tournament_id).await? {
        return Err(WebError::bad_request(
            "Das Format Jeder gegen jeden hat kein Bracket",
        ));
    }
    let match_count = generate_bracket(&state.pool, tournament_id).await?;

    audit(
        &state.pool,
        "bracket_generate",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "matches": match_count }),
    )
    .await?;

    Ok(Json(json!({ "bracket_matches_created": match_count })))
}
