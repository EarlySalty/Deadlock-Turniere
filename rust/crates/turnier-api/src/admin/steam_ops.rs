//! Geteilte Steam-Lobby-Operationen für Bracket- UND Group-Matches.
//!
//! Bündelt das Copy-Paste der `admin_routes.py`-Lobby-Endpunkte (create/start/
//! fetch/leave/reset/manual) in [`MatchKind`]-parametrisierte Funktionen. Die
//! Fehler-Mappings entsprechen exakt dem Original (MatchNotFound→404,
//! MatchState→400, SteamTask→502, Timeout→504, Bridge/Runtime→502).

use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use turnier_match::{MatchKind, SteamTaskError};

use crate::error::{WebError, WebResult};
use crate::state::AppState;

/// Body von `manual-lobby` (`party_code` Pflicht, `steam_party_id` optional).
#[derive(Debug, Deserialize)]
pub struct ManualLobbyBody {
    pub party_code: String,
    #[serde(default)]
    pub steam_party_id: Option<String>,
}

/// Mappt einen [`SteamTaskError`] exakt wie die Python-try/except-Leiter:
/// NotFound→404, State→400, Failed/Bridge/InvalidResult→502, Timeout→504.
pub fn map_steam_error(err: SteamTaskError) -> WebError {
    match err {
        SteamTaskError::NotFound(msg) => WebError::not_found(msg),
        SteamTaskError::State(msg) => WebError::bad_request(msg),
        SteamTaskError::Timeout { .. } => {
            WebError::new(StatusCode::GATEWAY_TIMEOUT, err_timeout_message(&err))
        }
        SteamTaskError::Failed(msg) | SteamTaskError::InvalidResult(msg) => {
            WebError::new(StatusCode::BAD_GATEWAY, msg)
        }
        SteamTaskError::Bridge(_) => WebError::new(StatusCode::BAD_GATEWAY, err.to_string()),
        SteamTaskError::Db(e) => e.into(),
    }
}

/// Erzeugt die generische Timeout-Detailmeldung (Original nutzt aktionsabhängige
/// Texte; der Status 504 ist überall identisch).
fn err_timeout_message(err: &SteamTaskError) -> String {
    err.to_string()
}

/// `create-lobby` für beide Match-Arten.
pub async fn create_lobby(
    state: &AppState,
    kind: MatchKind,
    tournament_id: i64,
    match_id: i64,
    actor: &str,
) -> WebResult<Json<Value>> {
    let result = match kind {
        MatchKind::Bracket => state.match_manager.create_lobby(tournament_id, match_id).await,
        MatchKind::Group => state.match_manager.create_group_lobby(tournament_id, match_id).await,
    }
    .map_err(map_steam_error)?;

    let action = if kind == MatchKind::Bracket { "match_create_lobby" } else { "group_match_create_lobby" };
    super::helpers::audit(
        &state.pool,
        action,
        actor,
        json!({
            "tournament_id": tournament_id,
            "match_id": match_id,
            "party_id": result.get("party_id"),
            "party_code": result.get("party_code"),
            "join_code": result.get("join_code"),
        }),
    )
    .await?;

    Ok(Json(json!({
        "success": true,
        "party_id": result.get("party_id"),
        "party_code": result.get("party_code"),
        "join_code": result.get("join_code"),
    })))
}

/// `start` für beide Match-Arten.
pub async fn start_match(
    state: &AppState,
    kind: MatchKind,
    tournament_id: i64,
    match_id: i64,
    actor: &str,
) -> WebResult<Json<Value>> {
    let result = match kind {
        MatchKind::Bracket => state.match_manager.start_match(tournament_id, match_id).await,
        MatchKind::Group => state.match_manager.start_group_match(tournament_id, match_id).await,
    }
    .map_err(map_steam_error)?;

    let action = if kind == MatchKind::Bracket { "match_start_steam" } else { "group_match_start_steam" };
    super::helpers::audit(
        &state.pool,
        action,
        actor,
        json!({
            "tournament_id": tournament_id,
            "match_id": match_id,
            "deadlock_match_id": result.get("match_id"),
        }),
    )
    .await?;

    Ok(Json(json!({ "success": true, "match_id": result.get("match_id") })))
}

/// `fetch-result` für beide Match-Arten (gibt das rohe Ergebnis-Dict zurück).
pub async fn fetch_result(
    state: &AppState,
    kind: MatchKind,
    tournament_id: i64,
    match_id: i64,
    actor: &str,
) -> WebResult<Json<Value>> {
    let result = match kind {
        MatchKind::Bracket => state.match_manager.fetch_match_result(tournament_id, match_id).await,
        MatchKind::Group => state.match_manager.fetch_group_match_result(tournament_id, match_id).await,
    }
    .map_err(map_steam_error)?;

    let action = if kind == MatchKind::Bracket {
        "match_fetch_result_steam"
    } else {
        "group_match_fetch_result_steam"
    };
    super::helpers::audit(
        &state.pool,
        action,
        actor,
        json!({
            "tournament_id": tournament_id,
            "match_id": match_id,
            "winner_id": result.get("winner_id"),
            "duration_s": result.get("duration_s"),
        }),
    )
    .await?;

    Ok(Json(result))
}

/// `leave-lobby` für beide Match-Arten.
pub async fn leave_lobby(
    state: &AppState,
    kind: MatchKind,
    tournament_id: i64,
    match_id: i64,
    actor: &str,
) -> WebResult<Json<Value>> {
    let result = match kind {
        MatchKind::Bracket => state.match_manager.leave_lobby(tournament_id, match_id).await,
        MatchKind::Group => state.match_manager.leave_group_lobby(tournament_id, match_id).await,
    }
    .map_err(map_steam_error)?;

    let action = if kind == MatchKind::Bracket { "match_leave_lobby" } else { "group_match_leave_lobby" };
    super::helpers::audit(
        &state.pool,
        action,
        actor,
        json!({ "tournament_id": tournament_id, "match_id": match_id }),
    )
    .await?;

    Ok(Json(result))
}

/// `reset` für beide Match-Arten (Status-Reset + Discord-Channel löschen).
///
/// Portiert `_reset_match_record`: 404 wenn Match fehlt, 400 bei
/// abgeschlossenem/abgebrochenem Match, sonst Reset + Audit.
pub async fn reset_match(
    state: &AppState,
    kind: MatchKind,
    tournament_id: i64,
    match_id: i64,
    actor: &str,
) -> WebResult<Json<Value>> {
    let select_sql = match kind {
        MatchKind::Bracket => {
            "SELECT discord_channel_id, status FROM bracket_matches WHERE id = ? AND tournament_id = ?"
        }
        MatchKind::Group => {
            "SELECT gm.discord_channel_id, gm.status FROM group_matches gm \
             JOIN groups g ON g.id = gm.group_id WHERE gm.id = ? AND g.tournament_id = ?"
        }
    };
    let row = sqlx::query(select_sql)
        .bind(match_id)
        .bind(tournament_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| WebError::not_found("Match nicht gefunden"))?;
    let status: String = row.get("status");
    if ["completed", "forfeit", "cancelled"].contains(&status.as_str()) {
        return Err(WebError::bad_request(
            "Abgeschlossene oder abgebrochene Matches können nicht zurückgesetzt werden",
        ));
    }
    let discord_channel_id: Option<String> = row.get("discord_channel_id");
    if let Some(channel_id) = discord_channel_id.filter(|c| !c.is_empty()) {
        if let Err(err) = state.notifier.delete_match_channel(&channel_id).await {
            tracing::error!(error = %err, "Discord-Channel konnte beim Match-Reset nicht gelöscht werden");
        }
    }

    let (update_sql, action) = match kind {
        MatchKind::Bracket => (
            "UPDATE bracket_matches SET status = 'pending', steam_party_id = NULL, party_code = NULL, \
             deadlock_match_id = NULL, discord_channel_id = NULL WHERE id = ? AND tournament_id = ?",
            "match_reset",
        ),
        MatchKind::Group => (
            "UPDATE group_matches SET status = 'pending', steam_party_id = NULL, party_code = NULL, \
             deadlock_match_id = NULL, discord_channel_id = NULL \
             WHERE id = ? AND group_id IN (SELECT id FROM groups WHERE tournament_id = ?)",
            "group_match_reset",
        ),
    };

    let mut tx = state.pool.begin().await?;
    sqlx::query(update_sql)
        .bind(match_id)
        .bind(tournament_id)
        .execute(&mut *tx)
        .await?;
    super::helpers::audit(
        &mut *tx,
        action,
        actor,
        json!({ "tournament_id": tournament_id, "match_id": match_id }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "success": true })))
}

/// `manual-lobby` für beide Match-Arten (party_code/steam_party_id manuell setzen).
pub async fn manual_lobby(
    state: &AppState,
    kind: MatchKind,
    tournament_id: i64,
    match_id: i64,
    body: ManualLobbyBody,
    actor: &str,
) -> WebResult<Json<Value>> {
    let party_code = body.party_code.trim().to_string();
    if party_code.is_empty() {
        return Err(WebError::bad_request("party_code ist erforderlich"));
    }
    let steam_party_id = body
        .steam_party_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let (update_sql, action, fail_detail) = match kind {
        MatchKind::Bracket => (
            "UPDATE bracket_matches SET party_code = ?, steam_party_id = ?, status = 'lobby_created' \
             WHERE id = ? AND tournament_id = ? AND status NOT IN ('completed', 'forfeit', 'cancelled')",
            "match_manual_lobby",
            "Match kann nicht manuell gesetzt werden",
        ),
        MatchKind::Group => (
            "UPDATE group_matches SET party_code = ?, steam_party_id = ?, status = 'lobby_created' \
             WHERE id = ? AND group_id IN (SELECT id FROM groups WHERE tournament_id = ?) \
             AND status NOT IN ('completed', 'forfeit', 'cancelled')",
            "group_match_manual_lobby",
            "Gruppen-Match kann nicht manuell gesetzt werden",
        ),
    };

    let mut tx = state.pool.begin().await?;
    let affected = sqlx::query(update_sql)
        .bind(&party_code)
        .bind(&steam_party_id)
        .bind(match_id)
        .bind(tournament_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(WebError::bad_request(fail_detail));
    }
    super::helpers::audit(
        &mut *tx,
        action,
        actor,
        json!({ "tournament_id": tournament_id, "match_id": match_id, "party_code": party_code }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "success": true, "party_code": party_code })))
}
