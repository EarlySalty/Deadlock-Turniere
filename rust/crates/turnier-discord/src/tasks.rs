//! Lifecycle-Protokollierung einzelner Discord-Effekte in `discord_tasks`.
//!
//! Das Python-Original schrieb erst `PENDING` und ersetzte das SOFORT durch
//! `RUNNING` — der `PENDING`-Zustand war faktisch nie beobachtbar (kein externer
//! Worker liest die Tabelle; der Versand läuft synchron inline; Befund
//! discord_notifier.py:161/193/224/293 — "safe"). Der Port fügt deshalb direkt
//! mit `status='RUNNING'` ein und schreibt nur den Abschluss als einzelnes
//! UPDATE. Die beobachtbaren End-Zeilen (`DONE`/`FAILED` mit
//! `result_payload`/`error`) bleiben identisch.

use chrono::Utc;
use serde::Serialize;
use sqlx::Row;
use turnier_db::Pool;

use crate::error::BrokerError;

/// Task-Typen, die das Original protokolliert (`CREATE_CHANNEL`,
/// `SEND_MATCH_INFO`, `DELETE_CHANNEL`, `SEND_DM`).
#[derive(Debug, Clone, Copy)]
pub enum TaskType {
    CreateChannel,
    SendMatchInfo,
    DeleteChannel,
    SendDm,
}

impl TaskType {
    fn as_str(self) -> &'static str {
        match self {
            Self::CreateChannel => "CREATE_CHANNEL",
            Self::SendMatchInfo => "SEND_MATCH_INFO",
            Self::DeleteChannel => "DELETE_CHANNEL",
            Self::SendDm => "SEND_DM",
        }
    }
}

/// Aktueller UTC-Zeitstempel als ISO-8601-String (wie
/// `datetime.now(timezone.utc).isoformat()` im Original).
fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

/// Legt eine `discord_tasks`-Zeile direkt mit `status='RUNNING'` an und gibt
/// deren `id` zurück. `payload` wird als JSON-Text serialisiert.
pub async fn create_running(pool: &Pool, task_type: TaskType, payload: &impl Serialize) -> sqlx::Result<i64> {
    let now = now_iso();
    let payload_json = serde_json::to_string(payload).unwrap_or_else(|_| "null".to_string());
    let row = sqlx::query(
        "INSERT INTO discord_tasks (type, payload, status, created_at, updated_at) \
         VALUES (?, ?, 'RUNNING', ?, ?) RETURNING id",
    )
    .bind(task_type.as_str())
    .bind(payload_json)
    .bind(&now)
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i64, _>("id"))
}

/// Schreibt den Abschluss eines Tasks (`DONE`/`FAILED`) als einzelnes UPDATE.
async fn finish(
    pool: &Pool,
    task_id: i64,
    status_value: &str,
    result_payload: Option<String>,
    error: Option<&str>,
) -> sqlx::Result<()> {
    let now = now_iso();
    sqlx::query(
        "UPDATE discord_tasks \
         SET status = ?, result_payload = ?, error = ?, updated_at = ? \
         WHERE id = ?",
    )
    .bind(status_value)
    .bind(result_payload)
    .bind(error)
    .bind(&now)
    .bind(task_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Markiert einen Task als `DONE` mit (optionalem) Result-Payload.
pub async fn mark_done(pool: &Pool, task_id: i64, result_payload: Option<&serde_json::Value>) -> sqlx::Result<()> {
    let payload = result_payload.map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".to_string()));
    finish(pool, task_id, "DONE", payload, None).await
}

/// Markiert einen Task als `FAILED` mit Fehlertext. `error` ist die exakte
/// Meldung, die der Aufrufer auch nach außen weiterreicht (`str(exc)`-Parität).
pub async fn mark_failed(pool: &Pool, task_id: i64, error: &str) -> sqlx::Result<()> {
    finish(pool, task_id, "FAILED", None, Some(error)).await
}

/// Übersetzt einen [`BrokerError`] in den `str(exc)`-äquivalenten Fehlertext,
/// der im Original in `error`/`failed` landet (genau die `Display`-Ausgabe).
pub fn error_text(err: &BrokerError) -> String {
    err.to_string()
}
