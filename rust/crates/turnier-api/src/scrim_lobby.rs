use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use turnier_db::Pool;

use crate::state::AppState;

const OPERATIONS_PATH: &str = "/scrims/v1/operations";
const CONTRACT_VERSION: &str = "scrim-steam.v1";
const PROVISION_TIMEOUT: Duration = Duration::from_secs(25);
const OTHER_TIMEOUT: Duration = Duration::from_secs(10);
const RECONCILE_EVERY: Duration = Duration::from_secs(15);
const COLLECT_EVERY: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct ScrimLobbyClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl ScrimLobbyClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(PROVISION_TIMEOUT)
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.trim().to_string(),
        }
    }

    async fn send(
        &self,
        request: &OperationRequest,
        timeout: Duration,
    ) -> Result<OperationResponse, String> {
        if self.base_url.is_empty() {
            return Err("Steam-Bot-Adresse ist nicht konfiguriert".to_string());
        }
        let url = format!("{}{OPERATIONS_PATH}", self.base_url);
        let mut request_builder = self.http.post(&url).timeout(timeout).json(request);
        if !self.token.is_empty() {
            request_builder = request_builder.header("X-Internal-Token", &self.token);
        }
        let response = request_builder
            .send()
            .await
            .map_err(|error| format!("Steam-Bot nicht erreichbar: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| format!("Steam-Bot-Antwort nicht lesbar: {error}"))?;
        if !status.is_success() {
            return Err(format!("Steam-Bot meldete HTTP {status}: {body}"));
        }
        serde_json::from_str(&body)
            .map_err(|error| format!("Steam-Bot-Antwort unverständlich: {error}"))
    }
}

#[derive(Serialize)]
struct OperationRequest {
    version: &'static str,
    meta: OperationMeta,
    aggregate: AggregateRef,
    operation: Operation,
}

#[derive(Serialize)]
struct OperationMeta {
    operation_id: String,
    idempotency_key: String,
    generation: u64,
}

#[derive(Serialize)]
struct AggregateRef {
    kind: &'static str,
    id: String,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
enum Operation {
    LobbyProvision(ProvisionPayload),
    LobbyReconcile(ReconcilePayload),
    FinalResultCollect(CollectPayload),
    LobbyRelease(ReleasePayload),
}

#[derive(Serialize)]
struct ProvisionPayload {
    teams: [SideAssignment; 2],
    lobby_code: Option<String>,
}

#[derive(Serialize)]
struct SideAssignment {
    side: &'static str,
    team_ref: AggregateRef,
    players: Vec<Value>,
}

#[derive(Serialize)]
struct ReconcilePayload {
    party_id: Option<String>,
    lobby_code: Option<String>,
    connect_code: Option<String>,
    match_id: Option<String>,
}

#[derive(Serialize)]
struct CollectPayload {
    match_id: Option<String>,
    party_id: Option<String>,
    finality_hint: &'static str,
}

#[derive(Serialize)]
struct ReleasePayload {
    party_id: Option<String>,
    match_id: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OperationResponse {
    status: String,
    #[serde(default)]
    data: Option<OperationData>,
    #[serde(default)]
    error: Option<OperationError>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
enum OperationData {
    LobbyProvisioned(ProvisionResult),
    LobbyReconciled(ReconcileResult),
    LobbyReleased(ReleaseResult),
    FinalResult(ScrimMatchResult),
}

#[derive(Debug, Deserialize)]
struct ProvisionResult {
    #[serde(default)]
    party_id: Option<String>,
    #[serde(default)]
    lobby_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReconcileResult {
    #[serde(default)]
    match_id: Option<String>,
    #[serde(default)]
    connect_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReleaseResult {
    released: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ScrimMatchResult {
    #[serde(default)]
    match_id: Option<String>,
    outcome: String,
    finality: String,
    #[serde(default)]
    duration_s: Option<u32>,
    #[serde(default)]
    players: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct OperationError {
    #[serde(default)]
    message: Value,
}

impl OperationError {
    fn text(&self) -> String {
        match &self.message {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ScrimLobbyRow {
    id: i64,
    code: String,
    team1_name: Option<String>,
    team2_name: Option<String>,
    bans_per_team: i32,
    lobby_status: String,
    lobby_party_id: Option<String>,
    lobby_join_code: Option<String>,
    lobby_match_id: Option<String>,
    discord_lobby_posted_at: Option<DateTime<Utc>>,
    discord_result_posted_at: Option<DateTime<Utc>>,
}

struct LobbyCadence {
    last_reconcile: Mutex<std::collections::HashMap<i64, Instant>>,
    last_collect: Mutex<std::collections::HashMap<i64, Instant>>,
}

impl LobbyCadence {
    fn due(map: &Mutex<std::collections::HashMap<i64, Instant>>, id: i64, every: Duration) -> bool {
        let mut map = map.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let due = map
            .get(&id)
            .map(|last| last.elapsed() >= every)
            .unwrap_or(true);
        if due {
            map.insert(id, Instant::now());
        }
        due
    }
}

static WORKER_CADENCE: Lazy<LobbyCadence> = Lazy::new(|| LobbyCadence {
    last_reconcile: Mutex::new(std::collections::HashMap::new()),
    last_collect: Mutex::new(std::collections::HashMap::new()),
});

pub fn spawn_scrim_lobby_worker(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = process_scrim_lobby_tick(&state, Some(&WORKER_CADENCE)).await {
                tracing::warn!(%error, "Scrim-Draft-Lobby-Job fehlgeschlagen");
            }
        }
    });
}

pub async fn process_scrim_lobby_once(state: &AppState) -> Result<usize, sqlx::Error> {
    process_scrim_lobby_tick(state, None).await
}

async fn process_scrim_lobby_tick(
    state: &AppState,
    cadence: Option<&LobbyCadence>,
) -> Result<usize, sqlx::Error> {
    let rows: Vec<ScrimLobbyRow> = sqlx::query_as(
        "SELECT id, code, team1_name, team2_name, bans_per_team, lobby_status, \
                lobby_party_id, lobby_join_code, lobby_match_id, \
                discord_lobby_posted_at, discord_result_posted_at \
         FROM turnier.draft_sessions \
         WHERE code IS NOT NULL \
           AND lobby_status IN ('angefordert', 'bereit', 'gestartet') \
         ORDER BY id \
         FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&state.pool)
    .await?;
    let mut processed = 0;
    for row in rows {
        let result = match row.lobby_status.as_str() {
            "angefordert" => handle_provision(state, &row).await,
            "bereit" => handle_reconcile(state, &row, cadence).await,
            "gestartet" => handle_collect(state, &row, cadence).await,
            _ => Ok(()),
        };
        if let Err(error) = result {
            tracing::warn!(
                code = %row.code,
                %error,
                "Scrim-Draft-Lobby-Schritt fehlgeschlagen"
            );
        }
        processed += 1;
    }
    Ok(processed)
}

async fn handle_provision(state: &AppState, row: &ScrimLobbyRow) -> Result<(), String> {
    let generation = Utc::now().timestamp().unsigned_abs();
    let request = OperationRequest {
        version: CONTRACT_VERSION,
        meta: OperationMeta {
            operation_id: format!("draft-{}-provision-{generation}", row.code),
            idempotency_key: format!("draft-{}-provision-{generation}", row.code),
            generation,
        },
        aggregate: AggregateRef {
            kind: "lobby",
            id: row.code.clone(),
        },
        operation: Operation::LobbyProvision(ProvisionPayload {
            teams: side_assignments(&row.code),
            lobby_code: Some(row.code.clone()),
        }),
    };
    let response = match state.scrim_lobby.send(&request, PROVISION_TIMEOUT).await {
        Ok(response) => response,
        Err(error) => {
            mark_failed(&state.pool, &row.code, &error).await;
            return Err(error);
        }
    };
    match response_status(&response).as_str() {
        "done" => {
            let Some(OperationData::LobbyProvisioned(provisioned)) = response.data else {
                return Err(format_unexpected(&response));
            };
            let party_id = provisioned.party_id.unwrap_or_default();
            let join_code = provisioned.lobby_code.unwrap_or_default();
            sqlx::query(
                "UPDATE turnier.draft_sessions \
                 SET lobby_status = 'bereit', lobby_party_id = $1, lobby_join_code = $2, \
                     lobby_error = NULL \
                 WHERE id = $3 AND lobby_status = 'angefordert'",
            )
            .bind(if party_id.is_empty() {
                None
            } else {
                Some(party_id)
            })
            .bind(if join_code.is_empty() {
                None
            } else {
                Some(join_code)
            })
            .bind(row.id)
            .execute(&state.pool)
            .await
            .map_err(|error| error.to_string())?;
            // Post mit der Zeile nach dem UPDATE: die Zeile aus dem Query-Start
            // hat noch lobby_join_code = NULL, der Post wuerde nie den Code zeigen.
            let frisch = frische_lobby_zeile(&state.pool, row.id)
                .await?
                .unwrap_or_else(|| row.clone());
            post_lobby_announcement(state, &frisch).await
        }
        _ => {
            let error_text = response_error_text(&response);
            mark_failed(&state.pool, &row.code, &error_text).await;
            Err(error_text)
        }
    }
}

async fn handle_reconcile(
    state: &AppState,
    row: &ScrimLobbyRow,
    cadence: Option<&LobbyCadence>,
) -> Result<(), String> {
    if let Some(cadence) = cadence {
        if !LobbyCadence::due(&cadence.last_reconcile, row.id, RECONCILE_EVERY) {
            return Ok(());
        }
    }
    let request = OperationRequest {
        version: CONTRACT_VERSION,
        meta: OperationMeta {
            operation_id: format!("draft-{}-reconcile-{}", row.code, Utc::now().timestamp()),
            idempotency_key: format!("draft-{}-reconcile", row.code),
            generation: Utc::now().timestamp().unsigned_abs(),
        },
        aggregate: AggregateRef {
            kind: "lobby",
            id: row.code.clone(),
        },
        operation: Operation::LobbyReconcile(ReconcilePayload {
            party_id: row.lobby_party_id.clone(),
            lobby_code: row.lobby_join_code.clone(),
            connect_code: None,
            match_id: row.lobby_match_id.clone(),
        }),
    };
    let response = match state.scrim_lobby.send(&request, OTHER_TIMEOUT).await {
        Ok(response) => response,
        Err(error) => {
            // Nur der Fehler selbst, kein Statuswechsel: ein Transportfehler
            // mitten in einem laufenden Match darf Match-ID und Ergebnis nicht
            // wegwerfen (BLOCK-Fund 2026-09-11, doppelter Provision-Lauf).
            merke_lobby_fehler(&state.pool, &row.code, &error).await;
            return Err(error);
        }
    };
    let Some(OperationData::LobbyReconciled(reconciled)) = response.data else {
        return Err(format_unexpected(&response));
    };
    if reconciled.match_id.is_none() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE turnier.draft_sessions \
         SET lobby_status = 'gestartet', lobby_match_id = $1, \
             lobby_join_code = COALESCE($2, lobby_join_code) \
         WHERE id = $3 AND lobby_status = 'bereit'",
    )
    .bind(reconciled.match_id)
    .bind(reconciled.connect_code)
    .bind(row.id)
    .execute(&state.pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn handle_collect(
    state: &AppState,
    row: &ScrimLobbyRow,
    cadence: Option<&LobbyCadence>,
) -> Result<(), String> {
    if let Some(cadence) = cadence {
        if !LobbyCadence::due(&cadence.last_collect, row.id, COLLECT_EVERY) {
            return Ok(());
        }
    }
    let request = OperationRequest {
        version: CONTRACT_VERSION,
        meta: OperationMeta {
            operation_id: format!("draft-{}-collect-{}", row.code, Utc::now().timestamp()),
            idempotency_key: format!("draft-{}-collect", row.code),
            generation: Utc::now().timestamp().unsigned_abs(),
        },
        aggregate: AggregateRef {
            kind: "lobby",
            id: row.code.clone(),
        },
        operation: Operation::FinalResultCollect(CollectPayload {
            match_id: row.lobby_match_id.clone(),
            party_id: row.lobby_party_id.clone(),
            finality_hint: "final",
        }),
    };
    let response = match state.scrim_lobby.send(&request, OTHER_TIMEOUT).await {
        Ok(response) => response,
        Err(error) => {
            // Wie Reconcile: kein Statuswechsel aus einem Transportfehler,
            // das Ergebnis bleibt sonst fuer immer verloren.
            merke_lobby_fehler(&state.pool, &row.code, &error).await;
            return Err(error);
        }
    };
    let Some(OperationData::FinalResult(result)) = response.data else {
        return Err(format_unexpected(&response));
    };
    if result.finality != "final" || result.outcome == "unknown" {
        return Ok(());
    }
    let stored = serde_json::to_value(&result)
        .map_err(|error| format!("Ergebnis nicht serialisierbar: {error}"))?;
    sqlx::query(
        "UPDATE turnier.draft_sessions \
         SET lobby_status = 'beendet', lobby_result = $1 \
         WHERE id = $2 AND lobby_status = 'gestartet'",
    )
    .bind(stored)
    .bind(row.id)
    .execute(&state.pool)
    .await
    .map_err(|error| error.to_string())?;

    // Release vor dem Post-Err: beendet faellt aus der Worker-Query, nur der
    // Freigabe-Weg hier sorgt dafuer, dass Posts nachgeholt und die Steam-Lobby
    // freigegeben werden, auch wenn der Post einmal scheitert.
    release_lobby(state, row).await;
    let frisch = frische_lobby_zeile(&state.pool, row.id)
        .await?
        .unwrap_or_else(|| row.clone());
    post_result_announcement(state, &frisch, &result).await?;
    Ok(())
}

async fn frische_lobby_zeile(pool: &Pool, id: i64) -> Result<Option<ScrimLobbyRow>, String> {
    sqlx::query_as::<_, ScrimLobbyRow>(
        "SELECT id, code, team1_name, team2_name, bans_per_team, lobby_status, \
                lobby_party_id, lobby_join_code, lobby_match_id, \
                discord_lobby_posted_at, discord_result_posted_at \
         FROM turnier.draft_sessions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())
}

async fn release_lobby(state: &AppState, row: &ScrimLobbyRow) {
    let request = OperationRequest {
        version: CONTRACT_VERSION,
        meta: OperationMeta {
            operation_id: format!("draft-{}-release-{}", row.code, Utc::now().timestamp()),
            idempotency_key: format!("draft-{}-release", row.code),
            generation: Utc::now().timestamp().unsigned_abs(),
        },
        aggregate: AggregateRef {
            kind: "lobby",
            id: row.code.clone(),
        },
        operation: Operation::LobbyRelease(ReleasePayload {
            party_id: row.lobby_party_id.clone(),
            match_id: row.lobby_match_id.clone(),
            reason: Some("draft-abgeschlossen".to_string()),
        }),
    };
    if let Ok(response) = state.scrim_lobby.send(&request, OTHER_TIMEOUT).await {
        if let Some(OperationData::LobbyReleased(released)) = response.data {
            if !released.released {
                tracing::warn!(code = %row.code, "Scrim-Draft-Lobby wurde nicht freigegeben");
            }
        }
    }
}

async fn mark_failed(pool: &Pool, code: &str, error: &str) {
    if let Err(db_error) = sqlx::query(
        "UPDATE turnier.draft_sessions \
         SET lobby_status = 'fehler', lobby_error = $1 WHERE code = $2",
    )
    .bind(error)
    .bind(code)
    .execute(pool)
    .await
    {
        tracing::warn!(code = %code, error = %db_error, "Lobby-Fehlerzustand konnte nicht gespeichert werden");
    }
}

async fn merke_lobby_fehler(pool: &Pool, code: &str, error: &str) {
    // Fehler sichtbar machen, ohne den Status zu aendern: Reconcile und
    // Collect laufen im naechsten Tick einfach wieder.
    if let Err(db_error) = sqlx::query(
        "UPDATE turnier.draft_sessions \
         SET lobby_error = $1 WHERE code = $2 AND lobby_status IN ('bereit', 'gestartet')",
    )
    .bind(error)
    .bind(code)
    .execute(pool)
    .await
    {
        tracing::warn!(code = %code, error = %db_error, "Lobby-Fehlernote konnte nicht gespeichert werden");
    }
}

fn response_status(response: &OperationResponse) -> String {
    response.status.clone()
}

fn response_error_text(response: &OperationResponse) -> String {
    response
        .error
        .as_ref()
        .map(OperationError::text)
        .unwrap_or_else(|| "Steam-Bot hat die Anfrage abgelehnt".to_string())
}

fn format_unexpected(response: &OperationResponse) -> String {
    format!(
        "Steam-Bot-Antwort passt nicht zum Vorgang (Status {})",
        response.status
    )
}

fn side_assignments(code: &str) -> [SideAssignment; 2] {
    [
        SideAssignment {
            side: "team0",
            team_ref: AggregateRef {
                kind: "team",
                id: format!("draft-{code}-team1"),
            },
            players: Vec::new(),
        },
        SideAssignment {
            side: "team1",
            team_ref: AggregateRef {
                kind: "team",
                id: format!("draft-{code}-team2"),
            },
            players: Vec::new(),
        },
    ]
}

async fn post_lobby_announcement(state: &AppState, row: &ScrimLobbyRow) -> Result<(), String> {
    if row.discord_lobby_posted_at.is_some() {
        return Ok(());
    }
    let Some(channel_id) = positive_channel(state) else {
        return Ok(());
    };
    let picks = load_picks(&state.pool, row.id).await?;
    let bans_text = format!("{} je Team", row.bans_per_team);
    state
        .notifier
        .send_scrim_lobby_post(
            channel_id,
            &format!("draft:{}:lobby", row.code),
            &row.code,
            row.team1_name.as_deref().unwrap_or("Team 1"),
            row.team2_name.as_deref().unwrap_or("Team 2"),
            row.lobby_join_code.as_deref().unwrap_or("offen"),
            &bans_text,
            &picks.0,
            &picks.1,
        )
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE turnier.draft_sessions SET discord_lobby_posted_at = now() WHERE id = $1")
        .bind(row.id)
        .execute(&state.pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn post_result_announcement(
    state: &AppState,
    row: &ScrimLobbyRow,
    result: &ScrimMatchResult,
) -> Result<(), String> {
    if row.discord_result_posted_at.is_some() {
        return Ok(());
    }
    let Some(channel_id) = positive_channel(state) else {
        return Ok(());
    };
    let team1 = row.team1_name.as_deref().unwrap_or("Team 1");
    let team2 = row.team2_name.as_deref().unwrap_or("Team 2");
    let winner_text = match result.outcome.as_str() {
        "team0" => team1.to_string(),
        "team1" => team2.to_string(),
        "draw" => "Unentschieden".to_string(),
        other => other.to_string(),
    };
    state
        .notifier
        .send_scrim_result_post(
            channel_id,
            &format!("draft:{}:ergebnis", row.code),
            team1,
            team2,
            &winner_text,
            result.duration_s.map(i64::from),
            result.match_id.as_deref(),
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE turnier.draft_sessions SET discord_result_posted_at = now() WHERE id = $1")
        .bind(row.id)
        .execute(&state.pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn positive_channel(state: &AppState) -> Option<i64> {
    let channel_id = state.config.scrim_announce_channel_id;
    (channel_id > 0).then_some(channel_id)
}

async fn load_picks(pool: &Pool, session_id: i64) -> Result<(Vec<String>, Vec<String>), String> {
    let rows: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT team_slot, hero_name FROM turnier.draft_actions \
         WHERE session_id = $1 AND action_type = 'pick' AND hero_name IS NOT NULL \
         ORDER BY sequence_index",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut team1 = Vec::new();
    let mut team2 = Vec::new();
    for (slot, hero) in rows {
        if let Some(hero) = hero {
            if slot == 1 {
                team1.push(hero);
            } else {
                team2.push(hero);
            }
        }
    }
    Ok((team1, team2))
}
