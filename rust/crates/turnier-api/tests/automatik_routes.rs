use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_db::{connect_str, run_migrations, Pool};

async fn setup() -> (Router, Pool, String) {
    let pool = connect_str(":memory:", 1).await.expect("pool open");
    run_migrations(&pool).await.expect("migrate");

    let mut config = Config::from_env();
    config.discord_admin_role_ids = "admin-role".to_string();
    config.discord_tournament_admin_role_ids = String::new();
    config.discord_mod_role_ids = "mod-role".to_string();
    config.discord_bot_token = String::new();
    config.steam_bridge_db_path = String::new();
    config.backend_allowed_hosts = "localhost".to_string();

    let state = AppState::build(pool.clone(), Arc::new(config))
        .await
        .expect("state build");
    let token = turnier_auth::create_session(
        &pool,
        "mod-user",
        "Mod User",
        "",
        &["mod-role".to_string()],
    )
    .await
    .expect("session");

    (build_router(state), pool, token)
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
    let body = body.map(|v| Body::from(v.to_string())).unwrap_or_else(Body::empty);
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
async fn preset_admin_roundtrip() {
    let (app, _pool, token) = setup().await;

    let created = create_preset(&app, &token, "Preset One").await;
    let preset_id = created["id"].as_i64().unwrap();
    assert_eq!(created["name"], "Preset One");
    assert_eq!(created["active"], true);

    let (status, listed) =
        send_json(&app, &token, Method::GET, "/api/admin/presets", None).await;
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

    let (status, listed) =
        send_json(&app, &token, Method::GET, "/api/admin/presets", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn manual_proposal_uses_preset_config() {
    let (app, _pool, token) = setup().await;
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

    let config: Value =
        serde_json::from_str(proposal["config_json"].as_str().unwrap()).unwrap();
    assert_eq!(config["name"], "Manual Cup");
    assert_eq!(config["category"], "fun");
    assert_eq!(config["preset_id"], preset_id);
}

#[tokio::test]
async fn invalid_proposal_transition_returns_conflict() {
    let (app, _pool, token) = setup().await;
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
        &token,
        Method::POST,
        &format!("/api/admin/proposals/{proposal_id}/event"),
        Some(json!({ "event": "approve" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["detail"], "Dieser Statuswechsel ist nicht möglich");
}

#[tokio::test]
async fn dm_optout_uses_own_session_discord_id() {
    let (app, pool, token) = setup().await;

    let (status, initial) =
        send_json(&app, &token, Method::GET, "/api/me/dm-optout", None).await;
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

    let owner: String = sqlx::query_scalar(
        "SELECT discord_id FROM tournament_dm_optout WHERE scope = 'fun'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner, "mod-user");

    let (status, cleared) = send_json(
        &app,
        &token,
        Method::DELETE,
        "/api/me/dm-optout/fun",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["scopes"].as_array().unwrap().len(), 0);
}
