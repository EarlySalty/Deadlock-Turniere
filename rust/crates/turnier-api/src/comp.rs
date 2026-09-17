//! Anonymous Comp-Finder API. Player capabilities never occur in URLs or JSON.
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State};
use axum::http::{header::CACHE_CONTROL, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use turnier_draft::comp::{self, CompError, Preference, Room};

use crate::error::{WebError, WebResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/comp/lobbies", post(create))
        .route("/api/comp/lobbies/{code}", get(read))
        .route("/api/comp/lobbies/{code}/join", post(join))
        .route("/api/comp/lobbies/{code}/preferences", post(save))
        .route("/api/comp/lobbies/{code}/leave", post(leave))
        .route("/api/comp/lobbies/{code}/remove", post(remove))
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(axum::middleware::from_fn(no_store))
}

async fn no_store(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

impl From<CompError> for WebError {
    fn from(error: CompError) -> Self {
        let detail = error.to_string();
        match error {
            CompError::NotFound => Self::not_found(detail),
            CompError::Unauthorized => Self::unauthorized(detail),
            CompError::Forbidden => Self::forbidden(detail),
            CompError::Full | CompError::Stale => Self::conflict(detail),
            CompError::Invalid(_) => Self::bad_request(detail),
            CompError::Database(error) => error.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameRequest {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreferencesRequest {
    revision: i64,
    preferences: Vec<Preference>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveRequest {
    member_id: String,
}

#[derive(Serialize)]
struct RoomResponse {
    #[serde(flatten)]
    room: Room,
    results: comp::solver::Results,
    unavailable_heroes: Vec<String>,
}

async fn reply(room: Room) -> WebResult<Json<RoomResponse>> {
    let allowed: HashSet<String> = turnier_draft::load_heroes()
        .await
        .into_iter()
        .map(|h| h.name)
        .collect();
    let mut unavailable: Vec<_> = room
        .members
        .iter()
        .flat_map(|m| &m.preferences)
        .filter(|p| !allowed.contains(&p.hero_name))
        .map(|p| p.hero_name.clone())
        .collect();
    unavailable.sort();
    unavailable.dedup();
    let preferences: Vec<Vec<Preference>> = room
        .members
        .iter()
        .map(|m| {
            m.preferences
                .iter()
                .filter(|p| allowed.contains(&p.hero_name))
                .cloned()
                .collect()
        })
        .collect();
    let results =
        tokio::task::spawn_blocking(move || comp::solver::solve(&preferences, comp::RESULT_LIMIT))
            .await
            .map_err(|_| WebError::internal("Aufstellungen konnten nicht berechnet werden."))?;
    Ok(Json(RoomResponse {
        room,
        results,
        unavailable_heroes: unavailable,
    }))
}

fn optional_token(headers: &HeaderMap) -> WebResult<Option<&str>> {
    match headers.get("x-comp-token") {
        None => Ok(None),
        Some(value) => {
            let value = value.to_str().map_err(|_| CompError::Unauthorized)?;
            comp::token_hash(value)?;
            Ok(Some(value))
        }
    }
}

fn token(headers: &HeaderMap) -> WebResult<&str> {
    optional_token(headers)?.ok_or_else(|| CompError::Unauthorized.into())
}

async fn create(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<NameRequest>,
) -> WebResult<Json<RoomResponse>> {
    rate_limit(&state, &headers, peer, Access::Create)?;
    reply(comp::create(&state.pool, &body.name, token(&headers)?).await?).await
}

async fn read(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<RoomResponse>> {
    rate_limit(&state, &headers, peer, Access::Read)?;
    reply(comp::get(&state.pool, &code, optional_token(&headers)?).await?).await
}

async fn join(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(body): Json<NameRequest>,
) -> WebResult<Json<RoomResponse>> {
    rate_limit(&state, &headers, peer, Access::Write)?;
    reply(comp::join(&state.pool, &code, &body.name, token(&headers)?).await?).await
}

async fn save(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(body): Json<PreferencesRequest>,
) -> WebResult<Json<RoomResponse>> {
    rate_limit(&state, &headers, peer, Access::Write)?;
    let token = token(&headers)?;
    let allowed = turnier_draft::load_heroes()
        .await
        .into_iter()
        .map(|h| h.name)
        .collect();
    reply(
        comp::save_preferences(
            &state.pool,
            &code,
            token,
            body.revision,
            &body.preferences,
            &allowed,
        )
        .await?,
    )
    .await
}

async fn leave(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(code): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<Value>> {
    rate_limit(&state, &headers, peer, Access::Write)?;
    comp::leave(&state.pool, &code, token(&headers)?).await?;
    Ok(Json(json!({"left": true})))
}

async fn remove(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(code): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RemoveRequest>,
) -> WebResult<Json<RoomResponse>> {
    rate_limit(&state, &headers, peer, Access::Write)?;
    reply(comp::remove_member(&state.pool, &code, token(&headers)?, &body.member_id).await?).await
}

#[derive(Clone, Copy)]
enum Access {
    Read,
    Write,
    Create,
}

struct Budget {
    minute: Instant,
    reads: u32,
    writes: u32,
    creations: Vec<Instant>,
    last_seen: Instant,
}

/// Bounded, process-local limiter, matching the single-process deployment.
/// Shared-NAT six-player teams can poll every two seconds without being blocked.
#[derive(Default)]
pub struct RateLimiter {
    clients: HashMap<IpAddr, Budget>,
}

impl RateLimiter {
    fn check(&mut self, ip: IpAddr, access: Access, now: Instant) -> WebResult<()> {
        self.clients
            .retain(|_, budget| now.duration_since(budget.last_seen) < Duration::from_secs(3600));
        if self.clients.len() >= 10_000 && !self.clients.contains_key(&ip) {
            return Err(WebError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "Der Comp-Finder ist gerade ausgelastet. Bitte später versuchen.",
            ));
        }
        let budget = self.clients.entry(ip).or_insert(Budget {
            minute: now,
            reads: 0,
            writes: 0,
            creations: Vec::new(),
            last_seen: now,
        });
        budget.last_seen = now;
        if now.duration_since(budget.minute) >= Duration::from_secs(60) {
            budget.minute = now;
            budget.reads = 0;
            budget.writes = 0;
        }
        budget
            .creations
            .retain(|t| now.duration_since(*t) < Duration::from_secs(3600));
        let (count, limit) = match access {
            Access::Read => (&mut budget.reads, 600),
            _ => (&mut budget.writes, 120),
        };
        if *count >= limit {
            return Err(WebError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "Zu viele Anfragen. Bitte eine Minute warten.",
            ));
        }
        *count += 1;
        if matches!(access, Access::Create) {
            if budget.creations.len() >= 10 {
                return Err(WebError::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Du kannst höchstens 10 Comp-Lobbys pro Stunde erstellen.",
                ));
            }
            budget.creations.push(now);
        }
        Ok(())
    }
}

fn rate_limit(
    state: &AppState,
    headers: &HeaderMap,
    peer: SocketAddr,
    access: Access,
) -> WebResult<()> {
    let ip = crate::draft::client_ip(headers, peer.ip());
    state
        .comp_rate_limit
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .check(ip, access, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_read_and_write_budgets_are_bounded_and_expire() {
        let mut limiter = RateLimiter::default();
        let ip = "127.0.0.1".parse().unwrap();
        let now = Instant::now();
        for _ in 0..10 {
            limiter.check(ip, Access::Create, now).unwrap();
        }
        assert_eq!(
            limiter.check(ip, Access::Create, now).unwrap_err().status,
            StatusCode::TOO_MANY_REQUESTS
        );
        for _ in 0..600 {
            limiter.check(ip, Access::Read, now).unwrap();
        }
        assert!(limiter.check(ip, Access::Read, now).is_err());
        assert!(limiter
            .check(ip, Access::Read, now + Duration::from_secs(60))
            .is_ok());
        assert!(limiter
            .check(ip, Access::Create, now + Duration::from_secs(3601))
            .is_ok());
    }
}
