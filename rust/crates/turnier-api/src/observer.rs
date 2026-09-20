use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::sync::{mpsc, Mutex};
use turnier_observer::live::{DeadlockLiveClient, LiveAccumulator, LiveRowKind};
use turnier_observer::{
    AgentAck, AgentHeartbeat, CameraAction, CameraCommand, Director, ObserverMode,
};

use crate::error::{WebError, WebResult};
use crate::extract::AdminUser;
use crate::state::AppState;

const BOT2_ACCOUNT_ID: i16 = 2;
const COMMAND_TTL_SECONDS: i64 = 4;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/observer/sessions",
            post(create_session).get(list_sessions),
        )
        .route("/api/observer/sessions/{id}", get(get_session))
        .route("/api/observer/sessions/{id}/mode", post(set_mode))
        .route("/api/observer/sessions/{id}/retry", post(retry_session))
        .route("/api/observer/sessions/{id}/finish", post(finish_session))
        .route(
            "/api/observer/bot2/lease",
            get(bot2_lease_status).post(bot2_lease_set),
        )
        .route("/api/observer/agent/heartbeat", post(agent_heartbeat))
        .route("/api/observer/agent/commands", get(agent_commands))
        .route(
            "/api/observer/agent/commands/{id}/ack",
            post(agent_command_ack),
        )
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    scrim_match_id: Option<i32>,
    draft_code: Option<String>,
    steam_match_id: Option<String>,
    mode: Option<ObserverMode>,
}

#[derive(Debug, Deserialize)]
struct SetModeRequest {
    mode: ObserverMode,
}

#[derive(Debug, Serialize)]
struct ObserverSessionDto {
    id: i64,
    session_key: String,
    scrim_match_id: Option<i32>,
    draft_code: Option<String>,
    steam_match_id: Option<String>,
    lobby_party_id: Option<String>,
    bot_account_id: i16,
    mode: String,
    state: String,
    enabled: bool,
    current_account_id: Option<String>,
    current_score: Option<f64>,
    recommended_account_id: Option<String>,
    recommended_score: Option<f64>,
    fallback_reason: Option<String>,
    last_live_event_at: Option<DateTime<Utc>>,
    last_agent_heartbeat_at: Option<DateTime<Utc>>,
    last_agent_version: Option<String>,
    last_vconsole_ok: Option<bool>,
    last_game_connected: Option<bool>,
    /// False im normalen Safe Mode. Dann darf weder der Server Auto aktivieren
    /// noch der PC-Agent VConsole/Game-Control benutzen.
    game_control_enabled: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

async fn create_session(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
    Json(input): Json<CreateSessionRequest>,
) -> WebResult<Json<ObserverSessionDto>> {
    if !state.config.observer_enabled {
        return Err(WebError::conflict(
            "Der Auto-Observer ist serverseitig noch nicht freigeschaltet",
        ));
    }
    if input.scrim_match_id.is_none()
        && input.draft_code.is_none()
        && input.steam_match_id.is_none()
    {
        return Err(WebError::bad_request(
            "scrim_match_id, draft_code oder steam_match_id ist erforderlich",
        ));
    }
    let steam_match_id = resolve_steam_match_id(&state, &input).await?;
    let lobby_party_id = resolve_lobby_party_id(&state, &input).await?;
    let active_bot2: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM scrim.observer_sessions \
         WHERE bot_account_id=$1 AND enabled=TRUE AND finished_at IS NULL)",
    )
    .bind(BOT2_ACCOUNT_ID)
    .fetch_one(&state.pool)
    .await?;
    if active_bot2 {
        return Err(WebError::conflict(
            "Steam Bot 2 beobachtet bereits eine andere Partie; bitte die laufende Observer-Session zuerst beenden",
        ));
    }
    if let Some(match_id) = steam_match_id {
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM scrim.observer_sessions \
             WHERE steam_match_id=$1 AND enabled=TRUE AND finished_at IS NULL)",
        )
        .bind(match_id)
        .fetch_one(&state.pool)
        .await?;
        if duplicate {
            return Err(WebError::conflict(
                "Für diese Deadlock-Match-ID läuft bereits ein Observer",
            ));
        }
    }

    let mode = mode_str(input.mode.unwrap_or(ObserverMode::Shadow));
    let row = sqlx::query(
        "INSERT INTO scrim.observer_sessions( \
             session_key, scrim_match_id, draft_code, steam_match_id, lobby_party_id, bot_account_id, mode, state \
         ) VALUES ( \
             'obs-' || substr(md5(random()::text || clock_timestamp()::text), 1, 20), \
             $1, NULLIF(BTRIM($2), ''), $3, $4, $5, $6, CASE WHEN $3 IS NULL THEN 'waiting' ELSE 'pairing' END \
         ) RETURNING *",
    )
    .bind(input.scrim_match_id)
    .bind(input.draft_code.unwrap_or_default())
    .bind(steam_match_id)
    .bind(lobby_party_id)
    .bind(BOT2_ACCOUNT_ID)
    .bind(mode)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(session_from_row(
        &row,
        state.config.observer_game_control_enabled,
    )?))
}

async fn list_sessions(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
) -> WebResult<Json<Value>> {
    let rows = sqlx::query("SELECT * FROM scrim.observer_sessions ORDER BY id DESC LIMIT 50")
        .fetch_all(&state.pool)
        .await?;
    let game_control_enabled = state.config.observer_game_control_enabled;
    let sessions = rows
        .iter()
        .map(|row| session_from_row(row, game_control_enabled))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(json!({ "sessions": sessions })))
}

async fn get_session(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
    Path(id): Path<i64>,
) -> WebResult<Json<Value>> {
    let row = load_session(&state, id).await?;
    let decisions = sqlx::query(
        "SELECT observed_at, account_id, hero_id, score, current_score, switched, reason, factors, frame_age_ms \
         FROM scrim.observer_decisions WHERE observer_session_id=$1 \
         ORDER BY observed_at DESC LIMIT 30",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let recent = decisions
        .into_iter()
        .map(|row| {
            json!({
                "observed_at": row.try_get::<DateTime<Utc>, _>("observed_at").ok(),
                "account_id": row.try_get::<Option<i64>, _>("account_id").ok().flatten().map(|v| v.to_string()),
                "hero_id": row.try_get::<Option<i32>, _>("hero_id").ok().flatten(),
                "score": row.try_get::<f64, _>("score").unwrap_or_default(),
                "current_score": row.try_get::<Option<f64>, _>("current_score").ok().flatten(),
                "switched": row.try_get::<bool, _>("switched").unwrap_or(false),
                "reason": row.try_get::<String, _>("reason").unwrap_or_default(),
                "factors": row.try_get::<Value, _>("factors").unwrap_or_else(|_| json!({})),
                "frame_age_ms": row.try_get::<Option<i64>, _>("frame_age_ms").ok().flatten(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "session": session_from_row(&row, state.config.observer_game_control_enabled)?,
        "recent_decisions": recent,
    })))
}

async fn set_mode(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
    Path(id): Path<i64>,
    Json(input): Json<SetModeRequest>,
) -> WebResult<Json<ObserverSessionDto>> {
    if matches!(input.mode, ObserverMode::Auto) {
        if !state.config.observer_game_control_enabled {
            return Err(WebError::conflict(
                "Auto-Kamerasteuerung ist aus Anti-Cheat-Sicherheitsgruenden serverseitig deaktiviert. Shadow und Assist bleiben verfuegbar.",
            ));
        }
        if state.config.observer_agent_token.trim().len() < 24 {
            return Err(WebError::conflict(
                "Auto-Modus braucht einen konfigurierten Observer-Agent-Token",
            ));
        }
        let readiness = sqlx::query(
            "SELECT last_agent_heartbeat_at, last_vconsole_ok, last_game_connected, \
                    scrim_match_id, draft_code, lobby_party_id \
             FROM scrim.observer_sessions WHERE id=$1 AND finished_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| WebError::not_found("Observer-Session nicht gefunden"))?;
        let scrim_match_id: Option<i32> = readiness.try_get("scrim_match_id")?;
        let draft_code: Option<String> = readiness.try_get("draft_code")?;
        let lobby_party_id: Option<i64> = readiness.try_get("lobby_party_id")?;
        if !auto_scope_allowed(scrim_match_id, draft_code.as_deref(), lobby_party_id) {
            return Err(WebError::conflict(
                "Auto ist nur fuer eine nachweislich eigene Scrim-/Draft-Lobby erlaubt. Manuelle Match-ID- und Public-Tests bleiben Shadow/Assist.",
            ));
        }
        let heartbeat: Option<DateTime<Utc>> = readiness.try_get("last_agent_heartbeat_at")?;
        let vconsole_ok: Option<bool> = readiness.try_get("last_vconsole_ok")?;
        let game_connected: Option<bool> = readiness.try_get("last_game_connected")?;
        let fresh = heartbeat
            .is_some_and(|at| Utc::now().signed_duration_since(at) <= Duration::seconds(8));
        if !fresh || vconsole_ok != Some(true) || game_connected != Some(true) {
            return Err(WebError::conflict(
                "Auto-Modus bleibt gesperrt, bis der lokale Observer-Agent frisch verbunden, VConsole bereit und die Deadlock-Spectator-Session bestätigt ist",
            ));
        }
        let lease = proxy_bot2_lease(&state, None).await?;
        let reserved = lease.get("reserved").and_then(Value::as_bool) == Some(true);
        let steam_connected = lease.get("steam_connected").and_then(Value::as_bool) == Some(true);
        if !reserved || steam_connected {
            return Err(WebError::conflict(
                "Auto-Modus braucht die aktive Steam-Bot-2-Observer-Reservierung ohne Headless-Steam-Session",
            ));
        }
    }
    let previous_mode: Option<String> = sqlx::query_scalar(
        "SELECT mode FROM scrim.observer_sessions WHERE id=$1 AND finished_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let Some(previous_mode) = previous_mode else {
        return Err(WebError::not_found("Observer-Session nicht gefunden"));
    };
    if previous_mode == "auto" && !matches!(input.mode, ObserverMode::Auto) {
        enqueue_command(&state, id, &CameraAction::Directed, "mode_left_auto", None).await?;
    }
    let row = sqlx::query(
        "UPDATE scrim.observer_sessions SET mode=$2, updated_at=now() \
         WHERE id=$1 AND finished_at IS NULL RETURNING *",
    )
    .bind(id)
    .bind(mode_str(input.mode))
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| WebError::not_found("Observer-Session nicht gefunden"))?;
    Ok(Json(session_from_row(
        &row,
        state.config.observer_game_control_enabled,
    )?))
}

async fn retry_session(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
    Path(id): Path<i64>,
) -> WebResult<Json<ObserverSessionDto>> {
    let row = sqlx::query(
        "UPDATE scrim.observer_sessions SET state=CASE WHEN steam_match_id IS NULL THEN 'waiting' ELSE 'pairing' END, \
         fallback_reason=NULL, enabled=TRUE, updated_at=now() \
         WHERE id=$1 AND finished_at IS NULL RETURNING *",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| WebError::not_found("Observer-Session nicht gefunden"))?;
    Ok(Json(session_from_row(
        &row,
        state.config.observer_game_control_enabled,
    )?))
}

async fn finish_session(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
    Path(id): Path<i64>,
) -> WebResult<Json<ObserverSessionDto>> {
    enqueue_directed_if_needed(&state, id, "session_finished").await?;
    let row = sqlx::query(
        "UPDATE scrim.observer_sessions SET state='finished', enabled=FALSE, finished_at=now(), updated_at=now() \
         WHERE id=$1 RETURNING *",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| WebError::not_found("Observer-Session nicht gefunden"))?;
    Ok(Json(session_from_row(
        &row,
        state.config.observer_game_control_enabled,
    )?))
}

#[derive(Debug, Deserialize)]
struct Bot2LeaseRequest {
    reserved: bool,
}

async fn bot2_lease_status(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
) -> WebResult<Json<Value>> {
    proxy_bot2_lease(&state, None).await.map(Json)
}

async fn bot2_lease_set(
    State(state): State<AppState>,
    AdminUser(_): AdminUser,
    Json(input): Json<Bot2LeaseRequest>,
) -> WebResult<Json<Value>> {
    proxy_bot2_lease(&state, Some(input.reserved))
        .await
        .map(Json)
}

async fn proxy_bot2_lease(state: &AppState, reserved: Option<bool>) -> WebResult<Value> {
    if state.config.steam_bot_internal_token.trim().is_empty() {
        return Err(WebError::conflict(
            "Steam-Bot-Internal-Token ist fuer die Bot-2-Reservierung nicht konfiguriert",
        ));
    }
    let url = format!(
        "{}/observer/v1/lease",
        state
            .config
            .observer_steam_bot2_base_url
            .trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(
            state.config.network.observer_request_seconds,
        ))
        .build()
        .map_err(|err| WebError::internal(format!("Steam-Bot-2-Client: {err}")))?;
    let mut request = match reserved {
        Some(value) => client.post(url).json(&json!({ "reserved": value })),
        None => client.get(url),
    };
    request = request.header(
        "X-Internal-Token",
        state.config.steam_bot_internal_token.as_str(),
    );
    let response = request.send().await.map_err(|err| {
        WebError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Steam Bot 2 nicht erreichbar: {err}"),
        )
    })?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or_else(|_| json!({}));
    if !status.is_success() {
        let message = body
            .get("message")
            .or_else(|| body.get("detail"))
            .and_then(Value::as_str)
            .unwrap_or("Steam Bot 2 hat die Observer-Reservierung abgelehnt");
        return Err(WebError::new(status, message));
    }
    Ok(body)
}

#[derive(Debug, Deserialize)]
struct AgentCommandsQuery {
    after_id: Option<i64>,
}

async fn agent_commands(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgentCommandsQuery>,
) -> WebResult<Json<Value>> {
    require_agent(&state, &headers)?;
    let rows = sqlx::query(
        "SELECT c.id, s.session_key, c.action, c.account_id, c.lobby_id, c.reason, c.score, c.issued_at, c.expires_at \
         FROM scrim.observer_commands c \
         JOIN scrim.observer_sessions s ON s.id=c.observer_session_id \
         WHERE c.id > $1 AND s.bot_account_id=$2 \
           AND c.acked_at IS NULL AND c.expires_at > now() \
         ORDER BY c.id ASC LIMIT 20",
    )
    .bind(query.after_id.unwrap_or(0))
    .bind(BOT2_ACCOUNT_ID)
    .fetch_all(&state.pool)
    .await?;
    let mut commands = Vec::with_capacity(rows.len());
    for row in rows {
        let action_raw: String = row.try_get("action")?;
        let account = row
            .try_get::<Option<i64>, _>("account_id")?
            .and_then(|value| u32::try_from(value).ok());
        let lobby_id = row
            .try_get::<Option<i64>, _>("lobby_id")?
            .and_then(|value| u64::try_from(value).ok());
        let action = parse_action(&action_raw, account, lobby_id)?;
        commands.push(CameraCommand {
            id: row.try_get("id")?,
            session_key: row.try_get("session_key")?,
            action,
            reason: row.try_get("reason")?,
            score: row.try_get("score")?,
            issued_at: row.try_get("issued_at")?,
            expires_at: row.try_get("expires_at")?,
        });
    }
    Ok(Json(json!({ "commands": commands })))
}

async fn agent_command_ack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(ack): Json<AgentAck>,
) -> WebResult<Json<Value>> {
    require_agent(&state, &headers)?;
    let updated = sqlx::query(
        "UPDATE scrim.observer_commands SET acked_at=now(), ack_ok=$2, ack_detail=$3 \
         WHERE id=$1 AND acked_at IS NULL RETURNING observer_session_id",
    )
    .bind(id)
    .bind(ack.ok)
    .bind(ack.detail.as_deref())
    .fetch_optional(&state.pool)
    .await?;
    let Some(row) = updated else {
        return Err(WebError::not_found(
            "Observer-Kommando nicht gefunden oder bereits bestätigt",
        ));
    };
    let session_id: i64 = row.try_get("observer_session_id")?;
    if !ack.ok {
        sqlx::query(
            "UPDATE scrim.observer_sessions SET state='degraded', fallback_reason='agent_command_failed', updated_at=now() \
             WHERE id=$1 AND finished_at IS NULL",
        )
        .bind(session_id)
        .execute(&state.pool)
        .await?;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn agent_heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(heartbeat): Json<AgentHeartbeat>,
) -> WebResult<Json<Value>> {
    require_agent(&state, &headers)?;
    if heartbeat.bot_account_id != BOT2_ACCOUNT_ID {
        return Err(WebError::forbidden(
            "Nur Steam Bot 2 ist fuer den Observer freigegeben",
        ));
    }
    sqlx::query(
        "UPDATE scrim.observer_sessions SET last_agent_heartbeat_at=now(), last_agent_version=$1, \
         last_vconsole_ok=$2, last_game_connected=$3, updated_at=now() \
         WHERE bot_account_id=$4 AND enabled=TRUE AND finished_at IS NULL",
    )
    .bind(&heartbeat.agent_version)
    .bind(heartbeat.vconsole_connected)
    .bind(heartbeat.game_connected)
    .bind(BOT2_ACCOUNT_ID)
    .execute(&state.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub fn spawn_observer_worker(state: AppState) {
    if !state.config.observer_enabled {
        tracing::info!("Scrim-Observer deaktiviert (SCRIM_OBSERVER_ENABLED)");
        return;
    }
    let active = Arc::new(Mutex::new(HashSet::<i64>::new()));
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(StdDuration::from_millis(
            state.config.scheduler.observer_tick_milliseconds,
        ));
        loop {
            tick.tick().await;
            if let Err(err) = discover_draft_observers(&state).await {
                tracing::warn!(error = %err, "Observer konnte gestartete Draft-Lobbys nicht automatisch anbinden");
            }
            if let Err(err) = finish_completed_draft_observers(&state).await {
                tracing::warn!(error = %err, "Observer konnte beendete Draft-Lobbys nicht abschliessen");
            }
            if let Err(err) = ensure_spectate_lobby_bootstrap(&state).await {
                tracing::warn!(error = %err, "Observer konnte Steam Bot 2 nicht automatisch in die Spectator-Lobby schicken");
            }
            let rows = match sqlx::query(
                "SELECT id, steam_match_id FROM scrim.observer_sessions \
                 WHERE enabled=TRUE AND finished_at IS NULL AND steam_match_id IS NOT NULL \
                   AND state IN ('waiting','pairing','live') ORDER BY id",
            )
            .fetch_all(&state.pool)
            .await
            {
                Ok(rows) => rows,
                Err(err) => {
                    tracing::error!(error = %err, "Observer-Worker konnte Sessions nicht laden");
                    continue;
                }
            };
            for row in rows {
                let id: i64 = match row.try_get("id") {
                    Ok(id) => id,
                    Err(_) => continue,
                };
                let match_id: i64 = match row.try_get("steam_match_id") {
                    Ok(match_id) if match_id > 0 => match_id,
                    _ => continue,
                };
                let mut guard = active.lock().await;
                if !guard.insert(id) {
                    continue;
                }
                drop(guard);
                let state_clone = state.clone();
                let active_clone = Arc::clone(&active);
                tokio::spawn(async move {
                    if let Err(err) =
                        run_observer_session(state_clone.clone(), id, match_id as u64).await
                    {
                        tracing::warn!(observer_session_id = id, error = %err, "Observer-Live-Session beendet");
                        let _ = mark_degraded(&state_clone, id, &err.to_string()).await;
                    }
                    active_clone.lock().await.remove(&id);
                });
            }
        }
    });
}

async fn discover_draft_observers(state: &AppState) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO scrim.observer_sessions( \
             session_key, draft_code, steam_match_id, lobby_party_id, bot_account_id, mode, state \
         ) \
         SELECT 'draft-' || LOWER(ds.code), ds.code, ds.lobby_match_id::BIGINT, \
                CASE WHEN COALESCE(BTRIM(ds.lobby_party_id), '') ~ '^[1-9][0-9]*$' \
                     THEN ds.lobby_party_id::BIGINT ELSE NULL END, \
                $1, 'shadow', 'pairing' \
           FROM turnier.draft_sessions ds \
          WHERE ds.lobby_status='gestartet' \
            AND COALESCE(BTRIM(ds.lobby_match_id), '') ~ '^[1-9][0-9]*$' \
            AND NOT EXISTS( \
                SELECT 1 FROM scrim.observer_sessions os \
                 WHERE os.bot_account_id=$1 AND os.enabled=TRUE AND os.finished_at IS NULL \
            ) \
          ORDER BY ds.id \
          LIMIT 1 \
         ON CONFLICT DO NOTHING",
    )
    .bind(BOT2_ACCOUNT_ID)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn ensure_spectate_lobby_bootstrap(state: &AppState) -> Result<(), String> {
    if !state.config.observer_game_control_enabled {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT os.id, os.lobby_party_id \
           FROM scrim.observer_sessions os \
          WHERE os.enabled=TRUE AND os.finished_at IS NULL \
            AND os.state IN ('waiting','pairing','live') \
            AND os.lobby_party_id IS NOT NULL \
            AND os.last_agent_heartbeat_at > now() - interval '8 seconds' \
            AND os.last_vconsole_ok=TRUE \
            AND COALESCE(os.last_game_connected, FALSE)=FALSE \
            AND NOT EXISTS ( \
                SELECT 1 FROM scrim.observer_commands c \
                 WHERE c.observer_session_id=os.id \
                   AND c.action='spectate_lobby' \
                   AND c.issued_at > now() - interval '30 seconds' \
            ) \
          ORDER BY os.id",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|err| err.to_string())?;
    if rows.is_empty() {
        return Ok(());
    }

    let lease = proxy_bot2_lease(state, None)
        .await
        .map_err(|err| err.detail)?;
    let reserved = lease.get("reserved").and_then(Value::as_bool) == Some(true);
    let steam_connected = lease.get("steam_connected").and_then(Value::as_bool) == Some(true);
    if !reserved || steam_connected {
        return Ok(());
    }

    for row in rows {
        let id: i64 = row.try_get("id").map_err(|err| err.to_string())?;
        let lobby_id: i64 = row
            .try_get("lobby_party_id")
            .map_err(|err| err.to_string())?;
        if lobby_id <= 0 {
            continue;
        }
        enqueue_command(
            state,
            id,
            &CameraAction::SpectateLobby {
                lobby_id: lobby_id as u64,
            },
            "auto_spectate_scrim_lobby",
            None,
        )
        .await
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

async fn finish_completed_draft_observers(state: &AppState) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        "SELECT os.id, os.mode \
           FROM scrim.observer_sessions os \
           JOIN turnier.draft_sessions ds ON ds.code=os.draft_code \
          WHERE os.finished_at IS NULL AND ds.lobby_status='beendet'",
    )
    .fetch_all(&state.pool)
    .await?;
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let mode: String = row.try_get("mode")?;
        if mode == "auto" {
            enqueue_command(state, id, &CameraAction::Directed, "match_finished", None).await?;
        }
        sqlx::query(
            "UPDATE scrim.observer_sessions SET state='finished', enabled=FALSE, finished_at=now(), updated_at=now() WHERE id=$1",
        )
        .bind(id)
        .execute(&state.pool)
        .await?;
    }
    Ok(())
}

async fn run_observer_session(
    state: AppState,
    session_id: i64,
    match_id: u64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE scrim.observer_sessions SET state='pairing', fallback_reason=NULL, updated_at=now() WHERE id=$1",
    )
    .bind(session_id)
    .execute(&state.pool)
    .await?;

    let live = DeadlockLiveClient::with_network(
        &state.config.observer_deadlock_api_base_url,
        &state.config.network,
    )?;
    let broadcast_url = live.resolve_broadcast_url(match_id).await?;
    let (tx, mut rx) = mpsc::channel(512);
    let controller = {
        let live = live.clone();
        let url = broadcast_url.clone();
        let query = state.config.observer_controller_query.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            live.stream_rows(&url, &query, LiveRowKind::Controller, tx)
                .await
        })
    };
    let pawn = {
        let live = live.clone();
        let url = broadcast_url;
        let query = state.config.observer_pawn_query.clone();
        let tx = tx.clone();
        tokio::spawn(async move { live.stream_rows(&url, &query, LiveRowKind::Pawn, tx).await })
    };
    drop(tx);

    let mut accumulator = LiveAccumulator::default();
    let mut director = Director::default();
    let mut last_action = CameraAction::Directed;
    let started = tokio::time::Instant::now();
    let mut last_row = tokio::time::Instant::now();
    let mut evaluate = tokio::time::interval(StdDuration::from_millis(
        state.config.scheduler.observer_evaluate_milliseconds,
    ));

    loop {
        tokio::select! {
            maybe_row = rx.recv() => {
                match maybe_row {
                    Some(row) => {
                        last_row = tokio::time::Instant::now();
                        accumulator.apply(row);
                        sqlx::query("UPDATE scrim.observer_sessions SET state='live', last_live_event_at=now(), updated_at=now() WHERE id=$1 AND finished_at IS NULL")
                            .bind(session_id).execute(&state.pool).await?;
                    }
                    None => break,
                }
            }
            _ = evaluate.tick() => {
                let control = sqlx::query(
                    "SELECT mode, enabled, finished_at, last_agent_heartbeat_at, last_vconsole_ok, last_game_connected \
                     FROM scrim.observer_sessions WHERE id=$1"
                )
                    .bind(session_id).fetch_optional(&state.pool).await?;
                let Some(control) = control else { break };
                let enabled: bool = control.try_get("enabled")?;
                let finished_at: Option<DateTime<Utc>> = control.try_get("finished_at")?;
                if !enabled || finished_at.is_some() { break; }
                let mode: String = control.try_get("mode")?;
                if mode == "auto" {
                    let heartbeat: Option<DateTime<Utc>> = control.try_get("last_agent_heartbeat_at")?;
                    let vconsole_ok: Option<bool> = control.try_get("last_vconsole_ok")?;
                    let game_connected: Option<bool> = control.try_get("last_game_connected")?;
                    let agent_fresh = heartbeat.is_some_and(|at| {
                        Utc::now().signed_duration_since(at) <= Duration::seconds(8)
                    });
                    if !agent_fresh || vconsole_ok != Some(true) || game_connected != Some(true) {
                        if last_action != CameraAction::Directed {
                            enqueue_command(
                                &state,
                                session_id,
                                &CameraAction::Directed,
                                "agent_or_game_not_ready",
                                None,
                            )
                            .await?;
                        }
                        anyhow::bail!("agent_or_game_not_ready");
                    }
                }

                if last_row.elapsed() > StdDuration::from_secs(state.config.scheduler.observer_stale_seconds) && started.elapsed() > StdDuration::from_secs(state.config.scheduler.observer_startup_grace_seconds) {
                    if mode == "auto" && last_action != CameraAction::Directed {
                        enqueue_command(&state, session_id, &CameraAction::Directed, "live_feed_stale", None).await?;
                    }
                    anyhow::bail!("live_feed_stale");
                }

                let now = Utc::now();
                let frame = accumulator.frame(now);
                if frame.players.is_empty() { continue; }
                let decision = director.decide(&frame, now);
                persist_decision(&state, session_id, &decision, now).await?;
                sqlx::query(
                    "UPDATE scrim.observer_sessions SET recommended_account_id=$2, recommended_score=$3, \
                     current_account_id=$4, current_score=$5, updated_at=now() WHERE id=$1",
                )
                .bind(session_id)
                .bind(decision.account_id.map(i64::from))
                .bind(decision.score)
                .bind(director.current_account_id().map(i64::from))
                .bind(decision.current_score)
                .execute(&state.pool)
                .await?;

                if mode == "auto" && decision.action != last_action {
                    enqueue_command(
                        &state,
                        session_id,
                        &decision.action,
                        &decision.reason,
                        Some(decision.score),
                    )
                    .await?;
                    last_action = decision.action.clone();
                }
            }
        }
    }

    controller.abort();
    pawn.abort();
    Ok(())
}

async fn persist_decision(
    state: &AppState,
    session_id: i64,
    decision: &turnier_observer::DirectorDecision,
    observed_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO scrim.observer_decisions( \
             observer_session_id, observed_at, account_id, hero_id, score, current_score, switched, reason, factors, frame_age_ms \
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(session_id)
    .bind(observed_at)
    .bind(decision.account_id.map(i64::from))
    .bind(decision.hero_id.and_then(|v| i32::try_from(v).ok()))
    .bind(decision.score)
    .bind(decision.current_score)
    .bind(decision.switched)
    .bind(&decision.reason)
    .bind(serde_json::to_value(decision.breakdown).unwrap_or_else(|_| json!({})))
    .bind(decision.frame_age_ms)
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn enqueue_command(
    state: &AppState,
    session_id: i64,
    action: &CameraAction,
    reason: &str,
    score: Option<f64>,
) -> Result<i64, sqlx::Error> {
    let ttl_seconds = if matches!(action, CameraAction::SpectateLobby { .. }) {
        30
    } else {
        COMMAND_TTL_SECONDS
    };
    let expires_at = Utc::now() + Duration::seconds(ttl_seconds);
    let (action_name, account_id, lobby_id) = action_parts(action);
    sqlx::query_scalar(
        "INSERT INTO scrim.observer_commands(observer_session_id, action, account_id, lobby_id, reason, score, expires_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id",
    )
    .bind(session_id)
    .bind(action_name)
    .bind(account_id)
    .bind(lobby_id)
    .bind(reason)
    .bind(score)
    .bind(expires_at)
    .fetch_one(&state.pool)
    .await
}

async fn enqueue_directed_if_needed(
    state: &AppState,
    session_id: i64,
    reason: &str,
) -> WebResult<()> {
    let auto_was_active: bool =
        sqlx::query_scalar("SELECT mode='auto' FROM scrim.observer_sessions WHERE id=$1")
            .bind(session_id)
            .fetch_optional(&state.pool)
            .await?
            .unwrap_or(false);
    if auto_was_active {
        enqueue_command(state, session_id, &CameraAction::Directed, reason, None).await?;
    }
    Ok(())
}

async fn mark_degraded(state: &AppState, session_id: i64, reason: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE scrim.observer_sessions SET state='degraded', fallback_reason=$2, updated_at=now() \
         WHERE id=$1 AND finished_at IS NULL",
    )
    .bind(session_id)
    .bind(reason.chars().take(160).collect::<String>())
    .execute(&state.pool)
    .await?;
    Ok(())
}

async fn resolve_steam_match_id(
    state: &AppState,
    input: &CreateSessionRequest,
) -> WebResult<Option<i64>> {
    if let Some(raw) = input.steam_match_id.as_deref() {
        let parsed = raw
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                WebError::bad_request("steam_match_id muss eine positive Dezimalzahl sein")
            })?;
        return Ok(Some(parsed));
    }
    if let Some(id) = input.scrim_match_id {
        return Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT steam_match_id FROM scrim.matches WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .flatten());
    }
    if let Some(code) = input.draft_code.as_deref() {
        let raw: Option<String> =
            sqlx::query_scalar("SELECT lobby_match_id FROM turnier.draft_sessions WHERE code=$1")
                .bind(code.trim().to_uppercase())
                .fetch_optional(&state.pool)
                .await?
                .flatten();
        return Ok(raw
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0));
    }
    Ok(None)
}

async fn resolve_lobby_party_id(
    state: &AppState,
    input: &CreateSessionRequest,
) -> WebResult<Option<i64>> {
    if let Some(code) = input.draft_code.as_deref() {
        let raw: Option<String> =
            sqlx::query_scalar("SELECT lobby_party_id FROM turnier.draft_sessions WHERE code=$1")
                .bind(code.trim().to_uppercase())
                .fetch_optional(&state.pool)
                .await?
                .flatten();
        return Ok(raw
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0));
    }
    if let Some(id) = input.scrim_match_id {
        let raw: Option<String> =
            sqlx::query_scalar("SELECT party_id FROM scrim.matches WHERE id=$1")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
                .flatten();
        return Ok(raw
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0));
    }
    Ok(None)
}

async fn load_session(state: &AppState, id: i64) -> WebResult<sqlx::postgres::PgRow> {
    sqlx::query("SELECT * FROM scrim.observer_sessions WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| WebError::not_found("Observer-Session nicht gefunden"))
}

fn session_from_row(
    row: &sqlx::postgres::PgRow,
    game_control_enabled: bool,
) -> Result<ObserverSessionDto, sqlx::Error> {
    Ok(ObserverSessionDto {
        id: row.try_get("id")?,
        session_key: row.try_get("session_key")?,
        scrim_match_id: row.try_get("scrim_match_id")?,
        draft_code: row.try_get("draft_code")?,
        steam_match_id: row
            .try_get::<Option<i64>, _>("steam_match_id")?
            .map(|v| v.to_string()),
        lobby_party_id: row
            .try_get::<Option<i64>, _>("lobby_party_id")?
            .map(|v| v.to_string()),
        bot_account_id: row.try_get("bot_account_id")?,
        mode: row.try_get("mode")?,
        state: row.try_get("state")?,
        enabled: row.try_get("enabled")?,
        current_account_id: row
            .try_get::<Option<i64>, _>("current_account_id")?
            .map(|v| v.to_string()),
        current_score: row.try_get("current_score")?,
        recommended_account_id: row
            .try_get::<Option<i64>, _>("recommended_account_id")?
            .map(|v| v.to_string()),
        recommended_score: row.try_get("recommended_score")?,
        fallback_reason: row.try_get("fallback_reason")?,
        last_live_event_at: row.try_get("last_live_event_at")?,
        last_agent_heartbeat_at: row.try_get("last_agent_heartbeat_at")?,
        last_agent_version: row.try_get("last_agent_version")?,
        last_vconsole_ok: row.try_get("last_vconsole_ok")?,
        last_game_connected: row.try_get("last_game_connected")?,
        game_control_enabled,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}

fn require_agent(state: &AppState, headers: &HeaderMap) -> WebResult<()> {
    let expected = state.config.observer_agent_token.trim();
    if expected.len() < 24 {
        return Err(WebError::forbidden("Observer-Agent ist nicht konfiguriert"));
    }
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    let lhs = Sha256::digest(provided.as_bytes());
    let rhs = Sha256::digest(expected.as_bytes());
    if lhs.as_slice() != rhs.as_slice() {
        return Err(WebError::unauthorized("Observer-Agent nicht autorisiert"));
    }
    Ok(())
}

fn auto_scope_allowed(
    scrim_match_id: Option<i32>,
    draft_code: Option<&str>,
    lobby_party_id: Option<i64>,
) -> bool {
    lobby_party_id.is_some()
        && (scrim_match_id.is_some() || draft_code.is_some_and(|code| !code.trim().is_empty()))
}

fn mode_str(mode: ObserverMode) -> &'static str {
    match mode {
        ObserverMode::Shadow => "shadow",
        ObserverMode::Assist => "assist",
        ObserverMode::Auto => "auto",
        ObserverMode::Manual => "manual",
    }
}

fn action_parts(action: &CameraAction) -> (&'static str, Option<i64>, Option<i64>) {
    match action {
        CameraAction::SpectateLobby { lobby_id } => {
            ("spectate_lobby", None, i64::try_from(*lobby_id).ok())
        }
        CameraAction::Directed => ("directed", None, None),
        CameraAction::HeroChase { account_id } => {
            ("hero_chase", Some(i64::from(*account_id)), None)
        }
        CameraAction::PlayerView { account_id } => {
            ("player_view", Some(i64::from(*account_id)), None)
        }
    }
}

fn parse_action(
    action: &str,
    account_id: Option<u32>,
    lobby_id: Option<u64>,
) -> WebResult<CameraAction> {
    match action {
        "spectate_lobby" => lobby_id
            .map(|lobby_id| CameraAction::SpectateLobby { lobby_id })
            .ok_or_else(|| WebError::internal("Observer-Spectate-Kommando ohne lobby_id")),
        "directed" => Ok(CameraAction::Directed),
        "hero_chase" => account_id
            .map(|account_id| CameraAction::HeroChase { account_id })
            .ok_or_else(|| WebError::internal("Observer-Kommando ohne account_id")),
        "player_view" => account_id
            .map(|account_id| CameraAction::PlayerView { account_id })
            .ok_or_else(|| WebError::internal("Observer-Kommando ohne account_id")),
        _ => Err(WebError::internal("Unbekannte Observer-Kameraaktion")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_typed_actions_can_be_decoded() {
        assert!(matches!(
            parse_action("directed", None, None).unwrap(),
            CameraAction::Directed
        ));
        assert!(matches!(
            parse_action("spectate_lobby", None, Some(42)).unwrap(),
            CameraAction::SpectateLobby { lobby_id: 42 }
        ));
        assert!(parse_action("spectate_lobby", None, None).is_err());
        assert!(parse_action("player_view", None, None).is_err());
        assert!(parse_action("raw_console", Some(1), None).is_err());
    }

    #[test]
    fn public_match_id_test_can_never_enable_auto_scope() {
        assert!(!auto_scope_allowed(None, None, None));
        assert!(!auto_scope_allowed(None, None, Some(123)));
    }

    #[test]
    fn auto_scope_requires_own_scrim_or_draft_and_lobby() {
        assert!(auto_scope_allowed(Some(7), None, Some(123)));
        assert!(auto_scope_allowed(None, Some("ABC123"), Some(123)));
        assert!(!auto_scope_allowed(Some(7), None, None));
        assert!(!auto_scope_allowed(None, Some("   "), Some(123)));
    }
}
