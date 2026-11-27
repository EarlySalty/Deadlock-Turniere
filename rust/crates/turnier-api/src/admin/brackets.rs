//! Gruppen-/Bracket-Generierung. Delegiert an [`turnier_engine::generate_groups_in_tx`],
//! [`turnier_engine::generate_group_matches_in_tx`] und [`turnier_engine::generate_bracket_in_tx`].

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use sqlx::Row;
use turnier_core::TournamentMode;
use turnier_engine::league::DEFAULT_SLOT_MINUTES;
use turnier_engine::{
    generate_bracket_in_tx, generate_group_matches_in_tx, generate_groups_in_tx,
    generate_league_plan_in_tx, league_slot_minutes_in_tx,
};

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

async fn locked_tournament_is_league(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tournament_id: i64,
) -> WebResult<bool> {
    let row = crate::public::helpers::lock_tournament_row(tx, tournament_id).await?;
    let mode: TournamentMode = row.try_get("tournament_mode")?;
    Ok(mode == TournamentMode::League)
}

/// Body von `groups/generate` (`num_groups` optional).
#[derive(Debug, Default, Deserialize)]
struct GroupsBody {
    #[serde(default)]
    num_groups: Option<i64>,
    #[serde(default)]
    slot_minutes: Option<i64>,
}

/// `POST /api/admin/tournaments/{id}/groups/generate`: Gruppen (Snake-Draft) +
/// Gruppen-Matches generieren.
async fn generate_groups_route(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
    body: Option<Json<GroupsBody>>,
) -> WebResult<Json<Value>> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let mut tx = state.pool.begin().await?;
    if locked_tournament_is_league(&mut tx, tournament_id).await? {
        drop(tx);
        return generate_league_plan(&state, &user.discord_id, tournament_id, body.slot_minutes)
            .await;
    }
    // num_groups auf [2, 8] klemmen (wie das Original); fehlend → None (Auto).
    let num_groups = body.num_groups.map(|n| n.clamp(2, 8) as usize);

    let group_ids = generate_groups_in_tx(&mut tx, tournament_id, num_groups).await?;
    let match_count = generate_group_matches_in_tx(&mut tx, tournament_id).await?;

    audit(
        &mut *tx,
        "groups_generate",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "groups": group_ids.len(),
            "matches": match_count,
        }),
    )
    .await?;
    tx.commit().await?;

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
    let status = crate::public::helpers::lock_tournament_status(&mut tx, tournament_id).await?;
    if status != turnier_core::TournamentStatus::GroupPhase {
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

/// `POST /api/admin/tournaments/{id}/bracket/generate`: Bracket generieren.
async fn generate_bracket_route(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    if locked_tournament_is_league(&mut tx, tournament_id).await? {
        return Err(WebError::bad_request(
            "Das Format Jeder gegen jeden hat kein Bracket",
        ));
    }
    let match_count = generate_bracket_in_tx(&mut tx, tournament_id).await?;

    audit(
        &mut *tx,
        "bracket_generate",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "matches": match_count }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "bracket_matches_created": match_count })))
}
