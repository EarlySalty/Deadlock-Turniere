use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, HOST};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_config::Config;

const ROUTE: &str = "/internal/turnier/v1/mcp";
const TOKEN: &str = "mcp-internal-token";

fn config(token: &str) -> Config {
    Config {
        turnier_internal_api_token: token.to_string(),
        discord_bot_token: String::new(),
        steam_bridge_db_path: String::new(),
        backend_allowed_hosts: "localhost".to_string(),
        ..Config::default()
    }
}

async fn router_with(pool: sqlx::PgPool, token: &str) -> Router {
    let state = AppState::build(pool, Arc::new(config(token)))
        .await
        .expect("state build");
    build_router(state)
}

async fn offline_router(token: &str) -> Router {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(500))
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .expect("lazy pool");
    router_with(pool, token).await
}

struct Call<'a> {
    peer: IpAddr,
    token: Option<&'a str>,
    extra: &'a [(&'a str, &'a str)],
    uri: &'a str,
    body: String,
}

impl<'a> Call<'a> {
    fn local(body: Value) -> Self {
        Self {
            peer: IpAddr::V4(Ipv4Addr::LOCALHOST),
            token: Some(TOKEN),
            extra: &[],
            uri: ROUTE,
            body: body.to_string(),
        }
    }
}

async fn send(app: &Router, call: Call<'_>) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(call.uri)
        .header(HOST, "localhost")
        .header(CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(SocketAddr::new(call.peer, 45_000)));
    if let Some(token) = call.token {
        builder = builder.header("X-Internal-Token", token);
    }
    for (name, value) in call.extra {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(call.body)).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, body)
}

fn rpc(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 7, "method": method, "params": params})
}

fn tool_call(name: &str, arguments: Value) -> Value {
    rpc("tools/call", json!({"name": name, "arguments": arguments}))
}

#[tokio::test]
async fn boundary_rejects_remote_peers_missing_or_foreign_credentials() {
    let app = offline_router(TOKEN).await;
    let list = rpc("tools/list", json!({}));

    let mut remote = Call::local(list.clone());
    remote.peer = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
    assert_eq!(send(&app, remote).await.0, StatusCode::FORBIDDEN);

    let mut missing = Call::local(list.clone());
    missing.token = None;
    assert_eq!(send(&app, missing).await.0, StatusCode::UNAUTHORIZED);

    let mut wrong = Call::local(list.clone());
    wrong.token = Some("other-token");
    assert_eq!(send(&app, wrong).await.0, StatusCode::UNAUTHORIZED);

    let session = format!("Bearer {}", "a".repeat(64));
    let user_credentials = [
        (AUTHORIZATION.as_str(), session.as_str()),
        (COOKIE.as_str(), "session_token=fixture"),
    ];
    let mut user = Call::local(list.clone());
    user.token = None;
    user.extra = &user_credentials;
    assert_eq!(send(&app, user).await.0, StatusCode::UNAUTHORIZED);

    let mut garbage = Call::local(Value::Null);
    garbage.token = None;
    garbage.body = "{not json".to_string();
    assert_eq!(send(&app, garbage).await.0, StatusCode::UNAUTHORIZED);

    let unconfigured = offline_router("").await;
    let mut empty = Call::local(list);
    empty.token = Some("");
    assert_eq!(send(&unconfigured, empty).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn no_public_mcp_route_exists() {
    let app = offline_router(TOKEN).await;
    for uri in ["/api/mcp", "/mcp", "/api/internal/turnier/v1/mcp"] {
        let mut call = Call::local(rpc("tools/list", json!({})));
        call.uri = uri;
        let (status, _) = send(&app, call).await;
        assert!(
            status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED,
            "{uri} answered {status}"
        );
    }
    let get = Request::builder()
        .method(Method::GET)
        .uri(ROUTE)
        .header(HOST, "localhost")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            45_000,
        )))
        .header("X-Internal-Token", TOKEN)
        .body(Body::empty())
        .expect("request");
    let response = app.clone().oneshot(get).await.expect("response");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn boundary_runs_before_the_body_is_read() {
    let app = offline_router(TOKEN).await;
    let mut anonymous = Call::local(Value::Null);
    anonymous.token = None;
    anonymous.body = "x".repeat(200_000);
    assert_eq!(send(&app, anonymous).await.0, StatusCode::UNAUTHORIZED);

    let mut oversized = Call::local(Value::Null);
    oversized.body = "x".repeat(200_000);
    assert_eq!(send(&app, oversized).await.0, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn protocol_lists_only_read_tools_and_rejects_everything_else() {
    let app = offline_router(TOKEN).await;

    let (status, body) = send(&app, Call::local(rpc("initialize", json!({})))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["protocolVersion"], "2025-06-18");

    let (_, body) = send(&app, Call::local(rpc("tools/list", json!({})))).await;
    let names: Vec<&str> = body["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(
        names,
        [
            "turnier_list",
            "turnier_get",
            "turnier_bracket",
            "turnier_groups"
        ]
    );

    for name in [
        "turnier_signup",
        "turnier_checkin",
        "turnier_my_status",
        "turnier_admin_list",
        "turnier_create",
        "turnier_update",
        "turnier_advance",
    ] {
        let (status, body) = send(
            &app,
            Call::local(tool_call(name, json!({"tournament_id": 1}))),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["error"]["code"], -32602, "{name}");
    }

    let (_, body) = send(&app, Call::local(rpc("resources/list", json!({})))).await;
    assert_eq!(body["error"]["code"], -32601);

    let (_, body) = send(&app, Call::local(json!({"id": 1, "method": "tools/list"}))).await;
    assert_eq!(body["error"]["code"], -32600);

    let (status, _) = send(
        &app,
        Call::local(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let (_, body) = send(
        &app,
        Call::local(tool_call(
            "turnier_get",
            json!({"tournament_id": 1, "user_id": "42"}),
        )),
    )
    .await;
    assert_eq!(body["result"]["isError"], true);

    let (_, body) = send(
        &app,
        Call::local(json!({"jsonrpc": "2.0", "id": {"x": 1}, "method": "ping"})),
    )
    .await;
    assert_eq!(body["error"]["code"], -32600);
    assert_eq!(body["id"], Value::Null);

    let (status, _) = send(
        &app,
        Call::local(json!({"jsonrpc": "2.0", "method": "ping"})),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);

    for id in [json!(9_223_372_036_854_775_808u64), json!(1.0), json!(null)] {
        let (_, body) = send(
            &app,
            Call::local(tool_call("turnier_bracket", json!({"tournament_id": id}))),
        )
        .await;
        assert_eq!(body["result"]["isError"], true);
        assert_eq!(
            body["result"]["content"][0]["text"],
            "tournament_id muss eine positive Zahl sein"
        );
    }

    let (_, body) = send(&app, Call::local(tool_call("turnier_list", Value::Null))).await;
    assert_eq!(
        body["result"]["content"][0]["text"],
        "Turnierdaten gerade nicht verfügbar"
    );
}

#[tokio::test]
async fn database_failure_returns_a_fixed_error_without_details() {
    let app = offline_router(TOKEN).await;
    let (status, body) = send(&app, Call::local(tool_call("turnier_list", json!({})))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["result"]["isError"], true);
    let text = body.to_string();
    assert!(text.contains("Turnierdaten gerade nicht verfügbar"));
    for leak in ["postgres", "127.0.0.1", "unused", "pool", TOKEN] {
        assert!(!text.contains(leak), "{leak} leaked: {text}");
    }
}

#[cfg(feature = "testing")]
mod database {
    use super::*;
    use turnier_db::{test_pool, Pool};

    const ADMIN_ID: i64 = 910_000_000_000_020_001;
    const CAPTAIN_ID: i64 = 910_000_000_000_020_002;
    const GUEST_CAPTAIN_ID: i64 = -910_000_000_000_020_003;
    const GUEST_SOLO_ID: i64 = -910_000_000_000_020_004;
    const PLAYER_STEAM_ID: &str = "76561198000000042";
    const PARTY_CODE: &str = "PARTYCODE42";

    async fn seed_tournament(pool: &Pool, name: &str, status: &str, is_test: bool) -> i64 {
        sqlx::query_scalar(
            r#"INSERT INTO turnier."tournaments"
             (name, status, description, team_size, registration_start, registration_end,
              group_phase_start, bracket_start, bracket_format, created_by, created_at, updated_at,
              invite_mode, invite_window_start, invite_window_end, lobby_settings, checkin_start,
              tournament_mode, series_format, exclude_from_leaderboard, reminder_offsets,
              tournament_game_mode, auto_lobby_enabled, is_test, rules, final_series_format,
              match_objective, no_show_grace_minutes, start_reminder_offsets, source)
             VALUES ($1, $2, 'Offenes Turnier', 2, now(), now() + interval '1 hour', NULL, NULL,
                     'single_elimination', $3, now(), now(), 'always', NULL, NULL, NULL, now(),
                     'bracket_only', 1, false, '[1440]'::jsonb, 'standard', false, $4,
                     'Fair spielen', 1, 'match_win', 10, '[15]'::jsonb, 'test_seed')
             RETURNING id"#,
        )
        .bind(name)
        .bind(status)
        .bind(ADMIN_ID)
        .bind(is_test)
        .fetch_one(pool)
        .await
        .expect("seed tournament")
    }

    async fn seed_team(
        pool: &Pool,
        tournament_id: i64,
        name: &str,
        captain: i64,
        member: &str,
    ) -> i64 {
        let team_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO turnier."teams"
             (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status)
             VALUES ($1, $2, $3, $4, now(), 'open') RETURNING id"#,
        )
        .bind(tournament_id)
        .bind(name)
        .bind(name.to_lowercase())
        .bind(captain)
        .fetch_one(pool)
        .await
        .expect("seed team");
        sqlx::query(
            r#"INSERT INTO turnier."team_members"
             (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
             VALUES ($1, $2, $3, $4, NULL, 0, 'captain', now())"#,
        )
        .bind(team_id)
        .bind(captain)
        .bind(member)
        .bind(PLAYER_STEAM_ID)
        .execute(pool)
        .await
        .expect("seed member");
        team_id
    }

    async fn seed_match(
        pool: &Pool,
        tournament_id: i64,
        team1: i64,
        team2: i64,
        winner: i64,
    ) -> i64 {
        sqlx::query_scalar(
            r#"INSERT INTO turnier."bracket_matches"
             (tournament_id, round, position, bracket_type, team1_id, team2_id, winner_id, status,
              on_stream, party_code, steam_party_id)
             VALUES ($1, 1, 1, 'winners', $2, $3, $4, 'completed', false, $5, $5) RETURNING id"#,
        )
        .bind(tournament_id)
        .bind(team1)
        .bind(team2)
        .bind(winner)
        .bind(PARTY_CODE)
        .fetch_one(pool)
        .await
        .expect("seed bracket match")
    }

    async fn seed_game(pool: &Pool, match_id: i64, number: i64, winner_team: i64) {
        sqlx::query(
            r#"INSERT INTO turnier."match_games"
             (bracket_match_id, game_number, status, party_code, winner_team, created_at)
             VALUES ($1, $2, 'completed', $3, $4, now())"#,
        )
        .bind(match_id)
        .bind(number)
        .bind(PARTY_CODE)
        .bind(winner_team)
        .execute(pool)
        .await
        .expect("seed series game");
    }

    async fn seed_group(pool: &Pool, tournament_id: i64, winner: i64, loser: i64) {
        let group_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO turnier."groups" (tournament_id, name, seeding_order)
             VALUES ($1, 'Gruppe A', 1) RETURNING id"#,
        )
        .bind(tournament_id)
        .fetch_one(pool)
        .await
        .expect("seed group");
        for (team, wins, losses, points) in [(winner, 1, 0, 3), (loser, 0, 1, 0)] {
            sqlx::query(
                r#"INSERT INTO turnier."group_teams" (group_id, team_id, wins, losses, points)
                 VALUES ($1, $2, $3, $4, $5)"#,
            )
            .bind(group_id)
            .bind(team)
            .bind(wins)
            .bind(losses)
            .bind(points)
            .execute(pool)
            .await
            .expect("seed group team");
        }
        sqlx::query(
            r#"INSERT INTO turnier."group_matches"
             (group_id, team1_id, team2_id, winner_id, status, party_code, steam_party_id)
             VALUES ($1, $2, $3, $2, 'completed', $4, $4)"#,
        )
        .bind(group_id)
        .bind(winner)
        .bind(loser)
        .bind(PARTY_CODE)
        .execute(pool)
        .await
        .expect("seed group match");
    }

    async fn seed_solo(pool: &Pool, tournament_id: i64) {
        sqlx::query(
            r#"INSERT INTO turnier."tournament_signups"
             (tournament_id, discord_id, discord_name, rank_score, signed_up_at)
             VALUES ($1, $2, 'Gastspielerin Mara', 0, now())"#,
        )
        .bind(tournament_id)
        .bind(GUEST_SOLO_ID)
        .execute(pool)
        .await
        .expect("seed guest signup");
    }

    fn structured(body: &Value) -> &Value {
        assert_eq!(body["result"]["isError"], false, "{body}");
        &body["result"]["structuredContent"]
    }

    fn assert_minimised(text: &str) {
        for leak in [
            "Jörg",
            "Gastspielerin Mara",
            "Spieler Bravo",
            PLAYER_STEAM_ID,
            PARTY_CODE,
            &ADMIN_ID.to_string(),
            &CAPTAIN_ID.to_string(),
            &GUEST_CAPTAIN_ID.to_string(),
            &GUEST_SOLO_ID.to_string(),
            "created_by",
            "lobby_settings",
        ] {
            assert!(!text.contains(leak), "{leak} leaked: {text}");
        }
    }

    #[tokio::test]
    async fn read_contract_hides_drafts_tests_and_guest_data() {
        let db = test_pool().await.expect("central test pool");
        let pool = db.pool().clone();
        let app = router_with(pool.clone(), TOKEN).await;

        let public = seed_tournament(&pool, "MCP Offen", "bracket", false).await;
        let draft = seed_tournament(&pool, "MCP Entwurf", "draft", false).await;
        let test = seed_tournament(&pool, "MCP Test", "bracket", true).await;
        let guest_team = seed_team(&pool, public, "Team Jörg", GUEST_CAPTAIN_ID, "Jörg").await;
        let bravo = seed_team(&pool, public, "Team Bravo", CAPTAIN_ID, "Spieler Bravo").await;
        let final_match = seed_match(&pool, public, guest_team, bravo, guest_team).await;
        seed_game(&pool, final_match, 1, 1).await;
        seed_game(&pool, final_match, 2, 2).await;
        seed_game(&pool, final_match, 3, 1).await;
        seed_group(&pool, public, guest_team, bravo).await;
        seed_solo(&pool, public).await;
        let draft_team = seed_team(&pool, draft, "Team Geheim", CAPTAIN_ID, "Spieler Bravo").await;
        seed_match(&pool, draft, draft_team, draft_team, draft_team).await;

        let (_, body) = send(&app, Call::local(tool_call("turnier_list", json!({})))).await;
        let listed: Vec<i64> = structured(&body)["tournaments"]
            .as_array()
            .expect("tournaments")
            .iter()
            .filter_map(|t| t["id"].as_i64())
            .collect();
        assert!(listed.contains(&public));
        assert!(!listed.contains(&draft) && !listed.contains(&test));
        assert_minimised(&body.to_string());

        let (_, body) = send(
            &app,
            Call::local(tool_call("turnier_get", json!({"tournament_id": public}))),
        )
        .await;
        let detail = structured(&body);
        assert_eq!(detail["tournament"]["name"], "MCP Offen");
        assert_eq!(detail["solo_signup_count"], 1);
        let teams: Vec<&str> = detail["teams"]
            .as_array()
            .expect("teams")
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert_eq!(teams, [format!("Team {guest_team}").as_str(), "Team Bravo"]);
        assert_minimised(&body.to_string());

        let (_, body) = send(
            &app,
            Call::local(tool_call(
                "turnier_bracket",
                json!({"tournament_id": public}),
            )),
        )
        .await;
        let matches = &structured(&body)["matches"];
        assert_eq!(matches[0]["team1"], format!("Team {guest_team}"));
        assert_eq!(matches[0]["team2"], "Team Bravo");
        assert_eq!(matches[0]["winner"], format!("Team {guest_team}"));
        assert_eq!(matches[0]["series_wins_team1"], 2);
        assert_eq!(matches[0]["series_wins_team2"], 1);
        assert_minimised(&body.to_string());

        let (_, body) = send(
            &app,
            Call::local(tool_call(
                "turnier_groups",
                json!({"tournament_id": public}),
            )),
        )
        .await;
        let group = &structured(&body)["groups"][0];
        assert_eq!(group["name"], "Gruppe A");
        let standings: Vec<&str> = group["standings"]
            .as_array()
            .expect("standings")
            .iter()
            .filter_map(|row| row["team"].as_str())
            .collect();
        assert!(standings.contains(&format!("Team {guest_team}").as_str()));
        assert!(standings.contains(&"Team Bravo"));
        assert_eq!(group["matches"][0]["winner"], format!("Team {guest_team}"));
        assert_minimised(&body.to_string());

        for tool in ["turnier_get", "turnier_bracket", "turnier_groups"] {
            for id in [draft, test, i64::from(i32::MAX)] {
                let (_, body) = send(
                    &app,
                    Call::local(tool_call(tool, json!({"tournament_id": id}))),
                )
                .await;
                assert_eq!(body["result"]["isError"], true, "{tool} {id}");
                assert_eq!(
                    body["result"]["content"][0]["text"],
                    "Turnier nicht gefunden"
                );
                let text = body.to_string();
                assert!(!text.contains("Team Geheim") && !text.contains("MCP Entwurf"));
            }
        }
    }
}
