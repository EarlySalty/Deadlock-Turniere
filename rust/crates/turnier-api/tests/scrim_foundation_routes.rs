use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(feature = "testing")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(feature = "testing")]
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
#[cfg(feature = "testing")]
use axum::extract::State;
use axum::http::header::{CONTENT_TYPE, HOST};
use axum::http::{Method, Request, StatusCode};
#[cfg(feature = "testing")]
use axum::routing::post;
#[cfg(feature = "testing")]
use axum::Json;
use axum::Router;
use serde_json::{json, Value};
#[cfg(feature = "testing")]
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
#[cfg(feature = "testing")]
use sqlx::Row;
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_discord::{BrokerClient, DiscordNotifier};
use turnier_match::MatchManager;
#[cfg(feature = "testing")]
use turnier_match::SteamBridge;
use turnier_steam::SteamRankResolver;

fn app() -> Router {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .expect("lazy pool");
    app_with_pool(pool)
}

fn app_with_pool(pool: PgPool) -> Router {
    app_with_pool_and_broker(pool, None)
}

fn app_with_pool_and_broker(pool: PgPool, broker_base_url: Option<&str>) -> Router {
    app_with_pool_broker_and_signup_role(pool, broker_base_url, None)
}

fn app_with_pool_broker_and_signup_role(
    pool: PgPool,
    broker_base_url: Option<&str>,
    signup_role_id: Option<i64>,
) -> Router {
    build_router(state_with_pool_broker_and_signup_role(
        pool,
        broker_base_url,
        signup_role_id,
    ))
}

fn state_with_pool_broker_and_signup_role(
    pool: PgPool,
    broker_base_url: Option<&str>,
    signup_role_id: Option<i64>,
) -> AppState {
    let config = Config {
        turnier_internal_api_token: "internal-token".to_string(),
        discord_bot_token: String::new(),
        discord_master_broker_base_url: broker_base_url.unwrap_or_default().to_string(),
        discord_master_broker_token: broker_base_url
            .map(|_| "broker-token".to_string())
            .unwrap_or_default(),
        scrim_signup_role_id: signup_role_id,
        scrim_reserve_role_id: None,
        steam_bridge_db_path: String::new(),
        backend_allowed_hosts: "localhost".to_string(),
        discord_mod_role_ids: "99".to_string(),
        ..Config::default()
    };
    let config = Arc::new(config);

    let role_sets = turnier_auth::RoleSets::from_config(&config);
    let oauth = turnier_auth::OAuthClient::new(&config);
    let broker = BrokerClient::from_config(&config);
    let match_manager = Arc::new(MatchManager::new(pool.clone(), None, None, &config));
    let notifier = Arc::new(DiscordNotifier::new(broker, pool.clone(), &config));
    let rank_resolver = Arc::new(SteamRankResolver::from_pool(pool.clone(), None, None));
    let heroes = Arc::new(turnier_draft::HeroesProvider::new(
        turnier_draft::ReqwestHeroFetcher::new(
            config.assets.heroes_url.clone(),
            config.network.heroes_request_seconds,
        ),
        std::time::Duration::from_secs(config.assets.heroes_cache_seconds),
        std::time::Duration::from_secs(config.assets.heroes_fallback_cache_seconds),
    ));
    AppState {
        pool,
        config,
        heroes,
        role_sets,
        oauth,
        match_manager,
        rank_resolver,
        notifier,
        draft_lobby_creations: Arc::new(Mutex::new(HashMap::new())),
        comp_rate_limit: Arc::new(Mutex::new(turnier_api::comp::RateLimiter::default())),
        draft_viewers: Arc::new(Mutex::new(HashMap::new())),
        scrim_lobby: Arc::new(turnier_api::scrim_lobby::ScrimLobbyClient::new(
            "http://127.0.0.1:9",
            "",
        )),
    }
}

#[cfg(feature = "testing")]
async fn record_and_fail_broker(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    Json(payload): Json<Value>,
) -> StatusCode {
    let idempotency_key = payload["idempotency_key"]
        .as_str()
        .expect("broker idempotency key");
    if idempotency_key.chars().count() > 128 {
        return StatusCode::BAD_REQUEST;
    }
    requests.lock().expect("broker requests").push(payload);
    StatusCode::SERVICE_UNAVAILABLE
}

#[cfg(feature = "testing")]
async fn record_and_accept_broker(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    requests.lock().expect("broker requests").push(payload);
    Json(json!({}))
}

#[cfg(feature = "testing")]
async fn record_and_accept_message(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    let channel_id = payload["channel_id"].clone();
    let mut requests = requests.lock().expect("broker requests");
    let message_id =
        912_345_678_901_234_568_u64 + u64::try_from(requests.len()).expect("broker request count");
    requests.push(payload);
    Json(json!({"result":{
        "channel_id": channel_id,
        "message_id":message_id.to_string()
    }}))
}

#[cfg(feature = "testing")]
async fn record_and_accept_message_under_selection_lock(
    State((requests, pool, match_id)): State<(Arc<Mutex<Vec<Value>>>, PgPool, i32)>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    let mut tx = pool.begin().await.expect("selection lock check");
    let lock_available: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(20260727, $1)")
        .bind(match_id)
        .fetch_one(&mut *tx)
        .await
        .expect("selection lock state");
    assert!(
        !lock_available,
        "result selection lock was not held at the Discord boundary"
    );
    tx.rollback().await.expect("selection lock check rollback");
    record_and_accept_message(State(requests), Json(payload)).await
}

#[cfg(feature = "testing")]
async fn record_and_mismatch_message_channel(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    Json(payload): Json<Value>,
) -> Json<Value> {
    requests.lock().expect("broker requests").push(payload);
    Json(json!({"result":{
        "channel_id":"202",
        "message_id":"912345678901234568"
    }}))
}

#[cfg(feature = "testing")]
async fn record_committed_dispatch_and_fail(
    State((requests, pool)): State<(Arc<Mutex<Vec<Value>>>, PgPool)>,
    Json(payload): Json<Value>,
) -> StatusCode {
    let idempotency_key = payload["idempotency_key"]
        .as_str()
        .expect("broker idempotency key");
    let announcement_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM scrim.announcement_drafts WHERE idempotency_key=$1")
            .bind(idempotency_key)
            .fetch_optional(&pool)
            .await
            .expect("announcement state at Discord boundary");
    if let Some(status) = announcement_status {
        assert_eq!(
            status, "publishing",
            "Discord delivery started before the announcement claim was committed"
        );
    } else {
        let reminder_state: (String, String) = sqlx::query_as(
            "SELECT reminder.status, effect.state \
               FROM scrim.match_request_reminders reminder \
               JOIN scrim.match_request_reminder_effects link ON link.reminder_id=reminder.id \
               JOIN scrim.outbox_effects effect ON effect.id=link.outbox_effect_id \
              WHERE effect.payload->>'idempotency_key'=$1",
        )
        .bind(idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("reminder state at Discord boundary");
        assert_eq!(
            reminder_state,
            ("uncertain".to_string(), "uncertain".to_string()),
            "Discord delivery started before the reminder claim was committed"
        );
    }
    requests.lock().expect("broker requests").push(payload);
    StatusCode::SERVICE_UNAVAILABLE
}

#[cfg(feature = "testing")]
async fn accept_broker_once_per_idempotency_key(
    State(requests): State<Arc<Mutex<HashMap<String, Value>>>>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let idempotency_key = payload["idempotency_key"]
        .as_str()
        .expect("broker idempotency key")
        .to_owned();
    if idempotency_key.chars().count() > 128 {
        return Err(StatusCode::BAD_REQUEST);
    }
    requests
        .lock()
        .expect("broker requests")
        .entry(idempotency_key)
        .or_insert(payload);
    Ok(Json(json!({})))
}

#[cfg(feature = "testing")]
async fn fail_once_then_create_role(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let key_is_too_long = payload["idempotency_key"]
        .as_str()
        .is_some_and(|key| key.chars().count() > 128);
    let mut requests = requests.lock().expect("broker requests");
    let first_request = requests.is_empty();
    requests.push(payload);
    if key_is_too_long {
        Err(StatusCode::BAD_REQUEST)
    } else if first_request {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    } else {
        Ok(Json(json!({"result":{"role_id":"912345678901234567"}})))
    }
}

#[cfg(feature = "testing")]
async fn fail_once_then_send_message(
    State(requests): State<Arc<Mutex<Vec<Value>>>>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let mut requests = requests.lock().expect("broker requests");
    let first_request = requests.is_empty();
    requests.push(payload);
    if first_request {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    } else {
        Ok(Json(json!({"result":{"message_id":"912345678901234568"}})))
    }
}

#[cfg(feature = "testing")]
async fn record_and_fail_second_channel_once(
    State((requests, selection)): State<(Arc<Mutex<Vec<Value>>>, Option<(PgPool, i32)>)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if let Some((pool, match_id)) = selection {
        let selection_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                 SELECT 1 FROM scrim.match_result_selections WHERE match_id=$1\
             )",
        )
        .bind(match_id)
        .fetch_one(&pool)
        .await
        .expect("selection at Discord boundary");
        assert!(
            selection_exists,
            "Discord delivery started before the result selection was committed"
        );
    }
    let mut requests = requests.lock().expect("broker requests");
    let first_attempt = !requests.iter().any(|request| {
        request["channel_id"] == payload["channel_id"]
            && request["idempotency_key"] == payload["idempotency_key"]
    });
    let channel_id = payload["channel_id"].clone();
    let message_id = channel_id
        .as_str()
        .map(str::to_string)
        .or_else(|| channel_id.as_i64().map(|id| id.to_string()))
        .expect("channel id");
    let fail = message_id == "1510102" && first_attempt;
    let request_number = requests.len();
    requests.push(payload);
    if fail {
        Err(StatusCode::SERVICE_UNAVAILABLE)
    } else {
        Ok(Json(json!({"result":{
            "channel_id": channel_id,
            "message_id": format!("91{message_id}{request_number}")
        }})))
    }
}

#[cfg(feature = "testing")]
struct PausingBroker {
    calls: AtomicUsize,
    retry_started: tokio::sync::Notify,
    both_retries_started: tokio::sync::Notify,
    release_retry: tokio::sync::Notify,
}

#[cfg(feature = "testing")]
async fn pause_first_retry_broker(
    State(state): State<Arc<PausingBroker>>,
    Json(_payload): Json<Value>,
) -> StatusCode {
    match state.calls.fetch_add(1, Ordering::SeqCst) {
        2 => {
            state.retry_started.notify_one();
            state.release_retry.notified().await;
        }
        3 => state.both_retries_started.notify_one(),
        _ => {}
    }
    StatusCode::SERVICE_UNAVAILABLE
}

#[cfg(feature = "testing")]
async fn accept_broker(Json(_payload): Json<Value>) -> Json<Value> {
    Json(json!({}))
}

#[derive(Clone, Copy, Default)]
struct TestHeaders<'a> {
    token: Option<&'a str>,
    request_id: Option<&'a str>,
    idempotency_key: Option<&'a str>,
    actor_id: Option<&'a str>,
    actor_name: Option<&'a str>,
}

async fn send(
    app: &Router,
    peer_ip: IpAddr,
    headers: TestHeaders<'_>,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(HOST, "localhost")
        .header(CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(SocketAddr::new(peer_ip, 45_000)));
    for (name, value) in [
        ("X-Internal-Token", headers.token),
        ("X-Request-Id", headers.request_id),
        ("Idempotency-Key", headers.idempotency_key),
        ("X-Actor-Discord-Id", headers.actor_id),
        ("X-Actor-Display-Name", headers.actor_name),
    ] {
        if let Some(value) = value {
            builder = builder.header(name, value);
        }
    }
    let request = builder
        .body(
            body.map(|value| Body::from(value.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .expect("request");
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, body)
}

fn interaction() -> Value {
    json!({
        "schema_version": "turnier-scrim-match-request-response:v1",
        "event": "scrimreq:v1:interaction:44",
        "idempotency": "scrimreq:v1:interaction:44",
        "action": "slot",
        "request": "31",
        "team": "2",
        "slot": 0,
        "interaction": "44",
        "guild": "55",
        "channel": "66",
        "message": "77",
        "actor": "88",
        "actor_role_ids": ["99"]
    })
}

fn batch_request() -> Value {
    json!({
        "deadline_at": "2099-08-01T20:00:00Z",
        "slots": [
            {"day":"sat", "from_minute":1200, "to_minute":1320},
            {"day":"sun", "from_minute":1200, "to_minute":1320}
        ],
        "pairings": [{"team_a_id":"1", "team_b_id":"2", "slots": null}]
    })
}

#[tokio::test]
async fn reads_require_loopback_dedicated_token_and_bff_actor() {
    let route = "/internal/turnier/v1/scrims/command-center";
    let (status, _) = send(
        &app(),
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
        TestHeaders {
            token: Some("internal-token"),
            ..TestHeaders::default()
        },
        Method::GET,
        route,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(
        &app(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders::default(),
        Method::GET,
        route,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = send(
        &app(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            ..TestHeaders::default()
        },
        Method::GET,
        route,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "X-Actor-Discord-Id fehlt");
}

#[tokio::test]
async fn canonical_dl_interaction_requires_request_and_idempotency_headers() {
    let route = "/internal/turnier/v1/scrims/interactions/match-request-response";
    let app = app();
    let mut mismatch = interaction();
    mismatch["idempotency"] = json!("scrimreq:v1:interaction:other");
    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            ..TestHeaders::default()
        },
        Method::POST,
        route,
        Some(interaction()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "X-Request-Id fehlt");

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("request:44"),
            ..TestHeaders::default()
        },
        Method::POST,
        route,
        Some(interaction()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "Idempotency-Key fehlt");

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("request-44"),
            idempotency_key: Some("scrimreq:v1:interaction:44"),
            ..TestHeaders::default()
        },
        Method::POST,
        route,
        Some(interaction()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "X-Request-Id ist ungültig");

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("request:44"),
            idempotency_key: Some("scrimreq:v1:interaction:44"),
            ..TestHeaders::default()
        },
        Method::POST,
        route,
        Some(mismatch),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "Idempotency-Key stimmt nicht überein");
}

#[tokio::test]
async fn old_invented_adapter_routes_are_absent() {
    let route = "/internal/turnier/v1/scrims/interactions/match-request-response";
    let mut mismatch = interaction();
    mismatch["idempotency"] = json!("scrimreq:v1:interaction:other");
    let (status, body) = send(
        &app(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("request:44"),
            idempotency_key: Some("scrimreq:v1:interaction:44"),
            ..TestHeaders::default()
        },
        Method::POST,
        route,
        Some(mismatch),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "Idempotency-Key stimmt nicht überein");

    for old_route in [
        "/internal/turnier/v1/scrims/website/commands",
        "/internal/turnier/v1/scrims/dl-bots/interactions",
        "/internal/turnier/v1/scrims/action-receipts",
    ] {
        let (status, _) = send(
            &app(),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            TestHeaders {
                token: Some("internal-token"),
                ..TestHeaders::default()
            },
            Method::POST,
            old_route,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "route {old_route}");
    }
}

#[tokio::test]
async fn bff_mutations_take_actor_only_from_headers() {
    let route = "/internal/turnier/v1/scrims/match-request-batches";
    let body = batch_request();
    let (status, response) = send(
        &app(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("request:1"),
            idempotency_key: Some("batch:1"),
            ..TestHeaders::default()
        },
        Method::POST,
        route,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response["detail"], "X-Actor-Discord-Id fehlt");

    let mut body = batch_request();
    body["actor"] = json!({"discord_id":"123"});
    let (status, _) = send(
        &app(),
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("request:1"),
            idempotency_key: Some("batch:1"),
            actor_id: Some("123"),
            actor_name: Some("Actor"),
        },
        Method::POST,
        route,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn canonical_internal_route_matrix_is_registered_with_exact_methods() {
    let app = app();
    let headers = TestHeaders {
        token: Some("internal-token"),
        ..TestHeaders::default()
    };
    let reads = [
        "/internal/turnier/v1/caster/teams",
        "/internal/turnier/v1/scrims/command-center",
        "/internal/turnier/v1/scrims/me",
        "/internal/turnier/v1/scrims/pool",
        "/internal/turnier/v1/scrims/coaches",
        "/internal/turnier/v1/scrims/teams",
        "/internal/turnier/v1/scrims/history",
        "/internal/turnier/v1/scrims/teams/1/board",
        "/internal/turnier/v1/scrims/teams/1/timeline",
        "/internal/turnier/v1/scrims/match-requests/defaults",
        "/internal/turnier/v1/scrims/match-request-batches",
        "/internal/turnier/v1/scrims/match-request-batches/1",
        "/internal/turnier/v1/scrims/match-requests/1",
        "/internal/turnier/v1/scrims/match-requests/1/status-preview",
        "/internal/turnier/v1/scrims/match-requests/1/replacement-needs",
        "/internal/turnier/v1/scrims/replacement-needs/1/candidates",
        "/internal/turnier/v1/scrims/matches",
        "/internal/turnier/v1/scrims/matches/1",
        "/internal/turnier/v1/scrims/blocks/1/announcement-preview",
        "/internal/turnier/v1/scrims/actions/1",
    ];
    for route in reads {
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            headers,
            Method::GET,
            route,
            None,
        )
        .await;
        assert_ne!(status, StatusCode::NOT_FOUND, "missing GET {route}");
        assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED, "wrong GET {route}");
    }

    let mutations = [
        (Method::POST, "/internal/turnier/v1/scrims/signup"),
        (Method::PUT, "/internal/turnier/v1/scrims/me/availability"),
        (Method::POST, "/internal/turnier/v1/scrims/teams"),
        (Method::PATCH, "/internal/turnier/v1/scrims/teams/1"),
        (Method::PATCH, "/internal/turnier/v1/scrims/participants/1"),
        (Method::POST, "/internal/turnier/v1/scrims/teams/1/announce"),
        (Method::POST, "/internal/turnier/v1/scrims/teams/1/suggest"),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/1/substitute",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/participants/1/resync-discord",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-request-batches",
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/match-requests/1",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/1/release",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/1/reminders",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/1/status-publications",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/replacement-needs/1/requests",
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/replacement-requests/1",
        ),
        (Method::POST, "/internal/turnier/v1/scrims/matches"),
        (
            Method::PUT,
            "/internal/turnier/v1/scrims/matches/1/lobby-code",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches/1/match-ids",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches/1/result-fetches",
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/matches/1/result-refs/1",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/blocks/1/announcement-publications",
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/interactions/match-request-response",
        ),
    ];
    for (method, route) in mutations {
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            headers,
            method.clone(),
            route,
            Some(json!({})),
        )
        .await;
        assert_ne!(status, StatusCode::NOT_FOUND, "missing {method} {route}");
        assert_ne!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "wrong {method} {route}"
        );
    }

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::GET,
        "/internal/turnier/v1/scrims",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::PATCH,
        "/internal/turnier/v1/scrims/matches/1/match-ids/1",
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn every_canonical_mutation_requires_request_and_idempotency_headers() {
    let app = app();
    let headers = TestHeaders {
        token: Some("internal-token"),
        actor_id: Some("123456789"),
        actor_name: Some("Operator"),
        ..TestHeaders::default()
    };
    let unknown_day = json!({"status":"unknown","from":null,"to":null});
    let availability = json!({
        "mon": unknown_day,
        "tue": unknown_day,
        "wed": unknown_day,
        "thu": unknown_day,
        "fri": unknown_day,
        "sat": unknown_day,
        "sun": unknown_day
    });
    let cases = vec![
        (
            Method::POST,
            "/internal/turnier/v1/scrims/signup",
            json!({}),
        ),
        (
            Method::PUT,
            "/internal/turnier/v1/scrims/me/availability",
            availability,
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams",
            json!({"name":"Team"}),
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/teams/1",
            json!({}),
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/participants/1",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/1/announce",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/1/suggest",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/1/substitute",
            json!({"participant_id":1,"window":{"day":"fri","from":1200,"to":1320}}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/participants/1/resync-discord",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-request-batches",
            batch_request(),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/1/release",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/1/reminders",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/1/status-publications",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/replacement-needs/1/requests",
            json!({"participant_id":"1"}),
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/replacement-requests/1",
            json!({"action":"accept"}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches",
            json!({"team_a_id":"1","team_b_id":"2"}),
        ),
        (
            Method::PUT,
            "/internal/turnier/v1/scrims/matches/1/lobby-code",
            json!({"lobby_code":"ABC123"}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches/1/match-ids",
            json!({"match_ids":["123"]}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches/1/result-fetches",
            json!({}),
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/matches/1/result-refs/1",
            json!({"message":"clarification"}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/blocks/1/announcement-publications",
            json!({"message":"announcement"}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/interactions/match-request-response",
            interaction(),
        ),
    ];

    for (method, route, body) in cases {
        let (status, response) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            headers,
            method.clone(),
            route,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{method} {route}");
        assert_eq!(response["detail"], "X-Request-Id fehlt", "{method} {route}");
    }
}

#[tokio::test]
async fn self_service_mutations_keep_the_internal_boundary_checks() {
    let headers = TestHeaders {
        token: Some("internal-token"),
        request_id: Some("request:self-service"),
        idempotency_key: Some("scrim:self-service"),
        actor_id: Some("123456789"),
        actor_name: Some("Player"),
    };
    for (method, route, body) in [
        (
            Method::POST,
            "/internal/turnier/v1/scrims/signup",
            json!({}),
        ),
        (
            Method::PUT,
            "/internal/turnier/v1/scrims/me/availability",
            json!({}),
        ),
    ] {
        let (status, _) = send(
            &app(),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            headers,
            method.clone(),
            route,
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);

        let (status, _) = send(
            &app(),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            TestHeaders {
                token: Some("wrong-token"),
                ..headers
            },
            method,
            route,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn self_service_routes_create_update_and_only_change_availability() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let app = app_with_pool(db.pool().clone());
    let signup_route = "/internal/turnier/v1/scrims/signup";
    let headers = TestHeaders {
        token: Some("internal-token"),
        request_id: Some("request:signup"),
        idempotency_key: Some("scrim:signup"),
        actor_id: Some("950001"),
        actor_name: Some("Signup Player"),
    };
    let unknown = json!({"status":"unknown","from":null,"to":null});
    let slots = json!({
        "mon": unknown,
        "tue": unknown,
        "wed": unknown,
        "thu": unknown,
        "fri": unknown,
        "sat": {"status":"available","from":1200,"to":1320},
        "sun": unknown
    });

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::POST,
        signup_route,
        Some(json!({
            "rank":"Oracle",
            "roles":"Flex",
            "availability":"ignored by structured slots",
            "availability_slots":slots
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(created["id"].is_number());
    assert_eq!(created["display_name"], "Signup Player");
    assert_eq!(created["availability_confirmed"], true);
    assert_eq!(created["availability_slots"]["sat"]["from"], 1200);
    assert_eq!(created["status"], "new");
    assert_eq!(created["source"], "web_form");

    let update_headers = TestHeaders {
        request_id: Some("request:signup-update"),
        idempotency_key: Some("scrim:signup-update"),
        actor_name: Some("Renamed Player"),
        ..headers
    };
    let (status, updated) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        update_headers,
        Method::POST,
        signup_route,
        Some(json!({
            "rank":"Phantom",
            "roles":"Duo",
            "availability":"new free text",
            "availability_slots":null
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["id"], created["id"]);
    assert_eq!(updated["display_name"], "Renamed Player");
    assert_eq!(updated["availability"], "new free text");
    assert_eq!(
        updated["availability_slots"]["sat"]["from"], 1200,
        "structured availability survives a text-only signup update"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM scrim.participants WHERE discord_id=950001")
            .fetch_one(db.pool())
            .await
            .expect("participant count");
    assert_eq!(count, 1);

    let availability_headers = TestHeaders {
        request_id: Some("request:availability"),
        idempotency_key: Some("scrim:availability"),
        ..update_headers
    };
    let new_slots = json!({
        "mon": unknown,
        "tue": unknown,
        "wed": unknown,
        "thu": unknown,
        "fri": {"status":"available","from":1140,"to":1260},
        "sat": unknown,
        "sun": unknown
    });
    let (status, availability) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        availability_headers,
        Method::PUT,
        "/internal/turnier/v1/scrims/me/availability",
        Some(new_slots),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(availability["rank"], "Phantom");
    assert_eq!(availability["roles"], "Duo");
    assert_eq!(availability["availability_slots"]["fri"]["from"], 1140);
    assert_eq!(availability["status"], "new");
    assert_eq!(availability["source"], "web_form");
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn signup_role_failure_is_reported_and_retried_from_saved_state() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/member/add-role",
            post(fail_once_then_send_message),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app =
        app_with_pool_broker_and_signup_role(db.pool().clone(), Some(&broker_url), Some(9001));
    let headers = TestHeaders {
        token: Some("internal-token"),
        request_id: Some("request:signup-discord-retry"),
        idempotency_key: Some("scrim:signup-discord-retry"),
        actor_id: Some("950002"),
        actor_name: Some("Signup Retry"),
    };
    let body = json!({"rank":"Oracle","roles":"Flex"});

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::POST,
        "/internal/turnier/v1/scrims/signup",
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let saved: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM scrim.participants WHERE discord_id=950002")
            .fetch_one(db.pool())
            .await
            .expect("saved signup");
    assert_eq!(saved, 1);

    let (status, participant) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::POST,
        "/internal/turnier/v1/scrims/signup",
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(participant["display_name"], "Signup Retry");
    let requests = requests.lock().expect("broker requests");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1]);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn runtime_control_blocks_real_mutations_until_turniere_owns_runtime() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    let app = app_with_pool(db.pool().clone());

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("batch:runtime", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-request-batches",
        Some(batch_request()),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["detail"]
        .as_str()
        .is_some_and(|detail| detail.contains("Runtime")));

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:runtime", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/teams",
        Some(json!({
            "name":"Blocked Team",
            "coach_discord_id":"123456789",
            "default_from":1200,
            "default_to":1320
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body["detail"]
        .as_str()
        .is_some_and(|detail| detail.contains("Runtime")));
    let blocked_team_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM scrim.teams WHERE name='Blocked Team'")
            .fetch_one(db.pool())
            .await
            .expect("blocked team count");
    assert_eq!(blocked_team_count, 0);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn mutation_waits_for_runtime_transition_lock_and_reads_post_transition_state() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    let app = app_with_pool(db.pool().clone());
    let mut tx = db.pool().begin().await.expect("runtime lock tx");
    sqlx::query("SELECT pg_advisory_xact_lock(724060001, 724060002)")
        .execute(&mut *tx)
        .await
        .expect("hold runtime lock");
    let applied: bool = sqlx::query_scalar(
        "SELECT applied FROM scrim.transition_runtime_control(\
             0, 'draining', 'turniere', '123456789', 'Coach', 'request:transition', 'request:transition', '{}'::jsonb\
         )",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("transition in locked tx");
    assert!(applied);

    let task_app = app.clone();
    let pending = tokio::spawn(async move {
        send(
            &task_app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("batch:transition", "123456789"),
            Method::POST,
            "/internal/turnier/v1/scrims/match-request-batches",
            Some(batch_request()),
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        !pending.is_finished(),
        "mutation did not wait for runtime lock"
    );

    tx.commit().await.expect("commit transition");
    let (status, body) = pending.await.expect("mutation task");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn active_coaches_only_and_planning_create_is_persistent_and_idempotent() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    sqlx::query(
        "INSERT INTO core.meta_users(id, username, display_name, role) VALUES \
         (223456789, 'admin', 'Admin', 'admin')",
    )
    .execute(db.pool())
    .await
    .expect("admin seed");
    let app = app_with_pool(db.pool().clone());
    let route = "/internal/turnier/v1/scrims/match-request-batches";

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("batch:admin", "223456789"),
        Method::POST,
        route,
        Some(batch_request()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["detail"], "Aktiver Scrim-Coach erforderlich");

    let body = json!({
        "deadline_at": "2099-08-01T20:00:00Z",
        "slots": [
            {"day":"sat", "from_minute":1200, "to_minute":1320},
            {"day":"sun", "from_minute":1200, "to_minute":1320}
        ],
        "pairings": [{
            "team_a_id":"1",
            "team_b_id":null,
            "slots":[
                {"day":"fri", "from_minute":1140, "to_minute":1260},
                {"day":"fri", "from_minute":1290, "to_minute":1410}
            ]
        }]
    });
    let (status, response) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("batch:create", "123456789"),
        Method::POST,
        route,
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["accepted"], true);

    let (status, replay) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("batch:create", "123456789"),
        Method::POST,
        route,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, response);

    let row = sqlx::query(
        "SELECT b.template, mr.team_b_id, mr.slot_options \
           FROM scrim.match_request_batches b \
           JOIN scrim.match_requests mr ON mr.batch_id = b.id",
    )
    .fetch_one(db.pool())
    .await
    .expect("created planning row");
    assert_eq!(row.get::<String, _>("template"), "regular_scrim");
    assert_eq!(row.get::<Option<i32>, _>("team_b_id"), None);
    let slots = row.get::<Value, _>("slot_options");
    assert_eq!(slots[0]["day"], "fri");

    let batch_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scrim.match_request_batches")
        .fetch_one(db.pool())
        .await
        .expect("batch count");
    assert_eq!(batch_count, 1);

    let audit = sqlx::query(
        "SELECT actor_pseudonym, request_id, correlation_id, after_data \
           FROM scrim.audit_events WHERE event_type = 'match_request_batch_created'",
    )
    .fetch_one(db.pool())
    .await
    .expect("batch audit");
    assert!(audit
        .get::<String, _>("actor_pseudonym")
        .starts_with("act_"));
    assert_eq!(
        audit.get::<Option<String>, _>("request_id").as_deref(),
        Some("request:1")
    );
    assert_eq!(
        audit.get::<Option<String>, _>("correlation_id").as_deref(),
        Some("batch:create")
    );
    let after_data = audit.get::<Value, _>("after_data");
    assert_eq!(after_data["template"], "regular_scrim");
    assert!(!after_data.to_string().contains("123456789"));
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn read_me_next_match_uses_earliest_active_upcoming_match_only() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_teams(db.pool(), &[1, 2]).await;
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.participants(id, discord_id, display_name, rank_source, rank_verified, status, source, created_at, updated_at)
        VALUES (501, 555, 'Player', 'self', false, 'assigned', 'test', now(), now());
        INSERT INTO scrim.team_members(team_id, participant_id, role, is_captain, is_bench)
        VALUES (1, 501, 'player', true, false);
        INSERT INTO scrim.matches(id, team_a_id, team_b_id, status, scheduled_at, created_at) VALUES
            (201, 1, 2, 'scheduled', now() - interval '1 hour', now() - interval '5 hours'),
            (202, 1, 2, 'cancelled', now() + interval '1 hour', now() - interval '4 hours'),
            (203, 1, 2, 'scheduled', now() + interval '2 hours', now() - interval '3 hours'),
            (204, 1, 2, 'scheduled', now() + interval '1 hour', now() - interval '2 hours'),
            (205, 1, 2, 'scheduled', now() + interval '30 minutes', now() - interval '1 hour');
        INSERT INTO scrim.match_result_refs(
            match_id, steam_match_id, source_user_id, source_display_name, fetch_status,
            winner_team_id, normalized_result_json, validation_status, entered_at, updated_at
        ) VALUES (205, 205001, 'actor', 'Actor', 'fetched', 1, '{}'::jsonb, 'valid', now(), now());
        INSERT INTO scrim.match_result_selections(
            match_id, result_ref_id, selected_by_user_id, selected_by_display_name, selection_reason
        )
        SELECT 205, id, '42', 'Actor', 'contract_test'
          FROM scrim.match_result_refs
         WHERE match_id = 205;
        "#,
    )
    .execute(db.pool())
    .await
    .expect("next match seed");
    let app = app_with_pool(db.pool().clone());

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            actor_id: Some("555"),
            actor_name: Some("Player"),
            ..TestHeaders::default()
        },
        Method::GET,
        "/internal/turnier/v1/scrims/me",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["next_match"]["id"], "204");
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn team_member_readmodel_includes_teamboard_fields_on_all_team_reads() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_coach(db.pool(), 123456789).await;
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.teams(id, name, created_at)
        VALUES (840010, 'Route Board Team', now());
        INSERT INTO scrim.participants(
            id, discord_id, display_name, rank, rank_source, rank_verified, roles,
            availability, availability_slots, notes, status, source, created_at, updated_at
        ) VALUES
            (
                840001, 840001001, 'Structured Availability', 'Phantom', 'self', true,
                'support', 'Fri 20-22',
                '{"fri":{"status":"available","from":1200,"to":1320}}'::jsonb,
                'structured note', 'assigned', 'test', now(), now()
            ),
            (
                840002, 840002002, 'Text Availability', 'Oracle', 'self', true,
                'carry', 'Ask in Discord', NULL,
                'text note', 'assigned', 'test', now(), now()
            );
        INSERT INTO scrim.team_members(team_id, participant_id, role, is_captain, is_bench)
        VALUES
            (840010, 840001, 'player', true, false),
            (840010, 840002, 'player', false, false);
        "#,
    )
    .execute(db.pool())
    .await
    .expect("teamboard fixture");
    let app = app_with_pool(db.pool().clone());
    let headers = TestHeaders {
        token: Some("internal-token"),
        actor_id: Some("123456789"),
        actor_name: Some("Coach"),
        ..TestHeaders::default()
    };

    for route in [
        "/internal/turnier/v1/scrims/command-center",
        "/internal/turnier/v1/scrims/teams",
        "/internal/turnier/v1/scrims/teams/840010/board",
    ] {
        let (status, body) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            headers,
            Method::GET,
            route,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{route}");
        let team = if route.ends_with("/command-center") {
            body["teams"]
                .as_array()
                .expect("command-center teams")
                .iter()
                .find(|team| team["id"] == "840010")
                .expect("command-center team")
        } else if route.ends_with("/teams") {
            body.as_array()
                .expect("teams array")
                .iter()
                .find(|team| team["id"] == "840010")
                .expect("teams team")
        } else {
            &body["team"]
        };
        let members = team["members"].as_array().expect("members");
        let structured = members
            .iter()
            .find(|member| member["participant_id"] == "840001")
            .expect("structured member");
        assert_eq!(structured["team_id"], "840010");
        assert_eq!(structured["discord_id"], "840001001");
        assert_eq!(structured["rank"], "Phantom");
        assert_eq!(structured["roles"], "support");
        assert_eq!(structured["availability"], "Fri 20-22");
        assert_eq!(structured["notes"], "structured note");
        assert_eq!(
            structured["availability_slots"]["fri"]["status"],
            "available"
        );
        assert_eq!(structured["availability_slots"]["fri"]["from"], 1200);
        assert!(structured.get("rank_source").is_none());
        assert!(structured.get("source").is_none());

        let text_only = members
            .iter()
            .find(|member| member["participant_id"] == "840002")
            .expect("text-only member");
        assert_eq!(text_only["availability"], "Ask in Discord");
        assert!(text_only.get("availability_slots").is_some());
        assert!(text_only["availability_slots"].is_null());
    }
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn planning_create_requires_effective_slots_not_global_slots() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1]).await;
    let app = app_with_pool(db.pool().clone());
    let body = json!({
        "technical_template_key": "training",
        "deadline_at": "2099-08-01T20:00:00Z",
        "slots": [],
        "pairings": [{
            "team_a_id": "1",
            "team_b_id": null,
            "slots": [
                {"day":"fri", "from_minute":1140, "to_minute":1260},
                {"day":"fri", "from_minute":1290, "to_minute":1410}
            ]
        }]
    });

    let (status, response) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("batch:effective_slots", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-request-batches",
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["accepted"], true);

    let slot_count: i32 = sqlx::query_scalar(
        "SELECT jsonb_array_length(slot_options) FROM scrim.match_requests WHERE team_a_id = 1",
    )
    .fetch_one(db.pool())
    .await
    .expect("stored effective slots");
    assert_eq!(slot_count, 2);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn planning_create_rejects_pairings_without_effective_slots() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_coach(db.pool(), 123456789).await;
    let app = app_with_pool(db.pool().clone());
    let body = json!({
        "deadline_at": "2099-08-01T20:00:00Z",
        "pairings": [{
            "team_a_id": "1",
            "team_b_id": null,
            "slots": null
        }]
    });

    let (status, response) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("batch:no_effective_slots", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-request-batches",
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response["detail"], "Each match needs two to five slots");
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn concurrent_planning_create_serializes_active_team_race() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2, 3]).await;
    let app = app_with_pool(db.pool().clone());
    let route = "/internal/turnier/v1/scrims/match-request-batches";
    let mut body_a = batch_request();
    body_a["pairings"][0]["team_a_id"] = json!("1");
    body_a["pairings"][0]["team_b_id"] = json!("2");
    let mut body_b = batch_request();
    body_b["pairings"][0]["team_a_id"] = json!("1");
    body_b["pairings"][0]["team_b_id"] = json!("3");

    let (a, b) = tokio::join!(
        send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("batch:race:a", "123456789"),
            Method::POST,
            route,
            Some(body_a),
        ),
        send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("batch:race:b", "123456789"),
            Method::POST,
            route,
            Some(body_b),
        )
    );
    let statuses = [a.0, b.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::BAD_REQUEST)
            .count(),
        1
    );
    let team_one_requests: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.match_requests WHERE team_a_id = 1 OR team_b_id = 1",
    )
    .fetch_one(db.pool())
    .await
    .expect("team one requests");
    assert_eq!(team_one_requests, 1);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn release_match_request_persists_selected_slot_and_replays_idempotently() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_release_fixture(db.pool()).await;
    let app = app_with_pool(db.pool().clone());

    for _ in 0..2 {
        let (status, body) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("release:91", "123456789"),
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/91/release",
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["accepted"], true);
    }

    let row =
        sqlx::query("SELECT released_slot_index, status FROM scrim.match_requests WHERE id = 91")
            .fetch_one(db.pool())
            .await
            .expect("released request");
    assert_eq!(row.get::<Option<i32>, _>("released_slot_index"), Some(0));
    assert_eq!(row.get::<String, _>("status"), "closed");

    let audit = sqlx::query(
        "SELECT actor_pseudonym, request_id, correlation_id, after_data \
           FROM scrim.audit_events WHERE event_type = 'match_request_released'",
    )
    .fetch_one(db.pool())
    .await
    .expect("release audit");
    assert!(audit
        .get::<String, _>("actor_pseudonym")
        .starts_with("act_"));
    assert_eq!(
        audit.get::<Option<String>, _>("request_id").as_deref(),
        Some("request:1")
    );
    assert_eq!(
        audit.get::<Option<String>, _>("correlation_id").as_deref(),
        Some("release:91")
    );
    let after_data = audit.get::<Value, _>("after_data");
    assert_eq!(after_data["request_id"], "91");
    assert!(!after_data.to_string().contains("123456789"));
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn participant_interaction_persists_only_for_own_team_and_original_message() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_interaction_fixture(db.pool()).await;
    let app = app_with_pool(db.pool().clone());
    let route = "/internal/turnier/v1/scrims/interactions/match-request-response";

    let mut body = interaction();
    body["request"] = json!("31");
    body["team"] = json!("2");
    body["actor"] = json!("88");
    let (status, response) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("interaction:request"),
            idempotency_key: Some("scrimreq:v1:interaction:44"),
            ..TestHeaders::default()
        },
        Method::POST,
        route,
        Some(body),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["accepted"], true);

    let saved_slot: i32 = sqlx::query_scalar(
        "SELECT slot_index FROM scrim.match_request_responses WHERE request_id = 31",
    )
    .fetch_one(db.pool())
    .await
    .expect("saved response");
    assert_eq!(saved_slot, 0);

    let audit = sqlx::query(
        "SELECT actor_pseudonym, request_id, correlation_id, after_data \
           FROM scrim.audit_events WHERE event_type = 'match_request_response_recorded'",
    )
    .fetch_one(db.pool())
    .await
    .expect("response audit");
    assert!(audit
        .get::<String, _>("actor_pseudonym")
        .starts_with("act_"));
    assert_eq!(
        audit.get::<Option<String>, _>("request_id").as_deref(),
        Some("interaction:request")
    );
    assert_eq!(
        audit.get::<Option<String>, _>("correlation_id").as_deref(),
        Some("scrimreq:v1:interaction:44")
    );
    let after_data = audit.get::<Value, _>("after_data");
    assert_eq!(after_data["request_id"], "31");
    assert!(!after_data.to_string().contains("88"));

    let mut foreign = interaction();
    foreign["event"] = json!("scrimreq:v1:interaction:45");
    foreign["idempotency"] = json!("scrimreq:v1:interaction:45");
    foreign["request"] = json!("31");
    foreign["team"] = json!("2");
    foreign["interaction"] = json!("45");
    foreign["actor"] = json!("99");
    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("interaction:request2"),
            idempotency_key: Some("scrimreq:v1:interaction:45"),
            ..TestHeaders::default()
        },
        Method::POST,
        route,
        Some(foreign),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        body["detail"],
        "Diese Scrim-Aktion gehört nicht zu deinem Team"
    );
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn participant_interaction_revalidates_slots_after_advisory_lock() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_interaction_fixture(db.pool()).await;
    let app = app_with_pool(db.pool().clone());

    let mut lock_tx = db.pool().begin().await.expect("advisory lock tx");
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(31_i32)
        .bind(880_i32)
        .execute(&mut *lock_tx)
        .await
        .expect("hold response lock");

    let pending = tokio::spawn(async move {
        send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            TestHeaders {
                token: Some("internal-token"),
                request_id: Some("interaction:race"),
                idempotency_key: Some("scrimreq:v1:interaction:44"),
                ..TestHeaders::default()
            },
            Method::POST,
            "/internal/turnier/v1/scrims/interactions/match-request-response",
            Some(interaction()),
        )
        .await
    });

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (\
                    SELECT 1 FROM pg_locks \
                     WHERE locktype = 'advisory' \
                       AND classid = $1::oid AND objid = $2::oid \
                       AND objsubid = 2 AND NOT granted\
                 )",
            )
            .bind(31_i32)
            .bind(880_i32)
            .fetch_one(db.pool())
            .await
            .expect("inspect advisory locks");
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("interaction reached response lock");

    sqlx::query("UPDATE scrim.match_requests SET slot_options = '[]'::jsonb WHERE id = 31")
        .execute(db.pool())
        .await
        .expect("replace slot options");
    lock_tx.commit().await.expect("release response lock");

    let (status, body) = pending.await.expect("interaction task");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "slot is out of range");
    let saved: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.match_request_responses WHERE request_id = 31",
    )
    .fetch_one(db.pool())
    .await
    .expect("response count");
    assert_eq!(saved, 0);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn maximal_valid_request_key_retries_team_role_creation_with_broker_safe_key() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/role/create",
            post(fail_once_then_create_role),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));
    let request_key = format!("{}:{}", "a".repeat(32), "b".repeat(95));
    assert_eq!(request_key.chars().count(), 128);
    let headers = coach_headers(&request_key, "123456789");
    let body = json!({
        "name":"Role Failure",
        "default_from":1200,
        "default_to":1320
    });

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::POST,
        "/internal/turnier/v1/scrims/teams",
        Some(body.clone()),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        created["discord_sync"]["detail"],
        "Team gespeichert, aber die Discord-Rolle konnte nicht erstellt werden. Löse denselben Vorgang noch einmal aus, damit die Rolle nachgereicht wird."
    );
    assert_eq!(created["discord_sync"]["ok"], false);
    let team_id = created["id"].as_i64().expect("numeric team id") as i32;
    let saved_role_id: Option<i64> =
        sqlx::query_scalar("SELECT discord_role_id FROM scrim.teams WHERE id=$1")
            .bind(team_id)
            .fetch_one(db.pool())
            .await
            .expect("committed team");
    assert_eq!(saved_role_id, None);
    {
        let requests = requests.lock().expect("broker requests");
        assert_eq!(requests.len(), 1);
    }

    let (retry_status, retried) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::POST,
        "/internal/turnier/v1/scrims/teams",
        Some(body.clone()),
    )
    .await;
    assert_eq!(retry_status, StatusCode::OK);
    assert_eq!(retried["id"], created["id"]);
    assert_eq!(retried["discord_sync"]["ok"], true);
    let saved_role_id: Option<i64> =
        sqlx::query_scalar("SELECT discord_role_id FROM scrim.teams WHERE id=$1")
            .bind(team_id)
            .fetch_one(db.pool())
            .await
            .expect("retried team");
    assert_eq!(saved_role_id, Some(912345678901234567));
    {
        let requests = requests.lock().expect("broker requests");
        assert_eq!(requests.len(), 2);
        let broker_key = format!("scrim-team-{team_id}-role-create");
        assert!(broker_key.chars().count() <= 128);
        assert!(requests
            .iter()
            .all(|request| request["idempotency_key"] == broker_key));
        assert!(requests
            .iter()
            .all(|request| request["reason"] == "Scrim-Team Role Failure"));
    }

    let (replay_status, replayed) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::POST,
        "/internal/turnier/v1/scrims/teams",
        Some(body),
    )
    .await;
    assert_eq!(replay_status, StatusCode::OK);
    assert_eq!(replayed["id"], created["id"]);
    assert_eq!(requests.lock().expect("broker requests").len(), 2);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn team_creation_rejects_zero_discord_role_id_without_rolling_back_team() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;

    let broker = Router::new().route(
        "/internal/master/v1/discord/role/create",
        post(|| async { Json(json!({"result":{"role_id":"0"}})) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:create:zero-role", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/teams",
        Some(json!({"name":"Zero Role"})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["discord_sync"]["ok"], false);
    let team_id = created["id"].as_i64().expect("numeric team id") as i32;
    let saved_role_id: Option<i64> =
        sqlx::query_scalar("SELECT discord_role_id FROM scrim.teams WHERE id=$1")
            .bind(team_id)
            .fetch_one(db.pool())
            .await
            .expect("committed team");
    assert_eq!(saved_role_id, None);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn announce_reports_reaction_failure_without_losing_posted_message() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_roster_fixture(db.pool()).await;

    let failed_requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-rich-message",
            post(|| async { Json(json!({"result":{"message_id":"12345"}})) }),
        )
        .route(
            "/internal/master/v1/discord/add-reaction",
            post(record_and_fail_broker),
        )
        .with_state(failed_requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, announcement) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:announce:reaction-failure", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/teams/10/announce",
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(announcement["message_id"], "12345");
    assert_eq!(announcement["ok"], true);
    assert_eq!(
        announcement["detail"],
        "Der Aufruf steht im Scrim-Kanal, aber der ✅-Haken konnte nicht gesetzt werden. Setz ihn bitte einmal selbst darunter."
    );
    let requests = failed_requests.lock().expect("broker requests");
    assert_eq!(requests.len(), 1);
    assert!(requests[0]["channel_id"].as_i64().is_some_and(|id| id > 0));
    assert_eq!(requests[0]["message_id"], "12345");
    assert_eq!(requests[0]["emoji"], "✅");
    // Der Schluessel leitet sich aus Kanal und Nachricht ab, nicht aus dem Aufrufer-Key:
    // der darf 128 Zeichen lang sein, der Broker nimmt ebenfalls 128.
    let reaction_key = requests[0]["idempotency_key"]
        .as_str()
        .expect("reaction idempotency key");
    assert_eq!(
        reaction_key,
        format!(
            "scrim-reaction-{}-12345",
            requests[0]["channel_id"].as_i64().expect("channel id")
        )
    );
    assert!(reaction_key.chars().count() <= 128);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn substitute_dm_hashes_the_request_idempotency_key_for_the_broker() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_roster_fixture(db.pool()).await;

    let failed_requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-dm",
            post(record_and_fail_broker),
        )
        .with_state(failed_requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));
    let request_key = "roster:substitute:dm-idempotency";
    let user_id = 9502_u64;

    let (status, substitute) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers(request_key, "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/teams/10/substitute",
        Some(json!({
            "participant_id": 502,
            "window": {"day": "fri", "from": 1200, "to": 1320}
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(substitute["dm"]["ok"], false);
    let requests = failed_requests.lock().expect("broker requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["user_id"], user_id);
    let dm_key = requests[0]["idempotency_key"]
        .as_str()
        .expect("DM idempotency key");
    assert_eq!(
        dm_key,
        format!(
            "scrim-dm-{:x}",
            Sha256::digest(format!("{request_key}\0{user_id}").as_bytes())
        )
    );
    assert!(dm_key.chars().count() <= 128);
    assert!(dm_key.starts_with("scrim-dm-"));

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn participant_discord_resync_is_blocked_until_turniere_owns_runtime() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_coach(db.pool(), 123456789).await;
    seed_roster_fixture(db.pool()).await;

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/member/add-role",
            post(record_and_fail_broker),
        )
        .route(
            "/internal/master/v1/discord/member/remove-role",
            post(record_and_fail_broker),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:resync:legacy-runtime", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/participants/501/resync-discord",
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(requests.lock().expect("broker requests").is_empty());
    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn maximal_valid_request_key_retries_team_coach_role_removal_from_saved_patch() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_coach(db.pool(), 987654321).await;
    seed_roster_fixture(db.pool()).await;
    sqlx::query(
        "UPDATE scrim.teams \
         SET coach='Coach', coach_discord_id=123456789, discord_role_id=8010 \
         WHERE id=10",
    )
    .execute(db.pool())
    .await
    .expect("coach retry fixture");

    let failed_removals = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/member/add-role",
            post(accept_broker),
        )
        .route(
            "/internal/master/v1/discord/member/remove-role",
            post(record_and_fail_broker),
        )
        .with_state(failed_removals.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));
    let request_key = format!("{}:{}", "a".repeat(32), "b".repeat(95));
    assert_eq!(request_key.chars().count(), 128);
    let headers = coach_headers(&request_key, "123456789");
    let body = json!({"coach_discord_id":"987654321"});

    let (first_status, first) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::PATCH,
        "/internal/turnier/v1/scrims/teams/10",
        Some(body.clone()),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(first["discord_sync"]["ok"], false);
    let saved_coach: Option<i64> =
        sqlx::query_scalar("SELECT coach_discord_id FROM scrim.teams WHERE id=10")
            .fetch_one(db.pool())
            .await
            .expect("committed coach replacement");
    assert_eq!(
        saved_coach,
        Some(987654321),
        "Discord-Fehler darf den DB-Stand nicht zurückrollen"
    );

    let (retry_status, retry) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::PATCH,
        "/internal/turnier/v1/scrims/teams/10",
        Some(body),
    )
    .await;
    assert_eq!(retry_status, StatusCode::OK);
    assert_eq!(retry["discord_sync"]["ok"], false);

    let failed_removals = failed_removals.lock().expect("failed removals");
    assert_eq!(failed_removals.len(), 2);
    assert!(failed_removals.iter().all(|request| {
        request["user_id"] == 123456789
            && request["role_id"] == 8010
            && request["reason"] == "scrim coach-123456789 remove role 8010"
    }));
    let first_key = failed_removals[0]["idempotency_key"]
        .as_str()
        .expect("first idempotency key");
    let retry_key = failed_removals[1]["idempotency_key"]
        .as_str()
        .expect("retry idempotency key");
    assert_eq!(first_key, retry_key);
    assert!(first_key.chars().count() <= 128);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn participant_role_sync_retry_replays_the_full_target_state() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_roster_fixture(db.pool()).await;
    sqlx::raw_sql(
        "UPDATE scrim.teams SET discord_role_id=8010 WHERE id=10;
         INSERT INTO scrim.teams(id, name, discord_role_id, created_at)
         VALUES(11, 'Retry Target', 8011, now());
         UPDATE scrim.participants SET status='assigned' WHERE id=501;
         INSERT INTO scrim.team_members(team_id, participant_id, role, is_captain, is_bench)
         VALUES(10, 501, 'player', false, false);",
    )
    .execute(db.pool())
    .await
    .expect("role retry fixture");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/member/add-role",
            post(record_and_fail_broker),
        )
        .route(
            "/internal/master/v1/discord/member/remove-role",
            post(record_and_fail_broker),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));
    let body = json!({"status":"assigned","team_id":11});

    let (first_status, first) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:patch-participant:retry", "123456789"),
        Method::PATCH,
        "/internal/turnier/v1/scrims/participants/501",
        Some(body.clone()),
    )
    .await;
    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(first["discord_sync"]["ok"], false);
    let saved_team: i32 =
        sqlx::query_scalar("SELECT team_id FROM scrim.team_members WHERE participant_id=501")
            .fetch_one(db.pool())
            .await
            .expect("committed participant team");
    assert_eq!(
        saved_team, 11,
        "Discord-Fehler darf den DB-Stand nicht zurückrollen"
    );

    let (retry_status, retry) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:patch-participant:retry", "123456789"),
        Method::PATCH,
        "/internal/turnier/v1/scrims/participants/501",
        Some(body),
    )
    .await;
    assert_eq!(retry_status, StatusCode::OK);
    assert_eq!(retry["discord_sync"]["ok"], false);

    let requests = requests.lock().expect("broker requests");
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request["role_id"] == 8010)
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request["role_id"] == 8011)
            .count(),
        2
    );

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn roster_operator_routes_apply_legacy_contract_and_database_effects() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_roster_fixture(db.pool()).await;
    let app = app_with_pool(db.pool().clone());
    let headers = coach_headers("roster:create", "123456789");

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::POST,
        "/internal/turnier/v1/scrims/teams",
        Some(json!({
            "name":"Alpha",
            "coach_discord_id":"123456789",
            "default_from":1200,
            "default_to":1320
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(created["id"].is_number());
    assert_eq!(created["name"], "Alpha");
    assert!(created.get("discord_sync").is_some());
    let team_id = created["id"].as_i64().expect("numeric team id") as i32;

    let (status, patched_team) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:patch-team", "123456789"),
        Method::PATCH,
        &format!("/internal/turnier/v1/scrims/teams/{team_id}"),
        Some(json!({"name":"Bravo","default_from":1260,"default_to":1380})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched_team["id"], team_id);
    assert_eq!(patched_team["name"], "Bravo");

    let (status, patched_coach) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:patch-coach", "123456789"),
        Method::PATCH,
        &format!("/internal/turnier/v1/scrims/teams/{team_id}"),
        Some(json!({"coach":"Alias"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched_coach["coach"], "Alias");

    let (status, cleared_coach) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:clear-coach", "123456789"),
        Method::PATCH,
        &format!("/internal/turnier/v1/scrims/teams/{team_id}"),
        Some(json!({"coach":null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(cleared_coach["coach"].is_null());

    let (status, participant) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:patch-participant", "123456789"),
        Method::PATCH,
        "/internal/turnier/v1/scrims/participants/501",
        Some(json!({
            "status":"assigned",
            "team_id":team_id,
            "is_captain":true,
            "rank":"Phantom",
            "roles":"Duo",
            "notes":"contract"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(participant["id"].is_number());
    assert_eq!(participant["id"], 501);
    assert_eq!(participant["team"]["id"], team_id);
    assert!(participant.get("discord_sync").is_some());

    let (status, cleared_participant) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:clear-participant-text", "123456789"),
        Method::PATCH,
        "/internal/turnier/v1/scrims/participants/501",
        Some(json!({
            "rank":null,
            "roles":null,
            "notes":null
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(cleared_participant["rank"].is_null());
    assert!(cleared_participant["roles"].is_null());
    assert!(cleared_participant["notes"].is_null());

    let (status, announcement) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:announce", "123456789"),
        Method::POST,
        &format!("/internal/turnier/v1/scrims/teams/{team_id}/announce"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(announcement["ok"].is_boolean());
    assert!(announcement.get("message_id").is_some());

    let (status, suggestion) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:suggest", "123456789"),
        Method::POST,
        &format!("/internal/turnier/v1/scrims/teams/{team_id}/suggest"),
        Some(json!({
            "window":{"day":"fri","from":1200,"to":1320},
            "size":6,
            "pool":"players"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(suggestion["team"]["id"], team_id);
    assert_eq!(suggestion["requested_size"], 6);
    assert!(suggestion["candidates"][0]["participant_id"].is_number());

    let (status, substitute) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:substitute", "123456789"),
        Method::POST,
        &format!("/internal/turnier/v1/scrims/teams/{team_id}/substitute"),
        Some(json!({
            "participant_id":502,
            "window":{"day":"fri","from":1200,"to":1320}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(substitute["participant"]["id"], 502);
    assert!(substitute.get("discord_sync").is_some());
    assert!(substitute.get("dm").is_some());

    let (status, resync) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:resync", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/participants/502/resync-discord",
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(resync.get("discord_sync").is_some());

    let team: (String, Option<i32>, Option<i32>) =
        sqlx::query_as("SELECT name, default_from, default_to FROM scrim.teams WHERE id=$1")
            .bind(team_id)
            .fetch_one(db.pool())
            .await
            .expect("mutated team");
    assert_eq!(team, ("Bravo".to_string(), Some(1260), Some(1380)));
    let assigned: (String, Option<String>, Option<String>, Option<String>, bool) = sqlx::query_as(
        "SELECT p.status, p.rank, p.roles, p.notes, tm.is_captain \
         FROM scrim.participants p \
         JOIN scrim.team_members tm ON tm.participant_id=p.id \
         WHERE p.id=501 AND tm.team_id=$1",
    )
    .bind(team_id)
    .fetch_one(db.pool())
    .await
    .expect("assigned participant");
    assert_eq!(assigned, ("assigned".to_string(), None, None, None, true));
    let substitute_until: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT substitute_until FROM scrim.team_members \
         WHERE team_id=$1 AND participant_id=502",
    )
    .bind(team_id)
    .fetch_one(db.pool())
    .await
    .expect("substitute membership");
    assert!(substitute_until.is_some());

    let team_count_before_replay: i64 = sqlx::query_scalar("SELECT count(*) FROM scrim.teams")
        .fetch_one(db.pool())
        .await
        .expect("team count before replay");
    let (status, replayed_create) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::POST,
        "/internal/turnier/v1/scrims/teams",
        Some(json!({
            "name":"Alpha",
            "coach_discord_id":"123456789",
            "default_from":1200,
            "default_to":1320
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed_create["id"], team_id);
    let team_count_after_replay: i64 = sqlx::query_scalar("SELECT count(*) FROM scrim.teams")
        .fetch_one(db.pool())
        .await
        .expect("team count after replay");
    assert_eq!(team_count_after_replay, team_count_before_replay);

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let (status, replayed_substitute) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("roster:substitute", "123456789"),
        Method::POST,
        &format!("/internal/turnier/v1/scrims/teams/{team_id}/substitute"),
        Some(json!({
            "participant_id":502,
            "window":{"day":"fri","from":1200,"to":1320}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed_substitute["participant"]["id"], 502);
    let replayed_until: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT substitute_until FROM scrim.team_members \
         WHERE team_id=$1 AND participant_id=502",
    )
    .bind(team_id)
    .fetch_one(db.pool())
    .await
    .expect("replayed substitute membership");
    assert_eq!(replayed_until, substitute_until);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn roster_operator_routes_validate_each_contract() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_roster_fixture(db.pool()).await;
    let app = app_with_pool(db.pool().clone());
    let cases = [
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams",
            json!({"name":""}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/teams/10",
            json!({"name":""}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/participants/501",
            json!({"team_id":"not-an-id"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/10/announce",
            json!({"note":"x".repeat(501)}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/10/suggest",
            json!({"window":{"day":"fri","from":1320,"to":1200}}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/10/substitute",
            json!({"participant_id":501,"window":{"day":"fri","from":1200,"to":1320}}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/participants/not-an-id/resync-discord",
            json!({}),
            StatusCode::BAD_REQUEST,
        ),
    ];
    for (index, (method, route, body, expected)) in cases.into_iter().enumerate() {
        let key = format!("roster:validation:{index}");
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers(&key, "123456789"),
            method,
            route,
            Some(body),
        )
        .await;
        assert_eq!(status, expected, "{route}");
    }
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn mutation_idempotency_key_accepts_128_bytes_and_rejects_129() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_roster_fixture(db.pool()).await;
    let app = app_with_pool(db.pool().clone());
    let valid_key = format!("{}:{}", "a".repeat(32), "b".repeat(95));
    let too_long_key = format!("{}:{}", "a".repeat(32), "b".repeat(96));
    assert_eq!(valid_key.len(), 128);
    assert_eq!(too_long_key.len(), 129);

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers(&valid_key, "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/teams/10/suggest",
        Some(json!({
            "window": {"day": "fri", "from": 1200, "to": 1320},
            "size": 1,
            "pool": "players"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers(&too_long_key, "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/teams/10/suggest",
        Some(json!({
            "window": {"day": "fri", "from": 1200, "to": 1320},
            "size": 1,
            "pool": "players"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn roster_team_patch_revalidates_the_window_after_locking() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_roster_fixture(db.pool()).await;
    sqlx::query("UPDATE scrim.teams SET default_from=1200, default_to=1380 WHERE id=10")
        .execute(db.pool())
        .await
        .expect("initial team window");
    let app = app_with_pool(db.pool().clone());

    let mut blocker = db.pool().begin().await.expect("blocker transaction");
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(0x4451_0008_0004_0001_i64)
        .execute(&mut *blocker)
        .await
        .expect("block roster mutations");

    let first_app = app.clone();
    let first = tokio::spawn(async move {
        send(
            &first_app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("roster:race:from", "123456789"),
            Method::PATCH,
            "/internal/turnier/v1/scrims/teams/10",
            Some(json!({"default_from":1350})),
        )
        .await
        .0
    });
    let second_app = app.clone();
    let second = tokio::spawn(async move {
        send(
            &second_app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("roster:race:to", "123456789"),
            Method::PATCH,
            "/internal/turnier/v1/scrims/teams/10",
            Some(json!({"default_to":1300})),
        )
        .await
        .0
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    blocker.rollback().await.expect("release roster mutations");
    let statuses = [
        first.await.expect("first patch"),
        second.await.expect("second patch"),
    ];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::BAD_REQUEST)
            .count(),
        1
    );
    let (from, to): (Option<i32>, Option<i32>) =
        sqlx::query_as("SELECT default_from, default_to FROM scrim.teams WHERE id=10")
            .fetch_one(db.pool())
            .await
            .expect("final team window");
    assert!(from.is_some_and(|from| to.is_some_and(|to| from < to)));
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn roster_operator_routes_keep_boundary_and_operator_authorization() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_roster_fixture(db.pool()).await;
    let app = app_with_pool(db.pool().clone());
    let cases = [
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams",
            json!({"name":"X"}),
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/teams/10",
            json!({}),
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/participants/501",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/10/announce",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/10/suggest",
            json!({}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/teams/10/substitute",
            json!({"participant_id":502,"window":{"day":"fri","from":1200,"to":1320}}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/participants/501/resync-discord",
            json!({}),
        ),
    ];
    for (index, (method, route, body)) in cases.into_iter().enumerate() {
        let key = format!("roster:auth:{index}");
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers(&key, "987654321"),
            method.clone(),
            route,
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{route}");

        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            coach_headers(&key, "987654321"),
            method,
            route,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{route}");
    }
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn operator_request_routes_keep_boundary_and_operator_authorization() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let app = app_with_pool(db.pool().clone());
    let routes = [
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/match-requests/1",
            Some(json!({"status":"cancelled"})),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/1/reminders",
            Some(json!({})),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/1/status-publications",
            Some(json!({})),
        ),
        (
            Method::GET,
            "/internal/turnier/v1/scrims/replacement-needs/1/candidates",
            None,
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/replacement-needs/1/requests",
            Some(json!({"participant_id":"1"})),
        ),
    ];
    for (index, (method, route, body)) in routes.into_iter().enumerate() {
        let headers = TestHeaders {
            token: Some("internal-token"),
            request_id: Some("request:operator_auth"),
            idempotency_key: Some("operator_auth:route"),
            actor_id: Some("987654321"),
            actor_name: Some("Not Coach"),
        };
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
            headers,
            method.clone(),
            route,
            body.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "boundary route {index}");

        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            headers,
            method,
            route,
            body,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "operator route {index}");
    }

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("request:replacement_boundary"),
            idempotency_key: Some("replacement_boundary:route"),
            actor_id: Some("987654321"),
            actor_name: Some("Candidate"),
        },
        Method::PATCH,
        "/internal/turnier/v1/scrims/replacement-requests/1",
        Some(json!({"action":"accept"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "replacement boundary");
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn match_request_patch_reminder_and_publication_persist_and_validate() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_operator_request_fixture(db.pool()).await;
    let broker = Router::new().route(
        "/internal/master/v1/discord/send-message",
        post(accept_broker),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match_request_patch:91", "123456789"),
        Method::PATCH,
        "/internal/turnier/v1/scrims/match-requests/91",
        Some(json!({"status":"cancelled","note":"Platzhalter"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    let patched: (String, Option<String>) =
        sqlx::query_as("SELECT status, override_reason FROM scrim.match_requests WHERE id=91")
            .fetch_one(db.pool())
            .await
            .expect("patched request");
    assert_eq!(
        patched,
        ("cancelled".to_string(), Some("Platzhalter".to_string()))
    );

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match_request_reminder_custom:92", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/92/reminders",
        Some(json!({"message":"Platzhalter"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let reminder_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.match_request_reminders WHERE request_id=92",
    )
    .fetch_one(db.pool())
    .await
    .expect("reminder count");
    assert_eq!(reminder_count, 0);

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match_request_reminder:92", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/92/reminders",
        Some(json!({"template":"frist_bald"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    let reminder: (String, i32, Vec<i32>, Vec<i64>, Option<i64>, String) = sqlx::query_as(
        "SELECT template, missing_count, target_participant_ids, target_discord_user_ids, \
                target_role_id, target_kind \
           FROM scrim.match_request_reminders WHERE request_id=92",
    )
    .fetch_one(db.pool())
    .await
    .expect("reminder");
    assert_eq!(
        reminder,
        (
            "frist_bald".to_string(),
            1,
            vec![101],
            vec![1001],
            None,
            "members".to_string()
        )
    );
    let reminder_command_result: Value = sqlx::query_scalar(
        "SELECT result_payload \
           FROM scrim.command_receipts \
          WHERE command_scope='match_request_reminders' \
            AND idempotency_key='match_request_reminder:92'",
    )
    .fetch_one(db.pool())
    .await
    .expect("reminder command result");
    assert!(
        reminder_command_result.get("discord").is_none(),
        "approved reminders must be dispatched only by the reminder worker"
    );

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("status_publication:93", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/93/status-publications",
        Some(json!({"channel_id":"7001","message":"Platzhalter"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    let publication: (String, String, Value) = sqlx::query_as(
        "SELECT decision, status_kind, payload \
           FROM scrim.status_publication_approvals WHERE target_kind='match_request' AND target_id='93'",
    )
    .fetch_one(db.pool())
    .await
    .expect("status publication");
    assert_eq!(publication.0, "approved");
    assert_eq!(publication.1, "match_status");
    assert_eq!(publication.2["message"], "Platzhalter");

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match_request_patch:invalid", "123456789"),
        Method::PATCH,
        "/internal/turnier/v1/scrims/match-requests/92",
        Some(json!({"status":"unknown"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match_request_reminder:invalid", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/92/reminders",
        Some(json!({"template":"Platzhalter"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "Invalid reminder template");
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("status_publication:invalid", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/93/status-publications",
        Some(json!({"channel_id":"not-an-id"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn reminder_dispatch_is_persisted_before_discord_call() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_operator_request_fixture(db.pool()).await;

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_committed_dispatch_and_fail),
        )
        .with_state((requests.clone(), db.pool().clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let state = state_with_pool_broker_and_signup_role(db.pool().clone(), Some(&broker_url), None);
    let app = build_router(state.clone());

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("runtime:reminder:persisted-before-send", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/92/reminders",
        Some(json!({"template":"frist_bald"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer");
    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational retry");

    assert_eq!(requests.lock().expect("broker requests").len(), 1);
    let dispatch: (String, String) = sqlx::query_as(
        "SELECT reminder.status, effect.state \
           FROM scrim.match_request_reminders reminder \
           JOIN scrim.match_request_reminder_effects link ON link.reminder_id=reminder.id \
           JOIN scrim.outbox_effects effect ON effect.id=link.outbox_effect_id \
          WHERE reminder.request_id=92",
    )
    .fetch_one(db.pool())
    .await
    .expect("persisted reminder dispatch state");
    assert_eq!(dispatch, ("uncertain".to_string(), "uncertain".to_string()));

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn abandoned_reminder_dispatch_is_not_sent_again() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_operator_request_fixture(db.pool()).await;

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_accept_message),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let state = state_with_pool_broker_and_signup_role(db.pool().clone(), Some(&broker_url), None);
    let app = build_router(state.clone());

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("runtime:reminder:dispatch-once", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/92/reminders",
        Some(json!({"template":"frist_bald"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let reminder_id: i64 =
        sqlx::query_scalar("SELECT id FROM scrim.match_request_reminders WHERE request_id=92")
            .fetch_one(db.pool())
            .await
            .expect("reminder id");
    sqlx::query(
        "UPDATE scrim.match_request_reminders \
            SET status='posting', updated_at=now() - interval '16 minutes' \
          WHERE id=$1",
    )
    .bind(reminder_id)
    .execute(db.pool())
    .await
    .expect("abandoned reminder dispatch");
    let payload = json!({
        "channel_id": 7001,
        "content": "Erinnerung für Team 1: Die Frist läuft bald ab.",
        "idempotency_key": format!("scrim-reminder:{reminder_id}")
    });
    let payload_hash =
        Sha256::digest(serde_json::to_vec(&payload).expect("reminder payload JSON")).to_vec();
    let effect_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.outbox_effects(\
             effect_type, idempotency_key, payload_hash, payload, state, remote_system, \
             lease_owner, lease_until, attempts\
         ) VALUES (\
             'discord_scrim_effect', $1, $2, $3, 'leased', 'discord', \
             'turnier_bot:scrim_reminder', now() - interval '15 minutes', 1\
         ) RETURNING id",
    )
    .bind(format!("scrim_reminder:{reminder_id}"))
    .bind(payload_hash)
    .bind(&payload)
    .fetch_one(db.pool())
    .await
    .expect("persisted reminder dispatch");
    sqlx::query(
        "INSERT INTO scrim.match_request_reminder_effects(reminder_id, outbox_effect_id) \
         VALUES ($1, $2)",
    )
    .bind(reminder_id)
    .bind(effect_id)
    .execute(db.pool())
    .await
    .expect("reminder effect link");

    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer");

    assert!(requests.lock().expect("broker requests").is_empty());
    let dispatch: (String, String) = sqlx::query_as(
        "SELECT reminder.status, effect.state \
           FROM scrim.match_request_reminders reminder \
           JOIN scrim.match_request_reminder_effects link ON link.reminder_id=reminder.id \
           JOIN scrim.outbox_effects effect ON effect.id=link.outbox_effect_id \
          WHERE reminder.id=$1",
    )
    .bind(reminder_id)
    .fetch_one(db.pool())
    .await
    .expect("persisted reminder dispatch state");
    assert_eq!(dispatch, ("uncertain".to_string(), "uncertain".to_string()));

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn turniere_runtime_consumes_result_fetches_and_scrim_reminders() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_operator_request_fixture(db.pool()).await;

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_accept_message),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });

    let steam_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("steam bridge");
    sqlx::query(
        "CREATE TABLE steam_tasks (\
             id INTEGER PRIMARY KEY AUTOINCREMENT, type TEXT, payload TEXT, status TEXT, \
             result TEXT, error TEXT, created_at INTEGER, updated_at INTEGER, \
             started_at INTEGER, finished_at INTEGER, attempts INTEGER DEFAULT 0\
         )",
    )
    .execute(&steam_pool)
    .await
    .expect("steam task schema");

    let mut state =
        state_with_pool_broker_and_signup_role(db.pool().clone(), Some(&broker_url), None);
    state.match_manager = Arc::new(MatchManager::new(
        db.pool().clone(),
        None,
        Some(SteamBridge::from_pool(steam_pool.clone())),
        &state.config,
    ));
    let app = build_router(state.clone());

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("runtime:reminder:92", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/92/reminders",
        Some(json!({"template":"frist_bald"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    sqlx::query(
        "UPDATE scrim.match_request_reminders \
            SET status='posting', updated_at=now() - interval '16 minutes' \
          WHERE request_id=92",
    )
    .execute(db.pool())
    .await
    .expect("abandoned reminder claim");
    let reminder_id: i64 =
        sqlx::query_scalar("SELECT id FROM scrim.match_request_reminders WHERE request_id=92")
            .fetch_one(db.pool())
            .await
            .expect("reminder id");
    let persisted_reminder_payload = json!({
        "channel_id": 201,
        "content": "Bereits versendeter Reminder mit unverändertem Inhalt.",
        "idempotency_key": format!("scrim-reminder:{reminder_id}")
    });
    let payload_hash = Sha256::digest(
        serde_json::to_vec(&persisted_reminder_payload).expect("reminder payload JSON"),
    )
    .to_vec();
    let effect_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.outbox_effects(\
             effect_type, idempotency_key, payload_hash, payload, state, remote_system, \
             lease_owner, lease_until, attempts\
         ) VALUES (\
             'discord_scrim_effect', $1, $2, $3, 'leased', 'discord', \
             'turnier_bot:scrim_reminder', now() - interval '15 minutes', 1\
         ) \
         RETURNING id",
    )
    .bind(format!("scrim_reminder:{reminder_id}"))
    .bind(payload_hash)
    .bind(&persisted_reminder_payload)
    .fetch_one(db.pool())
    .await
    .expect("persisted reminder effect");
    sqlx::query(
        "INSERT INTO scrim.match_request_reminder_effects(reminder_id, outbox_effect_id) \
         VALUES ($1, $2)",
    )
    .bind(reminder_id)
    .bind(effect_id)
    .execute(db.pool())
    .await
    .expect("reminder effect link");
    sqlx::query(
        "INSERT INTO scrim.match_request_responses(\
             request_id, team_id, participant_id, discord_user_id, slot_index, \
             response, source, responded_at, updated_at\
         ) VALUES (92, 1, 101, '1001', 0, 'available', 'button', now(), now())",
    )
    .execute(db.pool())
    .await
    .expect("late reminder response");
    let uncertain_reminder_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.match_request_reminders(\
             request_id, team_id, template, target_kind, target_participant_ids, \
             target_discord_user_ids, missing_count, approved_by_user_id, \
             approved_by_display_name, status, discord_channel_id, source_message_id, \
             updated_at\
         ) VALUES (\
             92, 1, 'frist_bald', 'members', ARRAY[101], ARRAY[1001]::BIGINT[], 1, \
             '123456789', 'Coach', 'failed', 201, 20001, now() - interval '16 minutes'\
         ) RETURNING id",
    )
    .fetch_one(db.pool())
    .await
    .expect("legacy uncertain reminder");
    let uncertain_payload = json!({
        "schema_version": "discord-scrim-effect:v1",
        "message_kind": "match_request_reminder",
        "operation": "post",
        "channel_id": "201",
        "body": {"content": "Unklare Legacy-Zustellung"}
    });
    let uncertain_hash =
        Sha256::digest(serde_json::to_vec(&uncertain_payload).expect("uncertain payload JSON"))
            .to_vec();
    let uncertain_effect_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.outbox_effects(\
             effect_type, idempotency_key, payload_hash, payload, state, remote_system\
         ) VALUES ('discord_scrim_effect', $1, $2, $3, 'uncertain', 'discord') \
         RETURNING id",
    )
    .bind(format!("scrim_reminder_uncertain:{uncertain_reminder_id}"))
    .bind(uncertain_hash)
    .bind(&uncertain_payload)
    .fetch_one(db.pool())
    .await
    .expect("legacy uncertain reminder effect");
    sqlx::query(
        "INSERT INTO scrim.match_request_reminder_effects(reminder_id, outbox_effect_id) \
         VALUES ($1, $2)",
    )
    .bind(uncertain_reminder_id)
    .bind(uncertain_effect_id)
    .execute(db.pool())
    .await
    .expect("legacy uncertain reminder link");

    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.matches(
            id, team_a_id, team_b_id, status, lobby_state, steam_match_id, created_at, updated_at
        ) VALUES (
            94, 1, 2, 'scheduled', 'result_fetching', 9007199254740994,
            now(), now() - interval '16 minutes'
        );
        INSERT INTO scrim.match_result_refs(
            match_id, steam_match_id, source_user_id, source_display_name,
            fetch_status, entered_at, updated_at
        ) VALUES
            (
                94, 9007199254740994, '123456789', 'Coach', 'fetching',
                now(), now() - interval '16 minutes'
            ),
            (94, 9007199254740995, '123456789', 'Coach', 'pending', now(), now()),
            (94, 9007199254740996, '123456789', 'Coach', 'pending', now(), now()),
            (94, 9007199254740997, '123456789', 'Coach', 'pending', now(), now());
        "#,
    )
    .execute(db.pool())
    .await
    .expect("result fetch seed");

    let steam_worker_pool = steam_pool.clone();
    let steam_task = tokio::spawn(async move {
        for outcome in [
            Some((0, Some("9007199254740990"), None)),
            Some((0, Some("9007199254740996"), Some("9007199254740991"))),
            Some((1, None, Some("9007199254740997"))),
            None,
            Some((0, Some("9007199254741096"), None)),
        ] {
            loop {
                if let Some(id) = sqlx::query_scalar::<_, i64>(
                    "SELECT id FROM steam_tasks WHERE status='PENDING'",
                )
                .fetch_optional(&steam_worker_pool)
                .await
                .expect("pending steam task")
                {
                    if let Some((winning_team, returned_match_id, deadlock_match_id)) = outcome {
                        sqlx::query(
                            "UPDATE steam_tasks \
                                SET status='DONE', result=$2, updated_at=1, finished_at=1 \
                              WHERE id=$1",
                        )
                        .bind(id)
                        .bind(
                            json!({
                                "success": true,
                                "match_id": returned_match_id,
                                "deadlock_match_id": deadlock_match_id,
                                "winning_team": winning_team,
                                "duration_s": 1800
                            })
                            .to_string(),
                        )
                        .execute(&steam_worker_pool)
                        .await
                        .expect("complete steam task");
                    } else {
                        sqlx::query(
                            "UPDATE steam_tasks \
                                SET status='FAILED', error='Steam nicht erreichbar', \
                                    updated_at=1, finished_at=1 \
                              WHERE id=$1",
                        )
                        .bind(id)
                        .execute(&steam_worker_pool)
                        .await
                        .expect("fail steam task");
                    }
                    break;
                }
                tokio::task::yield_now().await;
            }
        }
    });

    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer");
    let after_first_fetch: (String, i64, i64) = sqlx::query_as(
        "SELECT lobby_state, \
                (SELECT COUNT(*) FROM scrim.match_result_refs \
                  WHERE match_id=94 AND fetch_status='fetched'), \
                (SELECT COUNT(*) FROM scrim.match_result_refs \
                  WHERE match_id=94 AND fetch_status='failed') \
           FROM scrim.matches WHERE id=94",
    )
    .fetch_one(db.pool())
    .await
    .expect("first consumed result");
    assert_eq!(after_first_fetch, ("result_requested".to_string(), 0, 1));

    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer conflicting result");
    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer fallback result");
    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer failed final result");
    sqlx::query(
        "INSERT INTO scrim.matches(\
             id, team_a_id, team_b_id, status, lobby_state, steam_match_id, created_at, updated_at\
         ) VALUES (\
             96, 1, 2, 'scheduled', 'result_requested', 9007199254741096, now(), now()\
         )",
    )
    .execute(db.pool())
    .await
    .expect("result fetch without ref");
    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer without result ref");
    steam_task.await.expect("steam worker");

    let result: (String, Option<i32>, Option<Value>) = sqlx::query_as(
        "SELECT lobby_state, winner_team_id, result_json FROM scrim.matches WHERE id=94",
    )
    .fetch_one(db.pool())
    .await
    .expect("consumed result");
    assert_eq!(result.0, "finished");
    assert_eq!(result.1, None);
    assert_eq!(result.2, None);
    let result_without_ref: (String, Option<i32>, Option<Value>) = sqlx::query_as(
        "SELECT lobby_state, winner_team_id, result_json FROM scrim.matches WHERE id=96",
    )
    .fetch_one(db.pool())
    .await
    .expect("consumed result without ref");
    assert_eq!(result_without_ref.0, "finished");
    assert_eq!(result_without_ref.1, Some(1));
    let result_json = result_without_ref.2.expect("stored result without ref");
    assert_eq!(result_json["match_id"], "9007199254741096");
    assert_eq!(result_json["winning_team"], 0);
    let result_refs: Vec<(String, Option<i32>, String)> = sqlx::query_as(
        "SELECT fetch_status, winner_team_id, validation_status \
           FROM scrim.match_result_refs WHERE match_id=94 ORDER BY steam_match_id",
    )
    .fetch_all(db.pool())
    .await
    .expect("consumed result refs");
    assert_eq!(
        result_refs,
        vec![
            ("failed".to_string(), None, "unvalidated".to_string()),
            ("failed".to_string(), None, "unvalidated".to_string()),
            ("failed".to_string(), None, "unvalidated".to_string()),
            ("fetched".to_string(), Some(2), "valid".to_string()),
        ]
    );
    let mismatched_result: (Option<String>, Option<Value>) = sqlx::query_as(
        "SELECT last_error, raw_result_json \
           FROM scrim.match_result_refs \
          WHERE match_id=94 AND steam_match_id=9007199254740995",
    )
    .fetch_one(db.pool())
    .await
    .expect("mismatched result ref");
    assert_eq!(
        mismatched_result.0.as_deref(),
        Some(
            "Steam-Ergebnis gehört zu Match 9007199254740990, erwartet wurde Match 9007199254740995."
        )
    );
    assert_eq!(mismatched_result.1, None);
    let conflicting_result: (Option<String>, Option<Value>) = sqlx::query_as(
        "SELECT last_error, raw_result_json \
           FROM scrim.match_result_refs \
          WHERE match_id=94 AND steam_match_id=9007199254740996",
    )
    .fetch_one(db.pool())
    .await
    .expect("conflicting result ref");
    assert_eq!(
        conflicting_result.0.as_deref(),
        Some(
            "Steam-Ergebnis gehört zu Match 9007199254740991, erwartet wurde Match 9007199254740996."
        )
    );
    assert_eq!(conflicting_result.1, None);

    let reminder_status: String =
        sqlx::query_scalar("SELECT status FROM scrim.match_request_reminders WHERE id=$1")
            .bind(reminder_id)
            .fetch_one(db.pool())
            .await
            .expect("consumed reminder");
    assert_eq!(reminder_status, "uncertain");
    let reminder_effect_status: String =
        sqlx::query_scalar("SELECT state FROM scrim.outbox_effects WHERE id=$1")
            .bind(effect_id)
            .fetch_one(db.pool())
            .await
            .expect("uncertain reminder effect");
    assert_eq!(reminder_effect_status, "uncertain");
    let uncertain_status: String =
        sqlx::query_scalar("SELECT status FROM scrim.match_request_reminders WHERE id=$1")
            .bind(uncertain_reminder_id)
            .fetch_one(db.pool())
            .await
            .expect("uncertain reminder status");
    assert_eq!(uncertain_status, "uncertain");
    let confirmed_receipts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.effect_receipts \
          WHERE outbox_effect_id=$1 AND status='confirmed'",
    )
    .bind(effect_id)
    .fetch_one(db.pool())
    .await
    .expect("reminder delivery receipt");
    assert_eq!(confirmed_receipts, 0);
    let payloads = requests.lock().expect("broker requests");
    assert!(!payloads
        .iter()
        .any(|payload| payload == &persisted_reminder_payload));
    assert!(!payloads.iter().any(|payload| payload == &uncertain_payload));
    assert!(!payloads.iter().any(|payload| {
        payload["content"]
            .as_str()
            .is_some_and(|content| content.contains("Scrim beendet. Das Ergebnis ist eingetragen."))
    }));
    let result_idempotency_keys = payloads
        .iter()
        .filter_map(|payload| {
            payload["idempotency_key"]
                .as_str()
                .filter(|key| key.starts_with("scrim-result:"))
        })
        .collect::<Vec<_>>();
    assert!(result_idempotency_keys.is_empty());

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn scrim_reminder_requires_matching_discord_channel_receipt() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_operator_request_fixture(db.pool()).await;

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_mismatch_message_channel),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let state = state_with_pool_broker_and_signup_role(db.pool().clone(), Some(&broker_url), None);
    let app = build_router(state.clone());

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("runtime:reminder:channel-correlation", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/match-requests/92/reminders",
        Some(json!({"template":"frist_bald"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer");

    let delivery: (String, String, i64) = sqlx::query_as(
        "SELECT r.status, effect.state, \
                (SELECT COUNT(*) FROM scrim.effect_receipts receipt \
                  WHERE receipt.outbox_effect_id=effect.id AND receipt.status='confirmed') \
           FROM scrim.match_request_reminders r \
           JOIN scrim.match_request_reminder_effects link ON link.reminder_id=r.id \
           JOIN scrim.outbox_effects effect ON effect.id=link.outbox_effect_id \
          WHERE r.request_id=92",
    )
    .fetch_one(db.pool())
    .await
    .expect("reminder delivery status");
    assert_eq!(
        delivery,
        ("uncertain".to_string(), "uncertain".to_string(), 0)
    );
    assert_eq!(requests.lock().expect("broker requests").len(), 1);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn empty_steam_result_does_not_finish_scrim_match() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_teams(db.pool(), &[1, 2]).await;
    sqlx::query(
        "INSERT INTO scrim.matches(\
             id, team_a_id, team_b_id, status, lobby_state, party_id, created_at, updated_at\
         ) VALUES (97, 1, 2, 'scheduled', 'result_requested', 'party-empty', now(), now())",
    )
    .execute(db.pool())
    .await
    .expect("party result fetch seed");

    let steam_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("steam bridge");
    sqlx::query(
        "CREATE TABLE steam_tasks (\
             id INTEGER PRIMARY KEY AUTOINCREMENT, type TEXT, payload TEXT, status TEXT, \
             result TEXT, error TEXT, created_at INTEGER, updated_at INTEGER, \
             started_at INTEGER, finished_at INTEGER, attempts INTEGER DEFAULT 0\
         )",
    )
    .execute(&steam_pool)
    .await
    .expect("steam task schema");
    let mut state = state_with_pool_broker_and_signup_role(db.pool().clone(), None, None);
    state.match_manager = Arc::new(MatchManager::new(
        db.pool().clone(),
        None,
        Some(SteamBridge::from_pool(steam_pool.clone())),
        &state.config,
    ));
    let steam_task = tokio::spawn(async move {
        loop {
            if let Some(id) =
                sqlx::query_scalar::<_, i64>("SELECT id FROM steam_tasks WHERE status='PENDING'")
                    .fetch_optional(&steam_pool)
                    .await
                    .expect("pending steam task")
            {
                sqlx::query(
                    "UPDATE steam_tasks \
                        SET status='DONE', result='{}', updated_at=1, finished_at=1 \
                      WHERE id=$1",
                )
                .bind(id)
                .execute(&steam_pool)
                .await
                .expect("complete empty steam task");
                break;
            }
            tokio::task::yield_now().await;
        }
    });

    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer");
    steam_task.await.expect("steam worker");

    let result: (String, Option<i32>, Option<Value>) = sqlx::query_as(
        "SELECT lobby_state, winner_team_id, result_json FROM scrim.matches WHERE id=97",
    )
    .fetch_one(db.pool())
    .await
    .expect("empty result state");
    assert_eq!(result, ("result_failed".to_string(), None, None));
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn cancelled_match_discards_in_flight_steam_result() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_teams(db.pool(), &[1, 2]).await;
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.matches(
            id, team_a_id, team_b_id, status, lobby_state, steam_match_id, created_at, updated_at
        ) VALUES (
            98, 1, 2, 'scheduled', 'result_requested', 9007199254741098, now(), now()
        );
        INSERT INTO scrim.match_result_refs(
            match_id, steam_match_id, source_user_id, source_display_name,
            fetch_status, entered_at, updated_at
        ) VALUES (
            98, 9007199254741098, '123456789', 'Coach', 'pending', now(), now()
        );
        "#,
    )
    .execute(db.pool())
    .await
    .expect("result fetch seed");

    let steam_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("steam bridge");
    sqlx::query(
        "CREATE TABLE steam_tasks (\
             id INTEGER PRIMARY KEY AUTOINCREMENT, type TEXT, payload TEXT, status TEXT, \
             result TEXT, error TEXT, created_at INTEGER, updated_at INTEGER, \
             started_at INTEGER, finished_at INTEGER, attempts INTEGER DEFAULT 0\
         )",
    )
    .execute(&steam_pool)
    .await
    .expect("steam task schema");
    let mut state = state_with_pool_broker_and_signup_role(db.pool().clone(), None, None);
    state.match_manager = Arc::new(MatchManager::new(
        db.pool().clone(),
        None,
        Some(SteamBridge::from_pool(steam_pool.clone())),
        &state.config,
    ));
    let central_pool = db.pool().clone();
    let steam_task = tokio::spawn(async move {
        loop {
            if let Some(id) =
                sqlx::query_scalar::<_, i64>("SELECT id FROM steam_tasks WHERE status='PENDING'")
                    .fetch_optional(&steam_pool)
                    .await
                    .expect("pending steam task")
            {
                sqlx::query(
                    "UPDATE scrim.matches \
                        SET status='cancelled', lobby_state='cancelled', updated_at=now() \
                      WHERE id=98",
                )
                .execute(&central_pool)
                .await
                .expect("cancel claimed match");
                sqlx::query(
                    "UPDATE steam_tasks \
                        SET status='DONE', result=$2, updated_at=1, finished_at=1 \
                      WHERE id=$1",
                )
                .bind(id)
                .bind(
                    json!({
                        "success": true,
                        "match_id": "9007199254741098",
                        "winning_team": 0
                    })
                    .to_string(),
                )
                .execute(&steam_pool)
                .await
                .expect("complete steam task");
                break;
            }
            tokio::task::yield_now().await;
        }
    });

    turnier_api::internal_scrims::process_scrim_operational_once(&state)
        .await
        .expect("turniere operational consumer");
    steam_task.await.expect("steam worker");

    let result: (
        String,
        String,
        Option<i32>,
        Option<Value>,
        String,
        Option<Value>,
    ) = sqlx::query_as(
        "SELECT m.status, m.lobby_state, m.winner_team_id, m.result_json, \
                    result_ref.fetch_status, result_ref.raw_result_json \
               FROM scrim.matches m \
               JOIN scrim.match_result_refs result_ref ON result_ref.match_id=m.id \
              WHERE m.id=98",
    )
    .fetch_one(db.pool())
    .await
    .expect("cancelled result state");
    assert_eq!(
        result,
        (
            "cancelled".to_string(),
            "cancelled".to_string(),
            None,
            None,
            "failed".to_string(),
            None,
        )
    );
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn result_selection_is_saved_before_discord_delivery() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    sqlx::query(
        "UPDATE scrim.teams \
            SET discord_channel_id=CASE id WHEN 1 THEN 1510101 ELSE 1510102 END \
          WHERE id IN (1, 2)",
    )
    .execute(db.pool())
    .await
    .expect("team channel ids");
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.matches(id, team_a_id, team_b_id, status, lobby_state, created_at)
        VALUES (99, 1, 2, 'scheduled', 'finished', now());
        INSERT INTO scrim.match_result_refs(
            match_id, steam_match_id, source_user_id, source_display_name,
            fetch_status, winner_team_id, normalized_result_json,
            validation_status, fetched_at, entered_at, updated_at
        ) VALUES (
            99, 9007199254741099, '123456789', 'Coach', 'fetched', 2,
            '{"winner_team_id":2}'::jsonb, 'valid', now(), now(), now()
        );
        "#,
    )
    .execute(db.pool())
    .await
    .expect("selectable result ref");
    let result_ref_id: i64 =
        sqlx::query_scalar("SELECT id FROM scrim.match_result_refs WHERE match_id=99")
            .fetch_one(db.pool())
            .await
            .expect("result ref");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_fail_second_channel_once),
        )
        .with_state((requests.clone(), Some((db.pool().clone(), 99))));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));
    let route = format!("/internal/turnier/v1/scrims/matches/99/result-refs/{result_ref_id}");

    let (status, first_response) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:select:delivery-gate", "123456789"),
        Method::PATCH,
        &route,
        Some(json!({"message":"Steam-Ergebnis geprüft"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_GATEWAY,
        "first response: {first_response}"
    );
    let selection_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM scrim.match_result_selections WHERE match_id=99")
            .fetch_one(db.pool())
            .await
            .expect("selection count before delivery");
    assert_eq!(selection_count, 1);
    let partial_deliveries: Vec<(String, bool)> = sqlx::query_as(
        "SELECT effect.state, EXISTS(\
             SELECT 1 FROM scrim.effect_receipts receipt \
              WHERE receipt.outbox_effect_id=effect.id AND receipt.status='confirmed'\
         ) \
           FROM scrim.outbox_effects effect \
          WHERE effect.idempotency_key LIKE $1 \
          ORDER BY effect.id",
    )
    .bind(format!("scrim_result:99:{result_ref_id}:%"))
    .fetch_all(db.pool())
    .await
    .expect("partial result deliveries");
    assert_eq!(
        partial_deliveries,
        vec![
            ("delivered".to_string(), true),
            ("retry".to_string(), false),
        ]
    );

    let (status, selected) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:select:delivery-gate", "123456789"),
        Method::PATCH,
        &route,
        Some(json!({"message":"Steam-Ergebnis geprüft"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        selected["match"]["selected_result"]["result_ref_id"],
        result_ref_id.to_string()
    );
    let completed_deliveries: Vec<(String, bool)> = sqlx::query_as(
        "SELECT effect.state, EXISTS(\
             SELECT 1 FROM scrim.effect_receipts receipt \
              WHERE receipt.outbox_effect_id=effect.id AND receipt.status='confirmed'\
         ) \
           FROM scrim.outbox_effects effect \
          WHERE effect.idempotency_key LIKE $1 \
          ORDER BY effect.id",
    )
    .bind(format!("scrim_result:99:{result_ref_id}:%"))
    .fetch_all(db.pool())
    .await
    .expect("completed result deliveries");
    assert_eq!(
        completed_deliveries,
        vec![
            ("delivered".to_string(), true),
            ("delivered".to_string(), true),
        ]
    );

    let requests = requests.lock().expect("broker requests");
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests
            .iter()
            .filter(|payload| payload["channel_id"] == 1510101)
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|payload| payload["channel_id"] == 1510102)
            .count(),
        2
    );

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn selected_result_a_b_a_reselection_is_delivered() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    sqlx::query("UPDATE scrim.teams SET discord_channel_id = 700000 + id WHERE id IN (1, 2)")
        .execute(db.pool())
        .await
        .expect("team channel ids");
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.matches(id, team_a_id, team_b_id, status, lobby_state, created_at)
        VALUES (95, 1, 2, 'scheduled', 'finished', now());
        INSERT INTO scrim.match_result_refs(
            match_id, steam_match_id, source_user_id, source_display_name,
            fetch_status, winner_team_id, normalized_result_json,
            validation_status, fetched_at, entered_at, updated_at
        ) VALUES
            (
                95, 9007199254741001, '123456789', 'Coach', 'fetched', 1,
                '{"winner_team_id":1}'::jsonb, 'valid', now(), now(), now()
            ),
            (
                95, 9007199254741002, '123456789', 'Coach', 'fetched', 2,
                '{"winner_team_id":2}'::jsonb, 'valid', now(), now(), now()
            );
        "#,
    )
    .execute(db.pool())
    .await
    .expect("conflicting result refs");
    let replacement_ref_id: i64 = sqlx::query_scalar(
        "SELECT id FROM scrim.match_result_refs WHERE match_id=95 AND winner_team_id=1",
    )
    .fetch_one(db.pool())
    .await
    .expect("replacement result ref");
    let selected_ref_id: i64 = sqlx::query_scalar(
        "SELECT id FROM scrim.match_result_refs WHERE match_id=95 AND winner_team_id=2",
    )
    .fetch_one(db.pool())
    .await
    .expect("selected result ref");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_accept_message_under_selection_lock),
        )
        .with_state((requests.clone(), db.pool().clone(), 95));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let route = format!("/internal/turnier/v1/scrims/matches/95/result-refs/{selected_ref_id}");
    let mut selection_lock = db.pool().begin().await.expect("selection lock");
    sqlx::query("SELECT pg_advisory_xact_lock(20260727, 95)")
        .execute(&mut *selection_lock)
        .await
        .expect("hold selection lock");
    let task_app = app.clone();
    let task_route = route.clone();
    let pending = tokio::spawn(async move {
        send(
            &task_app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("match:select:canonical", "123456789"),
            Method::PATCH,
            &task_route,
            Some(json!({"message":"Steam-Ergebnis geprüft"})),
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        !pending.is_finished(),
        "result selection did not wait for the delivery lock"
    );
    selection_lock
        .commit()
        .await
        .expect("release selection lock");
    let (status, selected) = pending.await.expect("selection task");

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        selected["match"]["selected_result"]["result_ref_id"],
        selected_ref_id.to_string()
    );
    assert_eq!(selected["match"]["selected_result"]["winner_team_id"], "2");
    let persisted_ref_id: i64 = sqlx::query_scalar(
        "SELECT result_ref_id FROM scrim.match_result_selections WHERE match_id=95",
    )
    .fetch_one(db.pool())
    .await
    .expect("persisted selection");
    assert_eq!(persisted_ref_id, selected_ref_id);
    let selection_generation: i64 = sqlx::query_scalar(
        "SELECT id FROM scrim.match_result_selection_events \
          WHERE match_id=95 ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(db.pool())
    .await
    .expect("selection generation");

    {
        let payloads = requests.lock().expect("broker requests");
        assert_eq!(payloads.len(), 2);
        for (payload, channel_id) in payloads.iter().zip([700001, 700002]) {
            assert_eq!(payload["channel_id"], channel_id);
            assert_eq!(
                payload["content"],
                "Scrim beendet. Das Ergebnis ist eingetragen. Sieger: Route Team 2."
            );
            let key = payload["idempotency_key"]
                .as_str()
                .expect("idempotency key");
            let segments = key.split(':').collect::<Vec<_>>();
            assert_eq!(segments.len(), 5);
            assert_eq!(segments[3], selection_generation.to_string());
            assert_eq!(segments[4], channel_id.to_string());
        }
    }
    let stale_effect_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.outbox_effects(\
             effect_type, idempotency_key, payload_hash, payload, state, \
             next_attempt_at, attempts, remote_system\
         ) VALUES ('discord_scrim_effect', $1, $2, $3, 'retry', now(), 1, 'discord') \
         RETURNING id",
    )
    .bind(format!(
        "scrim_result:95:{selected_ref_id}:{selection_generation}:700003"
    ))
    .bind(vec![0_u8; 32])
    .bind(json!({}))
    .fetch_one(db.pool())
    .await
    .expect("stale selection effect");

    let replacement_route =
        format!("/internal/turnier/v1/scrims/matches/95/result-refs/{replacement_ref_id}");
    let (status, replacement) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:select:replacement", "123456789"),
        Method::PATCH,
        &replacement_route,
        Some(json!({"message":"Korrekturauswahl geprüft"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        replacement["match"]["selected_result"]["winner_team_id"],
        "1"
    );
    let stale_effect_state: String =
        sqlx::query_scalar("SELECT state FROM scrim.outbox_effects WHERE id=$1")
            .bind(stale_effect_id)
            .fetch_one(db.pool())
            .await
            .expect("stale selection effect state");
    assert_eq!(stale_effect_state, "cancelled");

    let (status, replayed) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:select:canonical", "123456789"),
        Method::PATCH,
        &route,
        Some(json!({"message":"Steam-Ergebnis geprüft"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed, selected);
    let persisted_ref_id: i64 = sqlx::query_scalar(
        "SELECT result_ref_id FROM scrim.match_result_selections WHERE match_id=95",
    )
    .fetch_one(db.pool())
    .await
    .expect("persisted replacement selection");
    assert_eq!(persisted_ref_id, replacement_ref_id);
    {
        let payloads = requests.lock().expect("broker requests");
        assert_eq!(payloads.len(), 4);
        assert!(payloads.iter().skip(2).all(|payload| {
            payload["content"]
                == "Scrim beendet. Das Ergebnis ist eingetragen. Sieger: Route Team 1."
        }));
    }

    let (status, reselected) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:select:canonical-again", "123456789"),
        Method::PATCH,
        &route,
        Some(json!({"message":"Erneute Korrekturauswahl geprüft"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        reselected["match"]["selected_result"]["winner_team_id"],
        "2"
    );
    let persisted_ref_id: i64 = sqlx::query_scalar(
        "SELECT result_ref_id FROM scrim.match_result_selections WHERE match_id=95",
    )
    .fetch_one(db.pool())
    .await
    .expect("persisted reselection");
    assert_eq!(persisted_ref_id, selected_ref_id);

    let payloads = requests.lock().expect("broker requests");
    assert_eq!(payloads.len(), 6);
    assert!(payloads.iter().skip(4).all(|payload| {
        payload["content"] == "Scrim beendet. Das Ergebnis ist eingetragen. Sieger: Route Team 2."
    }));
    assert_ne!(
        payloads[0]["idempotency_key"],
        payloads[4]["idempotency_key"]
    );
    assert_ne!(
        payloads[1]["idempotency_key"],
        payloads[5]["idempotency_key"]
    );

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn status_publication_dispatch_failure_is_retryable_without_rolling_back_state() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1, 2]).await;
    seed_operator_request_fixture(db.pool()).await;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_fail_broker),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    for _ in 0..2 {
        let (status, body) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("status_publication:broker_failure", "123456789"),
            Method::POST,
            "/internal/turnier/v1/scrims/match-requests/93/status-publications",
            Some(json!({"channel_id":"7001","message":"Test status"})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(
            body["detail"],
            "Gespeichert, aber die Discord-Nachricht ging nicht raus. Löse denselben Vorgang noch einmal aus, dann wird sie nachgereicht."
        );
    }

    let approval_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.status_publication_approvals \
          WHERE target_kind='match_request' AND target_id='93'",
    )
    .fetch_one(db.pool())
    .await
    .expect("persisted status publication");
    assert_eq!(approval_count, 1);
    let payloads = requests.lock().expect("broker requests");
    assert_eq!(payloads.len(), 2);
    assert_eq!(payloads[0], payloads[1]);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn replacement_candidates_are_ranked_and_requests_persist_and_transition() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1]).await;
    let need_id = seed_replacement_fixture(db.pool()).await;
    let app = app_with_pool(db.pool().clone());

    let route = format!("/internal/turnier/v1/scrims/replacement-needs/{need_id}/candidates");
    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            actor_id: Some("123456789"),
            actor_name: Some("Coach"),
            ..TestHeaders::default()
        },
        Method::GET,
        &route,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let candidates = body.as_array().expect("candidate array");
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0]["participant_id"], "302");
    assert_eq!(candidates[1]["participant_id"], "301");
    assert_eq!(candidates[2]["participant_id"], "303");
    assert!(candidates
        .iter()
        .all(|candidate| candidate["id"].is_string()));

    let route = format!("/internal/turnier/v1/scrims/replacement-needs/{need_id}/requests");
    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("replacement_request:create", "123456789"),
        Method::POST,
        &route,
        Some(json!({"participant_id":"302","reason":"Support wird gebraucht"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    let request_id: i64 = sqlx::query_scalar(
        "SELECT id FROM scrim.replacement_requests WHERE need_id=$1 AND participant_id=302",
    )
    .bind(need_id)
    .fetch_one(db.pool())
    .await
    .expect("replacement request");
    let effect: (String, Value) = sqlx::query_as(
        "SELECT effect.state, effect.payload \
           FROM scrim.replacement_request_effects link \
           JOIN scrim.outbox_effects effect ON effect.id=link.outbox_effect_id \
          WHERE link.replacement_request_id=$1",
    )
    .bind(request_id)
    .fetch_one(db.pool())
    .await
    .expect("replacement Discord effect");
    assert_eq!(effect.0, "pending");
    assert_eq!(effect.1["schema_version"], "discord-scrim-effect:v1");
    assert_eq!(effect.1["message_kind"], "replacement_request");
    assert_eq!(effect.1["operation"], "post");
    assert_eq!(effect.1["recipient_user_id"], "3002");
    assert_eq!(effect.1["body"]["flags"], 32_768);
    assert_eq!(
        effect.1["body"]["allowed_mentions"],
        json!({"parse":[],"replied_user":false})
    );
    let buttons = effect.1["body"]["components"][0]["components"][1]["components"]
        .as_array()
        .expect("replacement buttons");
    assert_eq!(
        buttons,
        &[
            json!({
                "type":2,
                "style":3,
                "label":"Ich springe ein",
                "custom_id":format!("scrimrepl:v1:{request_id}:accept")
            }),
            json!({
                "type":2,
                "style":4,
                "label":"Passt nicht",
                "custom_id":format!("scrimrepl:v1:{request_id}:decline")
            }),
        ]
    );

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("replacement_request:create:sibling", "123456789"),
        Method::POST,
        &route,
        Some(json!({"participant_id":"301","reason":"Zweite Anfrage"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let sibling_request_id: i64 = sqlx::query_scalar(
        "SELECT id FROM scrim.replacement_requests WHERE need_id=$1 AND participant_id=301",
    )
    .bind(need_id)
    .fetch_one(db.pool())
    .await
    .expect("sibling replacement request");

    let route = format!("/internal/turnier/v1/scrims/replacement-requests/{request_id}");
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("scrimrepl:v1:interaction:wrong-user"),
            idempotency_key: Some("scrimrepl:v1:interaction:wrong-user"),
            actor_id: Some("3001"),
            actor_name: Some("First"),
        },
        Method::PATCH,
        &route,
        Some(json!({"action":"accept"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("scrimrepl:v1:interaction:accept"),
            idempotency_key: Some("scrimrepl:v1:interaction:accept"),
            actor_id: Some("3002"),
            actor_name: Some("Best"),
        },
        Method::PATCH,
        &route,
        Some(json!({"action":"accept"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    assert_eq!(
        body["message"],
        "Ersatzanfrage angenommen. Du bist für den Scrim eingeplant."
    );
    let state: (String, bool) = sqlx::query_as(
        "SELECT status, responded_at IS NOT NULL FROM scrim.replacement_requests WHERE id=$1",
    )
    .bind(request_id)
    .fetch_one(db.pool())
    .await
    .expect("replacement response");
    assert_eq!(state, ("accepted".to_string(), true));
    let need_status: String =
        sqlx::query_scalar("SELECT status FROM scrim.replacement_needs WHERE id=$1")
            .bind(need_id)
            .fetch_one(db.pool())
            .await
            .expect("replacement need");
    assert_eq!(need_status, "filled");
    let sibling_state: (String, String) = sqlx::query_as(
        "SELECT request.status, effect.state \
           FROM scrim.replacement_requests request \
           JOIN scrim.replacement_request_effects link \
             ON link.replacement_request_id=request.id \
           JOIN scrim.outbox_effects effect ON effect.id=link.outbox_effect_id \
          WHERE request.id=$1",
    )
    .bind(sibling_request_id)
    .fetch_one(db.pool())
    .await
    .expect("cancelled sibling replacement effect");
    assert_eq!(
        sibling_state,
        ("cancelled".to_string(), "cancelled".to_string())
    );

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("replacement_request:invalid", "123456789"),
        Method::POST,
        &format!("/internal/turnier/v1/scrims/replacement-needs/{need_id}/requests"),
        Some(json!({"participant_id":"not-an-id"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            request_id: Some("scrimrepl:v1:interaction:repeat"),
            idempotency_key: Some("scrimrepl:v1:interaction:repeat"),
            actor_id: Some("3002"),
            actor_name: Some("Best"),
        },
        Method::PATCH,
        &route,
        Some(json!({"action":"decline"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn accepted_replacement_retries_the_team_role_after_a_retryable_broker_failure() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1]).await;
    sqlx::query("UPDATE scrim.teams SET discord_role_id=8101 WHERE id=1")
        .execute(db.pool())
        .await
        .expect("team role");
    let need_id = seed_replacement_fixture(db.pool()).await;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/member/add-role",
            post(fail_once_then_create_role),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("replacement_request:assign:create", "123456789"),
        Method::POST,
        &format!("/internal/turnier/v1/scrims/replacement-needs/{need_id}/requests"),
        Some(json!({"participant_id":"302","reason":"Support wird gebraucht"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id: i64 = sqlx::query_scalar(
        "SELECT id FROM scrim.replacement_requests WHERE need_id=$1 AND participant_id=302",
    )
    .bind(need_id)
    .fetch_one(db.pool())
    .await
    .expect("replacement request");

    for expected_status in [StatusCode::BAD_GATEWAY, StatusCode::OK] {
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            TestHeaders {
                token: Some("internal-token"),
                request_id: Some("scrimrepl:v1:interaction:assign"),
                idempotency_key: Some("scrimrepl:v1:interaction:assign"),
                actor_id: Some("3002"),
                actor_name: Some("Best"),
            },
            Method::PATCH,
            &format!("/internal/turnier/v1/scrims/replacement-requests/{request_id}"),
            Some(json!({"action":"accept"})),
        )
        .await;
        assert_eq!(status, expected_status);
        if status == StatusCode::BAD_GATEWAY {
            let assignment: Option<(i32, bool, bool)> = sqlx::query_as(
                "SELECT team_id, is_bench, substitute_until IS NOT NULL \
                   FROM scrim.team_members WHERE participant_id=302",
            )
            .fetch_optional(db.pool())
            .await
            .expect("substitute assignment after Discord failure");
            assert_eq!(assignment, Some((1, true, true)));
            let stripped = sqlx::query(
                "UPDATE scrim.command_receipts \
                    SET result_payload=result_payload - 'sync_plans' \
                  WHERE command_scope='replacement_request_patch' \
                    AND idempotency_key='scrimrepl:v1:interaction:assign'",
            )
            .execute(db.pool())
            .await
            .expect("legacy replacement receipt");
            assert_eq!(stripped.rows_affected(), 1);
        }
    }

    let requests = requests.lock().expect("broker requests");
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| {
        request["guild_id"] == 1289721245281292288_u64
            && request["user_id"] == 3002
            && request["role_id"] == 8101
            && request["reason"] == "scrim 302 add role 8101"
    }));
    let first_key = requests[0]["idempotency_key"]
        .as_str()
        .expect("first idempotency key");
    let retry_key = requests[1]["idempotency_key"]
        .as_str()
        .expect("retry idempotency key");
    assert_eq!(first_key, retry_key);
    assert!(first_key.chars().count() <= 128);
    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn replacement_request_without_discord_target_keeps_need_open() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[1]).await;
    let need_id = seed_replacement_fixture(db.pool()).await;
    sqlx::query(
        "UPDATE scrim.replacement_candidates SET discord_user_id=NULL \
          WHERE need_id=$1 AND participant_id=302",
    )
    .bind(need_id)
    .execute(db.pool())
    .await
    .expect("candidate without Discord target");
    let app = app_with_pool(db.pool().clone());

    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("replacement_request:no_target", "123456789"),
        Method::POST,
        &format!("/internal/turnier/v1/scrims/replacement-needs/{need_id}/requests"),
        Some(json!({"participant_id":"302","reason":"Support wird gebraucht"})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["detail"], "No linked Discord account; DM not sent.");
    let request_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM scrim.replacement_requests WHERE need_id=$1")
            .bind(need_id)
            .fetch_one(db.pool())
            .await
            .expect("replacement request count");
    assert_eq!(request_count, 0);
    let states: (String, String) = sqlx::query_as(
        "SELECT n.status, c.status \
           FROM scrim.replacement_needs n \
           JOIN scrim.replacement_candidates c ON c.need_id=n.id \
          WHERE n.id=$1 AND c.participant_id=302",
    )
    .bind(need_id)
    .fetch_one(db.pool())
    .await
    .expect("replacement states");
    assert_eq!(states, ("open".to_string(), "candidate".to_string()));
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn match_block_and_action_operator_routes_persist_the_canonical_flow() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810101, 810102]).await;
    sqlx::query(
        "UPDATE scrim.teams SET discord_channel_id = 700000 + id WHERE id IN (810101, 810102)",
    )
    .execute(db.pool())
    .await
    .expect("team channel ids");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_accept_message),
        )
        .with_state(requests);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:create", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/matches",
        Some(json!({"team_a_id":"810101","team_b_id":"810102"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let match_id = created["match"]["id"].as_str().expect("wire match id");
    let (status, replayed) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:create", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/matches",
        Some(json!({"team_a_id":"810101","team_b_id":"810102"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["match"]["id"], match_id);
    let created_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.matches WHERE team_a_id=810101 AND team_b_id=810102",
    )
    .fetch_one(db.pool())
    .await
    .expect("created match count");
    assert_eq!(created_count, 1);
    let route = format!("/internal/turnier/v1/scrims/matches/{match_id}/lobby-code");
    let (status, lobby) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lobby["match"]["join_code"], "A1B2C");
    let (status, replayed) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed, lobby);

    let route = format!("/internal/turnier/v1/scrims/matches/{match_id}/match-ids");
    let (status, result_refs) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:ids", "123456789"),
        Method::POST,
        &route,
        Some(json!({"match_ids":["9007199254740101","9007199254740102"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result_refs["match"]["lobby_state"], "result_requested");
    let (status, replayed) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:ids", "123456789"),
        Method::POST,
        &route,
        Some(json!({"match_ids":["9007199254740101","9007199254740102"]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed, result_refs);
    let match_id_i32 = match_id.parse::<i32>().expect("database match id");
    let refs = sqlx::query(
        "UPDATE scrim.match_result_refs \
            SET fetch_status='fetched', validation_status='valid', winner_team_id=810101, \
                normalized_result_json='{\"winner\":\"team_a\"}'::jsonb, fetched_at=now(), updated_at=now() \
          WHERE match_id=$1 \
          RETURNING id",
    )
    .bind(match_id_i32)
    .fetch_all(db.pool())
    .await
    .expect("result refs");
    assert_eq!(refs.len(), 2);
    sqlx::query("UPDATE scrim.matches SET lobby_state='in_progress' WHERE id=$1")
        .bind(match_id_i32)
        .execute(db.pool())
        .await
        .expect("fetchable match state");

    let route = format!("/internal/turnier/v1/scrims/matches/{match_id}/result-fetches");
    let (status, fetch) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:fetch", "123456789"),
        Method::POST,
        &route,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetch["match_id"], match_id);
    assert_eq!(fetch["lobby_state"], "result_requested");
    let (status, replayed) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:fetch", "123456789"),
        Method::POST,
        &route,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed, fetch);
    for unsupported_result_data in [
        json!({"winner_team_id": "810101"}),
        json!({"score": "2:0"}),
        json!({"notes": "manuell erfasst"}),
    ] {
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("match:fetch", "123456789"),
            Method::POST,
            &route,
            Some(unsupported_result_data),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    for (index, row) in refs.iter().enumerate() {
        let ref_id = row.get::<i64, _>("id");
        let route = format!("/internal/turnier/v1/scrims/matches/{match_id}/result-refs/{ref_id}");
        let (status, selected) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers(
                if index == 0 {
                    "match:select:one"
                } else {
                    "match:select:two"
                },
                "123456789",
            ),
            Method::PATCH,
            &route,
            Some(json!({"message":"wrong winner"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            selected["match"]["selected_result"]["result_ref_id"],
            ref_id.to_string()
        );
        let (status, replayed) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers(
                if index == 0 {
                    "match:select:one"
                } else {
                    "match:select:two"
                },
                "123456789",
            ),
            Method::PATCH,
            &route,
            Some(json!({"message":"wrong winner"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(replayed, selected);
    }
    let selected_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM scrim.match_result_selections WHERE match_id=$1")
            .bind(match_id_i32)
            .fetch_one(db.pool())
            .await
            .expect("selection count");
    assert_eq!(selected_count, 1);

    let drafts_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.announcement_drafts \
          WHERE block_key='block:two_week_scrim_block'",
    )
    .fetch_one(db.pool())
    .await
    .expect("draft count");
    let (status, preview) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            actor_id: Some("123456789"),
            actor_name: Some("Coach"),
            ..TestHeaders::default()
        },
        Method::GET,
        "/internal/turnier/v1/scrims/blocks/two_week_scrim_block/announcement-preview",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["block_id"], "two_week_scrim_block");
    let drafts_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.announcement_drafts \
          WHERE block_key='block:two_week_scrim_block'",
    )
    .fetch_one(db.pool())
    .await
    .expect("draft count");
    assert_eq!(
        drafts_after, drafts_before,
        "preview must be side-effect free"
    );

    let (status, publication) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("announcement:publish", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/blocks/two_week_scrim_block/announcement-publications",
        Some(json!({
            "title":"Platzhalter",
            "channel_id":"9007199254740301",
            "message":"Platzhalter"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{publication}");
    assert_eq!(publication["block_id"], "two_week_scrim_block");
    let stored_publications: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.announcement_drafts \
          WHERE block_key='block:two_week_scrim_block' AND status IN ('approved','published')",
    )
    .fetch_one(db.pool())
    .await
    .expect("publication count");
    assert_eq!(stored_publications, 1);

    let action_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.command_receipts(\
             command_scope, idempotency_key, payload_hash, payload, state, result_payload, completed_at\
         ) VALUES ('test_action', 'test:action', $1, '{}'::jsonb, 'completed', \
                   '{\"accepted\":true}'::jsonb, now()) RETURNING id",
    )
    .bind(vec![7_u8; 32])
    .fetch_one(db.pool())
    .await
    .expect("action receipt");
    let route = format!("/internal/turnier/v1/scrims/actions/{action_id}");
    let (status, action) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        TestHeaders {
            token: Some("internal-token"),
            actor_id: Some("123456789"),
            actor_name: Some("Coach"),
            ..TestHeaders::default()
        },
        Method::GET,
        &route,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(action["id"], action_id.to_string());
    assert_eq!(action["state"], "completed");
    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn announcement_receipt_uses_normalized_channel_id() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_accept_message),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, response) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("announcement:normalized-channel", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/blocks/normalized-channel/announcement-publications",
        Some(json!({
            "title":"Scrim am Samstag",
            "channel_id":" 9007199254740301 ",
            "message":"Der nächste Scrim findet am Samstag um 20 Uhr statt."
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response}");
    let publication: (String, Option<String>) = sqlx::query_as(
        "SELECT status, remote_message_id FROM scrim.announcement_drafts \
          WHERE block_key='block:normalized-channel'",
    )
    .fetch_one(db.pool())
    .await
    .expect("announcement receipt");
    assert_eq!(
        publication,
        (
            "published".to_string(),
            Some("discord:9007199254740301:912345678901234568".to_string())
        )
    );

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn announcement_broker_confirmation_for_another_channel_stays_uncertain() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_mismatch_message_channel),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, response) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("announcement:wrong-channel", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/blocks/wrong-channel/announcement-publications",
        Some(json!({
            "title":"Scrim am Samstag",
            "channel_id":"9007199254740301",
            "message":"Der nächste Scrim findet am Samstag um 20 Uhr statt."
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(
        response["detail"],
        "Gespeichert. Die Discord-Zustellung ist unklar. Prüfe den Zielkanal, bevor du erneut sendest."
    );
    assert_eq!(requests.lock().expect("broker requests").len(), 1);
    let publication: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status, published_at IS NOT NULL, remote_message_id \
           FROM scrim.announcement_drafts WHERE block_key='block:wrong-channel'",
    )
    .fetch_one(db.pool())
    .await
    .expect("announcement dispatch");
    assert_eq!(publication, ("publishing".to_string(), false, None));

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn announcement_dispatch_in_progress_is_not_sent_again() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_committed_dispatch_and_fail),
        )
        .with_state((requests.clone(), db.pool().clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));
    let body = json!({
        "title":"Scrim am Samstag",
        "channel_id":"9007199254740301",
        "message":"Der nächste Scrim findet am Samstag um 20 Uhr statt."
    });

    let (status, response) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("announcement:dispatch-once", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/blocks/dispatch-once/announcement-publications",
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(
        response["detail"],
        "Gespeichert. Die Discord-Zustellung ist unklar. Prüfe den Zielkanal, bevor du erneut sendest."
    );
    let (retry_status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("announcement:dispatch-once", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/blocks/dispatch-once/announcement-publications",
        Some(body),
    )
    .await;
    assert_eq!(retry_status, StatusCode::BAD_GATEWAY);
    assert_eq!(requests.lock().expect("broker requests").len(), 1);
    let publication: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT status, published_at IS NOT NULL, remote_message_id \
           FROM scrim.announcement_drafts WHERE block_key='block:dispatch-once'",
    )
    .fetch_one(db.pool())
    .await
    .expect("announcement dispatch");
    assert_eq!(publication, ("publishing".to_string(), false, None));

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn merge_critic_announcement_rejects_content_above_discords_limit() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    let app = app_with_pool(db.pool().clone());

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("announcement:discord-limit", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/blocks/discord-limit/announcement-publications",
        Some(json!({
            "channel_id":"9007199254740301",
            "message":"x".repeat(2_001)
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM scrim.announcement_drafts WHERE block_key='block:discord-limit'",
    )
    .fetch_one(db.pool())
    .await
    .expect("announcement count");
    assert_eq!(stored, 0);
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn successful_lobby_code_delivery_is_not_replayed() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810101, 810102]).await;
    sqlx::query(
        "UPDATE scrim.teams SET discord_channel_id = 700000 + id WHERE id IN (810101, 810102)",
    )
    .execute(db.pool())
    .await
    .expect("team channel ids");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_accept_broker),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:create:lobby-delivered", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/matches",
        Some(json!({"team_a_id":"810101","team_b_id":"810102"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let route = format!(
        "/internal/turnier/v1/scrims/matches/{}/lobby-code",
        created["match"]["id"].as_str().expect("wire match id")
    );

    for _ in 0..2 {
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("match:lobby:delivered", "123456789"),
            Method::PUT,
            &route,
            Some(json!({"lobby_code":"a1b2c"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    assert_eq!(requests.lock().expect("broker requests").len(), 2);
    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn lobby_code_replay_retries_only_the_failed_team_channel() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810101, 810102]).await;
    sqlx::query(
        "UPDATE scrim.teams SET discord_channel_id = 700000 + id WHERE id IN (810101, 810102)",
    )
    .execute(db.pool())
    .await
    .expect("team channel ids");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_fail_second_channel_once),
        )
        .with_state((requests.clone(), None));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:create:lobby-partial", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/matches",
        Some(json!({"team_a_id":"810101","team_b_id":"810102"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let route = format!(
        "/internal/turnier/v1/scrims/matches/{}/lobby-code",
        created["match"]["id"].as_str().expect("wire match id")
    );

    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby:partial", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby:partial", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let requests = requests.lock().expect("broker requests");
    assert_eq!(
        requests
            .iter()
            .filter(|payload| payload["channel_id"] == "1510101")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|payload| payload["channel_id"] == "1510102")
            .count(),
        2
    );
    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn lobby_code_replay_delivers_a_team_channel_configured_later() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810101, 810102]).await;
    sqlx::query("UPDATE scrim.teams SET discord_channel_id = 1510101 WHERE id = 810101")
        .execute(db.pool())
        .await
        .expect("first team channel id");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_accept_broker),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:create:lobby-late-channel", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/matches",
        Some(json!({"team_a_id":"810101","team_b_id":"810102"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let route = format!(
        "/internal/turnier/v1/scrims/matches/{}/lobby-code",
        created["match"]["id"].as_str().expect("wire match id")
    );
    let headers = coach_headers("match:lobby:late-channel", "123456789");
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    sqlx::query("UPDATE scrim.teams SET discord_channel_id = 1510102 WHERE id = 810102")
        .execute(db.pool())
        .await
        .expect("second team channel id");
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        headers,
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let requests = requests.lock().expect("broker requests");
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["channel_id"], "1510101");
    assert_eq!(requests[1]["channel_id"], "1510102");
    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn failed_lobby_code_delivery_retries_without_sending_stale_codes() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810101, 810102]).await;
    sqlx::query(
        "UPDATE scrim.teams SET discord_channel_id = 700000 + id WHERE id IN (810101, 810102)",
    )
    .execute(db.pool())
    .await
    .expect("team channel ids");

    let requests = Arc::new(Mutex::new(Vec::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(record_and_fail_broker),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:create:lobby-delivery", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/matches",
        Some(json!({"team_a_id":"810101","team_b_id":"810102"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let match_id = created["match"]["id"].as_str().expect("wire match id");
    let route = format!("/internal/turnier/v1/scrims/matches/{match_id}/lobby-code");
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby:delivery", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby:delivery", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby:correction", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"b2c3d"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby:delivery", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let persisted: String = sqlx::query_scalar("SELECT join_code FROM scrim.matches WHERE id=$1")
        .bind(match_id.parse::<i32>().expect("database match id"))
        .fetch_one(db.pool())
        .await
        .expect("persisted lobby code");
    assert_eq!(persisted, "B2C3D");
    let payloads = requests.lock().expect("broker requests");
    assert_eq!(
        payloads
            .iter()
            .filter(|payload| payload["content"] == "Lobby Code: A1B2C")
            .count(),
        4
    );
    assert_eq!(
        payloads
            .iter()
            .filter(|payload| payload["content"] == "Lobby Code: B2C3D")
            .count(),
        2
    );
    assert_eq!(payloads.len(), 6);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn merge_critic_lobby_code_a_b_a_uses_a_fresh_broker_operation() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810101, 810102]).await;
    sqlx::query(
        "UPDATE scrim.teams SET discord_channel_id = 700000 + id WHERE id IN (810101, 810102)",
    )
    .execute(db.pool())
    .await
    .expect("team channel ids");

    let requests = Arc::new(Mutex::new(HashMap::new()));
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(accept_broker_once_per_idempotency_key),
        )
        .with_state(requests.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:create:lobby-a-b-a", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/matches",
        Some(json!({"team_a_id":"810101","team_b_id":"810102"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let match_id = created["match"]["id"].as_str().expect("wire match id");
    let route = format!("/internal/turnier/v1/scrims/matches/{match_id}/lobby-code");

    for (idempotency_key, lobby_code) in [
        ("match:lobby:a-b-a:first-a".to_string(), "a1b2c"),
        (format!("match_lobby_a_b_a:{}", "b".repeat(96)), "b2c3d"),
        ("match:lobby:a-b-a:second-a".to_string(), "a1b2c"),
    ] {
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers(&idempotency_key, "123456789"),
            Method::PUT,
            &route,
            Some(json!({"lobby_code":lobby_code})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let requests = requests.lock().expect("broker requests");
    assert_eq!(
        requests
            .values()
            .filter(|payload| payload["content"] == "Lobby Code: A1B2C")
            .count(),
        4
    );
    assert_eq!(
        requests
            .values()
            .filter(|payload| payload["content"] == "Lobby Code: B2C3D")
            .count(),
        2
    );
    assert_eq!(requests.len(), 6);

    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn lobby_code_correction_waits_for_in_flight_replay_delivery() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810101, 810102]).await;
    sqlx::query(
        "UPDATE scrim.teams SET discord_channel_id = 700000 + id WHERE id IN (810101, 810102)",
    )
    .execute(db.pool())
    .await
    .expect("team channel ids");

    let broker_state = Arc::new(PausingBroker {
        calls: AtomicUsize::new(0),
        retry_started: tokio::sync::Notify::new(),
        both_retries_started: tokio::sync::Notify::new(),
        release_retry: tokio::sync::Notify::new(),
    });
    let broker = Router::new()
        .route(
            "/internal/master/v1/discord/send-message",
            post(pause_first_retry_broker),
        )
        .with_state(broker_state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("broker listener");
    let broker_url = format!("http://{}", listener.local_addr().expect("broker address"));
    let broker_task = tokio::spawn(async move {
        axum::serve(listener, broker).await.expect("broker server");
    });
    let app = app_with_pool_and_broker(db.pool().clone(), Some(&broker_url));

    let (status, created) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:create:lobby-race", "123456789"),
        Method::POST,
        "/internal/turnier/v1/scrims/matches",
        Some(json!({"team_a_id":"810101","team_b_id":"810102"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let route = format!(
        "/internal/turnier/v1/scrims/matches/{}/lobby-code",
        created["match"]["id"].as_str().expect("wire match id")
    );
    let (status, _) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("match:lobby:race", "123456789"),
        Method::PUT,
        &route,
        Some(json!({"lobby_code":"a1b2c"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    let replay_app = app.clone();
    let replay_route = route.clone();
    let replay = tokio::spawn(async move {
        send(
            &replay_app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("match:lobby:race", "123456789"),
            Method::PUT,
            &replay_route,
            Some(json!({"lobby_code":"a1b2c"})),
        )
        .await
    });
    tokio::time::timeout(
        Duration::from_secs(2),
        broker_state.retry_started.notified(),
    )
    .await
    .expect("replay reached the broker");
    tokio::time::timeout(
        Duration::from_secs(2),
        broker_state.both_retries_started.notified(),
    )
    .await
    .expect("both team-channel deliveries reached the broker");
    assert_eq!(
        broker_state.calls.load(Ordering::SeqCst),
        4,
        "team-channel deliveries did not start in parallel"
    );

    let correction_app = app.clone();
    let correction_route = route.clone();
    let mut correction = tokio::spawn(async move {
        send(
            &correction_app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers("match:lobby:race:correction", "123456789"),
            Method::PUT,
            &correction_route,
            Some(json!({"lobby_code":"b2c3d"})),
        )
        .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut correction)
            .await
            .is_err(),
        "correction committed while the old code was still being delivered"
    );

    broker_state.release_retry.notify_one();
    assert_eq!(
        replay.await.expect("replay task").0,
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        correction.await.expect("correction task").0,
        StatusCode::BAD_GATEWAY
    );
    broker_task.abort();
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn match_block_and_action_operator_routes_reject_invalid_input_and_inactive_actors() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810201, 810202]).await;
    let app = app_with_pool(db.pool().clone());

    let invalid_cases = [
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches",
            json!({"team_a_id":"810201","team_b_id":"810201"}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches",
            json!({"team_a_id":"810201","team_b_id":"810202","note":"not persisted"}),
        ),
        (
            Method::PUT,
            "/internal/turnier/v1/scrims/matches/999999/lobby-code",
            json!({"lobby_code":"TOO-LONG"}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches/999999/match-ids",
            json!({"match_ids":[]}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches/999999/result-fetches",
            json!({"match_id_ref":"zero"}),
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/matches/999999/result-refs/1",
            json!({"message":"  "}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/blocks/two_week_scrim_block/announcement-publications",
            json!({"message":"announcement"}),
        ),
    ];
    for (index, (method, route, body)) in invalid_cases.into_iter().enumerate() {
        let key = format!("invalid:{index}");
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers(&key, "123456789"),
            method,
            route,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{route}");
    }

    for route in [
        "/internal/turnier/v1/scrims/blocks/%20/announcement-preview",
        "/internal/turnier/v1/scrims/actions/not-an-id",
    ] {
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            TestHeaders {
                token: Some("internal-token"),
                actor_id: Some("123456789"),
                actor_name: Some("Coach"),
                ..TestHeaders::default()
            },
            Method::GET,
            route,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{route}");
    }

    for (index, (method, route, body)) in [
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches",
            json!({"team_a_id":"810201","team_b_id":"810202"}),
        ),
        (
            Method::PUT,
            "/internal/turnier/v1/scrims/matches/1/lobby-code",
            json!({"lobby_code":"A1B2C"}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches/1/match-ids",
            json!({"match_ids":["123"]}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/matches/1/result-fetches",
            json!({}),
        ),
        (
            Method::PATCH,
            "/internal/turnier/v1/scrims/matches/1/result-refs/1",
            json!({"message":"reason"}),
        ),
        (
            Method::POST,
            "/internal/turnier/v1/scrims/blocks/1/announcement-publications",
            json!({"message":"announcement"}),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let key = format!("inactive:{index}");
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            coach_headers(&key, "223456789"),
            method,
            route,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{route}");
    }
    for route in [
        "/internal/turnier/v1/scrims/blocks/1/announcement-preview",
        "/internal/turnier/v1/scrims/actions/1",
    ] {
        let (status, _) = send(
            &app,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            TestHeaders {
                token: Some("internal-token"),
                actor_id: Some("223456789"),
                actor_name: Some("Inactive"),
                ..TestHeaders::default()
            },
            Method::GET,
            route,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{route}");
    }
}

#[cfg(feature = "testing")]
fn coach_headers<'a>(idempotency_key: &'a str, actor_id: &'a str) -> TestHeaders<'a> {
    TestHeaders {
        token: Some("internal-token"),
        request_id: Some("request:1"),
        idempotency_key: Some(idempotency_key),
        actor_id: Some(actor_id),
        actor_name: Some("Coach"),
    }
}

#[cfg(feature = "testing")]
async fn enable_turniere_runtime(pool: &PgPool) {
    let applied: bool = sqlx::query_scalar(
        "SELECT applied FROM scrim.transition_runtime_control(\
             0, 'draining', 'turniere', '123456789', 'Coach', 'test:runtime', 'test:runtime', '{}'::jsonb\
         )",
    )
    .fetch_one(pool)
    .await
    .expect("enable turniere runtime");
    assert!(applied);
}

#[cfg(feature = "testing")]
async fn seed_coach(pool: &PgPool, discord_id: i64) {
    sqlx::query(
        "INSERT INTO coaching.coaches(id, discord_user_id, display_name, status) \
         VALUES($1, $2, 'Coach', 'active')",
    )
    .bind(format!("scrim-route-coach-{discord_id}"))
    .bind(discord_id)
    .execute(pool)
    .await
    .expect("active coach seed");
}

#[cfg(feature = "testing")]
async fn seed_teams(pool: &PgPool, ids: &[i32]) {
    for id in ids {
        sqlx::query("INSERT INTO scrim.teams(id, name, created_at) VALUES($1, $2, now())")
            .bind(*id)
            .bind(format!("Route Team {id}"))
            .execute(pool)
            .await
            .expect("team seed");
    }
}

#[cfg(feature = "testing")]
async fn seed_roster_fixture(pool: &PgPool) {
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.teams(id, name, created_at)
        VALUES (10, 'Existing Team', now());
        INSERT INTO scrim.participants(
            id, discord_id, display_name, rank_source, rank_verified, availability_slots,
            status, source, created_at, updated_at
        )
        VALUES
            (501, 9501, 'Free Player', 'self', false, NULL, 'new', 'test', now(), now()),
            (502, 9502, 'Reserve Player', 'self', false, NULL, 'reserve', 'test', now(), now()),
            (
                503, 9503, 'Suggested Player', 'self', false,
                '{"mon":{"status":"unknown","from":null,"to":null},"tue":{"status":"unknown","from":null,"to":null},"wed":{"status":"unknown","from":null,"to":null},"thu":{"status":"unknown","from":null,"to":null},"fri":{"status":"available","from":1200,"to":1320},"sat":{"status":"unknown","from":null,"to":null},"sun":{"status":"unknown","from":null,"to":null}}'::jsonb,
                'new', 'test', now(), now()
            );
        "#,
    )
    .execute(pool)
    .await
    .expect("roster fixture");
}

#[cfg(feature = "testing")]
async fn seed_operator_request_fixture(pool: &PgPool) {
    sqlx::raw_sql(
        r#"
        UPDATE scrim.teams
           SET discord_role_id = 8000 + id,
               discord_channel_id = 7000 + id
         WHERE id IN (1, 2);
        INSERT INTO scrim.participants(id, discord_id, display_name, rank_source, rank_verified, status, source, created_at, updated_at)
        VALUES
            (101, 1001, 'A', 'self', false, 'assigned', 'test', now(), now()),
            (201, 2001, 'B', 'self', false, 'assigned', 'test', now(), now());
        INSERT INTO scrim.team_members(team_id, participant_id, role, is_captain, is_bench)
        VALUES (1, 101, 'player', true, false), (2, 201, 'player', true, false);
        INSERT INTO scrim.match_request_batches(id, template, deadline_at, status, created_by_user_id, created_by_display_name)
        VALUES (90, 'regular_scrim', now() - interval '1 hour', 'open', '123456789', 'Coach');
        INSERT INTO scrim.match_requests(id, batch_id, team_a_id, team_b_id, status, slot_options, team_query_message_ids)
        VALUES
            (91, 90, 1, 2, 'draft', '[]'::jsonb, '{}'::jsonb),
            (92, 90, 1, 2, 'open', '[{"day":"sat","from":1200,"to":1320}]'::jsonb,
             '{"1":{"channel_id":7001,"message_id":7101},"2":{"channel_id":7002,"message_id":7102}}'::jsonb),
            (93, 90, 1, 2, 'closed', '[{"day":"sat","from":1200,"to":1320}]'::jsonb, '{}'::jsonb);
        INSERT INTO scrim.match_request_responses(request_id, team_id, participant_id, discord_user_id, slot_index, response, source, responded_at, updated_at)
        VALUES (92, 2, 201, 2001, 0, 'available', 'button', now(), now());
        "#,
    )
    .execute(pool)
    .await
    .expect("operator request fixture");
}

#[cfg(feature = "testing")]
async fn seed_replacement_fixture(pool: &PgPool) -> i64 {
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.participants(id, discord_id, display_name, rank, rank_source, rank_verified, roles, status, source, created_at, updated_at)
        VALUES
            (101, 1001, 'Missing', 'Initiate', 'self', false, 'front', 'assigned', 'test', now(), now()),
            (301, 3001, 'First', 'Alchemist', 'self', false, 'front', 'new', 'test', now(), now()),
            (302, 3002, 'Best', 'Arcanist', 'self', false, 'support', 'new', 'test', now(), now()),
            (303, 3003, 'Stable', 'Alchemist', 'self', false, 'front', 'new', 'test', now(), now());
        INSERT INTO scrim.team_members(team_id, participant_id, role, is_captain, is_bench)
        VALUES (1, 101, 'player', true, false);
        "#,
    )
    .execute(pool)
    .await
    .expect("replacement participants");
    let need_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.replacement_needs(\
             team_id, participant_id, needed_role, reason, status, created_by_user_id, created_by_display_name\
         ) VALUES (1, 101, 'front', 'participant_unavailable', 'open', '123456789', 'Coach') \
         RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("replacement need");
    for (participant_id, score) in [(301, 50), (302, 90), (303, 50)] {
        sqlx::query(
            "INSERT INTO scrim.replacement_candidates(\
                 need_id, participant_id, discord_user_id, candidate_data, score_data, status\
             ) VALUES ($1, $2, $3, '{}'::jsonb, jsonb_build_object('score', $4::integer), 'candidate')",
        )
        .bind(need_id)
        .bind(participant_id)
        .bind(i64::from(participant_id) + 2700)
        .bind(score)
        .execute(pool)
        .await
        .expect("replacement candidate");
    }
    need_id
}

#[cfg(feature = "testing")]
async fn seed_release_fixture(pool: &PgPool) {
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.participants(id, discord_id, display_name, rank_source, rank_verified, status, source, created_at, updated_at)
        VALUES
            (101, 1001, 'A', 'self', false, 'assigned', 'test', now(), now()),
            (201, 2001, 'B', 'self', false, 'assigned', 'test', now(), now());
        INSERT INTO scrim.team_members(team_id, participant_id, role, is_captain, is_bench)
        VALUES (1, 101, 'player', true, false), (2, 201, 'player', true, false);
        INSERT INTO scrim.match_request_batches(id, template, deadline_at, status, created_by_user_id, created_by_display_name)
        VALUES (90, 'regular_scrim', now() - interval '1 hour', 'open', '123456789', 'Coach');
        INSERT INTO scrim.match_requests(id, batch_id, team_a_id, team_b_id, status, slot_options)
        VALUES (91, 90, 1, 2, 'open', '[{"day":"sat","from":1200,"to":1320},{"day":"sun","from":1200,"to":1320}]'::jsonb);
        INSERT INTO scrim.match_request_responses(request_id, team_id, participant_id, discord_user_id, slot_index, response, source, responded_at, updated_at)
        VALUES
            (91, 1, 101, 1001, 0, 'available', 'button', now(), now()),
            (91, 2, 201, 2001, 0, 'available', 'button', now(), now());
        "#,
    )
    .execute(pool)
    .await
    .expect("release fixture");
}

#[cfg(feature = "testing")]
async fn seed_interaction_fixture(pool: &PgPool) {
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.participants(id, discord_id, display_name, rank_source, rank_verified, status, source, created_at, updated_at)
        VALUES (880, 88, 'Participant', 'self', false, 'assigned', 'test', now(), now()),
               (990, 99, 'Foreign', 'self', false, 'assigned', 'test', now(), now());
        INSERT INTO scrim.team_members(team_id, participant_id, role, is_captain, is_bench)
        VALUES (2, 880, 'player', true, false), (1, 990, 'player', true, false);
        INSERT INTO scrim.match_request_batches(id, template, deadline_at, status, created_by_user_id, created_by_display_name)
        VALUES (30, 'regular_scrim', now() + interval '1 hour', 'open', '123456789', 'Coach');
        INSERT INTO scrim.match_requests(id, batch_id, team_a_id, team_b_id, status, slot_options, team_query_message_ids)
        VALUES (
            31, 30, 1, 2, 'open',
            '[{"day":"sat","from":1200,"to":1320},{"day":"sun","from":1200,"to":1320}]'::jsonb,
            '{"2":{"channel_id":66,"message_id":77}}'::jsonb
        );
        "#,
    )
    .execute(pool)
    .await
    .expect("interaction fixture");
}
