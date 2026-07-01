//! Discord-Voice-Steuerung (Admin): Teams in VC1/VC2 verschieben, alle in den
//! Sammelpunkt, einzelne User verschieben, Channel-Mitglieder abfragen.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::db;
use crate::error::{WebError, WebResult};
use crate::extract::AdminUser;
use crate::state::AppState;

use super::helpers::team_member_discord_ids;

/// Router der Voice-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/tournaments/{tournament_id}/voice/move-teams",
            post(move_teams),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/voice/move-sammelpunkt",
            post(move_sammelpunkt),
        )
        .route("/api/admin/voice/move-user", post(move_user))
        .route(
            "/api/admin/voice/channel-members/{channel_id}",
            get(channel_members),
        )
}

/// `?match_id=int`-Query für `move-teams` (Pflicht).
#[derive(Debug, Deserialize)]
struct MatchIdQuery {
    match_id: i64,
}

/// `POST .../voice/move-teams?match_id=int` — Team1→VC1, Team2→VC2.
async fn move_teams(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(tournament_id): Path<i64>,
    Query(query): Query<MatchIdQuery>,
) -> WebResult<Json<Value>> {
    let row = sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
        r#"SELECT team1_id, team2_id FROM turnier."bracket_matches" WHERE id = $1 AND tournament_id = $2"#,
    )
    .bind(query.match_id)
    .bind(tournament_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| WebError::not_found("Match nicht gefunden"))?;
    let (team1_id, team2_id) = row;

    let guild_id: i64 = state.config.discord_guild_id.parse().unwrap_or(0);

    let team1_ids = match team1_id {
        Some(id) => team_member_discord_ids(&state.pool, id).await?,
        None => Vec::new(),
    };
    let team2_ids = match team2_id {
        Some(id) => team_member_discord_ids(&state.pool, id).await?,
        None => Vec::new(),
    };

    let result1 = state
        .notifier
        .move_users_to_voice_channel(
            &team1_ids,
            state.config.discord_team1_voice_channel_id,
            guild_id,
        )
        .await;
    let result2 = state
        .notifier
        .move_users_to_voice_channel(
            &team2_ids,
            state.config.discord_team2_voice_channel_id,
            guild_id,
        )
        .await;

    Ok(Json(json!({ "team1": result1, "team2": result2 })))
}

/// `POST .../voice/move-sammelpunkt` — alle Teilnehmer in den Sammelpunkt-VC.
async fn move_sammelpunkt(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        r#"SELECT DISTINCT tm.discord_id FROM turnier."team_members" tm
         JOIN turnier."teams" t ON t.id = tm.team_id
         WHERE t.tournament_id = $1"#,
    )
    .bind(tournament_id)
    .fetch_all(&state.pool)
    .await?;
    let all_ids: Vec<String> = rows
        .into_iter()
        .map(|r| db::discord_id_to_string(r.0))
        .filter(|s| !s.is_empty())
        .collect();

    let guild_id: i64 = state.config.discord_guild_id.parse().unwrap_or(0);
    let result = state
        .notifier
        .move_users_to_voice_channel(
            &all_ids,
            state.config.discord_sammelpunkt_channel_id,
            guild_id,
        )
        .await;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

/// Body von `move-user` (`discord_id` + `channel_id`).
#[derive(Debug, Deserialize)]
struct VoiceMoveBody {
    discord_id: String,
    channel_id: i64,
}

/// `POST /api/admin/voice/move-user` — einzelnen User in einen Voice-Kanal verschieben.
async fn move_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<VoiceMoveBody>,
) -> WebResult<Json<Value>> {
    let guild_id: i64 = state.config.discord_guild_id.parse().unwrap_or(0);
    let result = state
        .notifier
        .move_users_to_voice_channel(&[body.discord_id], body.channel_id, guild_id)
        .await;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

/// `GET /api/admin/voice/channel-members/{channel_id}` — Mitglieder eines VC.
async fn channel_members(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(channel_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let members = state
        .notifier
        .get_voice_channel_members(channel_id)
        .await
        .map_err(|err| {
            tracing::error!(channel_id, error = %err, "Voice-Channel-Mitglieder konnten nicht geladen werden");
            WebError::internal("Voice-Channel-Mitglieder konnten nicht geladen werden")
        })?;
    Ok(Json(
        json!({ "channel_id": channel_id, "members": members }),
    ))
}
