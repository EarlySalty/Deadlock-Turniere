#![cfg(feature = "testing")]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use chrono::Utc;
use serde_json::{json, Value};
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_db::{test_pool, Pool, TestDb};

const ADMIN_ID: &str = "910000000000030001";
const SHARED_THIRD: i64 = 910_000_000_000_030_103;
const SEMI_ONLY: i64 = 910_000_000_000_030_104;

struct App {
    router: Router,
    pool: Pool,
    token: String,
    _db: TestDb,
}

async fn app() -> App {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool().clone();
    let config = Config {
        discord_admin_role_ids: "admin-role".to_string(),
        discord_bot_token: String::new(),
        steam_bridge_db_path: String::new(),
        backend_allowed_hosts: "localhost".to_string(),
        ..Config::default()
    };
    let state = AppState::build(pool.clone(), Arc::new(config))
        .await
        .expect("state");
    let token =
        turnier_auth::create_session(&pool, ADMIN_ID, "Admin", "", &["admin-role".to_string()])
            .await
            .expect("session");
    App {
        router: build_router(state),
        pool,
        token,
        _db: db,
    }
}

impl App {
    async fn call(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let body = body
            .map(|value| Body::from(value.to_string()))
            .unwrap_or_else(Body::empty);
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(HOST, "localhost")
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .body(body)
            .expect("request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("response");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .expect("body");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn set_excluded(&self, tournament_id: i64, excluded: bool) {
        let (status, body) = self
            .call(
                Method::PUT,
                &format!("/api/admin/tournaments/{tournament_id}"),
                Some(json!({ "exclude_from_leaderboard": excluded })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    async fn leaderboard_row(&self, discord_id: i64) -> Option<(Option<i64>, bool, i64)> {
        let (status, body) = self.call(Method::GET, "/api/leaderboard", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body.as_array()
            .expect("leaderboard array")
            .iter()
            .find(|row| row["discord_name"].as_str() == Some(discord_id.to_string().as_str()))
            .map(|row| {
                (
                    row["best_placement"].as_i64(),
                    row["best_placement_shared"].as_bool().expect("flag"),
                    row["total_points"].as_i64().expect("points"),
                )
            })
    }

    async fn stored_row(&self, discord_id: i64) -> Option<(Option<i64>, i64)> {
        sqlx::query_as(
            "SELECT best_placement, total_points FROM turnier.player_points WHERE discord_id = $1",
        )
        .bind(discord_id)
        .fetch_optional(&self.pool)
        .await
        .expect("stored points")
    }

    async fn assert_consistent(&self, discord_id: i64, expected: Option<(Option<i64>, bool)>) {
        let live = self.leaderboard_row(discord_id).await;
        let stored = self.stored_row(discord_id).await;
        assert_eq!(
            live.map(|(placement, shared, _)| (placement, shared)),
            expected
        );
        assert_eq!(
            live.map(|(placement, _, points)| (placement, points)),
            stored
        );
    }
}

async fn completed_tournament(pool: &Pool, name: &str, format: &str, excluded: bool) -> i64 {
    let now = Utc::now();
    sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
             (name, status, team_size, bracket_format, created_by, created_at, updated_at, \
              invite_mode, tournament_mode, series_format, exclude_from_leaderboard, \
              tournament_game_mode, auto_lobby_enabled, is_test, match_objective, \
              no_show_grace_minutes, source) \
         VALUES ($1, 'completed', 1, $5, $2, $3, $3, 'always', 'bracket_only', \
                 1, $4, 'standard', false, false, 'auto', 10, 'test') \
         RETURNING id",
    )
    .bind(name)
    .bind(910_000_000_000_030_001_i64)
    .bind(now)
    .bind(excluded)
    .bind(format)
    .fetch_one(pool)
    .await
    .expect("tournament")
}

async fn solo_team(pool: &Pool, tournament_id: i64, discord_id: i64) -> i64 {
    let team: i64 = sqlx::query_scalar(
        "INSERT INTO turnier.teams \
             (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status) \
         VALUES ($1, $2, $2, $3, now(), 'open') RETURNING id",
    )
    .bind(tournament_id)
    .bind(format!("team {discord_id}"))
    .bind(discord_id)
    .fetch_one(pool)
    .await
    .expect("team");
    sqlx::query(
        "INSERT INTO turnier.team_members \
             (team_id, discord_id, discord_name, rank_score, role, joined_at) \
         VALUES ($1, $2, 'Spieler', 0, 'captain', now())",
    )
    .bind(team)
    .bind(discord_id)
    .execute(pool)
    .await
    .expect("member");
    team
}

async fn completed_match(
    pool: &Pool,
    tournament_id: i64,
    round: i64,
    position: i64,
    teams: (i64, i64),
) {
    sqlx::query(
        "INSERT INTO turnier.bracket_matches \
             (tournament_id, round, position, bracket_type, team1_id, team2_id, winner_id, status, on_stream) \
         VALUES ($1, $2, $3, 'winners', $4, $5, $4, 'completed', true)",
    )
    .bind(tournament_id)
    .bind(round)
    .bind(position)
    .bind(teams.0)
    .bind(teams.1)
    .execute(pool)
    .await
    .expect("match");
}

async fn four_team_bracket(pool: &Pool) -> i64 {
    let id = completed_tournament(pool, "Vierer", "double_elimination", false).await;
    let first = solo_team(pool, id, 910_000_000_000_030_101).await;
    let second = solo_team(pool, id, 910_000_000_000_030_102).await;
    let third = solo_team(pool, id, SHARED_THIRD).await;
    let fourth = solo_team(pool, id, SEMI_ONLY).await;
    completed_match(pool, id, 1, 0, (first, third)).await;
    completed_match(pool, id, 1, 1, (second, fourth)).await;
    completed_match(pool, id, 2, 0, (first, second)).await;
    id
}

async fn two_team_final(pool: &Pool) -> i64 {
    let id = completed_tournament(pool, "Zweier", "single_elimination", true).await;
    let winner = solo_team(pool, id, 910_000_000_000_030_201).await;
    let runner_up = solo_team(pool, id, SHARED_THIRD).await;
    completed_match(pool, id, 1, 0, (winner, runner_up)).await;
    id
}

#[tokio::test]
async fn exclude_toggle_keeps_leaderboard_placement_and_shared_flag_consistent() {
    let app = app().await;
    let four = four_team_bracket(&app.pool).await;
    let two = two_team_final(&app.pool).await;

    app.set_excluded(two, true).await;
    app.assert_consistent(SHARED_THIRD, Some((Some(3), true)))
        .await;
    app.assert_consistent(SEMI_ONLY, Some((Some(3), true)))
        .await;

    app.set_excluded(two, false).await;
    app.assert_consistent(SHARED_THIRD, Some((Some(2), false)))
        .await;

    app.set_excluded(four, true).await;
    app.assert_consistent(SHARED_THIRD, Some((Some(2), false)))
        .await;
    app.assert_consistent(SEMI_ONLY, None).await;

    app.set_excluded(two, true).await;
    app.assert_consistent(SHARED_THIRD, None).await;
    let (status, body) = app.call(Method::GET, "/api/leaderboard", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));

    app.set_excluded(four, false).await;
    app.assert_consistent(SHARED_THIRD, Some((Some(3), true)))
        .await;
    app.assert_consistent(SEMI_ONLY, Some((Some(3), true)))
        .await;
}

#[tokio::test]
async fn leaderboard_orders_ties_by_placement_then_id_and_profile_matches_row() {
    let app = app().await;
    four_team_bracket(&app.pool).await;
    turnier_auth::create_session(&app.pool, &SHARED_THIRD.to_string(), "Dritter", "", &[])
        .await
        .expect("session");
    sqlx::query(
        "INSERT INTO turnier.rank_cache \
             (discord_id, source, steam_id, rank, rank_tier, subrank, rank_score, cached_at) \
         VALUES ($1, 'test', 'steam-dritter', 'Oracle', 5, 1, 501, now())",
    )
    .bind(SHARED_THIRD)
    .execute(&app.pool)
    .await
    .expect("rank cache");

    let (status, board) = app.call(Method::GET, "/api/leaderboard", None).await;
    assert_eq!(status, StatusCode::OK);
    let rows = board.as_array().expect("leaderboard array");
    let names: Vec<&str> = rows
        .iter()
        .map(|row| row["discord_name"].as_str().expect("name"))
        .collect();
    let semi_only = SEMI_ONLY.to_string();
    assert_eq!(
        names,
        vec![
            "910000000000030101",
            "910000000000030102",
            "Dritter",
            semi_only.as_str()
        ]
    );
    let positions: Vec<i64> = rows
        .iter()
        .map(|row| row["rank_position"].as_i64().expect("position"))
        .collect();
    assert_eq!(positions, vec![1, 2, 3, 4]);
    assert_eq!(rows[2]["total_points"], rows[3]["total_points"]);
    assert_eq!(rows[2]["rank"], "Oracle");
    assert_eq!(rows[2]["best_placement"], 3);
    assert_eq!(rows[2]["best_placement_shared"], true);
    assert_eq!(rows[3]["rank"], Value::Null);

    let (status, profile) = app.call(Method::GET, "/api/players/Dritter", None).await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    assert_eq!(profile["total_points"], rows[2]["total_points"]);
    assert_eq!(profile["best_placement"], rows[2]["best_placement"]);
    assert_eq!(profile["tournaments_played"], 1);
}
