#![cfg(feature = "testing")]

use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceExt;
use turnier_api::{build_router, AppState};
use turnier_config::Config;

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("x-comp-token", token);
    }
    let mut req = builder.body(Body::from(body.to_string())).unwrap();
    req.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:19000".parse::<SocketAddr>().unwrap(),
    ));
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let data: Value = serde_json::from_slice(&bytes).unwrap();
    let wire = data.to_string();
    assert!(!wire.contains("token_hash"));
    if let Some(token) = token {
        assert!(!wire.contains(token));
    }
    (status, data)
}

#[tokio::test]
async fn public_comp_flow_enforces_capabilities_capacity_and_preferences() {
    let db = turnier_db::test_pool().await.unwrap();
    let config = Config {
        backend_allowed_hosts: "localhost".into(),
        discord_bot_token: String::new(),
        steam_bridge_db_path: String::new(),
        ..Config::default()
    };
    let state = AppState::build(db.pool().clone(), Arc::new(config))
        .await
        .unwrap();
    let app = build_router(state);
    let tokens: Vec<_> = (0..7)
        .map(|i| format!("http-comp-capability-{i:032}"))
        .collect();
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/comp/lobbies",
            None,
            json!({"name":"Host"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, room) = request(
        &app,
        "POST",
        "/api/comp/lobbies",
        Some(&tokens[0]),
        json!({"name":"Host"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{room}");
    let base = format!("/api/comp/lobbies/{}", room["code"].as_str().unwrap());
    assert_eq!(room["you"], room["host_member_id"]);
    let (status, public) = request(&app, "GET", &base, None, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert!(public["you"].is_null());
    assert_eq!(public["results"]["waiting_for"], json!([0]));
    let heroes = turnier_draft::load_heroes().await;
    let hero = &heroes[0].name;
    let prefs = json!({"revision":0,"preferences":[{"hero_name":hero,"priority":0}]});
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/preferences"),
            None,
            prefs.clone()
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, saved) = request(
        &app,
        "POST",
        &format!("{base}/preferences"),
        Some(&tokens[0]),
        prefs.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["results"]["compositions"][0]["score"], 0);
    assert_eq!(
        saved["results"]["compositions"][0]["assignments"][0]["hero_name"],
        *hero
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/preferences"),
            Some(&tokens[0]),
            prefs
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/preferences"),
            Some(&tokens[0]),
            json!({"revision":1,"preferences":[{"hero_name":hero,"priority":3}]})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    for (i, token) in tokens.iter().enumerate().take(6).skip(1) {
        assert_eq!(
            request(
                &app,
                "POST",
                &format!("{base}/join"),
                Some(token),
                json!({"name":format!("Guest {i}")})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/join"),
            Some(&tokens[6]),
            json!({"name":"Overflow"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    // An idempotent retry with an existing capability succeeds even when full.
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/join"),
            Some(&tokens[1]),
            json!({"name":"Retry"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "POST",
            &format!("{base}/remove"),
            Some(&tokens[1]),
            json!({"member_id":room["host_member_id"]})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    for token in tokens.iter().take(6) {
        assert_eq!(
            request(
                &app,
                "POST",
                &format!("{base}/leave"),
                Some(token),
                Value::Null
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        request(&app, "GET", &base, None, Value::Null).await.0,
        StatusCode::NOT_FOUND
    );
}
