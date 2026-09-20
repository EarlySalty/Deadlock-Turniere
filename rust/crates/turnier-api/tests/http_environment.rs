//! Isolierter Prozessbeweis gegen implizite Proxy-Konfiguration durch reqwest.
use axum::{routing::any, Json, Router};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use turnier_config::Config;
use turnier_discord::BrokerClient;

struct Server {
    base: String,
    calls: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    async fn new(label: &'static str) -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let router = Router::new().fallback(any(move || {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                Json(json!({"source": label}))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://localhost:{}", listener.local_addr().unwrap().port());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self { base, calls, task }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn child_http_environment_probe() {
    let Ok(target) = std::env::var("TURNIER_HTTP_ENV_TEST_TARGET") else {
        return;
    };
    // Positivkontrolle: der ungeschützte Bibliotheks-Default MUSS den Proxy nutzen.
    let uncontrolled: Value = reqwest::Client::new()
        .get(format!("{target}/probe"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(uncontrolled["source"], "proxy");

    let mut value: toml::Value =
        toml::from_str(include_str!("../../../../config/bot.example.toml")).unwrap();
    value["discord_master_broker_base_url"] = toml::Value::String(target);
    let mut config = Config::parse_file(
        &toml::to_string(&value).unwrap(),
        Path::new("/srv/test/config/bot.toml"),
    )
    .unwrap();
    config.discord_master_broker_token = "LOCAL_TEST_SENTINEL_NOT_A_SECRET".into();
    let direct: Value = BrokerClient::from_config(&config)
        .post_internal("/probe", &json!({}))
        .await
        .unwrap();
    assert_eq!(direct["source"], "direct");
}

#[tokio::test]
async fn proxy_environment_cannot_reroute_the_configured_broker() {
    let target = Server::new("direct").await;
    let proxy = Server::new("proxy").await;
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", "child_http_environment_probe", "--nocapture"])
        .env_clear()
        .env("TURNIER_HTTP_ENV_TEST_TARGET", &target.base)
        .env("NO_PROXY", "")
        .env("no_proxy", "");
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        child.env(name, &proxy.base);
    }
    let result = tokio::task::spawn_blocking(move || child.output().unwrap())
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "isolierte HTTP-ENV-Prüfung fehlgeschlagen"
    );
    assert_eq!(target.calls.load(Ordering::SeqCst), 1);
    assert_eq!(proxy.calls.load(Ordering::SeqCst), 1);
}
