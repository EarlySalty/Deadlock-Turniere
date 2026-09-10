//! Draft-Router — Pick/Ban-Endpunkte (portiert `draft/routes.py`).
//!
//! - `GET  /api/draft/heroes` — Live-Heldenliste (öffentlich).
//! - `POST /api/draft/lobbies` — freie Lobby anlegen (öffentlich).
//! - `GET  /api/draft/lobbies/{code}` — freie Lobby ansehen (öffentlich).
//! - `POST /api/draft/lobbies/{code}/action` — Lobby-Aktion (öffentlich).
//! - `POST /api/draft/matches/{match_id}/start` — Draft starten (Admin).
//! - `GET  /api/draft/sessions/{session_id}` — Zustand lesen (Admin).
//! - `POST /api/draft/sessions/{session_id}/action` — Aktion ausführen (Admin).
//!
//! `taken_by` wird wie im Original aus dem Request-Body übernommen (nicht aus der
//! Session — Parität, siehe `docs/known-issues.md` KI-DR01).

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Path, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{WebError, WebResult};
use crate::extract::AdminUser;
use crate::state::AppState;

/// Router der Draft-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/draft/heroes", get(list_heroes))
        .route("/api/draft/lobbies", post(create_lobby))
        .route("/api/draft/lobbies/{code}", get(get_lobby))
        .route(
            "/api/draft/lobbies/{code}/action",
            post(submit_lobby_action),
        )
        .route("/api/draft/lobbies/{code}/claim", post(claim_lobby))
        .route("/api/draft/lobbies/{code}/ready", post(ready_lobby))
        .route("/api/draft/lobbies/{code}/leave", post(leave_lobby))
        .route("/api/draft/lobbies/{code}/rematch", post(rematch_lobby))
        .route(
            "/api/draft/lobbies/{code}/lobby/retry",
            post(retry_lobby_request_route),
        )
        .route(
            "/api/draft/matches/{match_id}/start",
            post(start_match_draft),
        )
        .route("/api/draft/sessions/{session_id}", get(get_session))
        .route(
            "/api/draft/sessions/{session_id}/action",
            post(submit_action),
        )
}

/// `GET /api/draft/heroes` — Live-Heldenliste mit statischem Fallback.
async fn list_heroes() -> Json<Value> {
    let heroes = turnier_draft::load_heroes()
        .await
        .into_iter()
        .map(|hero| {
            json!({
                "id": hero.id,
                "name": hero.name,
                "image_url": hero.image_url,
                "card_image_url": hero.card_image_url,
            })
        })
        .collect::<Vec<_>>();
    Json(json!({ "heroes": heroes }))
}

/// Request-Body zum Anlegen einer freien Draft-Lobby.
#[derive(Debug, Deserialize)]
struct CreateLobbyRequest {
    team1_name: Option<String>,
    team2_name: Option<String>,
    preset: Option<String>,
    round_seconds: Option<i32>,
    reserve_seconds: Option<i32>,
    bans_per_team: Option<i32>,
}

/// Request-Body einer freien Lobby-Aktion.
#[derive(Debug, Deserialize)]
struct LobbyActionRequest {
    token: String,
    hero_name: String,
}

/// Request-Body des Captain-Claims.
#[derive(Debug, Deserialize)]
struct ClaimRequest {
    team: i64,
}

/// Request-Body aller Token-Routen.
#[derive(Debug, Deserialize)]
struct TokenRequest {
    token: String,
}

/// `POST /api/draft/lobbies` — anonyme Draft-Lobby anlegen.
///
/// Mit `bans_per_team` entsteht ein Warteraum-Raum (Antwort nur `code`); ohne
/// bleibt es beim bisherigen Verhalten mit Preset und beiden Slot-Tokens.
async fn create_lobby(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<CreateLobbyRequest>,
) -> WebResult<Json<Value>> {
    enforce_lobby_rate_limit(&state, client_ip(&headers, address.ip()))?;
    match validate_lobby(body)? {
        LobbyCreateKind::Legacy(options) => {
            let credentials = turnier_draft::create_lobby(&state.pool, options).await?;
            Ok(Json(json!({
                "code": credentials.code,
                "team1_token": credentials.team1_token,
                "team2_token": credentials.team2_token,
            })))
        }
        LobbyCreateKind::Room(options) => {
            let code = turnier_draft::create_room(&state.pool, options).await?;
            Ok(Json(json!({ "code": code })))
        }
    }
}

enum LobbyCreateKind {
    Legacy(turnier_draft::CreateLobbyOptions),
    Room(turnier_draft::CreateRoomOptions),
}

fn validate_lobby(body: CreateLobbyRequest) -> WebResult<LobbyCreateKind> {
    if let Some(bans_per_team) = body.bans_per_team {
        return Ok(LobbyCreateKind::Room(validate_room(body, bans_per_team)?));
    }
    let team1_name = validate_team_name(
        body.team1_name
            .ok_or_else(|| WebError::bad_request(MISSING_TEAM_NAME))?,
    )?;
    let team2_name = validate_team_name(
        body.team2_name
            .ok_or_else(|| WebError::bad_request(MISSING_TEAM_NAME))?,
    )?;
    let preset = turnier_draft::preset(
        body.preset
            .as_deref()
            .ok_or_else(|| WebError::bad_request(UNKNOWN_PRESET))?,
    )
    .ok_or_else(|| WebError::bad_request(UNKNOWN_PRESET))?;
    let round_seconds = body.round_seconds.ok_or_else(|| {
        WebError::bad_request("Die Rundendauer muss zwischen 10 und 300 Sekunden liegen.")
    })?;
    if !(10..=300).contains(&round_seconds) {
        return Err(WebError::bad_request(
            "Die Rundendauer muss zwischen 10 und 300 Sekunden liegen.",
        ));
    }
    let reserve_seconds = body.reserve_seconds.ok_or_else(|| {
        WebError::bad_request("Die Reservezeit muss zwischen 0 und 600 Sekunden liegen.")
    })?;
    if !(0..=600).contains(&reserve_seconds) {
        return Err(WebError::bad_request(
            "Die Reservezeit muss zwischen 0 und 600 Sekunden liegen.",
        ));
    }
    Ok(LobbyCreateKind::Legacy(turnier_draft::CreateLobbyOptions {
        team1_name,
        team2_name,
        sequence: preset.to_vec(),
        round_seconds: Some(round_seconds),
        reserve_seconds: Some(reserve_seconds),
    }))
}

const MISSING_TEAM_NAME: &str = "Teamnamen müssen 1 bis 40 Zeichen lang sein.";
const UNKNOWN_PRESET: &str = "Dieses Draft-Preset wird nicht unterstützt.";

fn validate_room(
    body: CreateLobbyRequest,
    bans_per_team: i32,
) -> WebResult<turnier_draft::CreateRoomOptions> {
    if !(0..=6).contains(&bans_per_team) {
        return Err(WebError::bad_request(
            "Die Anzahl der Bans je Team muss zwischen 0 und 6 liegen.",
        ));
    }
    let round_seconds = body.round_seconds.unwrap_or(30);
    if round_seconds != 0 && !(10..=300).contains(&round_seconds) {
        return Err(WebError::bad_request(
            "Die Rundendauer muss 0 (aus) oder zwischen 10 und 300 Sekunden liegen.",
        ));
    }
    Ok(turnier_draft::CreateRoomOptions {
        team1_name: match body.team1_name {
            Some(name) => validate_team_name(name)?,
            None => "Team 1".to_string(),
        },
        team2_name: match body.team2_name {
            Some(name) => validate_team_name(name)?,
            None => "Team 2".to_string(),
        },
        sequence: turnier_draft::sequence_for_bans(bans_per_team),
        bans_per_team,
        round_seconds: Some(round_seconds),
    })
}

/// `POST /api/draft/lobbies/{code}/action` — Captain-Aktion ausführen.
async fn submit_lobby_action(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Json(body): Json<LobbyActionRequest>,
) -> WebResult<Json<turnier_draft::DraftState>> {
    turnier_draft::take_lobby_action(&state.pool, &code, &body.token, &body.hero_name).await?;
    let draft_state = turnier_draft::get_state_by_code(&state.pool, &code).await?;
    Ok(Json(draft_state))
}

/// `GET /api/draft/lobbies/{code}` — öffentlichen Vollzustand lesen.
///
/// Liefert die bestehenden Felder plus Warteraum-Vertrag: `phase`,
/// `team1`/`team2`, `you` (per `X-Draft-Token`), `spectators` (per
/// `X-Draft-Viewer`), indizierte `sequence`, `lobby` und `rematch_code`.
async fn get_lobby(
    State(state): State<AppState>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> WebResult<impl axum::response::IntoResponse> {
    let token = header_value(&headers, "x-draft-token");
    let viewer = header_value(&headers, "x-draft-viewer");
    let body = lobby_state(&state, &code, token.as_deref(), viewer.as_deref()).await?;
    Ok(([(CACHE_CONTROL, "no-store")], Json(body)))
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

async fn lobby_state(
    state: &AppState,
    code: &str,
    token: Option<&str>,
    viewer: Option<&str>,
) -> WebResult<Value> {
    let draft_state = turnier_draft::get_state_by_code(&state.pool, code).await?;
    let mut value = serde_json::to_value(&draft_state).unwrap_or_else(|_| json!({}));
    let Value::Object(map) = &mut value else {
        return Err(WebError::internal(
            "Draft-Zustand konnte nicht gelesen werden",
        ));
    };
    let phase = match draft_state.session.status.as_str() {
        "warteraum" => "warteraum",
        "completed" => "abgeschlossen",
        _ => "laeuft",
    };
    map.insert("phase".to_string(), json!(phase));
    map.insert(
        "team1".to_string(),
        json!({
            "name": draft_state.session.team1_name,
            "claimed": draft_state.session.team1_claimed,
            "ready": draft_state.session.team1_ready,
        }),
    );
    map.insert(
        "team2".to_string(),
        json!({
            "name": draft_state.session.team2_name,
            "claimed": draft_state.session.team2_claimed,
            "ready": draft_state.session.team2_ready,
        }),
    );
    let you_team = match token {
        Some(token) => token_to_team(&state.pool, code, token).await?,
        None => None,
    };
    map.insert("you".to_string(), json!({ "team": you_team }));
    map.insert(
        "spectators".to_string(),
        json!(count_viewers(state, code, viewer)),
    );
    map.insert(
        "sequence".to_string(),
        json!(draft_state
            .session
            .sequence
            .iter()
            .enumerate()
            .map(|(index, step)| json!({
                "index": index,
                "team": step.team_slot.as_i64(),
                "action": step.action_type.as_str(),
            }))
            .collect::<Vec<_>>()),
    );
    map.insert(
        "lobby".to_string(),
        json!({
            "status": draft_state.session.lobby_status,
            "join_code": draft_state.session.lobby_join_code,
            "error": draft_state.session.lobby_error,
            "match_id": draft_state.session.lobby_match_id,
            "result": draft_state.session.lobby_result,
        }),
    );
    let rematch_code: Option<String> = sqlx::query_scalar(
        "SELECT code FROM turnier.draft_sessions \
         WHERE rematch_of_code = $1 AND code IS NOT NULL \
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(code)
    .fetch_optional(&state.pool)
    .await?;
    map.insert("rematch_code".to_string(), json!(rematch_code));
    Ok(value)
}

async fn token_to_team(pool: &turnier_db::Pool, code: &str, token: &str) -> WebResult<Option<i64>> {
    let team: Option<i64> = sqlx::query_scalar(
        "SELECT CASE \
             WHEN $2::text = team1_token THEN 1::BIGINT \
             WHEN $2::text = team2_token THEN 2::BIGINT \
         END \
         FROM turnier.draft_sessions WHERE code = $1",
    )
    .bind(code)
    .bind(token)
    .fetch_optional(pool)
    .await?
    .flatten();
    Ok(team)
}

/// Zählt die Zuschauer des Raums (20 Sekunden Fenster) und merkt den
/// mitgelieferten Viewer als anwesend.
fn count_viewers(state: &AppState, code: &str, viewer: Option<&str>) -> i64 {
    let mut viewers = state
        .draft_viewers
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = Instant::now();
    let entry = viewers.entry(code.to_string()).or_default();
    entry.retain(|_, seen| now.duration_since(*seen) < Duration::from_secs(20));
    if let Some(viewer) = viewer {
        entry.insert(viewer.to_string(), now);
    }
    entry.len() as i64
}

/// `POST /api/draft/lobbies/{code}/claim` — Captain-Platz übernehmen.
async fn claim_lobby(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Json(body): Json<ClaimRequest>,
) -> WebResult<Json<Value>> {
    let outcome = turnier_draft::claim_room(&state.pool, &code, body.team).await?;
    Ok(Json(
        json!({ "team": outcome.team, "token": outcome.token }),
    ))
}

/// `POST /api/draft/lobbies/{code}/ready` — Bereit melden.
async fn ready_lobby(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Json(body): Json<TokenRequest>,
) -> WebResult<Json<Value>> {
    let started = turnier_draft::room_ready(&state.pool, &code, &body.token).await?;
    let mut value = lobby_state(&state, &code, Some(&body.token), None).await?;
    if let Value::Object(map) = &mut value {
        map.insert("started".to_string(), json!(started.started));
    }
    Ok(Json(value))
}

/// `POST /api/draft/lobbies/{code}/leave` — Platz freigeben.
async fn leave_lobby(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Json(body): Json<TokenRequest>,
) -> WebResult<Json<Value>> {
    turnier_draft::leave_room(&state.pool, &code, &body.token).await?;
    let value = lobby_state(&state, &code, Some(&body.token), None).await?;
    Ok(Json(value))
}

/// `POST /api/draft/lobbies/{code}/rematch` — neuen Raum mit getauschten Seiten.
async fn rematch_lobby(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Json(body): Json<TokenRequest>,
) -> WebResult<Json<Value>> {
    let new_code = turnier_draft::rematch_room(&state.pool, &code, &body.token).await?;
    Ok(Json(json!({ "code": new_code })))
}

/// `POST /api/draft/lobbies/{code}/lobby/retry` — fehlgeschlagene Lobby-Anfrage
/// erneut anstoßen.
async fn retry_lobby_request_route(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Json(body): Json<TokenRequest>,
) -> WebResult<Json<Value>> {
    turnier_draft::retry_lobby_request(&state.pool, &code, &body.token).await?;
    let value = lobby_state(&state, &code, Some(&body.token), None).await?;
    Ok(Json(value))
}

fn validate_team_name(name: String) -> WebResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(WebError::bad_request(
            "Teamnamen müssen 1 bis 40 Zeichen lang sein.",
        ));
    }
    Ok(name.to_string())
}

fn client_ip(headers: &HeaderMap, peer_ip: std::net::IpAddr) -> std::net::IpAddr {
    if !peer_ip.is_loopback() {
        return peer_ip;
    }

    let mut forwarded_values = headers.get_all("x-forwarded-for").iter();
    let Some(value) = forwarded_values.next() else {
        return peer_ip;
    };
    if forwarded_values.next().is_some() {
        return peer_ip;
    }
    let Ok(value) = value.to_str() else {
        return peer_ip;
    };
    let mut addresses = value.split(',');
    let Some(address) = addresses.next() else {
        return peer_ip;
    };
    if addresses.next().is_some() {
        return peer_ip;
    }
    address.trim().parse().unwrap_or(peer_ip)
}

fn enforce_lobby_rate_limit(state: &AppState, ip: std::net::IpAddr) -> WebResult<()> {
    let now = Instant::now();
    let mut creations = state
        .draft_lobby_creations
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // ponytail: Prozesslokal reicht bei einem Prozess; bei mehreren Instanzen neu denken.
    let per_ip = creations.entry(ip).or_default();
    per_ip.retain(|created| now.duration_since(*created) < Duration::from_secs(60 * 60));
    if per_ip.len() >= 10 {
        return Err(WebError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Du kannst höchstens 10 Draft-Lobbys pro Stunde erstellen.",
        ));
    }
    per_ip.push(now);
    Ok(())
}

/// `POST /api/draft/matches/{match_id}/start` — Draft für ein Bracket-Match starten.
async fn start_match_draft(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(match_id): Path<i64>,
) -> WebResult<Json<Value>> {
    // Existenz des Bracket-Matches vorab prüfen (wie im Original; FK ist Backstop).
    let exists: Option<(i64,)> =
        sqlx::query_as(r#"SELECT id FROM turnier."bracket_matches" WHERE id = $1"#)
            .bind(match_id)
            .fetch_optional(&state.pool)
            .await?;
    if exists.is_none() {
        return Err(WebError::not_found(format!(
            "Bracket-Match {match_id} nicht gefunden"
        )));
    }

    let session_id = turnier_draft::start_draft(&state.pool, match_id, &user.discord_id).await?;
    let draft_state = turnier_draft::get_draft_state(&state.pool, session_id).await?;
    Ok(Json(
        serde_json::to_value(draft_state).unwrap_or_else(|_| json!({})),
    ))
}

/// `GET /api/draft/sessions/{session_id}` — den Draft-Zustand lesen.
async fn get_session(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(session_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let draft_state = turnier_draft::get_draft_state(&state.pool, session_id).await?;
    Ok(Json(
        serde_json::to_value(draft_state).unwrap_or_else(|_| json!({})),
    ))
}

/// Request-Body von `submit_action`.
#[derive(Debug, Deserialize)]
struct DraftActionRequest {
    hero_name: String,
    taken_by: String,
    #[serde(default)]
    force: bool,
}

/// `POST /api/draft/sessions/{session_id}/action` — eine Ban/Pick-Aktion ausführen.
///
/// Antwort ist die flache Verschmelzung aus Draft-Zustand und Aktions-Ergebnis
/// (`{**state, **result}` im Original).
async fn submit_action(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(session_id): Path<i64>,
    Json(body): Json<DraftActionRequest>,
) -> WebResult<Json<Value>> {
    let outcome = turnier_draft::take_action(
        &state.pool,
        session_id,
        &body.hero_name,
        &body.taken_by,
        body.force,
    )
    .await?;
    let draft_state = turnier_draft::get_draft_state(&state.pool, session_id).await?;

    let mut merged = serde_json::to_value(draft_state).unwrap_or_else(|_| json!({}));
    if let (Value::Object(target), Ok(Value::Object(extra))) =
        (&mut merged, serde_json::to_value(outcome))
    {
        target.extend(extra);
    }
    Ok(Json(merged))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn forwarded_ketten_vom_proxy_werden_nicht_vertraut() {
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.7, 203.0.113.9"),
        );

        assert_eq!(client_ip(&headers, peer), peer);
    }
}
