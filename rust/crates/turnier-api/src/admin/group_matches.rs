//! Group-Match-Leitstand: Steam-Lobby-Operationen für Gruppen-Matches.
//!
//! DRY mit `matches.rs` über die [`super::steam_ops`]-Funktionen
//! (`MatchKind::Group`) — kein Copy-Paste der Bracket-Variante.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::Value;

use turnier_match::MatchKind;

use crate::error::WebResult;
use crate::extract::{AdminUser, ModUser};
use crate::state::AppState;

use super::steam_ops::{self, ManualLobbyBody};

/// Router der Group-Match-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/tournaments/{tournament_id}/group-matches/{match_id}/create-lobby", post(create_lobby))
        .route("/api/admin/tournaments/{tournament_id}/group-matches/{match_id}/start", post(start_match))
        .route("/api/admin/tournaments/{tournament_id}/group-matches/{match_id}/fetch-result", post(fetch_result))
        .route("/api/admin/tournaments/{tournament_id}/group-matches/{match_id}/leave-lobby", post(leave_lobby))
        .route("/api/admin/tournaments/{tournament_id}/group-matches/{match_id}/reset", post(reset_match))
        .route("/api/admin/tournaments/{tournament_id}/group-matches/{match_id}/manual-lobby", post(manual_lobby))
}

/// `POST .../group-matches/{match_id}/create-lobby` — Steam-Lobby für Group-Match.
async fn create_lobby(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::create_lobby(&state, MatchKind::Group, tournament_id, match_id, &user.discord_id).await
}

/// `POST .../group-matches/{match_id}/start` — Group-Match über Steam-Bot starten.
async fn start_match(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::start_match(&state, MatchKind::Group, tournament_id, match_id, &user.discord_id).await
}

/// `POST .../group-matches/{match_id}/fetch-result` — Group-Ergebnis aus Deadlock laden.
async fn fetch_result(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::fetch_result(&state, MatchKind::Group, tournament_id, match_id, &user.discord_id).await
}

/// `POST .../group-matches/{match_id}/leave-lobby` — Steam-Bot Gruppen-Lobby verlassen.
async fn leave_lobby(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::leave_lobby(&state, MatchKind::Group, tournament_id, match_id, &user.discord_id).await
}

/// `POST .../group-matches/{match_id}/reset` — Group-Match auf pending zurücksetzen (Admin).
async fn reset_match(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::reset_match(&state, MatchKind::Group, tournament_id, match_id, &user.discord_id).await
}

/// `POST .../group-matches/{match_id}/manual-lobby` — party_code/steam_party_id setzen.
async fn manual_lobby(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
    Json(body): Json<ManualLobbyBody>,
) -> WebResult<Json<Value>> {
    steam_ops::manual_lobby(&state, MatchKind::Group, tournament_id, match_id, body, &user.discord_id)
        .await
}
