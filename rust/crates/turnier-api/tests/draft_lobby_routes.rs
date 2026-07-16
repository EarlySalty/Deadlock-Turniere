#![cfg(feature = "testing")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, HOST};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_db::{test_pool, TestDb};

struct TestApp {
    app: Router,
    _db: TestDb,
}

struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

async fn setup() -> TestApp {
    let db = test_pool().await.expect("central test pool");
    let mut config = Config::from_env();
    config.discord_bot_token = String::new();
    config.steam_bridge_db_path = String::new();
    config.backend_allowed_hosts = "localhost".to_string();
    let state = AppState::build(db.pool().clone(), Arc::new(config))
        .await
        .expect("state build");
    TestApp {
        app: build_router(state),
        _db: db,
    }
}

fn lobby_body() -> Value {
    json!({
        "team1_name": "  Team Eins  ",
        "team2_name": "Team Zwei",
        "preset": "quick_no_ban",
        "round_seconds": 60,
        "reserve_seconds": 30
    })
}

async fn send_json(
    app: &Router,
    ip: IpAddr,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> TestResponse {
    send_json_with_forwarded_ip(app, ip, None, method, uri, body).await
}

async fn send_json_with_forwarded_ip(
    app: &Router,
    peer_ip: IpAddr,
    forwarded_ip: Option<IpAddr>,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> TestResponse {
    let body = body
        .map(|value| Body::from(value.to_string()))
        .unwrap_or_else(Body::empty);
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(HOST, "localhost")
        .header(CONTENT_TYPE, "application/json");
    if let Some(forwarded_ip) = forwarded_ip {
        builder = builder.header("x-forwarded-for", forwarded_ip.to_string());
    }
    let mut request = builder.body(body).expect("request");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(peer_ip, 40000)));
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    TestResponse {
        status,
        headers,
        body,
    }
}

#[tokio::test]
async fn lobby_anlegen_und_oeffentlich_ohne_tokens_lesen() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1));

    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(lobby_body()),
    )
    .await;
    assert_eq!(created.status, StatusCode::OK);
    let code = created.body["code"].as_str().expect("code");
    let team1_token = created.body["team1_token"].as_str().expect("team1 token");
    let team2_token = created.body["team2_token"].as_str().expect("team2 token");
    assert!(!team1_token.is_empty());
    assert!(!team2_token.is_empty());
    assert_ne!(team1_token, team2_token);

    let state = send_json(
        &ctx.app,
        ip,
        Method::GET,
        &format!("/api/draft/lobbies/{code}"),
        None,
    )
    .await;
    assert_eq!(state.status, StatusCode::OK);
    assert_eq!(
        state
            .headers
            .get(CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(state.body["team1_name"], "Team Eins");
    assert_eq!(state.body["team2_name"], "Team Zwei");
    assert_eq!(state.body["status"], "in_progress");
    assert_eq!(state.body["round_seconds"], 60);
    assert_eq!(state.body["team1_reserve_left"], 30);
    assert_eq!(state.body["team2_reserve_left"], 30);
    assert!(state.body["deadline_at"].is_string());
    assert!(state.body["actions"].as_array().expect("actions")[0]["is_auto"].is_boolean());

    let encoded = state.body.to_string();
    assert!(!encoded.contains("team1_token"));
    assert!(!encoded.contains("team2_token"));
    assert!(!encoded.contains(team1_token));
    assert!(!encoded.contains(team2_token));
}

#[tokio::test]
async fn lobby_aktion_prueft_token_und_liefert_neuen_vollzustand() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 2));
    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(lobby_body()),
    )
    .await;
    let code = created.body["code"].as_str().expect("code");
    let team1_token = created.body["team1_token"].as_str().expect("team1 token");
    let team2_token = created.body["team2_token"].as_str().expect("team2 token");
    let uri = format!("/api/draft/lobbies/{code}/action");

    let invalid = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri,
        Some(json!({"token": "falsch", "hero_name": "Abrams"})),
    )
    .await;
    assert_eq!(invalid.status, StatusCode::UNAUTHORIZED);

    let wrong_team = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri,
        Some(json!({"token": team2_token, "hero_name": "Abrams"})),
    )
    .await;
    assert_eq!(wrong_team.status, StatusCode::FORBIDDEN);

    let accepted = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri,
        Some(json!({"token": team1_token, "hero_name": "Abrams"})),
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK);
    assert_eq!(accepted.body["current_action_index"], 1);
    assert_eq!(accepted.body["actions"][0]["hero_name"], "Abrams");
    assert!(accepted.body.get("team1_token").is_none());
    assert!(accepted.body.get("team2_token").is_none());
}

#[tokio::test]
async fn unbekannter_lobby_code_liefert_404() {
    let ctx = setup().await;
    let response = send_json(
        &ctx.app,
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 3)),
        Method::GET,
        "/api/draft/lobbies/UNBEKANNT",
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert!(response.body["detail"].is_string());
}

#[tokio::test]
async fn lobby_anlegen_validiert_systemgrenzen() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 4));
    let cases = [
        (
            json!({"team1_name": "  ", "team2_name": "B", "preset": "quick_no_ban", "round_seconds": 60, "reserve_seconds": 0}),
            "Teamnamen müssen 1 bis 40 Zeichen lang sein.",
        ),
        (
            json!({"team1_name": "A".repeat(41), "team2_name": "B", "preset": "quick_no_ban", "round_seconds": 60, "reserve_seconds": 0}),
            "Teamnamen müssen 1 bis 40 Zeichen lang sein.",
        ),
        (
            json!({"team1_name": "A", "team2_name": "B", "preset": "quick_no_ban", "round_seconds": 5, "reserve_seconds": 0}),
            "Die Rundendauer muss zwischen 10 und 300 Sekunden liegen.",
        ),
        (
            json!({"team1_name": "A", "team2_name": "B", "preset": "quick_no_ban", "round_seconds": 60, "reserve_seconds": 601}),
            "Die Reservezeit muss zwischen 0 und 600 Sekunden liegen.",
        ),
        (
            json!({"team1_name": "A", "team2_name": "B", "preset": "unbekannt", "round_seconds": 60, "reserve_seconds": 0}),
            "Dieses Draft-Preset wird nicht unterstützt.",
        ),
    ];

    for (body, detail) in cases {
        let response =
            send_json(&ctx.app, ip, Method::POST, "/api/draft/lobbies", Some(body)).await;
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
        assert_eq!(response.body["detail"], detail);
    }
}

#[tokio::test]
async fn elfte_lobby_derselben_ip_liefert_429() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5));

    for _ in 0..10 {
        let response = send_json(
            &ctx.app,
            ip,
            Method::POST,
            "/api/draft/lobbies",
            Some(lobby_body()),
        )
        .await;
        assert_eq!(response.status, StatusCode::OK);
    }

    let limited = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(lobby_body()),
    )
    .await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        limited.body["detail"],
        "Du kannst höchstens 10 Draft-Lobbys pro Stunde erstellen."
    );
}

#[tokio::test]
async fn rate_limit_unterscheidet_client_ips_hinter_dem_proxy() {
    let ctx = setup().await;
    let proxy_ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let first_client = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7));
    let second_client = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 8));

    for _ in 0..10 {
        let response = send_json_with_forwarded_ip(
            &ctx.app,
            proxy_ip,
            Some(first_client),
            Method::POST,
            "/api/draft/lobbies",
            Some(lobby_body()),
        )
        .await;
        assert_eq!(response.status, StatusCode::OK);
    }

    let other_client = send_json_with_forwarded_ip(
        &ctx.app,
        proxy_ip,
        Some(second_client),
        Method::POST,
        "/api/draft/lobbies",
        Some(lobby_body()),
    )
    .await;
    assert_eq!(other_client.status, StatusCode::OK);
}

#[tokio::test]
async fn rate_limit_ignoriert_forwarded_ip_von_externem_peer() {
    let ctx = setup().await;
    let peer_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));

    for suffix in 1..=10 {
        let response = send_json_with_forwarded_ip(
            &ctx.app,
            peer_ip,
            Some(IpAddr::V4(Ipv4Addr::new(198, 51, 100, suffix))),
            Method::POST,
            "/api/draft/lobbies",
            Some(lobby_body()),
        )
        .await;
        assert_eq!(response.status, StatusCode::OK);
    }

    let limited = send_json_with_forwarded_ip(
        &ctx.app,
        peer_ip,
        Some(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 11))),
        Method::POST,
        "/api/draft/lobbies",
        Some(lobby_body()),
    )
    .await;
    assert_eq!(limited.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn helden_route_liefert_objekte_mit_live_vertrag() {
    let ctx = setup().await;
    let response = send_json(
        &ctx.app,
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 6)),
        Method::GET,
        "/api/draft/heroes",
        None,
    )
    .await;

    assert_eq!(response.status, StatusCode::OK);
    let heroes = response.body["heroes"].as_array().expect("heroes");
    assert!(!heroes.is_empty());
    assert!(heroes[0]["id"].is_number());
    assert!(heroes[0]["name"].is_string());
    assert!(heroes[0]["image_url"].is_string());
}
