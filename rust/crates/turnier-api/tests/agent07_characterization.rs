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
    TestApp {
        app: build_test_router(&pool).await,
        _db: db,
        pool,
    }
}

async fn build_test_router(pool: &Pool) -> Router {
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
    build_router(state)
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

// Agent07 characterizations; synthetic data only. These assert observed Main behavior,
// not a desired policy. Test-only file, never a production patch.
#[tokio::test]
async fn agent07_recruitment_modes_and_foreign_resource() {
    let ctx = setup().await;
    let token = session(&ctx.pool, PLAYER_ID, "Synthetic player", &[]).await;
    seed_consent(&ctx.pool, PLAYER_ID).await;
    for mode in ["closed", "application", "open"] {
        let tid = seed_tournament(&ctx.pool, mode, "registration", 2).await;
        let team = seed_team(&ctx.pool, tid, mode, CAPTAIN_ID).await;
        sqlx::query("UPDATE turnier.teams SET recruitment_status=$1 WHERE id=$2").bind(mode).bind(team).execute(&ctx.pool).await.unwrap();
        let other = seed_tournament(&ctx.pool, "Foreign fixture", "registration", 2).await;
        let (foreign, _) = send_json(&ctx.app, Some(&token), Method::POST, &format!("/api/tournaments/{other}/teams/{team}/join"), None).await;
        assert_eq!(foreign, StatusCode::NOT_FOUND);
        let (status, _) = send_json(&ctx.app, Some(&token), Method::POST, &format!("/api/tournaments/{tid}/teams/{team}/join"), None).await;
        assert_eq!(status, StatusCode::OK, "observed behavior: {mode}");
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM turnier.team_members WHERE team_id=$1").bind(team).fetch_one(&ctx.pool).await.unwrap();
        assert_eq!(n, 2);
    }
}

#[tokio::test]
async fn agent07_expired_invitation_and_absent_consent() {
    let ctx=setup().await;
    let token=session(&ctx.pool, INVITED_ID, "Synthetic invitee", &[]).await;
    for (label,with_consent,expired) in [("expired",true,true),("no consent",false,false)] {
        if with_consent { seed_consent(&ctx.pool, INVITED_ID).await; }
        else { sqlx::query("DELETE FROM turnier.user_consents WHERE discord_id=$1").bind(id(INVITED_ID)).execute(&ctx.pool).await.unwrap(); }
        let tid=seed_tournament(&ctx.pool,label,"registration",2).await;
        let team=seed_team(&ctx.pool,tid,label,CAPTAIN_ID).await;
        let signup=seed_signup(&ctx.pool,tid,INVITED_ID,"Synthetic invitee").await;
        let inv:i64=sqlx::query_scalar("INSERT INTO turnier.team_invitations(tournament_id,team_id,discord_id,signup_id,status,created_at,expires_at) VALUES($1,$2,$3,$4,'pending',now()-interval '2 hours',CASE WHEN $5 THEN now()-interval '1 hour' ELSE now()+interval '1 hour' END) RETURNING id")
          .bind(tid).bind(team).bind(id(INVITED_ID)).bind(signup).bind(expired).fetch_one(&ctx.pool).await.unwrap();
        let (status,body)=send_json(&ctx.app,Some(&token),Method::POST,&format!("/api/tournaments/{tid}/invitations/{inv}/accept"),None).await;
        assert_eq!(status,StatusCode::OK,"observed behavior: {label}");
        assert_eq!(body["status"],"accepted");
    }
}

#[tokio::test]
async fn agent07_public_subresources_visibility() {
    let ctx=setup().await;
    let tid=seed_tournament(&ctx.pool,"Hidden fixture","draft",2).await;
    seed_signup(&ctx.pool,tid,PLAYER_ID,"Synthetic check-in name").await;
    sqlx::query("INSERT INTO turnier.tournament_checkins(tournament_id,discord_id,checked_in_at) VALUES($1,$2,now())").bind(tid).bind(id(PLAYER_ID)).execute(&ctx.pool).await.unwrap();
    let (status,_)=send_json(&ctx.app,None,Method::GET,&format!("/api/tournaments/{tid}"),None).await;
    assert_eq!(status,StatusCode::NOT_FOUND);
    for tail in ["groups","bracket"] {
        let (status,_)=send_json(&ctx.app,None,Method::GET,&format!("/api/tournaments/{tid}/{tail}"),None).await;
        assert_eq!(status,StatusCode::OK);
    }
    let (status,body)=send_json(&ctx.app,None,Method::GET,&format!("/api/tournaments/{tid}/checkin-status"),None).await;
    assert_eq!(status,StatusCode::OK);
    assert_eq!(body["checked_in_names"],json!(["Synthetic check-in name"]));
}

#[tokio::test]
async fn agent07_manual_advance_requires_finalize() {
    let ctx=setup().await;
    let tid=seed_tournament(&ctx.pool,"Check-in fixture","checkin",1).await;
    seed_team(&ctx.pool,tid,"Synthetic A",CAPTAIN_ID).await;
    seed_team(&ctx.pool,tid,"Synthetic B",PLAYER_ID).await;
    let token=session(&ctx.pool,ADMIN_ID,"Synthetic moderator",&["mod-role"]).await;
    let (status,_)=send_json(&ctx.app,Some(&token),Method::POST,&format!("/api/admin/tournaments/{tid}/advance"),None).await;
    assert_eq!(status,StatusCode::BAD_REQUEST);
    let (status,body)=send_json(&ctx.app,Some(&token),Method::POST,&format!("/api/admin/tournaments/{tid}/finalize-checkin?confirm=false"),Some(json!({}))).await;
    assert_eq!(status,StatusCode::OK);
    assert_eq!(body["dry_run"],true);
    assert_eq!(body["removed_players"].as_array().unwrap().len(),2);
}
