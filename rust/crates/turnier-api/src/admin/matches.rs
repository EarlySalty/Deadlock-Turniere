//! Bracket-Match-Leitstand: manuelles Ergebnis (Bracket ODER Group), Best-of-
//! Serien, Steam-Lobby (erstellen/starten/Ergebnis/verlassen/zurücksetzen/manuell),
//! Live-ConVars und Event-Presets.
//!
//! Die Steam-Lobby-Operationen sind über [`MatchKind`] mit `group_matches.rs`
//! geteilt (siehe [`super::steam_ops`]).

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use turnier_match::{ApplyBracketParams, MatchError, MatchKind};

use crate::error::{WebError, WebResult};
use crate::extract::{AdminUser, ModUser};
use crate::state::AppState;

use super::helpers::audit;
use super::steam_ops::{self, ManualLobbyBody};

/// Router der Bracket-Match-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/result",
            post(set_match_result),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/games/{game_number}/start",
            post(start_series_game),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/games/{game_number}/result",
            post(submit_series_game_result),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/create-lobby",
            post(create_lobby),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/start",
            post(start_match),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/fetch-result",
            post(fetch_result),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/leave-lobby",
            post(leave_lobby),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/reset",
            post(reset_match),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/manual-lobby",
            post(manual_lobby),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/event-presets",
            get(event_presets),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/apply-convars",
            post(apply_convars),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/apply-event-preset",
            post(apply_event_preset),
        )
}

/// `?force=bool`-Query für die manuelle Ergebnis-Eintragung.
#[derive(Debug, Deserialize)]
struct ForceQuery {
    #[serde(default)]
    force: bool,
}

/// Body von `set_match_result` (`winner_id` Pflicht).
#[derive(Debug, Deserialize)]
struct ResultBody {
    #[serde(default)]
    winner_id: Option<i64>,
}

/// `POST .../matches/{match_id}/result` — manuelles Ergebnis (Bracket ODER Group).
async fn set_match_result(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
    Query(query): Query<ForceQuery>,
    Json(body): Json<ResultBody>,
) -> WebResult<Json<Value>> {
    let Some(winner_id) = body.winner_id else {
        return Err(WebError::bad_request("winner_id (int) ist erforderlich"));
    };

    // Turnier prüfen.
    let tournament_exists: Option<i64> =
        sqlx::query_scalar(r#"SELECT id FROM turnier."tournaments" WHERE id = $1"#)
            .bind(tournament_id)
            .fetch_optional(&state.pool)
            .await?;
    if tournament_exists.is_none() {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    }

    // Zuerst Bracket-Match.
    let bracket = sqlx::query(
        r#"SELECT id FROM turnier."bracket_matches" WHERE id = $1 AND tournament_id = $2"#,
    )
    .bind(match_id)
    .bind(tournament_id)
    .fetch_optional(&state.pool)
    .await?;

    if bracket.is_some() {
        let outcome = state
            .match_manager
            .apply_bracket_match_result(
                tournament_id,
                match_id,
                ApplyBracketParams {
                    winner_id: Some(winner_id),
                    source: "manual".to_string(),
                    force: query.force,
                    ..Default::default()
                },
            )
            .await
            .map_err(map_bracket_result_error)?;

        audit(
            &state.pool,
            "match_result_bracket",
            &user.discord_id,
            json!({
                "tournament_id": tournament_id,
                "match_id": match_id,
                "winner_id": outcome.winner_id,
                "winning_team": outcome.winning_team,
                "source": "manual",
            }),
        )
        .await?;

        return Ok(Json(json!({
            "status": "ok",
            "match_type": "bracket",
            "match_id": outcome.match_id,
            "winner_id": outcome.winner_id,
            "winning_team": outcome.winning_team,
        })));
    }

    // Dann Group-Match (Geschäftslogik inline wie im Original; force ignoriert).
    let group_match = sqlx::query(
        r#"SELECT gm.id, gm.group_id, gm.team1_id, gm.team2_id, gm.status
         FROM turnier."group_matches" gm JOIN turnier."groups" g ON gm.group_id = g.id
         WHERE gm.id = $1 AND g.tournament_id = $2"#,
    )
    .bind(match_id)
    .bind(tournament_id)
    .fetch_optional(&state.pool)
    .await?;

    if let Some(gm) = group_match {
        let gm_status: String = gm.get("status");
        if ["completed", "cancelled", "forfeit"].contains(&gm_status.as_str()) {
            return Err(WebError::bad_request(format!(
                "Group-Match {match_id} kann aus Status {gm_status} nicht verarbeitet werden"
            )));
        }
        let team1_id: i64 = gm.get("team1_id");
        let team2_id: i64 = gm.get("team2_id");
        let group_id: i64 = gm.get("group_id");
        if winner_id != team1_id && winner_id != team2_id {
            return Err(WebError::bad_request(
                "winner_id muss eines der beiden Teams im Match sein",
            ));
        }
        let loser_id = if winner_id == team1_id {
            team2_id
        } else {
            team1_id
        };

        let mut tx = state.pool.begin().await?;
        sqlx::query(
            r#"UPDATE turnier."group_matches" SET winner_id = $1, status = 'completed', played_at = now() WHERE id = $2"#,
        )
        .bind(winner_id)
        .bind(match_id)
        .execute(&mut *tx)
        .await?;
        // Standings inkrementell (Befund admin_routes.py:2236 — +3 fix, nicht
        // idempotent; 1:1 erhalten, needs-decision).
        sqlx::query(
            r#"UPDATE turnier."group_teams" SET wins = wins + 1, points = points + 3 WHERE group_id = $1 AND team_id = $2"#,
        )
        .bind(group_id)
        .bind(winner_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(r#"UPDATE turnier."group_teams" SET losses = losses + 1 WHERE group_id = $1 AND team_id = $2"#)
            .bind(group_id)
            .bind(loser_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            r#"INSERT INTO turnier."match_results" (group_match_id, winning_team, source) VALUES ($1, $2, 'manual')"#,
        )
        .bind(match_id)
        .bind(winner_id)
        .execute(&mut *tx)
        .await?;
        audit(
            &mut *tx,
            "match_result_group",
            &user.discord_id,
            json!({
                "tournament_id": tournament_id,
                "match_id": match_id,
                "winner_id": winner_id,
                "group_id": group_id,
            }),
        )
        .await?;
        tx.commit().await?;

        return Ok(Json(json!({
            "status": "ok",
            "match_type": "group",
            "match_id": match_id,
            "winner_id": winner_id,
        })));
    }

    Err(WebError::not_found("Match nicht gefunden"))
}

/// `POST .../matches/{match_id}/games/{game_number}/start` — Serien-Spiel N
/// sicherstellen (Admin).
async fn start_series_game(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((tournament_id, match_id, game_number)): Path<(i64, i64, i64)>,
) -> WebResult<Json<Value>> {
    let bm = sqlx::query(
        r#"SELECT status FROM turnier."bracket_matches" WHERE id = $1 AND tournament_id = $2"#,
    )
    .bind(match_id)
    .bind(tournament_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| {
        WebError::not_found("Match nicht gefunden oder gehört nicht zu diesem Turnier")
    })?;
    let status: String = bm.get("status");
    if ["completed", "forfeit", "cancelled"].contains(&status.as_str()) {
        return Err(WebError::bad_request("Match bereits abgeschlossen"));
    }

    let game_id =
        turnier_match::series::ensure_game_exists(&state.pool, match_id, game_number).await?;
    let games = turnier_match::series::get_series_games(&state.pool, match_id).await?;
    Ok(Json(
        json!({ "game_id": game_id, "game_number": game_number, "games": games }),
    ))
}

/// Body von `submit_series_game_result` (`winner_team` 1/2, `duration_s` opt.).
#[derive(Debug, Deserialize)]
struct GameResultBody {
    winner_team: i64,
    #[serde(default)]
    duration_s: Option<i64>,
}

/// `POST .../matches/{match_id}/games/{game_number}/result` — Serien-Spiel-Ergebnis
/// (Admin). Bei Serien-Ende wird der Bracket-Winner gesetzt.
async fn submit_series_game_result(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path((tournament_id, match_id, game_number)): Path<(i64, i64, i64)>,
    Json(body): Json<GameResultBody>,
) -> WebResult<Json<Value>> {
    // Pydantic-Feld-Validierung (FastAPI → 422).
    if body.winner_team < 1 || body.winner_team > 2 {
        return Err(WebError::unprocessable("winner_team muss 1 oder 2 sein"));
    }
    if matches!(body.duration_s, Some(d) if d < 0) {
        return Err(WebError::unprocessable("duration_s muss >= 0 sein"));
    }

    let bm = sqlx::query(
        r#"SELECT status FROM turnier."bracket_matches" WHERE id = $1 AND tournament_id = $2"#,
    )
    .bind(match_id)
    .bind(tournament_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| {
        WebError::not_found("Match nicht gefunden oder gehört nicht zu diesem Turnier")
    })?;
    let status: String = bm.get("status");
    if ["completed", "forfeit", "cancelled"].contains(&status.as_str()) {
        return Err(WebError::bad_request("Match bereits abgeschlossen"));
    }

    turnier_match::series::ensure_game_exists(&state.pool, match_id, game_number).await?;
    let stats = turnier_match::GameStats {
        duration_s: body.duration_s,
        ..Default::default()
    };
    let series_result = turnier_match::series::record_game_result(
        &state.pool,
        match_id,
        game_number,
        body.winner_team,
        &stats,
    )
    .await?;

    if series_result.series_done {
        let match_row =
            sqlx::query(r#"SELECT team1_id, team2_id FROM turnier."bracket_matches" WHERE id = $1 AND tournament_id = $2"#)
                .bind(match_id)
                .bind(tournament_id)
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| WebError::not_found("Bracket-Match nicht gefunden"))?;
        let team1_id: Option<i64> = match_row.get("team1_id");
        let team2_id: Option<i64> = match_row.get("team2_id");
        let winner_team = series_result.series_winner_team.unwrap_or(0);
        let winner_id = if winner_team == 1 { team1_id } else { team2_id };

        audit(
            &state.pool,
            "match_result_series",
            &user.discord_id,
            json!({
                "tournament_id": tournament_id,
                "match_id": match_id,
                "game_number": game_number,
                "winner_id": winner_id,
                "series_winner_team": winner_team,
                "wins_team1": series_result.wins_team1,
                "wins_team2": series_result.wins_team2,
                "source": "series_manual",
            }),
        )
        .await?;

        // Bracket-Result anwenden (winning_team 0-basiert wie im Original).
        state
            .match_manager
            .apply_bracket_match_result(
                tournament_id,
                match_id,
                ApplyBracketParams {
                    winning_team: Some(winner_team - 1),
                    winner_id,
                    duration_s: body.duration_s,
                    source: "series_manual".to_string(),
                    ..Default::default()
                },
            )
            .await?;
    }

    Ok(Json(series_result.to_value()))
}

/// `POST .../matches/{match_id}/create-lobby` — Steam-Lobby für Bracket-Match.
async fn create_lobby(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::create_lobby(
        &state,
        MatchKind::Bracket,
        tournament_id,
        match_id,
        &user.discord_id,
    )
    .await
}

/// `POST .../matches/{match_id}/start` — Bracket-Match über Steam-Bot starten.
async fn start_match(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::start_match(
        &state,
        MatchKind::Bracket,
        tournament_id,
        match_id,
        &user.discord_id,
    )
    .await
}

/// `POST .../matches/{match_id}/fetch-result` — Match-Ergebnis aus Deadlock laden.
async fn fetch_result(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::fetch_result(
        &state,
        MatchKind::Bracket,
        tournament_id,
        match_id,
        &user.discord_id,
    )
    .await
}

/// `POST .../matches/{match_id}/leave-lobby` — Steam-Bot Bracket-Lobby verlassen.
async fn leave_lobby(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::leave_lobby(
        &state,
        MatchKind::Bracket,
        tournament_id,
        match_id,
        &user.discord_id,
    )
    .await
}

/// `POST .../matches/{match_id}/reset` — Bracket-Match auf pending zurücksetzen (Admin).
async fn reset_match(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    steam_ops::reset_match(
        &state,
        MatchKind::Bracket,
        tournament_id,
        match_id,
        &user.discord_id,
    )
    .await
}

/// `POST .../matches/{match_id}/manual-lobby` — party_code/steam_party_id setzen.
async fn manual_lobby(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
    Json(body): Json<ManualLobbyBody>,
) -> WebResult<Json<Value>> {
    steam_ops::manual_lobby(
        &state,
        MatchKind::Bracket,
        tournament_id,
        match_id,
        body,
        &user.discord_id,
    )
    .await
}

/// `GET .../matches/{match_id}/event-presets` — Live-Event-Presets fürs Panel.
async fn event_presets(
    State(state): State<AppState>,
    _mod: ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    // Match laden (Existenz → 404). Nutzt die öffentliche repo::get_match-Route
    // über den Manager-Pool (ersetzt den Kapselungsbruch `_get_bracket_match`).
    let m = turnier_match::repo::get_match(
        state.match_manager.pool(),
        MatchKind::Bracket,
        tournament_id,
        match_id,
    )
    .await
    .map_err(|err| match err {
        MatchError::NotFound(msg) => WebError::not_found(msg),
        other => other.into(),
    })?;

    let presets = state.match_manager.list_match_event_presets().await;
    Ok(Json(json!({
        "success": true,
        "match_id": match_id,
        "party_id": m.steam_party_id,
        "party_code": m.party_code,
        "presets": presets,
    })))
}

/// Body von `apply_convars` (`convars` als Objekt).
#[derive(Debug, Deserialize)]
struct ConvarsBody {
    #[serde(default)]
    convars: Option<Value>,
}

/// `POST .../matches/{match_id}/apply-convars` — freie ConVars anwenden.
async fn apply_convars(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
    Json(body): Json<ConvarsBody>,
) -> WebResult<Json<Value>> {
    let convars = match body.convars {
        Some(Value::Object(map)) => map,
        Some(Value::Null) | None => serde_json::Map::new(),
        Some(_) => {
            return Err(WebError::bad_request("convars muss ein JSON-Objekt sein"));
        }
    };

    let result = state
        .match_manager
        .apply_match_convars(tournament_id, match_id, &convars)
        .await
        .map_err(steam_ops::map_steam_error)?;

    audit(
        &state.pool,
        "match_apply_convars",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "match_id": match_id,
            "party_id": result.get("party_id"),
            "applied_convars": result.get("applied_convars"),
        }),
    )
    .await?;

    Ok(Json(result))
}

/// Body von `apply_event_preset` (`preset_key` Pflicht, `enabled` default true).
#[derive(Debug, Deserialize)]
struct EventPresetBody {
    #[serde(default)]
    preset_key: Option<String>,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_true() -> bool {
    true
}

/// `POST .../matches/{match_id}/apply-event-preset` — Event-Preset anwenden.
async fn apply_event_preset(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
    Json(body): Json<EventPresetBody>,
) -> WebResult<Json<Value>> {
    let preset_key = body.preset_key.unwrap_or_default().trim().to_string();
    if preset_key.is_empty() {
        return Err(WebError::bad_request("preset_key ist erforderlich"));
    }

    let result = state
        .match_manager
        .apply_match_event_preset(tournament_id, match_id, &preset_key, body.enabled)
        .await
        .map_err(steam_ops::map_steam_error)?;

    audit(
        &state.pool,
        "match_apply_event_preset",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "match_id": match_id,
            "preset_key": result.get("preset_key"),
            "enabled": result.get("enabled"),
            "party_id": result.get("party_id"),
            "applied_convars": result.get("applied_convars"),
        }),
    )
    .await?;

    Ok(Json(result))
}

/// Mappt `MatchError` aus `apply_bracket_match_result` wie das Original
/// (NotFound→404, State/Invalid→400). DB/Tournament-Fehler via Default-From.
fn map_bracket_result_error(err: MatchError) -> WebError {
    match err {
        MatchError::NotFound(msg) => WebError::not_found(msg),
        MatchError::State(msg) | MatchError::Invalid(msg) => WebError::bad_request(msg),
        other => other.into(),
    }
}
