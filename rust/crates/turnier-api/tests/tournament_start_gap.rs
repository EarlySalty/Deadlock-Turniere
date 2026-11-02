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
use turnier_db::{test_pool, TestDb};

const ADMIN_ID: &str = "910000000000030001";

struct App {
    router: Router,
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
