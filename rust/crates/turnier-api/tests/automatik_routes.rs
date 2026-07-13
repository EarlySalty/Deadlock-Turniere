#![cfg(feature = "testing")]

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_automatik::presets;
use turnier_automatik::routine::{ensure_routine_proposal, RoutineTournamentPlan};
use turnier_config::Config;
use turnier_db::{test_pool, Pool, TestDb};

const MOD_USER_ID: &str = "910000000000000001";
const CASTER_CONFLICT_ID: &str = "910000000000000002";
const CASTER_USER_ID: &str = "910000000000000003";
const CASTER_APPROVER_ID: &str = "910000000000000004";

async fn setup() -> (Router, TestDb, Pool, String) {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool().clone();

    let mut config = Config::from_env();
    config.discord_admin_role_ids = "admin-role".to_string();
    config.discord_tournament_admin_role_ids = String::new();
    config.discord_mod_role_ids = "mod-role".to_string();
    config.discord_bot_token = String::new();
    config.steam_bridge_db_path = String::new();
    config.backend_allowed_hosts = "localhost".to_string();
    config.discord_oauth_internal_api_token = "internal-token".to_string();

    let state = AppState::build(pool.clone(), Arc::new(config))
        .await
        .expect("state build");
    let token = turnier_auth::create_session(
        &pool,
        MOD_USER_ID,
        "Mod User",
        "",
        &["mod-role".to_string()],
    )
    .await
    .expect("session");

    (build_router(state), db, pool, token)
}

async fn create_caster_session(pool: &Pool, discord_id: &str) -> String {
    let caster_role = Config::from_env().discord_caster_role_id.to_string();
    turnier_auth::create_session(
        pool,
        discord_id,
        "Caster User",
        "",
        &["mod-role".to_string(), caster_role],
    )
    .await
    .expect("caster session")
}

fn preset_body(name: &str) -> Value {
    json!({
        "name": name,
        "category": "fun",
        "config": {
            "team_size": 6,
            "bracket_format": "single_elimination",
            "series_format": 1,
            "final_series_format": 3,
            "tournament_mode": "group_stage",
            "tournament_game_mode": "standard",
            "match_objective": "auto",
            "invite_mode": "always",
            "reminder_offsets": "[1440,120,15]",
            "start_reminder_offsets": "[1440,60]",
            "rules": "rules",
            "description_template": "description"
        }
    })
}

async fn send_json(
    app: &Router,
    token: &str,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let body = body
        .map(|v| Body::from(v.to_string()))
        .unwrap_or_else(Body::empty);
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(HOST, "localhost")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .expect("request");

    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    if bytes.is_empty() {
        return (status, Value::Null);
    }
    let body = serde_json::from_slice(&bytes).expect("json body");
    (status, body)
}

async fn send_internal(
    app: &Router,
    token: Option<&str>,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(HOST, "localhost")
        .header(CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header("X-Internal-Token", token);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    if bytes.is_empty() {
        return (status, Value::Null);
    }
    let body = serde_json::from_slice(&bytes).expect("json body");
    (status, body)
}

async fn create_preset(app: &Router, token: &str, name: &str) -> Value {
    let (status, body) = send_json(
        app,
        token,
        Method::POST,
        "/api/admin/presets",
        Some(preset_body(name)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    body
}

#[tokio::test]
async fn internal_votes_require_token_roles_and_two_distinct_approvals() {
    let (app, _db, pool, session) = setup().await;
    let preset_json = create_preset(&app, &session, "Human Gate").await;
    let preset = presets::get(&pool, preset_json["id"].as_i64().unwrap())
        .await
        .unwrap()
        .unwrap();
    let proposal = ensure_routine_proposal(
        &pool,
        &preset,
        &RoutineTournamentPlan {
            registration_start: chrono::DateTime::parse_from_rfc3339("2026-07-12T18:00:00Z")
                .unwrap()
                .to_utc(),
            registration_end: chrono::DateTime::parse_from_rfc3339("2026-07-19T17:30:00Z")
                .unwrap()
                .to_utc(),
            checkin_start: chrono::DateTime::parse_from_rfc3339("2026-07-19T17:30:00Z")
                .unwrap()
                .to_utc(),
            event_start: chrono::DateTime::parse_from_rfc3339("2026-07-19T18:00:00Z")
                .unwrap()
                .to_utc(),
            bracket_start: chrono::DateTime::parse_from_rfc3339("2026-07-19T21:00:00Z")
                .unwrap()
                .to_utc(),
        },
    )
    .await
    .unwrap();
    let mut planned_config: Value = serde_json::from_str(
        &turnier_automatik::proposals::get_proposal(&pool, proposal.id)
            .await
            .unwrap()
            .unwrap()
            .config_json,
    )
    .unwrap();
    planned_config["name"] = json!("KI Human Gate");
    planned_config["description"] = json!("Von Mods freigegeben");
    planned_config["rules"] = json!("Keine Ausnahmen");
    planned_config["team_size"] = json!(5);
    planned_config["_ai_planned"] = json!(true);
    let planned_uri = format!("/internal/turnier/v1/proposals/{}/planned", proposal.id);
    let (status, planned) = send_internal(
        &app,
        Some("internal-token"),
        &planned_uri,
        json!({"config_json": planned_config.to_string()}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(planned["proposal"]["config_json"]
        .as_str()
        .unwrap()
        .contains("KI Human Gate"));
    let uri = format!("/internal/turnier/v1/proposals/{}/vote", proposal.id);

    let (status, _) = send_internal(
        &app,
        None,
        &uri,
        json!({"actor_id":"1337518124647579601","role_ids":["1337518124647579661"],"decision":"approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send_internal(
        &app,
        Some("internal-token"),
        &uri,
        json!({"actor_id":"1337518124647579601","role_ids":["1"],"decision":"approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, first) = send_internal(
        &app,
        Some("internal-token"),
        &uri,
        json!({"actor_id":"1337518124647579601","role_ids":["1337518124647579661"],"decision":"approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["approvals"], 1);
    assert_eq!(first["went_live"], false);

    let (status, _) = send_internal(
        &app,
        Some("internal-token"),
        &uri,
        json!({"actor_id":"1401891955931222602","role_ids":["1401891955931222110"],"decision":"reject"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, rejected) = send_internal(
        &app,
        Some("internal-token"),
        &uri,
        json!({"actor_id":"1401891955931222602","role_ids":["1401891955931222110"],"decision":"reject","reason":"Keine Zeit"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["approvals"], 1);
    assert_eq!(rejected["feedback"].as_array().unwrap().len(), 1);

    let second_body = json!({"actor_id":"1401891955931222602","role_ids":["1401891955931222110"],"decision":"approve"});
    let (status, second) =
        send_internal(&app, Some("internal-token"), &uri, second_body.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["approvals"], 2);
    assert_eq!(second["went_live"], true);
    assert_eq!(second["announcement_posted"], false);
    let tournament_id = second["tournament_id"].as_i64().unwrap();
    let materialized: (String, Option<String>, Option<String>, i64) = sqlx::query_as(
        "SELECT name, description, rules, team_size FROM turnier.tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(materialized.0, "KI Human Gate");
    assert_eq!(materialized.1.as_deref(), Some("Von Mods freigegeben"));
    assert_eq!(materialized.2.as_deref(), Some("Keine Ausnahmen"));
    assert_eq!(materialized.3, 5);

    let announcement_uri = format!(
        "/internal/turnier/v1/proposals/{}/announcement-rendered",
        proposal.id
    );
    let announcement_planned_uri = format!(
        "/internal/turnier/v1/proposals/{}/announcement-planned",
        proposal.id
    );
    let (status, announcement_planned) = send_internal(
        &app,
        Some("internal-token"),
        &announcement_planned_uri,
        json!({
            "role_ids":["1401891955931222110"],
            "draft":"Interne Vorlage"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(announcement_planned["proposal"]["config_json"]
        .as_str()
        .unwrap()
        .contains("Interne Vorlage"));
    let (status, announced) = send_internal(
        &app,
        Some("internal-token"),
        &announcement_uri,
        json!({
            "actor_id":"1401891955931222602",
            "role_ids":["1401891955931222110"],
            "message_id":"1474543558793887999"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(announced["announcement_posted"], true);
    assert_eq!(announced["feedback"].as_array().unwrap().len(), 1);
    let (status, repeated_announcement) = send_internal(
        &app,
        Some("internal-token"),
        &announcement_uri,
        json!({
            "actor_id":"1401891955931222602",
            "role_ids":["1401891955931222110"],
            "message_id":"1474543558793887999"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated_announcement["announcement_posted"], true);
    let marker_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.tournament_proposal_feedback \
         WHERE proposal_id = $1 AND applied_change_json->>'kind' = 'announcement_draft'",
    )
    .bind(proposal.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(marker_count, 1);

    let (status, repeated) = send_internal(&app, Some("internal-token"), &uri, second_body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated["tournament_id"], tournament_id);
    assert_eq!(repeated["announcement_posted"], true);
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, status FROM turnier.tournaments WHERE source='routine'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows, vec![(tournament_id, "registration".to_string())]);
}

#[tokio::test]
async fn failed_materialization_keeps_proposal_pending() {
    let (app, _db, pool, _session) = setup().await;
    let proposal_id = turnier_automatik::proposals::create_proposal(
        &pool,
        None,
        turnier_automatik::proposals::ProposalSource::Bot,
        Some("2026-07-19T18:00:00Z"),
        r#"{
            "registration_start":"2026-07-12T18:00:00Z",
            "registration_end":"2026-07-19T17:30:00Z",
            "checkin_start":"2026-07-19T17:30:00Z",
            "event_start":"2026-07-19T18:00:00Z",
            "bracket_start":"2026-07-19T21:00:00Z"
        }"#,
    )
    .await
    .unwrap();
    turnier_automatik::proposals::apply_event(
        &pool,
        proposal_id,
        turnier_automatik::proposals::ProposalEvent::SubmitForApproval,
    )
    .await
    .unwrap();
    let uri = format!("/internal/turnier/v1/proposals/{proposal_id}/vote");
    let (status, _) = send_internal(
        &app,
        Some("internal-token"),
        &uri,
        json!({"actor_id":"1337518124647579601","role_ids":["1337518124647579661"],"decision":"approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send_internal(
        &app,
        Some("internal-token"),
        &uri,
        json!({"actor_id":"1401891955931222602","role_ids":["1401891955931222110"],"decision":"approve"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let proposal = turnier_automatik::proposals::get_proposal(&pool, proposal_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        proposal.state,
        turnier_automatik::proposals::ProposalState::PendingApproval
    );
    assert!(proposal.tournament_id.is_none());
}

#[tokio::test]
async fn failed_revision_activation_does_not_block_retry() {
    let (app, _db, pool, _session) = setup().await;
    let proposal_id = turnier_automatik::proposals::create_proposal(
        &pool,
        None,
        turnier_automatik::proposals::ProposalSource::Bot,
        None,
        r#"{"name":"Alt"}"#,
    )
    .await
    .unwrap();
    turnier_automatik::proposals::apply_event(
        &pool,
        proposal_id,
        turnier_automatik::proposals::ProposalEvent::SubmitForApproval,
    )
    .await
    .unwrap();
    let revision_uri = format!("/internal/turnier/v1/proposals/{proposal_id}/revision");
    let revision_body = json!({
        "role_ids":["1337518124647579661"],
        "config_json":"{\"name\":\"Neu\"}"
    });
    let (status, revision) = send_internal(
        &app,
        Some("internal-token"),
        &revision_uri,
        revision_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let revised_id = revision["proposal"]["id"].as_i64().unwrap();
    let activate_uri =
        format!("/internal/turnier/v1/proposals/{proposal_id}/revision/{revised_id}/activate");
    let (status, _) = send_internal(
        &app,
        Some("internal-token"),
        &activate_uri,
        json!({
            "actor_id":"keine-id",
            "role_ids":["1337518124647579661"],
            "feedback":"Später",
            "channel_id":"1474543558793887937",
            "message_id":"1474543558793887999"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        turnier_automatik::proposals::get_proposal(&pool, revised_id)
            .await
            .unwrap()
            .is_none()
    );

    let (status, _) =
        send_internal(&app, Some("internal-token"), &revision_uri, revision_body).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn preset_admin_roundtrip() {
    let (app, _db, _pool, token) = setup().await;

    let created = create_preset(&app, &token, "Preset One").await;
    let preset_id = created["id"].as_i64().unwrap();
    assert_eq!(created["name"], "Preset One");
    assert_eq!(created["active"], true);

    let (status, listed) = send_json(&app, &token, Method::GET, "/api/admin/presets", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 1);

    let (status, fetched) = send_json(
        &app,
        &token,
        Method::GET,
        &format!("/api/admin/presets/{preset_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["id"], preset_id);

    let mut update = preset_body("Preset Two");
    update["category"] = json!("comp");
    update["config"]["team_size"] = json!(5);
    let (status, updated) = send_json(
        &app,
        &token,
        Method::PUT,
        &format!("/api/admin/presets/{preset_id}"),
        Some(update),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Preset Two");
    assert_eq!(updated["category"], "comp");
    assert_eq!(updated["team_size"], 5);

    let (status, inactive) = send_json(
        &app,
        &token,
        Method::PATCH,
        &format!("/api/admin/presets/{preset_id}/active"),
        Some(json!({ "active": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inactive["active"], false);

    let (status, body) = send_json(
        &app,
        &token,
        Method::DELETE,
        &format!("/api/admin/presets/{preset_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(body, Value::Null);

    let (status, listed) = send_json(&app, &token, Method::GET, "/api/admin/presets", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn manual_proposal_uses_preset_config() {
    let (app, _db, _pool, token) = setup().await;
    let preset = create_preset(&app, &token, "Manual Preset").await;
    let preset_id = preset["id"].as_i64().unwrap();

    let (status, proposal) = send_json(
        &app,
        &token,
        Method::POST,
        "/api/admin/proposals",
        Some(json!({
            "preset_id": preset_id,
            "name": "Manual Cup",
            "proposed_start": "2026-07-10T18:00:00Z"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(proposal["source"], "manual");
    assert_eq!(proposal["preset_id"], preset_id);
    assert!(!proposal["config_json"].as_str().unwrap().is_empty());

    let config: Value = serde_json::from_str(proposal["config_json"].as_str().unwrap()).unwrap();
    assert_eq!(config["name"], "Manual Cup");
    assert_eq!(config["category"], "fun");
    assert_eq!(config["preset_id"], preset_id);
}

#[tokio::test]
async fn invalid_proposal_transition_returns_conflict() {
    let (app, _db, pool, token) = setup().await;
    let caster_token = create_caster_session(&pool, CASTER_CONFLICT_ID).await;
    let preset = create_preset(&app, &token, "Transition Preset").await;
    let preset_id = preset["id"].as_i64().unwrap();
    let (status, proposal) = send_json(
        &app,
        &token,
        Method::POST,
        "/api/admin/proposals",
        Some(json!({ "preset_id": preset_id, "name": "Transition Cup" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let proposal_id = proposal["id"].as_i64().unwrap();

    let (status, body) = send_json(
        &app,
        &caster_token,
        Method::POST,
        &format!("/api/admin/proposals/{proposal_id}/event"),
        Some(json!({ "event": "approve" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["detail"], "Dieser Statuswechsel ist nicht möglich");
}

#[tokio::test]
async fn proposal_vote_requires_actor_caster_role() {
    let (app, _db, _pool, token) = setup().await;
    let preset = create_preset(&app, &token, "Caster Gate Preset").await;
    let preset_id = preset["id"].as_i64().unwrap();
    let (status, proposal) = send_json(
        &app,
        &token,
        Method::POST,
        "/api/admin/proposals",
        Some(json!({ "preset_id": preset_id, "name": "Caster Gate Cup" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let proposal_id = proposal["id"].as_i64().unwrap();

    let (status, _body) = send_json(
        &app,
        &token,
        Method::POST,
        &format!("/api/admin/proposals/{proposal_id}/votes"),
        Some(json!({ "caster_id": "spoofed-caster", "decision": "approve" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn legacy_caster_vote_is_disabled_for_mod_gate() {
    let (app, _db, pool, token) = setup().await;
    let caster_token = create_caster_session(&pool, CASTER_USER_ID).await;

    let preset = create_preset(&app, &token, "Actor Vote Preset").await;
    let preset_id = preset["id"].as_i64().unwrap();
    let (status, proposal) = send_json(
        &app,
        &token,
        Method::POST,
        "/api/admin/proposals",
        Some(json!({ "preset_id": preset_id, "name": "Actor Vote Cup" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let proposal_id = proposal["id"].as_i64().unwrap();

    let (status, _body) = send_json(
        &app,
        &caster_token,
        Method::POST,
        &format!("/api/admin/proposals/{proposal_id}/votes"),
        Some(json!({ "caster_id": "spoofed-caster", "decision": "approve" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let stored: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM turnier."tournament_proposal_votes" WHERE proposal_id = $1"#,
    )
    .bind(proposal_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, 0);
}

#[tokio::test]
async fn proposal_event_approve_is_disabled_for_human_gate() {
    let (app, _db, pool, token) = setup().await;
    let caster_token = create_caster_session(&pool, CASTER_APPROVER_ID).await;
    let preset = create_preset(&app, &token, "Event Caster Gate Preset").await;
    let preset_id = preset["id"].as_i64().unwrap();
    let (status, proposal) = send_json(
        &app,
        &token,
        Method::POST,
        "/api/admin/proposals",
        Some(json!({ "preset_id": preset_id, "name": "Event Caster Gate Cup" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let proposal_id = proposal["id"].as_i64().unwrap();

    let (status, body) = send_json(
        &app,
        &token,
        Method::POST,
        &format!("/api/admin/proposals/{proposal_id}/event"),
        Some(json!({ "event": "submit" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "pending_approval");

    let (status, _body) = send_json(
        &app,
        &caster_token,
        Method::POST,
        &format!("/api/admin/proposals/{proposal_id}/votes"),
        Some(json!({ "caster_id": CASTER_APPROVER_ID, "decision": "approve" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _body) = send_json(
        &app,
        &token,
        Method::POST,
        &format!("/api/admin/proposals/{proposal_id}/event"),
        Some(json!({ "event": "approve" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _body) = send_json(
        &app,
        &caster_token,
        Method::POST,
        &format!("/api/admin/proposals/{proposal_id}/event"),
        Some(json!({ "event": "approve" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let stored: String =
        sqlx::query_scalar(r#"SELECT state FROM turnier."tournament_proposals" WHERE id = $1"#)
            .bind(proposal_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, "pending_approval");
}

#[tokio::test]
async fn dm_optout_uses_own_session_discord_id() {
    let (app, _db, pool, token) = setup().await;

    let (status, initial) = send_json(&app, &token, Method::GET, "/api/me/dm-optout", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initial["scopes"].as_array().unwrap().len(), 0);

    let (status, set) = send_json(
        &app,
        &token,
        Method::PUT,
        "/api/me/dm-optout",
        Some(json!({ "scope": "fun" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(set["scopes"], json!(["fun"]));

    let owner: i64 = sqlx::query_scalar(
        r#"SELECT discord_id FROM turnier."tournament_dm_optout" WHERE scope = 'fun'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner, MOD_USER_ID.parse::<i64>().unwrap());

    let (status, cleared) =
        send_json(&app, &token, Method::DELETE, "/api/me/dm-optout/fun", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["scopes"].as_array().unwrap().len(), 0);
}
