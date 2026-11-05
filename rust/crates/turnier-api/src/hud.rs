use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::Duration;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use turnier_observer::broadcast::BroadcastClient;
use turnier_observer::hud::HudSnapshot;
use turnier_observer::hud_feed::{new_shared_feed, run_feed, FeedStatus, FeedTiming, SharedFeed};

use crate::error::{WebError, WebResult};
use crate::internal_scrims::require_internal_boundary;
use crate::state::AppState;

const SPECTATE_PATH: &str = "/observer/v1/spectate";
const BROADCAST_HOST_SUFFIX: &str = ".steamcontent.com";
const SPECTATE_SUCCESS: i32 = 1;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/internal/turnier/v1/hud", get(read_hud))
        .route("/internal/turnier/v1/hud/start", post(start_hud))
        .route("/internal/turnier/v1/hud/stop", post(stop_hud))
}

#[derive(Default)]
pub struct HudRegistry {
    active: Mutex<Option<ActiveFeed>>,
}

struct ActiveFeed {
    feed: SharedFeed,
    stop: watch::Sender<bool>,
}

impl HudRegistry {
    fn replace(&self, next: ActiveFeed) {
        if let Ok(mut active) = self.active.lock() {
            if let Some(previous) = active.replace(next) {
                let _ = previous.stop.send(true);
            }
        }
    }

    fn stop(&self) -> bool {
        self.active
            .lock()
            .ok()
            .and_then(|active| active.as_ref().map(|feed| feed.stop.send(true).is_ok()))
            .unwrap_or(false)
    }

    fn view(&self) -> HudView {
        let feed = self
            .active
            .lock()
            .ok()
            .and_then(|active| active.as_ref().map(|feed| feed.feed.clone()));
        let state = feed.and_then(|feed| feed.read().ok().map(|state| state.clone()));
        HudView {
            active: state.is_some(),
            status: state.as_ref().map(|state| state.status.clone()),
            snapshot: state.and_then(|state| state.snapshot),
        }
    }
}

#[derive(Debug, Serialize)]
struct HudView {
    active: bool,
    status: Option<FeedStatus>,
    snapshot: Option<HudSnapshot>,
}

#[derive(Debug, Deserialize)]
struct StartBody {
    match_id: u64,
    broadcast_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct StartResponse {
    match_id: u64,
    source: &'static str,
    lobby_id: Option<u64>,
}

#[derive(Debug, Serialize)]
struct SpectateRequest {
    match_id: u64,
}

#[derive(Debug, Deserialize)]
struct SpectateResponse {
    result: Option<i32>,
    result_name: Option<String>,
    lobby_id: Option<u64>,
    client_broadcast_url: Option<String>,
}

struct BroadcastSource {
    url: String,
    source: &'static str,
    lobby_id: Option<u64>,
}

async fn start_hud(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<StartBody>,
) -> WebResult<Json<StartResponse>> {
    require_internal_boundary(peer, &headers, &state)?;
    if body.match_id == 0 {
        return Err(WebError::bad_request("match_id fehlt"));
    }
    let source = match body.broadcast_url.as_deref().map(str::trim) {
        Some(url) if !url.is_empty() => override_source(url)?,
        _ => spectate_source(&state, body.match_id).await?,
    };
    let client = BroadcastClient::new(&source.url, &state.config.network)
        .map_err(|err| WebError::internal(err.to_string()))?;
    let feed = new_shared_feed(body.match_id);
    let (stop, stop_rx) = watch::channel(false);
    state.hud.replace(ActiveFeed {
        feed: feed.clone(),
        stop,
    });
    tokio::spawn(run_feed(
        client,
        feed,
        stop_rx,
        FeedTiming::from_network(&state.config.network),
    ));
    Ok(Json(StartResponse {
        match_id: body.match_id,
        source: source.source,
        lobby_id: source.lobby_id,
    }))
}

async fn stop_hud(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<HudView>> {
    require_internal_boundary(peer, &headers, &state)?;
    if !state.hud.stop() {
        return Err(WebError::not_found("Kein HUD-Feed aktiv"));
    }
    Ok(Json(state.hud.view()))
}

async fn read_hud(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<HudView>> {
    require_internal_boundary(peer, &headers, &state)?;
    Ok(Json(state.hud.view()))
}

fn override_source(url: &str) -> WebResult<BroadcastSource> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| WebError::bad_request("broadcast_url ist keine gültige Adresse"))?;
    let host_ok = parsed
        .host_str()
        .is_some_and(|host| host.ends_with(BROADCAST_HOST_SUFFIX));
    if !matches!(parsed.scheme(), "http" | "https") || !host_ok {
        return Err(WebError::bad_request(
            "broadcast_url muss auf steamcontent.com zeigen",
        ));
    }
    Ok(BroadcastSource {
        url: url.trim_end_matches('/').to_string(),
        source: "override",
        lobby_id: None,
    })
}

async fn spectate_source(state: &AppState, match_id: u64) -> WebResult<BroadcastSource> {
    let response = request_spectate(state, match_id).await?;
    if response.result != Some(SPECTATE_SUCCESS) {
        return Err(WebError::new(
            StatusCode::BAD_GATEWAY,
            format!(
                "Zuschauen abgelehnt: {}",
                response.result_name.as_deref().unwrap_or("unbekannt")
            ),
        ));
    }
    let url = response
        .client_broadcast_url
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .ok_or_else(|| {
            WebError::new(
                StatusCode::BAD_GATEWAY,
                "Steam lieferte keine Broadcast-Adresse",
            )
        })?;
    Ok(BroadcastSource {
        url: url.trim_end_matches('/').to_string(),
        source: "spectate",
        lobby_id: response.lobby_id,
    })
}

async fn request_spectate(state: &AppState, match_id: u64) -> WebResult<SpectateResponse> {
    let base = state.config.steam_bot_base_url.trim_end_matches('/');
    if base.is_empty() {
        return Err(WebError::internal(
            "Steam-Bot-Adresse ist nicht konfiguriert",
        ));
    }
    let network = &state.config.network;
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(network.hud_spectate_seconds))
        .connect_timeout(Duration::from_secs(network.lobby_connect_seconds))
        .build()
        .map_err(|err| WebError::internal(err.to_string()))?;
    let mut request = http
        .post(format!("{base}{SPECTATE_PATH}"))
        .json(&SpectateRequest { match_id });
    let token = state.config.steam_bot_internal_token.trim();
    if !token.is_empty() {
        request = request.header("X-Internal-Token", token);
    }
    let response = request.send().await.map_err(|err| {
        WebError::new(
            StatusCode::BAD_GATEWAY,
            format!("Steam-Bot nicht erreichbar: {err}"),
        )
    })?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if status == StatusCode::CONFLICT {
        return Err(WebError::conflict(
            "Der Steam-Bot ist für die Kamera reserviert",
        ));
    }
    if !status.is_success() {
        return Err(WebError::new(
            StatusCode::BAD_GATEWAY,
            format!("Steam-Bot meldete HTTP {status}: {text}"),
        ));
    }
    serde_json::from_str(&text).map_err(|err| {
        WebError::new(
            StatusCode::BAD_GATEWAY,
            format!("Steam-Bot-Antwort unverständlich: {err}"),
        )
    })
}
