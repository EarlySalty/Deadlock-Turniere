//! Zugriff auf die externe Steam-Bridge-SQLite (`steam_tasks`-Queue).
//!
//! Portiert `match/steam_bridge.py`. Diese DB gehört dem separaten Steam-Worker
//! und liegt unter `STEAM_BRIDGE_DB_PATH` (read+write). Ein EIGENER sqlx-Pool
//! (statt Connect-pro-Aufruf) ersetzt das Python-`aiosqlite.connect`-Muster; die
//! Spalten (`id, type, payload, status, result, error, created_at, updated_at,
//! started_at, finished_at, attempts`) sind 1:1 die des Workers.
//!
//! Verhalten 1:1 erhalten (auch die als „behavior-change" markierten Befunde):
//! `_fail_stale_running_tasks` läuft bei JEDEM `create_task`/`get_task`/
//! `has_active_task` (Befund steam_bridge.py:21-34 — bewusst NICHT in einen
//! Reaper-Task entkoppelt). Die FAILED-Antwort von `poll_task_result` bleibt ein
//! Ergebnis (kein Fehler) — modelliert als [`TaskOutcome::Failed`] (Befund
//! steam_bridge.py:55-92 — bewusst erhalten).
//!
//! Degradiert sauber: fehlt der Pfad oder die Datei, liefert [`SteamBridge::open`]
//! `Ok(None)` (mit Warnung) statt zu crashen.

use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant};

use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use thiserror::Error;

/// 120s — ab wann ein RUNNING-Task als hängend gilt (`STALE_RUNNING_TASK_TIMEOUT_MS`).
const STALE_RUNNING_TASK_TIMEOUT_MS: i64 = 120_000;

/// Task-Typ für Spieler-Einladungen (`GC_LOBBY_INVITE_PLAYER`).
pub const GC_LOBBY_INVITE_PLAYER: &str = "GC_LOBBY_INVITE_PLAYER";

/// Fehler beim Zugriff auf die Steam-Bridge-DB.
#[derive(Debug, Error)]
pub enum BridgeError {
    /// Ungültiges Argument (z. B. leere `party_id`). Im Original ein `ValueError`
    /// in `invite_players_to_lobby`; in der Praxis unerreichbar, da der Lobby-Flow
    /// nur mit validierter `party_id` aufruft.
    #[error("{0}")]
    InvalidArg(String),

    /// Ein Task wurde nicht gefunden (`RuntimeError(f"Steam task {id} nicht gefunden")`).
    #[error("Steam task {0} nicht gefunden")]
    TaskNotFound(i64),

    /// Das Result-JSON eines DONE-Tasks ist ungültig.
    #[error("Steam task {0} hat ungültiges Result-JSON")]
    BadResultJson(i64),

    /// Persistenz-Fehler auf der Bridge-DB.
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Aktueller Zeitstempel in Millisekunden (entspricht `_now_ms`).
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Ergebnis von [`SteamBridge::poll_task_result`]. Modelliert die zwei
/// Erfolgs-/Fehler-Konventionen des Originals EINHEITLICH als Enum: ein DONE-Task
/// liefert [`TaskOutcome::Done`] (dekodiertes Result-JSON), ein FAILED-Task
/// [`TaskOutcome::Failed`] (kein Fehler!), ein Timeout [`TaskOutcome::TimedOut`].
#[derive(Debug, Clone)]
pub enum TaskOutcome {
    /// Task DONE — das dekodierte Result als JSON-Objekt (leeres Result → `{}`
    /// mit implizitem `success:true` durch den Aufrufer).
    Done(Value),
    /// Task FAILED — Fehlertext (im Original `{success:false, error, task_id}`).
    Failed { task_id: i64, error: String },
    /// Timeout — der Task hat innerhalb des Limits nicht geantwortet.
    TimedOut { task_id: i64, timeout_s: f64 },
}

/// Read+Write-Handle auf die `steam_tasks`-Queue.
pub struct SteamBridge {
    pool: SqlitePool,
}

impl SteamBridge {
    /// Öffnet den Pool auf die Bridge-DB unter `db_path`.
    ///
    /// `Ok(None)`, wenn der Pfad leer ist ODER die Datei nicht existiert (mit
    /// Warnung) — der Match-Flow degradiert dann sauber (im Original crashte der
    /// `aiosqlite.connect`; hier vorab gefangen). Die DB wird NICHT angelegt
    /// (`create_if_missing(false)`): sie gehört dem Steam-Worker.
    pub async fn open(db_path: &str) -> Result<Option<Self>, BridgeError> {
        if db_path.trim().is_empty() {
            tracing::warn!("Steam-Bridge-DB-Pfad nicht konfiguriert — Steam-Tasks übersprungen");
            return Ok(None);
        }
        if !Path::new(db_path).exists() {
            tracing::warn!(
                path = db_path,
                "Steam-Bridge-DB nicht vorhanden — Steam-Tasks übersprungen"
            );
            return Ok(None);
        }
        let options = SqliteConnectOptions::from_str(db_path)
            .unwrap_or_else(|_| SqliteConnectOptions::new().filename(db_path))
            .create_if_missing(false)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        Ok(Some(Self { pool }))
    }

    /// Konstruktor aus einem bereits geöffneten Pool (für Tests mit Temp-DB).
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Legt einen Steam-Task an und gibt seine ID zurück.
    /// Läuft VORHER `_fail_stale_running_tasks` (1:1 erhalten).
    pub async fn create_task(&self, task_type: &str, payload: &Value) -> Result<i64, BridgeError> {
        let now = now_ms();
        self.fail_stale_running_tasks().await?;
        let payload_str = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
        let result = sqlx::query(
            "INSERT INTO steam_tasks(type, payload, status, created_at, updated_at) \
             VALUES (?, ?, 'PENDING', ?, ?)",
        )
        .bind(task_type)
        .bind(payload_str)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(result.last_insert_rowid())
    }

    /// Lädt eine Task-Zeile (oder `None`). Läuft VORHER den Stale-Cleanup.
    pub async fn get_task(&self, task_id: i64) -> Result<Option<TaskRecord>, BridgeError> {
        self.fail_stale_running_tasks().await?;
        let row = sqlx::query(
            "SELECT id, type, payload, status, result, error, \
                    created_at, updated_at, started_at, finished_at, attempts \
             FROM steam_tasks WHERE id = ?",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| TaskRecord {
            status: r.get::<Option<String>, _>("status").unwrap_or_default(),
            result: r.get("result"),
            error: r.get("error"),
        }))
    }

    /// Pollt bis DONE/FAILED oder Timeout. `poll_interval_s` wie im Original 0.5s.
    /// Task verschwindet → [`BridgeError::TaskNotFound`] (im Original `RuntimeError`).
    pub async fn poll_task_result(
        &self,
        task_id: i64,
        timeout_s: f64,
    ) -> Result<TaskOutcome, BridgeError> {
        let deadline = Instant::now() + Duration::from_secs_f64(timeout_s.max(0.0));
        let poll_interval = Duration::from_millis(500);
        loop {
            if Instant::now() >= deadline {
                return Ok(TaskOutcome::TimedOut { task_id, timeout_s });
            }
            let task = self
                .get_task(task_id)
                .await?
                .ok_or(BridgeError::TaskNotFound(task_id))?;
            let status = task.status.to_uppercase();
            if status == "DONE" {
                let payload = task.result.unwrap_or_default();
                if payload.is_empty() {
                    return Ok(TaskOutcome::Done(serde_json::json!({ "success": true })));
                }
                let value: Value = serde_json::from_str(&payload)
                    .map_err(|_| BridgeError::BadResultJson(task_id))?;
                return Ok(TaskOutcome::Done(value));
            }
            if status == "FAILED" {
                return Ok(TaskOutcome::Failed {
                    task_id,
                    error: task
                        .error
                        .filter(|e| !e.is_empty())
                        .unwrap_or_else(|| "Steam-Task fehlgeschlagen".to_string()),
                });
            }
            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Prüft, ob ein passender PENDING/RUNNING-Task existiert. Optionale Filter
    /// werden per `json_extract` auf das Payload angewendet (1:1 zum Original).
    /// Läuft VORHER den Stale-Cleanup.
    pub async fn has_active_task(
        &self,
        filter: &ActiveTaskFilter<'_>,
    ) -> Result<bool, BridgeError> {
        self.fail_stale_running_tasks().await?;

        let mut clauses = vec![
            "type = ?".to_string(),
            "status IN ('PENDING', 'RUNNING')".to_string(),
        ];
        if filter.match_id.is_some() {
            clauses.push("json_extract(payload, '$.match_id') = ?".to_string());
        }
        if filter.match_type.is_some() {
            clauses.push("json_extract(payload, '$.match_type') = ?".to_string());
        }
        if filter.party_id.is_some() {
            clauses.push("json_extract(payload, '$.party_id') = ?".to_string());
        }
        if filter.steam_id.is_some() {
            clauses.push("json_extract(payload, '$.steam_id') = ?".to_string());
        }
        let sql = format!(
            "SELECT 1 FROM steam_tasks WHERE {} LIMIT 1",
            clauses.join(" AND ")
        );

        let mut query = sqlx::query(&sql).bind(filter.task_type);
        if let Some(match_id) = filter.match_id {
            query = query.bind(match_id);
        }
        if let Some(match_type) = filter.match_type {
            query = query.bind(match_type);
        }
        if let Some(party_id) = filter.party_id {
            query = query.bind(party_id);
        }
        if let Some(steam_id) = filter.steam_id {
            query = query.bind(steam_id);
        }
        let found = query.fetch_optional(&self.pool).await?;
        Ok(found.is_some())
    }

    /// Lädt mehrere Spieler in eine Lobby ein (eindeutige, getrimmte Steam-IDs).
    /// Pro ID: vorhandener aktiver Invite → skipped; sonst Task anlegen + pollen.
    /// Timeout/FAILED → failed-Bucket. Portiert `invite_players_to_lobby`.
    pub async fn invite_players_to_lobby(
        &self,
        party_id: &str,
        steam_ids: &[String],
    ) -> Result<InviteResult, BridgeError> {
        let normalized_party_id = party_id.trim().to_string();
        if normalized_party_id.is_empty() {
            return Err(BridgeError::InvalidArg(
                "party_id ist erforderlich".to_string(),
            ));
        }

        let mut unique_steam_ids: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for steam_id in steam_ids {
            let normalized = steam_id.trim().to_string();
            if normalized.is_empty() || !seen.insert(normalized.clone()) {
                continue;
            }
            unique_steam_ids.push(normalized);
        }

        let mut result = InviteResult {
            party_id: normalized_party_id.clone(),
            invited: Vec::new(),
            failed: Vec::new(),
            skipped: Vec::new(),
        };

        for steam_id in &unique_steam_ids {
            let active = self
                .has_active_task(&ActiveTaskFilter {
                    task_type: GC_LOBBY_INVITE_PLAYER,
                    party_id: Some(&normalized_party_id),
                    steam_id: Some(steam_id),
                    ..ActiveTaskFilter::new(GC_LOBBY_INVITE_PLAYER)
                })
                .await?;
            if active {
                result.skipped.push(InviteSkipped {
                    steam_id: steam_id.clone(),
                    party_id: normalized_party_id.clone(),
                    reason: "already_active".to_string(),
                });
                continue;
            }

            let payload = serde_json::json!({
                "steam_id": steam_id,
                "party_id": normalized_party_id,
            });
            let task_id = self.create_task(GC_LOBBY_INVITE_PLAYER, &payload).await?;
            match self.poll_task_result(task_id, 30.0).await? {
                TaskOutcome::TimedOut { timeout_s, .. } => {
                    result.failed.push(InviteFailed {
                        steam_id: steam_id.clone(),
                        party_id: normalized_party_id.clone(),
                        task_id,
                        error: format!(
                            "Steam task {task_id} hat innerhalb von {timeout_s}s nicht geantwortet"
                        ),
                    });
                }
                TaskOutcome::Failed { error, .. } => {
                    let err = if error.is_empty() {
                        "Steam invite failed".to_string()
                    } else {
                        error
                    };
                    result.failed.push(InviteFailed {
                        steam_id: steam_id.clone(),
                        party_id: normalized_party_id.clone(),
                        task_id,
                        error: err,
                    });
                }
                TaskOutcome::Done(value) => {
                    result.invited.push(InviteInvited {
                        steam_id: steam_id.clone(),
                        party_id: normalized_party_id.clone(),
                        task_id,
                        result: value,
                    });
                }
            }
        }

        Ok(result)
    }

    /// Markiert hängende RUNNING-Tasks (älter als 120s) als FAILED.
    /// Portiert `_fail_stale_running_tasks`.
    async fn fail_stale_running_tasks(&self) -> Result<(), BridgeError> {
        let now = now_ms();
        let stale_before = now - STALE_RUNNING_TASK_TIMEOUT_MS;
        sqlx::query(
            "UPDATE steam_tasks \
             SET status = 'FAILED', \
                 error = COALESCE(error, 'Steam worker stale or crashed while task was RUNNING'), \
                 updated_at = ?, \
                 finished_at = ? \
             WHERE status = 'RUNNING' AND started_at IS NOT NULL AND started_at < ?",
        )
        .bind(now)
        .bind(now)
        .bind(stale_before)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Die für den Match-Flow relevanten Felder einer Task-Zeile.
#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// Filter für [`SteamBridge::has_active_task`].
#[derive(Debug, Clone)]
pub struct ActiveTaskFilter<'a> {
    pub task_type: &'a str,
    pub match_id: Option<i64>,
    pub match_type: Option<&'a str>,
    pub party_id: Option<&'a str>,
    pub steam_id: Option<&'a str>,
}

impl<'a> ActiveTaskFilter<'a> {
    /// Filter nur nach Typ (alle optionalen Felder leer).
    pub fn new(task_type: &'a str) -> Self {
        Self {
            task_type,
            match_id: None,
            match_type: None,
            party_id: None,
            steam_id: None,
        }
    }
}

/// Ergebnis von [`SteamBridge::invite_players_to_lobby`]. `success` entspricht
/// `not failed` (kein Eintrag in `failed`).
#[derive(Debug, Clone)]
pub struct InviteResult {
    pub party_id: String,
    pub invited: Vec<InviteInvited>,
    pub failed: Vec<InviteFailed>,
    pub skipped: Vec<InviteSkipped>,
}

impl InviteResult {
    /// `true`, solange kein Invite fehlgeschlagen ist (Original-`success`-Feld).
    pub fn success(&self) -> bool {
        self.failed.is_empty()
    }

    /// Wire-Form (`{success, party_id, invited, failed, skipped}`) als JSON.
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "success": self.success(),
            "party_id": self.party_id,
            "invited": self.invited.iter().map(InviteInvited::to_value).collect::<Vec<_>>(),
            "failed": self.failed.iter().map(InviteFailed::to_value).collect::<Vec<_>>(),
            "skipped": self.skipped.iter().map(InviteSkipped::to_value).collect::<Vec<_>>(),
        })
    }
}

/// Ein erfolgreicher Invite.
#[derive(Debug, Clone)]
pub struct InviteInvited {
    pub steam_id: String,
    pub party_id: String,
    pub task_id: i64,
    pub result: Value,
}

impl InviteInvited {
    fn to_value(&self) -> Value {
        serde_json::json!({
            "steam_id": self.steam_id,
            "party_id": self.party_id,
            "task_id": self.task_id,
            "result": self.result,
        })
    }
}

/// Ein fehlgeschlagener Invite.
#[derive(Debug, Clone)]
pub struct InviteFailed {
    pub steam_id: String,
    pub party_id: String,
    pub task_id: i64,
    pub error: String,
}

impl InviteFailed {
    fn to_value(&self) -> Value {
        serde_json::json!({
            "steam_id": self.steam_id,
            "party_id": self.party_id,
            "task_id": self.task_id,
            "error": self.error,
        })
    }
}

/// Ein übersprungener Invite (bereits aktiver Task).
#[derive(Debug, Clone)]
pub struct InviteSkipped {
    pub steam_id: String,
    pub party_id: String,
    pub reason: String,
}

impl InviteSkipped {
    fn to_value(&self) -> Value {
        serde_json::json!({
            "steam_id": self.steam_id,
            "party_id": self.party_id,
            "reason": self.reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU64, Ordering};

    /// Frische In-Memory-Bridge mit dem `steam_tasks`-Schema des Workers
    /// (shared-cache, damit alle Pool-Connections dieselbe DB sehen).
    async fn temp_bridge() -> SteamBridge {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let url = format!("sqlite:file:tb_match_bridge_{unique}?mode=memory&cache=shared");
        let options = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true);
        // max_connections=1: hält die shared-cache-In-Memory-DB am Leben.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE steam_tasks (\
                id INTEGER PRIMARY KEY AUTOINCREMENT, \
                type TEXT NOT NULL, \
                payload TEXT, \
                status TEXT NOT NULL DEFAULT 'PENDING', \
                result TEXT, \
                error TEXT, \
                created_at INTEGER, \
                updated_at INTEGER, \
                started_at INTEGER, \
                finished_at INTEGER, \
                attempts INTEGER DEFAULT 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        SteamBridge::from_pool(pool)
    }

    #[tokio::test]
    async fn open_fehlender_pfad_ist_none() {
        assert!(SteamBridge::open("").await.unwrap().is_none());
        assert!(SteamBridge::open("/nonexistent/path/x.sqlite3")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn create_und_poll_done() {
        let bridge = temp_bridge().await;
        let id = bridge
            .create_task("GC_TEST", &serde_json::json!({ "match_id": 5 }))
            .await
            .unwrap();
        sqlx::query("UPDATE steam_tasks SET status='DONE', result=? WHERE id=?")
            .bind(r#"{"party_id":"P1","success":true}"#)
            .bind(id)
            .execute(&bridge.pool)
            .await
            .unwrap();
        match bridge.poll_task_result(id, 5.0).await.unwrap() {
            TaskOutcome::Done(v) => assert_eq!(v["party_id"], serde_json::json!("P1")),
            other => panic!("erwartete Done, war {other:?}"),
        }
    }

    #[tokio::test]
    async fn poll_failed_ist_kein_fehler() {
        let bridge = temp_bridge().await;
        let id = bridge
            .create_task("GC_TEST", &serde_json::json!({}))
            .await
            .unwrap();
        sqlx::query("UPDATE steam_tasks SET status='FAILED', error='kaputt' WHERE id=?")
            .bind(id)
            .execute(&bridge.pool)
            .await
            .unwrap();
        match bridge.poll_task_result(id, 5.0).await.unwrap() {
            TaskOutcome::Failed { error, .. } => assert_eq!(error, "kaputt"),
            other => panic!("erwartete Failed, war {other:?}"),
        }
    }

    #[tokio::test]
    async fn poll_timeout() {
        let bridge = temp_bridge().await;
        let id = bridge
            .create_task("GC_TEST", &serde_json::json!({}))
            .await
            .unwrap();
        // Bleibt PENDING → Timeout.
        match bridge.poll_task_result(id, 0.05).await.unwrap() {
            TaskOutcome::TimedOut { task_id, .. } => assert_eq!(task_id, id),
            other => panic!("erwartete TimedOut, war {other:?}"),
        }
    }

    #[tokio::test]
    async fn has_active_task_matcht_payload() {
        let bridge = temp_bridge().await;
        bridge
            .create_task(
                "GC_CREATE_CUSTOM_LOBBY",
                &serde_json::json!({ "match_id": 7, "match_type": "bracket" }),
            )
            .await
            .unwrap();
        let found = bridge
            .has_active_task(&ActiveTaskFilter {
                match_id: Some(7),
                match_type: Some("bracket"),
                ..ActiveTaskFilter::new("GC_CREATE_CUSTOM_LOBBY")
            })
            .await
            .unwrap();
        assert!(found);
        let not_found = bridge
            .has_active_task(&ActiveTaskFilter {
                match_id: Some(8),
                ..ActiveTaskFilter::new("GC_CREATE_CUSTOM_LOBBY")
            })
            .await
            .unwrap();
        assert!(!not_found);
    }

    #[tokio::test]
    async fn stale_running_wird_failed() {
        let bridge = temp_bridge().await;
        let old = now_ms() - STALE_RUNNING_TASK_TIMEOUT_MS - 1000;
        sqlx::query(
            "INSERT INTO steam_tasks(type, payload, status, created_at, updated_at, started_at) \
             VALUES ('GC_X', '{}', 'RUNNING', ?, ?, ?)",
        )
        .bind(old)
        .bind(old)
        .bind(old)
        .execute(&bridge.pool)
        .await
        .unwrap();
        // get_task triggert den Cleanup.
        let _ = bridge.get_task(999).await.unwrap();
        let status: String = sqlx::query("SELECT status FROM steam_tasks WHERE type='GC_X'")
            .fetch_one(&bridge.pool)
            .await
            .unwrap()
            .get("status");
        assert_eq!(status, "FAILED");
    }

    #[tokio::test]
    async fn invite_skip_und_invite() {
        let bridge = temp_bridge().await;
        // Erster Steam-ID-Task wird sofort als DONE markiert via separater Logik:
        // wir simulieren, indem wir VORAB einen aktiven Invite anlegen → skipped.
        bridge
            .create_task(
                GC_LOBBY_INVITE_PLAYER,
                &serde_json::json!({ "party_id": "P", "steam_id": "111" }),
            )
            .await
            .unwrap();
        let out = bridge
            .invite_players_to_lobby("P", &["111".into()])
            .await
            .unwrap();
        assert_eq!(out.skipped.len(), 1);
        assert!(out.success());
    }
}
