#![cfg(feature = "testing")]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use turnier_api::scrim_lobby::process_scrim_lobby_once;
use turnier_api::AppState;
use turnier_config::Config;
use turnier_db::{test_pool, Pool, TestDb};
use turnier_draft::{claim_room, create_room, room_ready, take_lobby_action};

#[derive(Clone, Default)]
struct FakeSteamBot {
    provision_fail: Arc<AtomicBool>,
    reconcile_count: Arc<AtomicUsize>,
    collect_count: Arc<AtomicUsize>,
    release_count: Arc<AtomicUsize>,
    seen_token: Arc<Mutex<Option<String>>>,
}

async fn operations(
    State(bot): State<FakeSteamBot>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    *bot.seen_token.lock().unwrap() = headers
        .get("x-internal-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let kind = body["operation"]["type"].as_str().unwrap_or_default();
    let mut meta = body["meta"].clone();
    let aggregate = body["aggregate"].clone();
    let response_kind = match kind {
        "lobby_provision" => {
            if bot.provision_fail.load(Ordering::SeqCst) {
                let payload = json!({
                    "version": "scrim-steam.v1",
                    "meta": meta,
                    "aggregate": aggregate,
                    "operation": "lobby_provision",
                    "status": "failed",
                    "capabilities": broken_capabilities(),
                    "error": {
                        "code": "task_failed",
                        "message": "Steam-Client nicht eingeloggt",
                        "retryable": true
                    }
                });
                return (StatusCode::OK, Json(payload));
            }
            meta["idempotency_key"] = json!(meta["idempotency_key"]);
            "lobby_provisioned"
        }
        "lobby_reconcile" => {
            let count = bot.reconcile_count.fetch_add(1, Ordering::SeqCst);
            if count % 2 == 0 {
                meta["x"] = json!(null);
                let payload = json!({
                    "version": "scrim-steam.v1",
                    "meta": meta,
                    "aggregate": aggregate,
                    "operation": "lobby_reconcile",
                    "status": "done",
                    "capabilities": broken_capabilities(),
                    "data": {
                        "type": "lobby_reconciled",
                        "payload": {"party_id": "party-1", "match_id": null, "connect_code": null}
                    }
                });
                return (StatusCode::OK, Json(payload));
            }
            "lobby_reconciled"
        }
        "final_result_collect" => {
            let count = bot.collect_count.fetch_add(1, Ordering::SeqCst);
            if count % 2 == 0 {
                let payload = json!({
                    "version": "scrim-steam.v1",
                    "meta": meta,
                    "aggregate": aggregate,
                    "operation": "final_result_collect",
                    "status": "done",
                    "capabilities": broken_capabilities(),
                    "data": {
                        "type": "final_result",
                        "payload": {
                            "match_id": "987654321",
                            "outcome": "unknown",
                            "finality": "provisional",
                            "players": []
                        }
                    }
                });
                return (StatusCode::OK, Json(payload));
            }
            "final_result"
        }
        "lobby_release" => {
            bot.release_count.fetch_add(1, Ordering::SeqCst);
            "lobby_released"
        }
        _ => panic!("unbekannter Vorgang {kind}"),
    };

    let data = match response_kind {
        "lobby_provisioned" => json!({
            "type": "lobby_provisioned",
            "payload": {"party_id": "party-1", "lobby_code": "JOIN-11"}
        }),
        "lobby_reconciled" => json!({
            "type": "lobby_reconciled",
            "payload": {"party_id": "party-1", "match_id": "987654321", "connect_code": "CONNECT-22"}
        }),
        "final_result" => json!({
            "type": "final_result",
            "payload": {
                "match_id": "987654321",
                "outcome": "team0",
                "finality": "final",
                "duration_s": 2050,
                "players": []
            }
        }),
        _ => json!({"type": "lobby_released", "payload": {"released": true}}),
    };
    let payload = json!({
        "version": "scrim-steam.v1",
        "meta": meta,
        "aggregate": aggregate,
        "operation": kind,
        "status": "done",
        "capabilities": broken_capabilities(),
        "data": data
    });
    (StatusCode::OK, Json(payload))
}

fn broken_capabilities() -> Value {
    json!({
        "lobby_code": {"state": "available"},
        "side_assignment": {"state": "available"},
        "match_id_discovery": {"state": "available"},
        "final_result_retrieval": {"state": "available"},
        "spectator_payload": {"state": "available"},
        "connect_code_derivation": {"state": "available"}
    })
}

type BrokerLog = Arc<Mutex<Vec<Value>>>;

async fn broker(State(log): State<BrokerLog>, Json(payload): Json<Value>) -> Json<Value> {
    log.lock().unwrap().push(payload);
    Json(json!({"message_id": "42", "ok": true}))
}

async fn serve(app: Router) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Test-Listener binden");
    let address = listener.local_addr().expect("Test-Adresse");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("Test-Server laufen lassen");
    });
    (address, handle)
}

async fn build_state(steam_url: &str, broker_url: &str, db: &TestDb) -> AppState {
    let config = Config {
        discord_bot_token: String::new(),
        steam_bridge_db_path: String::new(),
        backend_allowed_hosts: "localhost".to_string(),
        steam_bot_base_url: steam_url.to_string(),
        steam_bot_internal_token: "test-token".to_string(),
        discord_master_broker_base_url: broker_url.to_string(),
        discord_master_broker_token: "broker-token".to_string(),
        scrim_announce_channel_id: 555,
        ..Config::default()
    };
    AppState::build(db.pool().clone(), Arc::new(config))
        .await
        .expect("state build")
}

async fn spiel_raum_fertig(pool: &Pool) -> (String, String) {
    let code = create_room(
        pool,
        turnier_draft::CreateRoomOptions {
            team1_name: "Mannschaft Eins".to_string(),
            team2_name: "Mannschaft Zwei".to_string(),
            sequence: turnier_draft::sequence_for_bans(0),
            bans_per_team: 0,
            round_seconds: Some(0),
        },
    )
    .await
    .expect("Raum anlegen");
    let erster = claim_room(pool, &code, 1).await.expect("Claim 1");
    let zweiter = claim_room(pool, &code, 2).await.expect("Claim 2");
    room_ready(pool, &code, &erster.token)
        .await
        .expect("Ready 1");
    room_ready(pool, &code, &zweiter.token)
        .await
        .expect("Ready 2");
    let helden = [
        "Abrams",
        "Bebop",
        "Calico",
        "Dynamo",
        "Grey Talon",
        "Haze",
        "Holliday",
        "Infernus",
        "Ivy",
        "Kelvin",
        "Lady Geist",
        "Lash",
    ];
    for held in helden {
        let token = if turnier_draft::get_state_by_code(pool, &code)
            .await
            .expect("Zustand")
            .current_team_slot
            == Some(1)
        {
            &erster.token
        } else {
            &zweiter.token
        };
        take_lobby_action(pool, &code, token, held)
            .await
            .expect("Zug");
    }
    (code, erster.token)
}

async fn lobby_felder(
    pool: &Pool,
    code: &str,
) -> (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Value>,
) {
    sqlx::query_as(
        "SELECT lobby_status, lobby_party_id, lobby_join_code, lobby_match_id, lobby_result \
         FROM turnier.draft_sessions WHERE code = $1",
    )
    .bind(code)
    .fetch_one(pool)
    .await
    .expect("Lobby-Felder lesen")
}

#[tokio::test]
async fn lobby_job_bringt_einen_draft_bis_zum_ergebnis() {
    let db = test_pool().await.expect("central test pool");
    let bot = FakeSteamBot::default();
    let (steam_addr, steam_handle) = serve(
        Router::new()
            .route("/scrims/v1/operations", post(operations))
            .with_state(bot.clone()),
    )
    .await;
    let broker_log: BrokerLog = Arc::new(Mutex::new(Vec::new()));
    let (broker_addr, broker_handle) = serve(
        Router::new()
            .route(
                "/internal/master/v1/discord/send-rich-message",
                post(broker),
            )
            .with_state(broker_log.clone()),
    )
    .await;
    let state = build_state(
        &format!("http://{steam_addr}"),
        &format!("http://{broker_addr}"),
        &db,
    )
    .await;

    let (code, captain_token) = spiel_raum_fertig(db.pool()).await;
    let (status, _, _, _, _) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "angefordert");

    process_scrim_lobby_once(&state)
        .await
        .expect("Provision-Schritt");
    let (status, party, join, _, _) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "bereit");
    assert_eq!(party.as_deref(), Some("party-1"));
    assert_eq!(join.as_deref(), Some("JOIN-11"));
    assert_eq!(
        bot.seen_token.lock().unwrap().as_deref(),
        Some("test-token")
    );
    assert_eq!(broker_log.lock().unwrap().len(), 1);
    assert_eq!(
        broker_log.lock().unwrap()[0]["idempotency_key"],
        json!(format!("draft:{code}:lobby"))
    );
    assert_eq!(broker_log.lock().unwrap()[0]["channel_id"], 555);

    process_scrim_lobby_once(&state)
        .await
        .expect("Reconcile ohne Match");
    let (status, _, _, match_id, _) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "bereit");
    assert!(match_id.is_none());

    process_scrim_lobby_once(&state)
        .await
        .expect("Reconcile mit Match");
    let (status, _, join, match_id, _) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "gestartet");
    assert_eq!(match_id.as_deref(), Some("987654321"));
    assert_eq!(join.as_deref(), Some("CONNECT-22"));
    assert_eq!(broker_log.lock().unwrap().len(), 1);

    process_scrim_lobby_once(&state)
        .await
        .expect("Ergebnis vorlaeufig");
    let (status, _, _, _, result) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "gestartet");
    assert!(result.is_none());

    process_scrim_lobby_once(&state)
        .await
        .expect("Ergebnis final");
    let (status, _, _, _, result) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "beendet");
    let result = result.expect("Ergebnis gespeichert");
    assert_eq!(result["outcome"], "team0");
    assert_eq!(result["duration_s"], 2050);
    assert_eq!(bot.release_count.load(Ordering::SeqCst), 1);

    let (team1_name, _team2_name): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT team1_name, team2_name FROM turnier.draft_sessions WHERE code = $1")
            .bind(&code)
            .fetch_one(db.pool())
            .await
            .expect("Teamnamen lesen");
    let (ergebnis_key, ergebnis_channel, ergebnis_description, ergebnis_match_id) = {
        let posts = broker_log.lock().unwrap();
        assert_eq!(posts.len(), 2);
        let match_field = posts[1]["embed"]["fields"]
            .as_array()
            .expect("fields")
            .iter()
            .find(|field| field["name"] == "Match-ID")
            .expect("Match-ID-Feld")["value"]
            .clone();
        (
            posts[1]["idempotency_key"].clone(),
            posts[1]["channel_id"].clone(),
            posts[1]["embed"]["description"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            match_field,
        )
    };
    assert_eq!(ergebnis_key, json!(format!("draft:{code}:ergebnis")));
    assert_eq!(ergebnis_channel, 555);
    assert!(
        ergebnis_description.contains(&format!(
            "Sieger: {}",
            team1_name.as_deref().unwrap_or("Team 1")
        )),
        "description={ergebnis_description}"
    );
    assert!(ergebnis_description.contains("34m 10s"));
    assert_eq!(ergebnis_match_id, "987654321");

    turnier_draft::retry_lobby_request(db.pool(), &code, &captain_token)
        .await
        .expect_err("Retry nach Beendet ist unzulaessig");

    steam_handle.abort();
    broker_handle.abort();
}

#[tokio::test]
async fn fehlende_provision_landet_im_fehler_und_retry_setzt_zurueck() {
    let db = test_pool().await.expect("central test pool");
    let bot = FakeSteamBot {
        provision_fail: Arc::new(AtomicBool::new(true)),
        reconcile_count: Arc::new(AtomicUsize::new(0)),
        collect_count: Arc::new(AtomicUsize::new(0)),
        release_count: Arc::new(AtomicUsize::new(0)),
        seen_token: Arc::new(Mutex::new(None)),
    };
    let (steam_addr, steam_handle) = serve(
        Router::new()
            .route("/scrims/v1/operations", post(operations))
            .with_state(bot.clone()),
    )
    .await;
    let broker_log: BrokerLog = Arc::new(Mutex::new(Vec::new()));
    let (broker_addr, broker_handle) = serve(
        Router::new()
            .route(
                "/internal/master/v1/discord/send-rich-message",
                post(broker),
            )
            .with_state(broker_log.clone()),
    )
    .await;
    let state = build_state(
        &format!("http://{steam_addr}"),
        &format!("http://{broker_addr}"),
        &db,
    )
    .await;

    let (code, captain_token) = spiel_raum_fertig(db.pool()).await;
    process_scrim_lobby_once(&state)
        .await
        .expect("Provision-Schritt mit Fehler");

    let (status, _, _, _, _) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "fehler");
    let error_text: Option<String> =
        sqlx::query_scalar("SELECT lobby_error FROM turnier.draft_sessions WHERE code = $1")
            .bind(&code)
            .fetch_one(db.pool())
            .await
            .expect("Fehlertext lesen");
    assert!(error_text
        .as_deref()
        .is_some_and(|text| text.contains("Steam-Client nicht eingeloggt")));
    assert!(broker_log.lock().unwrap().is_empty());

    turnier_draft::retry_lobby_request(db.pool(), &code, "falsch")
        .await
        .expect_err("Retry ohne Captain ist unzulaessig");
    turnier_draft::retry_lobby_request(db.pool(), &code, &captain_token)
        .await
        .expect("Retry durch den Captain");

    let (status, _, _, _, _) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "angefordert");
    let error_text: Option<String> =
        sqlx::query_scalar("SELECT lobby_error FROM turnier.draft_sessions WHERE code = $1")
            .bind(&code)
            .fetch_one(db.pool())
            .await
            .expect("Fehlertext lesen");
    assert!(error_text.is_none());

    bot.provision_fail.store(false, Ordering::SeqCst);
    process_scrim_lobby_once(&state)
        .await
        .expect("Provision nach Retry");
    let (status, _, _, _, _) = lobby_felder(db.pool(), &code).await;
    assert_eq!(status, "bereit");
    assert_eq!(broker_log.lock().unwrap().len(), 1);

    steam_handle.abort();
    broker_handle.abort();
}

#[tokio::test]
async fn lobby_post_wird_nur_einmal_gesendet() {
    let db = test_pool().await.expect("central test pool");
    let bot = FakeSteamBot::default();
    let (steam_addr, steam_handle) = serve(
        Router::new()
            .route("/scrims/v1/operations", post(operations))
            .with_state(bot.clone()),
    )
    .await;
    let broker_log: BrokerLog = Arc::new(Mutex::new(Vec::new()));
    let (broker_addr, broker_handle) = serve(
        Router::new()
            .route(
                "/internal/master/v1/discord/send-rich-message",
                post(broker),
            )
            .with_state(broker_log.clone()),
    )
    .await;
    let state = build_state(
        &format!("http://{steam_addr}"),
        &format!("http://{broker_addr}"),
        &db,
    )
    .await;

    let (code, _) = spiel_raum_fertig(db.pool()).await;
    for _ in 0..3 {
        process_scrim_lobby_once(&state)
            .await
            .expect("Lobby-Schritt");
    }

    let lobby_posts = broker_log
        .lock()
        .unwrap()
        .iter()
        .filter(|post| post["idempotency_key"] == json!(format!("draft:{code}:lobby")))
        .count();
    assert_eq!(lobby_posts, 1);

    steam_handle.abort();
    broker_handle.abort();
}
