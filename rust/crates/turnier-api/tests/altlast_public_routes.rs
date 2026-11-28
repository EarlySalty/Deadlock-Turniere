#![cfg(feature = "testing")]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_db::{test_pool, Pool, TestDb};

const ADMIN_ID: &str = "910000000000120001";
const CAPTAIN_ID: &str = "910000000000120002";
const PLAYER_ID: &str = "910000000000120003";

struct TestApp {
    app: Router,
    _db: TestDb,
    pool: Pool,
}

async fn setup() -> TestApp {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool().clone();
    let config = Config {
        discord_admin_role_ids: "admin-role".to_string(),
        discord_tournament_admin_role_ids: String::new(),
        discord_mod_role_ids: "mod-role".to_string(),
        discord_bot_token: String::new(),
        steam_bridge_db_path: String::new(),
        backend_allowed_hosts: "localhost".to_string(),
        ..Config::default()
    };
    let state = AppState::build(pool.clone(), Arc::new(config))
        .await
        .expect("state build");
    TestApp {
        app: build_router(state),
        _db: db,
        pool,
    }
}

async fn session(pool: &Pool, discord_id: &str, name: &str) -> String {
    turnier_auth::create_session(pool, discord_id, name, "", &[])
        .await
        .expect("session")
}

async fn send_json(
    app: &Router,
    token: Option<&str>,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let body = body
        .map(|value| Body::from(value.to_string()))
        .unwrap_or_else(Body::empty);
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            40_000,
        )))
        .header(HOST, "localhost")
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    if bytes.is_empty() {
        return (status, Value::Null);
    }
    (status, serde_json::from_slice(&bytes).expect("json body"))
}

fn id(value: &str) -> i64 {
    value.parse::<i64>().expect("numeric snowflake")
}

async fn seed_profile(pool: &Pool, discord_id: &str, auto_accept: bool) {
    sqlx::query(
        r#"INSERT INTO turnier."user_profiles"
         (discord_id, bio, invite_auto_accept, notify_discord_dm, notify_browser, updated_at,
          display_name, avatar_filename, notify_match_start, notify_checkin, notify_team_invite,
          notify_tournament_news, notify_registration_reminder)
         VALUES ($1, NULL, $2, true, true, now(), NULL, NULL, true, true, true, true, true)"#,
    )
    .bind(id(discord_id))
    .bind(auto_accept)
    .execute(pool)
    .await
    .expect("seed profile");
}

async fn seed_consent(pool: &Pool, discord_id: &str) {
    sqlx::query(
        r#"INSERT INTO turnier."user_consents" (discord_id, consented_at, consent_version)
         VALUES ($1, now(), 2)"#,
    )
    .bind(id(discord_id))
    .execute(pool)
    .await
    .expect("seed consent");
}

async fn seed_tournament(pool: &Pool, name: &str, status: &str, team_size: i64) -> i64 {
    sqlx::query_scalar(
        r#"INSERT INTO turnier."tournaments"
         (name, status, description, team_size, registration_start, registration_end,
          group_phase_start, bracket_start, bracket_format, created_by, created_at, updated_at,
          invite_mode, invite_window_start, invite_window_end, lobby_settings, checkin_start,
          tournament_mode, series_format, exclude_from_leaderboard, reminder_offsets,
          tournament_game_mode, auto_lobby_enabled, is_test, rules, final_series_format,
          match_objective, no_show_grace_minutes, start_reminder_offsets, source)
         VALUES ($1, $2, NULL, $3, now(), now() + interval '1 hour', NULL, NULL,
                 'single_elimination', $4, now(), now(), 'always', NULL, NULL, NULL, now(),
                 'bracket_only', 1, false, $5, 'standard', false, true, NULL, 1,
                 'match_win', 10, $6, 'test_seed')
         RETURNING id"#,
    )
    .bind(name)
    .bind(status)
    .bind(team_size)
    .bind(id(ADMIN_ID))
    .bind(json!([1440, 120, 15]))
    .bind(json!([120, 15]))
    .fetch_one(pool)
    .await
    .expect("seed tournament")
}

async fn seed_team(pool: &Pool, tournament_id: i64, name: &str, captain: &str) -> i64 {
    let team_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO turnier."teams"
         (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status)
         VALUES ($1, $2, $3, $4, now(), 'open') RETURNING id"#,
    )
    .bind(tournament_id)
    .bind(name)
    .bind(name.to_ascii_lowercase())
    .bind(id(captain))
    .fetch_one(pool)
    .await
    .expect("seed team");
    sqlx::query(
        r#"INSERT INTO turnier."team_members"
         (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
         VALUES ($1, $2, $3, NULL, NULL, 0, 'captain', now())"#,
    )
    .bind(team_id)
    .bind(id(captain))
    .bind(name)
    .execute(pool)
    .await
    .expect("seed captain");
    team_id
}

async fn seed_solo_signup(pool: &Pool, tournament_id: i64, discord_id: &str, name: &str) {
    sqlx::query(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, rank_score, signed_up_at)
         VALUES ($1, $2, $3, 0, now())"#,
    )
    .bind(tournament_id)
    .bind(id(discord_id))
    .bind(name)
    .execute(pool)
    .await
    .expect("seed signup");
}

#[tokio::test]
async fn direct_team_invite_does_not_add_player_without_acceptance() {
    let ctx = setup().await;
    let captain_token = session(&ctx.pool, CAPTAIN_ID, "Captain").await;
    for discord_id in [CAPTAIN_ID, PLAYER_ID] {
        seed_consent(&ctx.pool, discord_id).await;
        seed_profile(&ctx.pool, discord_id, false).await;
    }
    let tournament_id = seed_tournament(&ctx.pool, "Direct Invite", "registration", 3).await;
    let team_id = seed_team(&ctx.pool, tournament_id, "Captain Team", CAPTAIN_ID).await;
    seed_solo_signup(&ctx.pool, tournament_id, PLAYER_ID, "Player").await;

    let (status, _) = send_json(
        &ctx.app,
        Some(&captain_token),
        Method::POST,
        &format!("/api/tournaments/{tournament_id}/teams/{team_id}/invite/{PLAYER_ID}"),
        None,
    )
    .await;
    assert!(
        matches!(
            status,
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ),
        "removed direct invite route answered {status}"
    );

    let member_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM turnier."team_members" WHERE team_id = $1 AND discord_id = $2"#,
    )
    .bind(team_id)
    .bind(id(PLAYER_ID))
    .fetch_one(&ctx.pool)
    .await
    .expect("membership count");
    assert_eq!(member_count, 0);
}
