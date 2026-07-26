use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::header::{CONTENT_TYPE, HOST};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
#[cfg(feature = "testing")]
use sqlx::Row;
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_discord::{BrokerClient, DiscordNotifier};
use turnier_match::MatchManager;
use turnier_steam::SteamRankResolver;

fn app() -> Router {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .expect("lazy pool");
    app_with_pool(pool)
}

fn app_with_pool(pool: PgPool) -> Router {
    let mut config = Config::from_env();
    config.turnier_internal_api_token = "internal-token".to_string();
    config.discord_bot_token = String::new();
    config.discord_master_broker_token = String::new();
    config.scrim_signup_role_id = None;
    config.scrim_reserve_role_id = None;
    config.steam_bridge_db_path = String::new();
    config.backend_allowed_hosts = "localhost".to_string();
    config.discord_mod_role_ids = "99".to_string();
    let config = Arc::new(config);

    let role_sets = turnier_auth::RoleSets::from_config(&config);
    let oauth = turnier_auth::OAuthClient::new(&config);
    let broker = BrokerClient::from_config(&config);
    let match_manager = Arc::new(MatchManager::new(pool.clone(), None, None, &config));
    let notifier = Arc::new(DiscordNotifier::new(broker, pool.clone(), &config));
    let rank_resolver = Arc::new(SteamRankResolver::from_pool(pool.clone(), None, None));
    let state = AppState {
        pool,
        config,
        role_sets,
        oauth,
        match_manager,
        rank_resolver,
        notifier,
        draft_lobby_creations: Arc::new(Mutex::new(HashMap::new())),
    };
    build_router(state)
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
            json!({"participant_id":"1","window":{"day":"fri","from":1200,"to":1320}}),
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
            json!({"status":"accepted"}),
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
async fn match_block_and_action_operator_routes_persist_the_canonical_flow() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    seed_coach(db.pool(), 123456789).await;
    seed_teams(db.pool(), &[810101, 810102]).await;
    let app = app_with_pool(db.pool().clone());

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
#[tokio::test]
async fn remaining_unimplemented_mutations_return_disabled_unverified_capability() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_coach(db.pool(), 123456789).await;
    let app = app_with_pool(db.pool().clone());
    let (status, body) = send(
        &app,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        coach_headers("request_patch:invalid_body", "123456789"),
        Method::PATCH,
        "/internal/turnier/v1/scrims/match-requests/1",
        Some(json!({"unknown":"ignored","slot_index":"not validated here"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(body["available"], false);
    assert_eq!(body["verified"], false);

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
        "/internal/turnier/v1/scrims/replacement-needs/1/candidates",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    assert_eq!(body["available"], false);
    assert_eq!(body["verified"], false);
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
