#![cfg(feature = "testing")]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_db::{test_pool, Pool, TestDb};
use turnier_engine::generate_league_plan_in_tx;
use turnier_match::ApplyGroupParams;

const ADMIN_ID: &str = "910000000000020001";
const FIRST_PLAYER_ID: i64 = 910_000_000_000_020_100;

struct App {
    router: Router,
    state: AppState,
    pool: Pool,
    token: String,
    _db: TestDb,
}

struct League {
    id: i64,
    group_id: i64,
    teams: Vec<i64>,
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
        router: build_router(state.clone()),
        state,
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
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    async fn submit(&self, league: &League, match_id: i64, winner_id: i64) -> StatusCode {
        let uri = format!(
            "/api/admin/tournaments/{}/matches/{match_id}/result",
            league.id
        );
        let body = json!({ "winner_id": winner_id });
        self.call(Method::POST, &uri, Some(body)).await.0
    }

    async fn reset(&self, league: &League, match_id: i64) -> StatusCode {
        let uri = format!(
            "/api/admin/tournaments/{}/group-matches/{match_id}/reset",
            league.id
        );
        self.call(Method::POST, &uri, None).await.0
    }

    async fn advance(&self, league: &League) -> (StatusCode, Value) {
        let uri = format!("/api/admin/tournaments/{}/advance", league.id);
        self.call(Method::POST, &uri, None).await
    }
}

fn plan_start() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 7, 18, 0, 0)
        .single()
        .expect("valid time")
}

async fn league(pool: &Pool, team_count: i64, mode: &str) -> League {
    let now = Utc::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
             (name, status, team_size, bracket_format, created_by, created_at, updated_at, \
              invite_mode, tournament_mode, series_format, exclude_from_leaderboard, \
              tournament_game_mode, auto_lobby_enabled, is_test, match_objective, \
              no_show_grace_minutes, source, group_phase_start) \
         VALUES ('Liga', 'group_phase', 1, 'single_elimination', $1, $2, $2, 'always', $3, 1, \
                 false, 'standard', false, false, 'auto', 10, 'test', $4) \
         RETURNING id",
    )
    .bind(910_000_000_000_020_001_i64)
    .bind(now)
    .bind(mode)
    .bind(plan_start())
    .fetch_one(pool)
    .await
    .expect("tournament");
    let mut teams = Vec::new();
    for index in 0..team_count {
        let team: i64 = sqlx::query_scalar(
            "INSERT INTO turnier.teams \
                 (tournament_id, name, name_key, captain_discord_id, created_at, \
                  recruitment_status) \
             VALUES ($1, $2, $3, $4, $5, 'open') RETURNING id",
        )
        .bind(id)
        .bind(format!("Team {index}"))
        .bind(format!("team {index}"))
        .bind(FIRST_PLAYER_ID + index)
        .bind(now + Duration::seconds(index))
        .fetch_one(pool)
        .await
        .expect("team");
        sqlx::query(
            "INSERT INTO turnier.team_members \
                 (team_id, discord_id, discord_name, rank_score, role, joined_at) \
             VALUES ($1, $2, 'Spieler', 0, 'captain', $3)",
        )
        .bind(team)
        .bind(FIRST_PLAYER_ID + index)
        .bind(now)
        .execute(pool)
        .await
        .expect("member");
        teams.push(team);
    }
    let mut tx = pool.begin().await.expect("tx");
    let plan = generate_league_plan_in_tx(&mut tx, id, 50)
        .await
        .expect("plan");
    tx.commit().await.expect("commit");
    League {
        id,
        group_id: plan.group_id,
        teams,
    }
}

async fn match_between(pool: &Pool, league: &League, first: i64, second: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT id FROM turnier.group_matches WHERE group_id = $1 \
         AND ((team1_id = $2 AND team2_id = $3) OR (team1_id = $3 AND team2_id = $2))",
    )
    .bind(league.group_id)
    .bind(first)
    .bind(second)
    .fetch_one(pool)
    .await
    .expect("match")
}

async fn standing(pool: &Pool, league: &League, team_id: i64) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT wins, losses, points FROM turnier.group_teams \
         WHERE group_id = $1 AND team_id = $2",
    )
    .bind(league.group_id)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .expect("standing")
}

async fn all_match_ids(pool: &Pool, league: &League) -> Vec<(i64, i64)> {
    sqlx::query_as("SELECT id, team1_id FROM turnier.group_matches WHERE group_id = $1 ORDER BY id")
        .bind(league.group_id)
        .fetch_all(pool)
        .await
        .expect("matches")
}

#[tokio::test]
async fn manual_entry_correction_and_reset_restore_points_exactly() {
    let app = app().await;
    let liga = league(&app.pool, 4, "league").await;
    let (a, b, c) = (liga.teams[0], liga.teams[1], liga.teams[2]);
    let ab = match_between(&app.pool, &liga, a, b).await;
    let ac = match_between(&app.pool, &liga, a, c).await;

    assert_eq!(app.submit(&liga, ab, a).await, StatusCode::OK);
    assert_eq!(app.submit(&liga, ac, a).await, StatusCode::OK);
    assert_eq!(standing(&app.pool, &liga, a).await, (2, 0, 4));
    assert_eq!(standing(&app.pool, &liga, b).await, (0, 1, -1));
    assert_eq!(standing(&app.pool, &liga, c).await, (0, 1, -1));

    assert_eq!(app.submit(&liga, ab, b).await, StatusCode::OK);
    assert_eq!(standing(&app.pool, &liga, a).await, (1, 1, 1));
    assert_eq!(standing(&app.pool, &liga, b).await, (1, 0, 2));
    let results: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnier.match_results WHERE group_match_id = $1")
            .bind(ab)
            .fetch_one(&app.pool)
            .await
            .expect("results");
    assert_eq!(results, 1);

    assert_eq!(app.submit(&liga, ab, b).await, StatusCode::OK);
    assert_eq!(standing(&app.pool, &liga, b).await, (1, 0, 2));

    assert_eq!(app.reset(&liga, ab).await, StatusCode::OK);
    assert_eq!(standing(&app.pool, &liga, a).await, (1, 0, 2));
    assert_eq!(standing(&app.pool, &liga, b).await, (0, 0, 0));
    let row: (String, Option<i64>, Option<DateTime<Utc>>) = sqlx::query_as(
        "SELECT status, winner_id, played_at FROM turnier.group_matches WHERE id = $1",
    )
    .bind(ab)
    .fetch_one(&app.pool)
    .await
    .expect("row");
    assert_eq!(row, ("pending".to_string(), None, None));
}

#[tokio::test]
async fn automatic_result_is_corrected_and_reset_through_the_admin_paths() {
    let app = app().await;
    let liga = league(&app.pool, 2, "league").await;
    let (a, b) = (liga.teams[0], liga.teams[1]);
    let ab = match_between(&app.pool, &liga, a, b).await;

    app.state
        .match_manager
        .apply_group_match_result(
            liga.id,
            ab,
            ApplyGroupParams {
                winner_id: Some(a),
                source: "automatic".to_string(),
                ..Default::default()
            },
        )
        .await
        .expect("automatic");
    assert_eq!(standing(&app.pool, &liga, a).await, (1, 0, 2));

    assert_eq!(app.submit(&liga, ab, b).await, StatusCode::OK);
    assert_eq!(standing(&app.pool, &liga, a).await, (0, 1, -1));
    assert_eq!(standing(&app.pool, &liga, b).await, (1, 0, 2));

    assert_eq!(app.reset(&liga, ab).await, StatusCode::OK);
    assert_eq!(standing(&app.pool, &liga, a).await, (0, 0, 0));
    assert_eq!(standing(&app.pool, &liga, b).await, (0, 0, 0));
}

#[tokio::test]
async fn reset_with_stored_discord_channel_does_not_fail() {
    let app = app().await;
    let liga = league(&app.pool, 2, "league").await;
    let ab = match_between(&app.pool, &liga, liga.teams[0], liga.teams[1]).await;
    sqlx::query("UPDATE turnier.group_matches SET discord_channel_id = 123456789 WHERE id = $1")
        .bind(ab)
        .execute(&app.pool)
        .await
        .expect("channel");

    assert_eq!(app.reset(&liga, ab).await, StatusCode::OK);

    let channel: Option<i64> =
        sqlx::query_scalar("SELECT discord_channel_id FROM turnier.group_matches WHERE id = $1")
            .bind(ab)
            .fetch_one(&app.pool)
            .await
            .expect("channel");
    assert_eq!(channel, None);
}

#[tokio::test]
async fn reset_on_a_finished_league_keeps_the_discord_channel() {
    let app = app().await;
    let liga = league(&app.pool, 2, "league").await;
    let ab = match_between(&app.pool, &liga, liga.teams[0], liga.teams[1]).await;
    assert_eq!(app.submit(&liga, ab, liga.teams[0]).await, StatusCode::OK);
    sqlx::query("UPDATE turnier.group_matches SET discord_channel_id = 123456789 WHERE id = $1")
        .bind(ab)
        .execute(&app.pool)
        .await
        .expect("channel");
    set_status(&app.pool, &liga, "completed").await;

    assert_eq!(app.reset(&liga, ab).await, StatusCode::BAD_REQUEST);
    assert_eq!(channel_delete_attempts(&app.pool).await, 0);

    set_status(&app.pool, &liga, "group_phase").await;
    assert_eq!(app.reset(&liga, ab).await, StatusCode::OK);
    assert_eq!(channel_delete_attempts(&app.pool).await, 1);
}

async fn set_status(pool: &Pool, league: &League, status: &str) {
    sqlx::query("UPDATE turnier.tournaments SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(league.id)
        .execute(pool)
        .await
        .expect("status");
}

async fn channel_delete_attempts(pool: &Pool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.discord_tasks WHERE payload->>'channel_id' = '123456789'",
    )
    .fetch_one(pool)
    .await
    .expect("delete attempts")
}

#[tokio::test]
async fn group_stage_manual_path_keeps_plus_three_and_refuses_changes() {
    let app = app().await;
    let liga = league(&app.pool, 2, "group_stage").await;
    let (a, b) = (liga.teams[0], liga.teams[1]);
    let ab = match_between(&app.pool, &liga, a, b).await;

    assert_eq!(app.submit(&liga, ab, a).await, StatusCode::OK);
    assert_eq!(standing(&app.pool, &liga, a).await, (1, 0, 3));
    assert_eq!(standing(&app.pool, &liga, b).await, (0, 1, 0));

    assert_eq!(app.submit(&liga, ab, b).await, StatusCode::BAD_REQUEST);
    assert_eq!(app.reset(&liga, ab).await, StatusCode::BAD_REQUEST);
    assert_eq!(standing(&app.pool, &liga, a).await, (1, 0, 3));
}

#[tokio::test]
async fn finish_needs_all_results_uses_the_table_and_locks_results() {
    let app = app().await;
    let liga = league(&app.pool, 3, "league").await;
    let (a, b, c) = (liga.teams[0], liga.teams[1], liga.teams[2]);

    let bracket_uri = format!("/api/admin/tournaments/{}/bracket/generate", liga.id);
    let (bracket, _) = app.call(Method::POST, &bracket_uri, None).await;
    assert_eq!(bracket, StatusCode::BAD_REQUEST);

    let ab = match_between(&app.pool, &liga, a, b).await;
    assert_eq!(app.submit(&liga, ab, c).await, StatusCode::BAD_REQUEST);
    assert_eq!(app.submit(&liga, ab, b).await, StatusCode::OK);

    let (refused, body) = app.advance(&liga).await;
    assert_eq!(refused, StatusCode::BAD_REQUEST);
    assert!(body.to_string().contains("Offen: 2"), "{body}");

    let bc = match_between(&app.pool, &liga, b, c).await;
    let ac = match_between(&app.pool, &liga, a, c).await;
    assert_eq!(app.submit(&liga, bc, b).await, StatusCode::OK);
    assert_eq!(app.submit(&liga, ac, c).await, StatusCode::OK);

    let (done, body) = app.advance(&liga).await;
    assert_eq!(done, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "completed");

    let points: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT discord_id, total_points, matches_won, best_placement \
         FROM turnier.player_points ORDER BY best_placement",
    )
    .fetch_all(&app.pool)
    .await
    .expect("player points");
    let member = |offset: i64| FIRST_PLAYER_ID + offset;
    assert_eq!(points.len(), 3);
    assert_eq!(
        (points[0].0, points[0].2, points[0].3),
        (member(1), 2, 1),
        "{points:?}"
    );
    assert_eq!(
        (points[1].0, points[1].2, points[1].3),
        (member(2), 1, 2),
        "{points:?}"
    );
    assert_eq!(
        (points[2].0, points[2].2, points[2].3),
        (member(0), 0, 3),
        "{points:?}"
    );
    assert_eq!(points[2].1, 3 + 1);

    assert_eq!(app.submit(&liga, ab, a).await, StatusCode::BAD_REQUEST);
    assert_eq!(app.reset(&liga, ab).await, StatusCode::BAD_REQUEST);
    assert_eq!(standing(&app.pool, &liga, b).await, (2, 0, 4));
}

#[tokio::test]
async fn public_groups_show_rounds_times_and_ranked_table_with_negative_points() {
    let app = app().await;
    let liga = league(&app.pool, 4, "league").await;
    let (a, b, c, d) = (liga.teams[0], liga.teams[1], liga.teams[2], liga.teams[3]);
    for (loser, winner) in [(a, b), (a, c), (a, d)] {
        let game = match_between(&app.pool, &liga, loser, winner).await;
        assert_eq!(app.submit(&liga, game, winner).await, StatusCode::OK);
    }

    let (status, groups) = app
        .call(
            Method::GET,
            &format!("/api/tournaments/{}/groups", liga.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let group = &groups[0];
    assert_eq!(group["name"], "Tabelle");

    let table = group["teams"].as_array().expect("teams");
    assert_eq!(table.len(), 4);
    assert_eq!(table[3]["team_id"], a);
    assert_eq!(table[3]["points"], -3);
    assert!(table[..3].iter().all(|row| row["points"] == 2));

    let games = group["matches"].as_array().expect("matches");
    assert_eq!(games.len(), 6);
    let rounds: Vec<i64> = games
        .iter()
        .map(|game| game["round"].as_i64().expect("round"))
        .collect();
    assert_eq!(rounds, vec![1, 1, 2, 2, 3, 3]);
    assert!(games.iter().all(|game| game["scheduled_at"].is_string()));
}

#[tokio::test]
async fn group_stage_public_groups_have_no_round() {
    let app = app().await;
    let liga = league(&app.pool, 2, "group_stage").await;
    let (_, groups) = app
        .call(
            Method::GET,
            &format!("/api/tournaments/{}/groups", liga.id),
            None,
        )
        .await;
    assert!(groups[0]["matches"][0].get("round").is_none());
}

#[tokio::test]
async fn schedule_endpoint_and_start_update_move_the_plan() {
    let app = app().await;
    let liga = league(&app.pool, 3, "league").await;
    let uri = format!("/api/admin/tournaments/{}/league/schedule", liga.id);
    let new_start = plan_start() + Duration::days(1);

    let (status, body) = app
        .call(
            Method::POST,
            &uri,
            Some(json!({ "start": new_start, "slot_minutes": 30 })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["matches_updated"], 3);
    assert_eq!(scheduled_gaps(&app.pool, &liga).await, vec![30, 30]);
    assert_eq!(first_time(&app.pool, &liga).await, new_start);

    let (invalid, _) = app
        .call(
            Method::POST,
            &uri,
            Some(json!({ "start": new_start, "slot_minutes": 0 })),
        )
        .await;
    assert_eq!(invalid, StatusCode::BAD_REQUEST);

    let moved_start = new_start + Duration::hours(2);
    let (updated, body) = app
        .call(
            Method::PUT,
            &format!("/api/admin/tournaments/{}", liga.id),
            Some(json!({ "group_phase_start": moved_start })),
        )
        .await;
    assert_eq!(updated, StatusCode::OK, "{body}");
    assert_eq!(first_time(&app.pool, &liga).await, moved_start);
    assert_eq!(scheduled_gaps(&app.pool, &liga).await, vec![30, 30]);

    let (mode_change, mode_body) = app
        .call(
            Method::PUT,
            &format!("/api/admin/tournaments/{}", liga.id),
            Some(json!({ "force_tournament_mode": "group_stage" })),
        )
        .await;
    assert_eq!(mode_change, StatusCode::BAD_REQUEST, "{mode_body}");

    let (other_mode, _) = app
        .call(
            Method::POST,
            &format!(
                "/api/admin/tournaments/{}/league/schedule",
                league(&app.pool, 2, "group_stage").await.id
            ),
            Some(json!({ "start": new_start, "slot_minutes": 30 })),
        )
        .await;
    assert_eq!(other_mode, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn plan_can_be_rebuilt_until_a_game_was_played() {
    let app = app().await;
    let liga = league(&app.pool, 4, "league").await;
    let uri = format!("/api/admin/tournaments/{}/groups/generate", liga.id);

    let (status, body) = app
        .call(Method::POST, &uri, Some(json!({ "slot_minutes": 40 })))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["matches_created"], 6);

    let rebuilt_group: i64 =
        sqlx::query_scalar("SELECT id FROM turnier.groups WHERE tournament_id = $1")
            .bind(liga.id)
            .fetch_one(&app.pool)
            .await
            .expect("group");
    let rebuilt = League {
        id: liga.id,
        group_id: rebuilt_group,
        teams: liga.teams.clone(),
    };
    assert_eq!(scheduled_gaps(&app.pool, &rebuilt).await, vec![40, 40]);

    let games = all_match_ids(&app.pool, &rebuilt).await;
    assert_eq!(
        app.submit(&rebuilt, games[0].0, games[0].1).await,
        StatusCode::OK
    );
    let (refused, _) = app.call(Method::POST, &uri, None).await;
    assert_eq!(refused, StatusCode::BAD_REQUEST);
}

async fn first_time(pool: &Pool, league: &League) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT MIN(scheduled_at) FROM turnier.group_matches WHERE group_id = $1")
        .bind(league.group_id)
        .fetch_one(pool)
        .await
        .expect("first time")
}

async fn scheduled_gaps(pool: &Pool, league: &League) -> Vec<i64> {
    let times: Vec<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT DISTINCT scheduled_at FROM turnier.group_matches \
         WHERE group_id = $1 ORDER BY scheduled_at",
    )
    .bind(league.group_id)
    .fetch_all(pool)
    .await
    .expect("times");
    times
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).num_minutes())
        .collect()
}
