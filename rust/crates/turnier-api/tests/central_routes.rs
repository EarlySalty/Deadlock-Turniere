#![cfg(feature = "testing")]

use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use turnier_api::public::helpers::{self as public_helpers, RankInput};
use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_db::{test_pool, Pool, TestDb};

const ADMIN_ID: &str = "910000000000010001";
const CAPTAIN_ID: &str = "910000000000010002";
const PLAYER_ID: &str = "910000000000010003";
const INVITED_ID: &str = "910000000000010004";
const LEADER_ID: &str = "910000000000010005";
const APPLICANT_ID: &str = "910000000000010006";
const HELPER_ID: &str = "910000000000010007";

struct TestApp {
    app: Router,
    _db: TestDb,
    pool: Pool,
}

async fn setup() -> TestApp {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool().clone();

    let mut config = Config::default();
    config.discord_admin_role_ids = "admin-role".to_string();
    config.discord_tournament_admin_role_ids = String::new();
    config.discord_mod_role_ids = "mod-role".to_string();
    config.discord_bot_token = String::new();
    config.steam_bridge_db_path = String::new();
    config.backend_allowed_hosts = "localhost".to_string();

    let state = AppState::build(pool.clone(), Arc::new(config))
        .await
        .expect("state build");
    TestApp {
        app: build_router(state),
        _db: db,
        pool,
    }
}

async fn session(pool: &Pool, discord_id: &str, name: &str, roles: &[&str]) -> String {
    turnier_auth::create_session(
        pool,
        discord_id,
        name,
        "",
        &roles
            .iter()
            .map(|role| role.to_string())
            .collect::<Vec<_>>(),
    )
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
         VALUES ($1, NULL, $2, true, true, now(), NULL, NULL, true, true, true, true, true)
         ON CONFLICT (discord_id) DO UPDATE
         SET invite_auto_accept = EXCLUDED.invite_auto_accept, updated_at = now()"#,
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
         VALUES ($1, now(), 2)
         ON CONFLICT (discord_id) DO UPDATE
         SET consented_at = now(), consent_version = 2"#,
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

async fn seed_bracket_match(pool: &Pool, tournament_id: i64, team1: i64, team2: i64) -> i64 {
    sqlx::query_scalar(
        r#"INSERT INTO turnier."bracket_matches"
         (tournament_id, round, position, bracket_type, team1_id, team2_id, status, on_stream)
         VALUES ($1, 1, 1, 'winner', $2, $3, 'pending', false) RETURNING id"#,
    )
    .bind(tournament_id)
    .bind(team1)
    .bind(team2)
    .fetch_one(pool)
    .await
    .expect("seed bracket match")
}

async fn seed_report(pool: &Pool, tournament_id: i64, match_id: i64, winner_team_id: i64) -> i64 {
    sqlx::query_scalar(
        r#"INSERT INTO turnier."match_result_reports"
         (match_type, match_id, tournament_id, reported_by, winner_team_id, deadlock_match_id,
          is_no_show, no_show_team_id, status, created_at)
         VALUES ('bracket', $1, $2, $3, $4, 'dl-test-match', false, NULL, 'pending', now())
         RETURNING id"#,
    )
    .bind(match_id)
    .bind(tournament_id)
    .bind(id(CAPTAIN_ID))
    .bind(winner_team_id)
    .fetch_one(pool)
    .await
    .expect("seed report")
}

async fn seed_signup(pool: &Pool, tournament_id: i64, discord_id: &str, name: &str) -> i64 {
    sqlx::query_scalar(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, rank_score, signed_up_at)
         VALUES ($1, $2, $3, 0, now()) RETURNING id"#,
    )
    .bind(tournament_id)
    .bind(id(discord_id))
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("seed signup")
}

async fn seed_team_edges(pool: &Pool, tournament_id: i64, team_id: i64) {
    sqlx::query(
        r#"INSERT INTO turnier."team_applications"
         (team_id, discord_id, discord_name, status, created_at)
         VALUES ($1, $2, 'Applicant', 'pending', now())"#,
    )
    .bind(team_id)
    .bind(id(APPLICANT_ID))
    .execute(pool)
    .await
    .expect("seed team application");
    sqlx::query(
        r#"INSERT INTO turnier."team_invitations"
         (tournament_id, team_id, discord_id, signup_id, status, created_at, expires_at)
         VALUES ($1, $2, $3, NULL, 'pending', now(), NULL)"#,
    )
    .bind(tournament_id)
    .bind(team_id)
    .bind(id(INVITED_ID))
    .execute(pool)
    .await
    .expect("seed team invitation");
}

async fn assert_no_team_edges(pool: &Pool, team_id: i64) {
    let applications: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM turnier."team_applications" WHERE team_id = $1"#,
    )
    .bind(team_id)
    .fetch_one(pool)
    .await
    .expect("count applications");
    let invitations: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM turnier."team_invitations" WHERE team_id = $1"#)
            .bind(team_id)
            .fetch_one(pool)
            .await
            .expect("count invitations");
    assert_eq!(applications, 0);
    assert_eq!(invitations, 0);
}

async fn wait_for_unique_lock(pool: &Pool, table_name: &str) {
    let pattern = format!("%{table_name}%");
    for _ in 0..100 {
        let waiting: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)::BIGINT
             FROM pg_stat_activity
             WHERE datname = current_database()
               AND wait_event_type = 'Lock'
               AND query LIKE $1"#,
        )
        .bind(&pattern)
        .fetch_one(pool)
        .await
        .expect("pg_stat_activity");
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("expected unique-conflict lock wait on {table_name}");
}

#[tokio::test]
async fn consent_profile_and_delete_guard_use_pg_types() {
    let ctx = setup().await;
    let token = session(&ctx.pool, PLAYER_ID, "Player", &[]).await;

    let (status, body) = send_json(&ctx.app, Some(&token), Method::GET, "/api/consent", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["has_consent"], false);

    let (status, body) = send_json(
        &ctx.app,
        Some(&token),
        Method::POST,
        "/api/consent",
        Some(json!({ "consent_version": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["has_consent"], true);

    let (status, body) = send_json(
        &ctx.app,
        Some(&token),
        Method::PUT,
        "/api/profile",
        Some(json!({ "display_name": "PG Player", "invite_auto_accept": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["discord_id"], PLAYER_ID);
    assert_eq!(body["invite_auto_accept"], true);

    let tournament_id = seed_tournament(&ctx.pool, "Consent Guard", "registration", 2).await;
    sqlx::query(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, rank_score, signed_up_at)
         VALUES ($1, $2, 'Player', 0, now())"#,
    )
    .bind(tournament_id)
    .bind(id(PLAYER_ID))
    .execute(&ctx.pool)
    .await
    .expect("seed active signup");

    let (status, _body) =
        send_json(&ctx.app, Some(&token), Method::DELETE, "/api/consent", None).await;
    assert_eq!(status, StatusCode::CONFLICT);

    sqlx::query(r#"DELETE FROM turnier."tournament_signups" WHERE tournament_id = $1"#)
        .bind(tournament_id)
        .execute(&ctx.pool)
        .await
        .expect("remove signup");
    let (status, body) =
        send_json(&ctx.app, Some(&token), Method::DELETE, "/api/consent", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(body, Value::Null);
}

#[tokio::test]
async fn signup_team_join_leave_and_invitation_autoaccept() {
    let ctx = setup().await;
    let captain_token = session(&ctx.pool, CAPTAIN_ID, "Captain", &[]).await;
    let player_token = session(&ctx.pool, PLAYER_ID, "Player", &[]).await;
    let invited_token = session(&ctx.pool, INVITED_ID, "Invited", &[]).await;
    for discord_id in [CAPTAIN_ID, PLAYER_ID, INVITED_ID] {
        seed_consent(&ctx.pool, discord_id).await;
        seed_profile(&ctx.pool, discord_id, discord_id == INVITED_ID).await;
    }
    let tournament_id = seed_tournament(&ctx.pool, "Public Flow", "registration", 3).await;

    let signup_uri = format!("/api/tournaments/{tournament_id}/signup");
    let (status, _body) = send_json(
        &ctx.app,
        Some(&player_token),
        Method::POST,
        &signup_uri,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _body) = send_json(
        &ctx.app,
        Some(&player_token),
        Method::DELETE,
        &signup_uri,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, team) = send_json(
        &ctx.app,
        Some(&captain_token),
        Method::POST,
        &format!("/api/tournaments/{tournament_id}/teams"),
        Some(json!({ "name": "Captain Team" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let team_id = team["id"].as_i64().unwrap();
    assert_eq!(team["captain_discord_id"], CAPTAIN_ID);

    let (status, member) = send_json(
        &ctx.app,
        Some(&player_token),
        Method::POST,
        &format!("/api/tournaments/{tournament_id}/teams/{team_id}/join"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(member["discord_id"], PLAYER_ID);

    let (status, _body) = send_json(
        &ctx.app,
        Some(&player_token),
        Method::DELETE,
        &format!("/api/tournaments/{tournament_id}/teams/{team_id}/leave"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _body) = send_json(
        &ctx.app,
        Some(&invited_token),
        Method::POST,
        &signup_uri,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let signup_id: i64 = sqlx::query_scalar(
        r#"SELECT id FROM turnier."tournament_signups"
         WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(id(INVITED_ID))
    .fetch_one(&ctx.pool)
    .await
    .expect("invited signup");

    let (status, body) = send_json(
        &ctx.app,
        Some(&captain_token),
        Method::POST,
        &format!("/api/tournaments/{tournament_id}/teams/{team_id}/invite-by-signup/{signup_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "auto_accepted");

    let member_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM turnier."team_members" WHERE team_id = $1 AND discord_id = $2"#,
    )
    .bind(team_id)
    .bind(id(INVITED_ID))
    .fetch_one(&ctx.pool)
    .await
    .expect("autoaccept membership");
    assert_eq!(member_count, 1);
}

#[tokio::test]
async fn public_signup_unique_violation_returns_conflict() {
    let ctx = setup().await;
    let token = session(&ctx.pool, PLAYER_ID, "Player", &[]).await;
    seed_consent(&ctx.pool, PLAYER_ID).await;
    let tournament_id = seed_tournament(&ctx.pool, "Signup Unique", "registration", 2).await;

    let mut held = ctx.pool.begin().await.expect("held tx");
    sqlx::query(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, rank_score, signed_up_at)
         VALUES ($1, $2, 'Player', 0, now())"#,
    )
    .bind(tournament_id)
    .bind(id(PLAYER_ID))
    .execute(&mut *held)
    .await
    .expect("held signup");

    let app = ctx.app.clone();
    let token = token.clone();
    let uri = format!("/api/tournaments/{tournament_id}/signup");
    let request =
        tokio::spawn(async move { send_json(&app, Some(&token), Method::POST, &uri, None).await });
    wait_for_unique_lock(&ctx.pool, r#"turnier."tournament_signups""#).await;
    held.commit().await.expect("commit held signup");

    let (status, body) = request.await.expect("signup request");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["detail"],
        "Du bist bereits für dieses Turnier angemeldet"
    );
}

#[tokio::test]
async fn public_team_create_unique_violation_returns_conflict() {
    let ctx = setup().await;
    let token = session(&ctx.pool, CAPTAIN_ID, "Captain", &[]).await;
    seed_consent(&ctx.pool, CAPTAIN_ID).await;
    let tournament_id = seed_tournament(&ctx.pool, "Team Unique", "registration", 3).await;
    let team_name = "Race Team";

    let mut held = ctx.pool.begin().await.expect("held tx");
    sqlx::query(
        r#"INSERT INTO turnier."teams"
         (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status)
         VALUES ($1, $2, $3, $4, now(), 'open')"#,
    )
    .bind(tournament_id)
    .bind(team_name)
    .bind(turnier_engine::name_key(team_name))
    .bind(id(INVITED_ID))
    .execute(&mut *held)
    .await
    .expect("held team");

    let app = ctx.app.clone();
    let token = token.clone();
    let uri = format!("/api/tournaments/{tournament_id}/teams");
    let request = tokio::spawn(async move {
        send_json(
            &app,
            Some(&token),
            Method::POST,
            &uri,
            Some(json!({ "name": team_name })),
        )
        .await
    });
    wait_for_unique_lock(&ctx.pool, r#"turnier."teams""#).await;
    held.commit().await.expect("commit held team");

    let (status, body) = request.await.expect("team create request");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["detail"],
        "Ein Team mit diesem Namen existiert bereits"
    );
}

#[tokio::test]
async fn public_helpers_unique_violations_return_conflict() {
    let ctx = setup().await;
    let tournament_id = seed_tournament(&ctx.pool, "Helper Signup Unique", "registration", 3).await;

    let mut held_signup = ctx.pool.begin().await.expect("held signup tx");
    sqlx::query(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, rank_score, signed_up_at)
         VALUES ($1, $2, 'Helper', 0, now())"#,
    )
    .bind(tournament_id)
    .bind(id(HELPER_ID))
    .execute(&mut *held_signup)
    .await
    .expect("held helper signup");

    let pool = ctx.pool.clone();
    let signup_task = tokio::spawn(async move {
        let mut tx = pool.begin().await.expect("helper signup tx");
        let rank = RankInput {
            steam_id: None,
            rank: None,
            rank_score: 0,
        };
        public_helpers::upsert_signup(
            &mut tx,
            tournament_id,
            HELPER_ID,
            Some("Helper"),
            &rank,
            None,
        )
        .await
    });
    wait_for_unique_lock(&ctx.pool, r#"turnier."tournament_signups""#).await;
    held_signup
        .commit()
        .await
        .expect("commit held helper signup");
    let err = signup_task
        .await
        .expect("helper signup task")
        .expect_err("signup helper must conflict");
    assert_eq!(err.status, StatusCode::CONFLICT);
    assert_eq!(err.detail, "Du bist bereits für dieses Turnier angemeldet");

    let team_id = seed_team(&ctx.pool, tournament_id, "Helper Team", CAPTAIN_ID).await;
    let mut held_member = ctx.pool.begin().await.expect("held member tx");
    sqlx::query(
        r#"INSERT INTO turnier."team_members"
         (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
         VALUES ($1, $2, 'Helper', NULL, NULL, 0, 'member', now())"#,
    )
    .bind(team_id)
    .bind(id(INVITED_ID))
    .execute(&mut *held_member)
    .await
    .expect("held team member");

    let pool = ctx.pool.clone();
    let member_task = tokio::spawn(async move {
        let mut tx = pool.begin().await.expect("helper member tx");
        let rank = RankInput {
            steam_id: None,
            rank: None,
            rank_score: 0,
        };
        public_helpers::add_user_to_team(
            &pool,
            &mut tx,
            tournament_id,
            team_id,
            INVITED_ID,
            Some("Helper"),
            &rank,
        )
        .await
    });
    wait_for_unique_lock(&ctx.pool, r#"turnier."team_members""#).await;
    held_member.commit().await.expect("commit held team member");
    let err = member_task
        .await
        .expect("helper member task")
        .expect_err("member helper must conflict");
    assert_eq!(err.status, StatusCode::CONFLICT);
    assert_eq!(err.detail, "Spieler ist bereits Mitglied in diesem Team");
}

#[tokio::test]
async fn public_invitations_unique_violations_return_conflict() {
    let ctx = setup().await;
    let captain_token = session(&ctx.pool, CAPTAIN_ID, "Captain", &[]).await;
    let applicant_token = session(&ctx.pool, APPLICANT_ID, "Applicant", &[]).await;
    let tournament_id = seed_tournament(&ctx.pool, "Invitation Unique", "registration", 3).await;
    let team_id = seed_team(&ctx.pool, tournament_id, "Invite Team", CAPTAIN_ID).await;
    let signup_id = seed_signup(&ctx.pool, tournament_id, INVITED_ID, "Invited").await;

    let mut held_invite = ctx.pool.begin().await.expect("held invite tx");
    sqlx::query(
        r#"INSERT INTO turnier."team_invitations"
         (tournament_id, team_id, discord_id, signup_id, status, created_at, expires_at)
         VALUES ($1, $2, $3, $4, 'pending', now(), NULL)"#,
    )
    .bind(tournament_id)
    .bind(team_id)
    .bind(id(INVITED_ID))
    .bind(signup_id)
    .execute(&mut *held_invite)
    .await
    .expect("held invitation");

    let app = ctx.app.clone();
    let captain_token = captain_token.clone();
    let invite_uri =
        format!("/api/tournaments/{tournament_id}/teams/{team_id}/invite-by-signup/{signup_id}");
    let invite_request = tokio::spawn(async move {
        send_json(&app, Some(&captain_token), Method::POST, &invite_uri, None).await
    });
    wait_for_unique_lock(&ctx.pool, r#"turnier."team_invitations""#).await;
    held_invite.commit().await.expect("commit held invitation");
    let (status, body) = invite_request.await.expect("invite request");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["detail"],
        "Für diesen Spieler existiert bereits eine Einladung"
    );

    sqlx::query(r#"UPDATE turnier."teams" SET recruitment_status = 'application' WHERE id = $1"#)
        .bind(team_id)
        .execute(&ctx.pool)
        .await
        .expect("enable applications");
    let mut held_application = ctx.pool.begin().await.expect("held application tx");
    sqlx::query(
        r#"INSERT INTO turnier."team_applications"
         (team_id, discord_id, discord_name, status, created_at)
         VALUES ($1, $2, 'Applicant', 'pending', now())"#,
    )
    .bind(team_id)
    .bind(id(APPLICANT_ID))
    .execute(&mut *held_application)
    .await
    .expect("held application");

    let app = ctx.app.clone();
    let applicant_token = applicant_token.clone();
    let apply_uri = format!("/api/tournaments/{tournament_id}/teams/{team_id}/apply");
    let apply_request = tokio::spawn(async move {
        send_json(&app, Some(&applicant_token), Method::POST, &apply_uri, None).await
    });
    wait_for_unique_lock(&ctx.pool, r#"turnier."team_applications""#).await;
    held_application
        .commit()
        .await
        .expect("commit held application");
    let (status, body) = apply_request.await.expect("application request");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        body["detail"],
        "Für dieses Team existiert bereits eine Bewerbung"
    );
}

#[tokio::test]
async fn admin_tournament_delete_removes_team_application_and_invitation_rows() {
    let ctx = setup().await;
    let admin_token = session(&ctx.pool, ADMIN_ID, "Admin", &["admin-role", "mod-role"]).await;
    let tournament_id =
        seed_tournament(&ctx.pool, "Admin Tournament Delete", "registration", 3).await;
    let team_id = seed_team(&ctx.pool, tournament_id, "Delete Team", CAPTAIN_ID).await;
    seed_team_edges(&ctx.pool, tournament_id, team_id).await;

    let (status, _body) = send_json(
        &ctx.app,
        Some(&admin_token),
        Method::DELETE,
        &format!("/api/admin/tournaments/{tournament_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_no_team_edges(&ctx.pool, team_id).await;
}

#[tokio::test]
async fn admin_team_delete_removes_team_application_and_invitation_rows() {
    let ctx = setup().await;
    let mod_token = session(&ctx.pool, ADMIN_ID, "Mod", &["admin-role", "mod-role"]).await;
    let tournament_id = seed_tournament(&ctx.pool, "Admin Team Delete", "registration", 3).await;
    let team_id = seed_team(&ctx.pool, tournament_id, "Admin Delete Team", CAPTAIN_ID).await;
    seed_team_edges(&ctx.pool, tournament_id, team_id).await;

    let (status, _body) = send_json(
        &ctx.app,
        Some(&mod_token),
        Method::DELETE,
        &format!("/api/admin/tournaments/{tournament_id}/teams/{team_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_no_team_edges(&ctx.pool, team_id).await;
}

#[tokio::test]
async fn public_team_disband_removes_team_application_and_invitation_rows() {
    let ctx = setup().await;
    let captain_token = session(&ctx.pool, CAPTAIN_ID, "Captain", &[]).await;
    let tournament_id = seed_tournament(&ctx.pool, "Public Team Delete", "registration", 3).await;
    let team_id = seed_team(&ctx.pool, tournament_id, "Public Delete Team", CAPTAIN_ID).await;
    seed_team_edges(&ctx.pool, tournament_id, team_id).await;

    let (status, body) = send_json(
        &ctx.app,
        Some(&captain_token),
        Method::DELETE,
        &format!("/api/tournaments/{tournament_id}/teams/{team_id}/leave"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "team_aufgeloest");
    assert_no_team_edges(&ctx.pool, team_id).await;
}

#[tokio::test]
async fn leaderboard_profile_reads_points_rank_and_history() {
    let ctx = setup().await;
    let _token = session(&ctx.pool, LEADER_ID, "LeaderName", &[]).await;
    seed_profile(&ctx.pool, LEADER_ID, false).await;
    let tournament_id = seed_tournament(&ctx.pool, "Leaderboard Cup", "completed", 1).await;
    let team_id = seed_team(&ctx.pool, tournament_id, "Leader Team", LEADER_ID).await;

    sqlx::query(
        r#"INSERT INTO turnier."rank_cache"
         (discord_id, source, steam_id, rank, rank_tier, subrank, rank_score, cached_at)
         VALUES ($1, 'test', 'steam-leader', 'Oracle', 5, 1, 501, now())"#,
    )
    .bind(id(LEADER_ID))
    .execute(&ctx.pool)
    .await
    .expect("rank cache");
    sqlx::query(
        r#"INSERT INTO turnier."player_points"
         (discord_id, total_points, tournaments_played, matches_played, matches_won,
          best_placement, updated_at)
         VALUES ($1, 42, 2, 5, 4, 1, now())"#,
    )
    .bind(id(LEADER_ID))
    .execute(&ctx.pool)
    .await
    .expect("player points");

    let (status, profile) =
        send_json(&ctx.app, None, Method::GET, "/api/players/LeaderName", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(profile["rank"], "Oracle");
    assert_eq!(profile["total_points"], 42);
    assert_eq!(
        profile["tournament_history"][0]["tournament_name"],
        "Leaderboard Cup"
    );
    assert_eq!(profile["tournament_history"][0]["team_name"], "Leader Team");

    let linked_team: i64 =
        sqlx::query_scalar(r#"SELECT team_id FROM turnier."team_members" WHERE discord_id = $1"#)
            .bind(id(LEADER_ID))
            .fetch_one(&ctx.pool)
            .await
            .expect("history team");
    assert_eq!(linked_team, team_id);
}

#[tokio::test]
async fn admin_tournament_create_update_jsonb_and_bools() {
    let ctx = setup().await;
    let admin_token = session(&ctx.pool, ADMIN_ID, "Admin", &["admin-role"]).await;

    let (status, created) = send_json(
        &ctx.app,
        Some(&admin_token),
        Method::POST,
        "/api/admin/tournaments",
        Some(json!({
            "name": "Admin PG Cup",
            "team_size": 4,
            "bracket_format": "single_elimination",
            "series_format": 1,
            "final_series_format": 3,
            "invite_mode": "always",
            "lobby_settings_preset": "custom",
            "lobby_settings": { "citadel_player_starting_gold": 1000 },
            "force_tournament_mode": "bracket_only",
            "tournament_game_mode": "standard",
            "auto_lobby_enabled": false,
            "exclude_from_leaderboard": true,
            "reminder_offsets": [1440, 30],
            "start_reminder_offsets": [120, 15],
            "match_objective": "match_win",
            "is_test": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let tournament_id = created["id"].as_i64().unwrap();
    assert_eq!(created["auto_lobby_enabled"], false);
    assert_eq!(created["exclude_from_leaderboard"], true);

    let (status, updated) = send_json(
        &ctx.app,
        Some(&admin_token),
        Method::PUT,
        &format!("/api/admin/tournaments/{tournament_id}"),
        Some(json!({
            "name": "Admin PG Cup Updated",
            "description": "updated",
            "auto_lobby_enabled": true,
            "lobby_settings_preset": "standard"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Admin PG Cup Updated");
    assert_eq!(updated["auto_lobby_enabled"], true);

    let row: (Value, bool) = sqlx::query_as(
        r#"SELECT reminder_offsets, auto_lobby_enabled FROM turnier."tournaments" WHERE id = $1"#,
    )
    .bind(tournament_id)
    .fetch_one(&ctx.pool)
    .await
    .expect("tournament row");
    assert_eq!(row.0, json!([1440, 30]));
    assert!(row.1);
}

#[tokio::test]
async fn operations_confirm_and_reject_result_reports() {
    let ctx = setup().await;
    let mod_token = session(&ctx.pool, ADMIN_ID, "Mod", &["mod-role"]).await;
    let tournament_id = seed_tournament(&ctx.pool, "Ops Cup", "bracket", 1).await;
    let team1 = seed_team(&ctx.pool, tournament_id, "Alpha", CAPTAIN_ID).await;
    let team2 = seed_team(&ctx.pool, tournament_id, "Beta", PLAYER_ID).await;

    let reject_match = seed_bracket_match(&ctx.pool, tournament_id, team1, team2).await;
    let reject_report = seed_report(&ctx.pool, tournament_id, reject_match, team1).await;
    let (status, body) = send_json(
        &ctx.app,
        Some(&mod_token),
        Method::POST,
        &format!("/api/admin/result-reports/{reject_report}/reject"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");

    let confirm_match = seed_bracket_match(&ctx.pool, tournament_id, team1, team2).await;
    let confirm_report = seed_report(&ctx.pool, tournament_id, confirm_match, team2).await;
    let (status, body) = send_json(
        &ctx.app,
        Some(&mod_token),
        Method::POST,
        &format!("/api/admin/result-reports/{confirm_report}/confirm"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["winner_id"], team2);

    let match_winner: i64 =
        sqlx::query_scalar(r#"SELECT winner_id FROM turnier."bracket_matches" WHERE id = $1"#)
            .bind(confirm_match)
            .fetch_one(&ctx.pool)
            .await
            .expect("confirmed match");
    assert_eq!(match_winner, team2);
}

#[tokio::test]
async fn test_mode_seed_simulate_and_wipe_use_throwaway_db() {
    let ctx = setup().await;
    let mod_token = session(&ctx.pool, ADMIN_ID, "Mod", &["mod-role"]).await;

    let (status, users) = send_json(
        &ctx.app,
        Some(&mod_token),
        Method::POST,
        "/api/admin/test/users",
        Some(json!({ "count": 2 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(users["created"].as_array().unwrap().len(), 2);

    let (status, tournament) = send_json(
        &ctx.app,
        Some(&mod_token),
        Method::POST,
        "/api/admin/test/tournaments",
        Some(json!({
            "name": "Generated Test Cup",
            "team_size": 1,
            "num_teams": 2,
            "mode": "bracket_only",
            "advance_to": "bracket"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tournament_id = tournament["tournament_id"].as_i64().unwrap();

    let (status, simulate) = send_json(
        &ctx.app,
        Some(&mod_token),
        Method::POST,
        &format!("/api/admin/test/tournaments/{tournament_id}/simulate-round"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(simulate["simulated_matches"].as_i64().unwrap() >= 0);

    let (status, wiped) = send_json(
        &ctx.app,
        Some(&mod_token),
        Method::DELETE,
        "/api/admin/test/wipe",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(wiped["deleted_tournaments"].as_i64().unwrap() >= 1);

    let remaining: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM turnier."tournaments" WHERE name = 'Generated Test Cup'"#,
    )
    .fetch_one(&ctx.pool)
    .await
    .expect("remaining tournaments");
    assert_eq!(remaining, 0);
}
