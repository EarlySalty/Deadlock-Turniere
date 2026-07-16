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
            })
        })
        .collect::<Vec<_>>();
    Json(json!({ "heroes": heroes }))
}

/// Request-Body zum Anlegen einer freien Draft-Lobby.
#[derive(Debug, Deserialize)]
struct CreateLobbyRequest {
    team1_name: String,
    team2_name: String,
    preset: String,
    round_seconds: i32,
    reserve_seconds: i32,
}

/// Request-Body einer freien Lobby-Aktion.
#[derive(Debug, Deserialize)]
struct LobbyActionRequest {
    token: String,
    hero_name: String,
}

/// `POST /api/draft/lobbies` — anonyme Draft-Lobby anlegen.
async fn create_lobby(
    State(state): State<AppState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<CreateLobbyRequest>,
) -> WebResult<Json<Value>> {
    let options = validate_lobby(body)?;
    enforce_lobby_rate_limit(&state, client_ip(&headers, address.ip()))?;
    let credentials = turnier_draft::create_lobby(&state.pool, options).await?;
    Ok(Json(json!({
        "code": credentials.code,
        "team1_token": credentials.team1_token,
        "team2_token": credentials.team2_token,
    })))
}

/// `GET /api/draft/lobbies/{code}` — öffentlichen Vollzustand lesen.
async fn get_lobby(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> WebResult<impl axum::response::IntoResponse> {
    let draft_state = turnier_draft::get_state_by_code(&state.pool, &code).await?;
    Ok(([(CACHE_CONTROL, "no-store")], Json(draft_state)))
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

fn validate_lobby(body: CreateLobbyRequest) -> WebResult<turnier_draft::CreateLobbyOptions> {
    let team1_name = validate_team_name(body.team1_name)?;
    let team2_name = validate_team_name(body.team2_name)?;
    if !(10..=300).contains(&body.round_seconds) {
        return Err(WebError::bad_request(
            "PLATZHALTER: Rundendauer liegt ausserhalb des erlaubten Bereichs",
        ));
    }
    if !(0..=600).contains(&body.reserve_seconds) {
        return Err(WebError::bad_request(
            "PLATZHALTER: Reservezeit liegt ausserhalb des erlaubten Bereichs",
        ));
    }
    let sequence = turnier_draft::preset(&body.preset)
        .ok_or_else(|| WebError::bad_request("PLATZHALTER: Unbekanntes Draft-Preset"))?;
    Ok(turnier_draft::CreateLobbyOptions {
        team1_name,
        team2_name,
        sequence: sequence.to_vec(),
        round_seconds: Some(body.round_seconds),
        reserve_seconds: Some(body.reserve_seconds),
    })
}

fn validate_team_name(name: String) -> WebResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(WebError::bad_request(
            "PLATZHALTER: Teamname ist leer oder laenger als 40 Zeichen",
        ));
    }
    Ok(name.to_string())
}

fn client_ip(headers: &HeaderMap, peer_ip: std::net::IpAddr) -> std::net::IpAddr {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(peer_ip)
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
            "PLATZHALTER: Zu viele Draft-Lobbys in einer Stunde",
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
