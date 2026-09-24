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
    let config = Config {
        discord_bot_token: String::new(),
        steam_bridge_db_path: String::new(),
        backend_allowed_hosts: "localhost".to_string(),
        ..Config::default()
    };
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
    let namen = [
        state.body["team1_name"].as_str().expect("team1 name"),
        state.body["team2_name"].as_str().expect("team2 name"),
    ];
    assert!(namen.contains(&"Team Eins"));
    assert!(namen.contains(&"Team Zwei"));
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

    let state = send_json(
        &ctx.app,
        ip,
        Method::GET,
        &format!("/api/draft/lobbies/{code}"),
        None,
    )
    .await;
    let (slot1_token, slot2_token) = if state.body["team1_name"] == "Team Eins" {
        (team1_token, team2_token)
    } else {
        (team2_token, team1_token)
    };

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
        Some(json!({"token": slot2_token, "hero_name": "Abrams"})),
    )
    .await;
    assert_eq!(wrong_team.status, StatusCode::FORBIDDEN);

    let accepted = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri,
        Some(json!({"token": slot1_token, "hero_name": "Abrams"})),
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
    assert!(heroes[0]["card_image_url"].is_string());
}

fn raum_body(bans_per_team: i32, round_seconds: i32) -> Value {
    json!({
        "team1_name": "Team Eins",
        "team2_name": "Team Zwei",
        "bans_per_team": bans_per_team,
        "round_seconds": round_seconds
    })
}

async fn lese_zustand(ctx: &TestApp, code: &str, token: Option<&str>) -> TestResponse {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/draft/lobbies/{code}"))
        .header(HOST, "localhost")
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header("x-draft-token", token);
    }
    let request = builder.body(Body::empty()).expect("request");
    let response = ctx.app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    TestResponse {
        status,
        headers,
        body: serde_json::from_slice(&bytes).expect("json body"),
    }
}

#[tokio::test]
async fn raum_anlegen_mit_bans_und_timer_aus_liefert_warteraum_vertrag() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 21));

    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(raum_body(2, 0)),
    )
    .await;
    assert_eq!(created.status, StatusCode::OK);
    let code = created.body["code"].as_str().expect("code").to_string();
    assert!(created.body["team1_token"].is_null());

    let state = lese_zustand(&ctx, &code, None).await;
    assert_eq!(state.status, StatusCode::OK);
    assert_eq!(state.body["phase"], "warteraum");
    assert_eq!(state.body["bans_per_team"], 2);
    assert_eq!(state.body["round_seconds"], 0);
    assert!(state.body["deadline_at"].is_null());
    let namen = [
        state.body["team1"]["name"].as_str().expect("team1 name"),
        state.body["team2"]["name"].as_str().expect("team2 name"),
    ];
    assert!(namen.contains(&"Team Eins"));
    assert!(namen.contains(&"Team Zwei"));
    assert_eq!(state.body["team1"]["claimed"], false);
    assert_eq!(state.body["team2"]["claimed"], false);
    assert_eq!(state.body["team1"]["ready"], false);
    assert_eq!(state.body["team2"]["ready"], false);
    assert_eq!(state.body["you"]["team"], Value::Null);
    assert_eq!(state.body["lobby"]["status"], "keine");
    assert!(state.body["lobby"]["join_code"].is_null());
    assert!(state.body["lobby"]["error"].is_null());
    assert!(state.body["lobby"]["match_id"].is_null());
    assert!(state.body["lobby"]["result"].is_null());
    assert!(state.body["rematch_code"].is_null());
    let sequence = state.body["sequence"].as_array().expect("sequence");
    assert_eq!(sequence.len(), 16);
    assert_eq!(sequence[0]["index"], 0);
    assert_eq!(sequence[0]["team"], 1);
    assert_eq!(sequence[0]["action"], "ban");
}

#[tokio::test]
async fn claim_und_ready_starten_erst_nach_beiden_captains() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 22));
    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(raum_body(1, 30)),
    )
    .await;
    let code = created.body["code"].as_str().expect("code").to_string();
    let uri = |suffix: &str| format!("/api/draft/lobbies/{code}/{suffix}");

    let claim1 = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("claim"),
        Some(json!({"team": 1})),
    )
    .await;
    assert_eq!(claim1.status, StatusCode::OK);
    let token1 = claim1.body["token"].as_str().expect("token").to_string();
    assert_eq!(claim1.body["team"], 1);

    let doppelt = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("claim"),
        Some(json!({"team": 1})),
    )
    .await;
    assert_eq!(doppelt.status, StatusCode::CONFLICT);

    let unbekanntes_team = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("claim"),
        Some(json!({"team": 3})),
    )
    .await;
    assert_eq!(unbekanntes_team.status, StatusCode::BAD_REQUEST);

    let ready1 = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("ready"),
        Some(json!({"token": token1})),
    )
    .await;
    assert_eq!(ready1.status, StatusCode::OK);
    assert_eq!(ready1.body["started"], false);
    assert_eq!(ready1.body["phase"], "warteraum");

    let state = lese_zustand(&ctx, &code, Some(&token1)).await;
    assert_eq!(state.body["you"]["team"], 1);
    assert_eq!(state.body["team1"]["claimed"], true);
    assert_eq!(state.body["team1"]["ready"], true);
    assert_eq!(state.body["team2"]["claimed"], false);

    let claim2 = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("claim"),
        Some(json!({"team": 2})),
    )
    .await;
    assert_eq!(claim2.status, StatusCode::OK);
    let token2 = claim2.body["token"].as_str().expect("token").to_string();
    let ready2 = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("ready"),
        Some(json!({"token": token2})),
    )
    .await;
    assert_eq!(ready2.status, StatusCode::OK);
    assert_eq!(ready2.body["started"], true);
    assert_eq!(ready2.body["phase"], "laeuft");
    assert!(ready2.body["deadline_at"].is_string());
}

#[tokio::test]
async fn leave_ohne_start_gibt_den_slot_frei_und_meldet_fremde_tokens() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 23));
    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(raum_body(0, 0)),
    )
    .await;
    let code = created.body["code"].as_str().expect("code").to_string();
    let uri = |suffix: &str| format!("/api/draft/lobbies/{code}/{suffix}");

    let fremder_token = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("ready"),
        Some(json!({"token": "falsch"})),
    )
    .await;
    assert_eq!(fremder_token.status, StatusCode::UNAUTHORIZED);

    let claim = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("claim"),
        Some(json!({"team": 2})),
    )
    .await;
    let token = claim.body["token"].as_str().expect("token").to_string();

    let leave = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("leave"),
        Some(json!({"token": token})),
    )
    .await;
    assert_eq!(leave.status, StatusCode::OK);
    assert_eq!(leave.body["team2"]["claimed"], false);
    assert_eq!(leave.body["team2"]["ready"], false);

    let wieder = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &uri("claim"),
        Some(json!({"team": 2})),
    )
    .await;
    assert_eq!(wieder.status, StatusCode::OK);
}

#[tokio::test]
async fn rematch_route_liefert_neuen_raum_mit_getauschten_seiten() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 24));
    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(raum_body(0, 0)),
    )
    .await;
    let code = created.body["code"].as_str().expect("code").to_string();

    let claim1 = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/claim"),
        Some(json!({"team": 1})),
    )
    .await;
    let token1 = claim1.body["token"].as_str().expect("token").to_string();
    let claim2 = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/claim"),
        Some(json!({"team": 2})),
    )
    .await;
    let token2 = claim2.body["token"].as_str().expect("token").to_string();
    send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/ready"),
        Some(json!({"token": token1})),
    )
    .await;
    send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/ready"),
        Some(json!({"token": token2})),
    )
    .await;

    let helden = [
        "Abrams",
        "Bebop",
        "Calico",
        "Dynamo",
        "Grey Talon",
        "Haze",
        "Holliday",
        "Infernus",
        "Ivy",
        "Kelvin",
        "Lady Geist",
        "Lash",
    ];
    for held in helden {
        let zustand = lese_zustand(&ctx, &code, None).await;
        let token = if zustand.body["current_team_slot"] == 1 {
            &token1
        } else {
            &token2
        };
        let zug = send_json(
            &ctx.app,
            ip,
            Method::POST,
            &format!("/api/draft/lobbies/{code}/action"),
            Some(json!({"token": token, "hero_name": held})),
        )
        .await;
        assert_eq!(zug.status, StatusCode::OK);
    }

    let alter_raum_vor_rematch = lese_zustand(&ctx, &code, None).await;
    let alt1 = alter_raum_vor_rematch.body["team1"]["name"]
        .as_str()
        .expect("alter Slot 1")
        .to_string();
    let alt2 = alter_raum_vor_rematch.body["team2"]["name"]
        .as_str()
        .expect("alter Slot 2")
        .to_string();

    let rematch = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/rematch"),
        Some(json!({"token": token1})),
    )
    .await;
    assert_eq!(rematch.status, StatusCode::OK);
    let neuer_code = rematch.body["code"]
        .as_str()
        .expect("neuer Code")
        .to_string();
    assert_ne!(neuer_code, code);

    let neuer_raum = lese_zustand(&ctx, &neuer_code, None).await;
    assert_eq!(neuer_raum.body["phase"], "warteraum");
    assert_eq!(neuer_raum.body["team1"]["name"], alt2);
    assert_eq!(neuer_raum.body["team2"]["name"], alt1);
    assert_eq!(neuer_raum.body["bans_per_team"], 0);

    let alter_raum = lese_zustand(&ctx, &code, None).await;
    assert_eq!(alter_raum.body["rematch_code"], neuer_code);

    let fremd = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/rematch"),
        Some(json!({"token": "falsch"})),
    )
    .await;
    assert_eq!(fremd.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn lobby_retry_setzt_einen_fehler_zurueck() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 25));
    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(raum_body(2, 0)),
    )
    .await;
    let code = created.body["code"].as_str().expect("code").to_string();
    let claim = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/claim"),
        Some(json!({"team": 1})),
    )
    .await;
    let token = claim.body["token"].as_str().expect("token").to_string();

    sqlx::query(
        "UPDATE turnier.draft_sessions \
         SET lobby_status = 'fehler', lobby_error = 'Steam-Bot nicht erreichbar' \
         WHERE code = $1",
    )
    .bind(&code)
    .execute(ctx._db.pool())
    .await
    .expect("Lobby-Fehler simulieren");

    let fremd = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/lobby/retry"),
        Some(json!({"token": "falsch"})),
    )
    .await;
    assert_eq!(fremd.status, StatusCode::UNAUTHORIZED);

    let retry = send_json(
        &ctx.app,
        ip,
        Method::POST,
        &format!("/api/draft/lobbies/{code}/lobby/retry"),
        Some(json!({"token": token})),
    )
    .await;
    assert_eq!(retry.status, StatusCode::OK);
    assert_eq!(retry.body["lobby"]["status"], "angefordert");
    assert!(retry.body["lobby"]["error"].is_null());
}

#[tokio::test]
async fn zuschauer_zaehlen_ueber_den_viewer_header() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 26));
    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(raum_body(2, 0)),
    )
    .await;
    let code = created.body["code"].as_str().expect("code").to_string();

    let leere = lese_zustand(&ctx, &code, None).await;
    assert_eq!(leere.body["spectators"], 0);

    for (viewer, erwartet) in [("viewer-a", 1), ("viewer-b", 2), ("viewer-a", 2)] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/draft/lobbies/{code}"))
            .header(HOST, "localhost")
            .header("x-draft-viewer", viewer)
            .body(Body::empty())
            .expect("request");
        let response = ctx.app.clone().oneshot(request).await.expect("response");
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        let body: Value = serde_json::from_slice(&bytes).expect("json body");
        assert_eq!(body["spectators"], erwartet, "Zuschauer {viewer}");
    }
}

#[tokio::test]
async fn legacy_lobby_behaelt_ihren_vertrag() {
    let ctx = setup().await;
    let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 27));
    let created = send_json(
        &ctx.app,
        ip,
        Method::POST,
        "/api/draft/lobbies",
        Some(lobby_body()),
    )
    .await;
    assert_eq!(created.status, StatusCode::OK);
    assert!(created.body["team1_token"].is_string());

    let state = lese_zustand(&ctx, created.body["code"].as_str().unwrap(), None).await;
    assert_eq!(state.body["phase"], "laeuft");
    assert_eq!(state.body["status"], "in_progress");
    assert_eq!(state.body["team1"]["claimed"], false);
    assert_eq!(state.body["lobby"]["status"], "keine");
}
