//! Lifecycle-Protokollierung einzelner Discord-Effekte in `discord_tasks`.
//!
//! Das Python-Original schrieb erst `PENDING` und ersetzte das SOFORT durch
//! `RUNNING` — der `PENDING`-Zustand war faktisch nie beobachtbar (kein externer
//! Worker liest die Tabelle; der Versand läuft synchron inline; Befund
//! discord_notifier.py:161/193/224/293 — "safe"). Der Port fügt deshalb direkt
//! mit `status='RUNNING'` ein und schreibt nur den Abschluss als einzelnes
//! UPDATE. Die beobachtbaren End-Zeilen (`DONE`/`FAILED` mit
//! `result_payload`/`error`) bleiben identisch.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use turnier_db::Pool;

use crate::error::BrokerError;

/// Persistierte Discord-Effekte. `ANNOUNCE_TOURNAMENT` ergaenzt die bestehenden
/// Match-/DM-Typen fuer den retry-faehigen Routine-Scheduler.
#[derive(Debug, Clone, Copy)]
pub enum TaskType {
    CreateChannel,
    SendMatchInfo,
    DeleteChannel,
    SendDm,
    AnnounceTournament,
}

impl TaskType {
    fn as_str(self) -> &'static str {
        match self {
            Self::CreateChannel => "CREATE_CHANNEL",
            Self::SendMatchInfo => "SEND_MATCH_INFO",
            Self::DeleteChannel => "DELETE_CHANNEL",
            Self::SendDm => "SEND_DM",
            Self::AnnounceTournament => "ANNOUNCE_TOURNAMENT",
        }
    }
}

fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

/// Legt eine `discord_tasks`-Zeile direkt mit `status='RUNNING'` an und gibt
/// deren `id` zurück. `payload` wird als JSONB gebunden.
pub async fn create_running(
    pool: &Pool,
    task_type: TaskType,
    payload: &impl Serialize,
) -> sqlx::Result<i64> {
    let now = now_utc();
    let payload_json = serde_json::to_value(payload).unwrap_or(Value::Null);
    let id = sqlx::query_scalar(
        "INSERT INTO turnier.discord_tasks (type, payload, status, created_at, updated_at) \
         VALUES ($1, $2, 'RUNNING', $3, $4) RETURNING id",
    )
    .bind(task_type.as_str())
    .bind(payload_json)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Schreibt den Abschluss eines Tasks (`DONE`/`FAILED`) als einzelnes UPDATE.
async fn finish(
    pool: &Pool,
    task_id: i64,
    status_value: &str,
    result_payload: Option<Value>,
    error: Option<&str>,
) -> sqlx::Result<()> {
    let now = now_utc();
    sqlx::query(
        "UPDATE turnier.discord_tasks \
         SET status = $1, result_payload = $2, error = $3, updated_at = $4 \
         WHERE id = $5",
    )
    .bind(status_value)
    .bind(result_payload)
    .bind(error)
    .bind(now)
    .bind(task_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Markiert einen Task als `DONE` mit (optionalem) Result-Payload.
pub async fn mark_done(
    pool: &Pool,
    task_id: i64,
    result_payload: Option<&serde_json::Value>,
) -> sqlx::Result<()> {
    let payload = result_payload.cloned();
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
