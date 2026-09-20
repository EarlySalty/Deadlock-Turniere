//! Echte Konfigurationsverbraucher mit lokalem HTTP-Gegenüber, ohne produktive Daten.
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use turnier_api::{build_router, AppState};
use turnier_config::Config;
use turnier_discord::BrokerClient;

const TEMPLATE: &str = include_str!("../../../../config/bot.example.toml");
const FIXTURE_CREDENTIAL: &str = "LOCAL_TEST_SENTINEL_NOT_A_SECRET";

struct Server {
    base: String,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    async fn start(router: Router) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { base, task }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn configured(base: &str) -> Config {
    let mut value: toml::Value = toml::from_str(TEMPLATE).unwrap();
    for field in [
        "discord_oauth_internal_api_base_url",
        "discord_master_broker_base_url",
    ] {
        value[field] = toml::Value::String(base.to_owned());
    }
    value["assets"]["heroes_url"] = toml::Value::String(format!("{base}/configured-assets"));
    value["assets"]["heroes_cache_seconds"] = toml::Value::Integer(600);
    value["steam_bridge_db_path"] = toml::Value::String(String::new());
    value["discord_guild_id"] = toml::Value::String("701".into());
    value["discord_admin_role_ids"] = toml::Value::String("702".into());
    value["discord_tournament_admin_role_ids"] = toml::Value::String("703".into());
    value["discord_mod_role_ids"] = toml::Value::String("704".into());
    value["backend_allowed_hosts"] = toml::Value::String("config.example".into());
    value["turnier_public_url"] = toml::Value::String("https://tournament.example/events".into());
    value["cors_extra_origins"] =
        toml::Value::Array(vec![toml::Value::String("https://client.example".into())]);
    value["limits"]["request_body_bytes"] = toml::Value::Integer(2048);
    value["limits"]["avatar_bytes"] = toml::Value::Integer(1024);
    value["limits"]["comp_body_bytes"] = toml::Value::Integer(1024);
    let mut config = Config::parse_file(
        &toml::to_string(&value).unwrap(),
        Path::new("/srv/test/config/bot.toml"),
    )
    .unwrap();
    // Tests injizieren einen erkennbaren Platzhalter, niemals Infisical-Zugangsdaten.
    config.discord_master_broker_token = FIXTURE_CREDENTIAL.into();
    config.discord_oauth_internal_api_token = FIXTURE_CREDENTIAL.into();
    config
}
async fn state(config: Arc<Config>) -> AppState {
    let options = sqlx::postgres::PgConnectOptions::new()
        .host("127.0.0.1")
        .port(1)
        .username("config_test")
        .database("config_test");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .min_connections(0)
        .connect_lazy_with(options);
    AppState::build(pool, config).await.unwrap()
}

#[tokio::test]
async fn app_state_uses_the_validated_snapshot_for_oauth_broker_heroes_and_roles() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let server = Server::start(Router::new()
        .route("/configured-assets", get(move || { let count = count.clone(); async move {
            count.fetch_add(1, Ordering::SeqCst);
            Json(json!([{"id": 10, "name": "Config-Test-Held", "player_selectable": true, "disabled": false}]))
        }}))
        .route("/internal/v1/discord/initiate", post(|headers: axum::http::HeaderMap, Json(body): Json<Value>| async move {
            assert!(headers.contains_key("x-internal-token"));
            assert_eq!(body["metadata"]["guild_id"], "701");
            assert_eq!(body["redirect_after"], "https://tournament.example/events/auth/discord/complete");
            Json(json!({"authorize_url": "https://login.example/authorize"}))
        }))
        .route("/internal/master/v1/discord/voice-channel/members", post(|headers: axum::http::HeaderMap, Json(body): Json<Value>| async move {
            assert!(headers.contains_key("x-internal-token"));
            assert_eq!(body["channel_id"], 705);
            Json(json!({"channel_id": 705, "members": [{"user_id": 706}]}))
        }))).await;
    let config = Arc::new(configured(&server.base));
    let state = state(config.clone()).await;
    assert!(Arc::ptr_eq(&state.config, &config));
    assert!(state.role_sets.is_admin(&["702".into()]));
    assert!(state.role_sets.is_admin(&["703".into()]));
    assert!(state.role_sets.is_mod(&["704".into()]));
    assert!(!state.role_sets.is_admin(&["704".into()]));
    assert_eq!(
        state.oauth.initiate_login().await.unwrap(),
        "https://login.example/authorize"
    );
    assert_eq!(
        state.notifier.get_voice_channel_members(705).await.unwrap(),
        vec![json!({"user_id": 706})]
    );
    let heroes = state.heroes.heroes().await;
    assert_eq!(heroes.len(), 1);
    assert_eq!(heroes[0].id, 10);
    assert_eq!(state.heroes.heroes().await, heroes);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!format!("{:?}", state.oauth).contains(FIXTURE_CREDENTIAL));
    assert!(!format!("{:?}", BrokerClient::from_config(&config)).contains(FIXTURE_CREDENTIAL));
}

#[tokio::test]
async fn production_router_obeys_configured_host_cors_body_limit_and_snapshot_status() {
    let config = Arc::new(configured("http://127.0.0.1:1"));
    let fingerprint = config.fingerprint().unwrap();
    let app = build_router(state(config).await);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("host", "config.example")
                .header("origin", "https://client.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://client.example"
    );
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["config_fingerprint"], fingerprint);
    assert_eq!(body["config_schema"], 1);
    assert_eq!(body["config_reload"], "restart_required");
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header("host", "not-allowed.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::BAD_REQUEST);
    let mut oversized = Request::builder()
        .method("POST")
        .uri("/api/comp/lobbies")
        .header("host", "config.example")
        .header("content-type", "application/json")
        .body(Body::from(json!({"name": "x".repeat(1500)}).to_string()))
        .unwrap();
    oversized.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:43210".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = app.oneshot(oversized).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn configured_broker_timeout_is_enforced_by_the_real_http_client() {
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let entered_tx = Arc::new(std::sync::Mutex::new(Some(entered_tx)));
    let server = Server::start(Router::new().route(
        "/stall",
        post(move || {
            let entered_tx = entered_tx.clone();
            async move {
                if let Some(tx) = entered_tx.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                std::future::pending::<Json<Value>>().await
            }
        }),
    ))
    .await;
    let mut config = configured(&server.base);
    config.network.broker_request_seconds = 1;
    config.network.broker_connect_seconds = 1;
    config.validate().unwrap();
    let client = BrokerClient::from_config(&config);
    let request =
        tokio::spawn(async move { client.post_internal::<Value, _>("/stall", &json!({})).await });
    entered_rx.await.unwrap();
    // Virtuelle Zeit erst nach dem echten HTTP-Request aktivieren, keine Schlafprobe.
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    let result = tokio::time::timeout(Duration::from_millis(50), request)
        .await
        .expect("konfigurierte Frist wurde nicht an den Client weitergegeben")
        .unwrap();
    assert!(result.is_err());
}
