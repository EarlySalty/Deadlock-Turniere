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

const ADMIN_ID: &str = "910000000000030001";

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
    async fn call(&self, method: Method, uri: &str, body: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(HOST, "localhost")
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .body(Body::from(body.to_string()))
            .expect("request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("response");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn create(&self, name: &str, start: DateTime<Utc>, is_test: bool) -> (StatusCode, Value) {
        let body = json!({
            "name": name,
            "force_tournament_mode": "bracket_only",
            "bracket_start": start.to_rfc3339(),
            "is_test": is_test,
        });
        self.call(Method::POST, "/api/admin/tournaments", body)
            .await
    }
}

fn evening() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, 18, 0, 0)
        .single()
        .expect("valid time")
}

fn detail(body: &Value) -> String {
    body.to_string()
}

#[tokio::test]
async fn turnierstarts_brauchen_vier_stunden_abstand() {
    let app = app().await;
    let (created, first) = app.create("Abendturnier", evening(), false).await;
    assert_eq!(created, StatusCode::CREATED, "{first}");
    let first_id = first["id"].as_i64().expect("id");

    let (too_close, body) = app
        .create("Zu früh", evening() + Duration::hours(3), false)
        .await;
    assert_eq!(too_close, StatusCode::CONFLICT, "{body}");
    assert!(detail(&body).contains(&format!(
        "Zu nah an Turnier #{first_id} Abendturnier: mindestens 4 Stunden Abstand zwischen Turnierstarts."
    )));

    let (test_ok, body) = app.create("Testlauf", evening(), true).await;
    assert_eq!(test_ok, StatusCode::CREATED, "{body}");

    let (same_day, second) = app
        .create("Spätturnier", evening() + Duration::hours(4), false)
        .await;
    assert_eq!(same_day, StatusCode::CREATED, "{second}");
    let second_id = second["id"].as_i64().expect("id");

    let (moved, body) = app
        .call(
            Method::PUT,
            &format!("/api/admin/tournaments/{second_id}"),
            json!({ "bracket_start": (evening() + Duration::hours(1)).to_rfc3339() }),
        )
        .await;
    assert_eq!(moved, StatusCode::CONFLICT, "{body}");
    assert!(detail(&body).contains(&format!("Turnier #{first_id} Abendturnier")));
}

async fn running_league(pool: &Pool, start: DateTime<Utc>) -> i64 {
    let now = Utc::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
             (name, status, team_size, bracket_format, created_by, created_at, updated_at, \
              invite_mode, tournament_mode, series_format, exclude_from_leaderboard, \
              tournament_game_mode, auto_lobby_enabled, is_test, match_objective, \
              no_show_grace_minutes, source, group_phase_start) \
         VALUES ('Liga', 'group_phase', 1, 'single_elimination', $1, $2, $2, 'always', \
                 'league', 1, false, 'standard', false, false, 'auto', 10, 'test', $3) \
         RETURNING id",
    )
    .bind(910_000_000_000_030_002_i64)
    .bind(now)
    .bind(start)
    .fetch_one(pool)
    .await
    .expect("league");
    for index in 0..2_i64 {
        sqlx::query(
            "INSERT INTO turnier.teams \
                 (tournament_id, name, name_key, captain_discord_id, created_at, \
                  recruitment_status) \
             VALUES ($1, $2, $3, $4, $5, 'open')",
        )
        .bind(id)
        .bind(format!("Team {index}"))
        .bind(format!("team {index}"))
        .bind(910_000_000_000_030_100_i64 + index)
        .bind(now + Duration::seconds(index))
        .execute(pool)
        .await
        .expect("team");
    }
    let mut tx = pool.begin().await.expect("tx");
    generate_league_plan_in_tx(&mut tx, id, 50)
        .await
        .expect("plan");
    tx.commit().await.expect("commit");
    id
}

async fn league_times(pool: &Pool, id: i64) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    sqlx::query_as(
        "SELECT t.group_phase_start, MIN(gm.scheduled_at) FROM turnier.tournaments t \
         JOIN turnier.groups g ON g.tournament_id = t.id \
         JOIN turnier.group_matches gm ON gm.group_id = g.id \
         WHERE t.id = $1 GROUP BY t.group_phase_start",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("league times")
}

#[tokio::test]
async fn rundenplan_verschieben_haelt_den_startabstand() {
    let app = app().await;
    let (created, first) = app.create("Abendturnier", evening(), false).await;
    assert_eq!(created, StatusCode::CREATED, "{first}");
    let first_id = first["id"].as_i64().expect("id");
    let late = evening() + Duration::hours(4);
    let league_id = running_league(&app.pool, late).await;

    let (moved, body) = app
        .call(
            Method::POST,
            &format!("/api/admin/tournaments/{league_id}/league/schedule"),
            json!({ "start": evening() + Duration::hours(1), "slot_minutes": 30 }),
        )
        .await;
    assert_eq!(moved, StatusCode::CONFLICT, "{body}");
    assert!(detail(&body).contains(&format!("Turnier #{first_id} Abendturnier")));
    assert_eq!(
        league_times(&app.pool, league_id).await,
        (Some(late), Some(late))
    );
}
