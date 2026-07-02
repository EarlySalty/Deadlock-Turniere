//! Gruppen-/Bracket-Generierung. Delegiert an [`turnier_engine::generate_groups`],
//! [`turnier_engine::generate_group_matches`] und [`turnier_engine::generate_bracket`].

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use turnier_engine::{generate_bracket, generate_group_matches, generate_groups};

use crate::error::WebResult;
use crate::extract::ModUser;
use crate::state::AppState;

use super::helpers::audit;

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
}

/// `POST /api/admin/tournaments/{id}/groups/generate` — Gruppen (Snake-Draft) +
/// Gruppen-Matches generieren.
async fn generate_groups_route(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
    body: Option<Json<GroupsBody>>,
) -> WebResult<Json<Value>> {
    // num_groups auf [2, 8] klemmen (wie das Original); fehlend → None (Auto).
    let num_groups = body
        .and_then(|Json(b)| b.num_groups)
        .map(|n| n.clamp(2, 8) as usize);

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

/// `POST /api/admin/tournaments/{id}/bracket/generate` — Bracket generieren.
async fn generate_bracket_route(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
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
