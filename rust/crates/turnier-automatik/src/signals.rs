//! Signal-Snapshots fuer spaetere Auswertung.

use chrono::{SecondsFormat, Utc};
use turnier_db::Pool;

use crate::error::AutomatikResult;

/// Eingabe fuer einen Signal-Snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignalSnapshotInput {
    pub participants: Option<i64>,
    pub teams: Option<i64>,
    pub no_shows: Option<i64>,
    pub poll_up: Option<i64>,
    pub poll_down: Option<i64>,
    pub poll_message_id: Option<String>,
    pub feedback_summary: Option<String>,
    pub collected_at: Option<String>,
}

/// DB-Zeile aus `tournament_signals`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SignalSnapshot {
    pub id: i64,
    pub tournament_id: i64,
    pub participants: Option<i64>,
    pub teams: Option<i64>,
    pub no_shows: Option<i64>,
    pub poll_up: Option<i64>,
    pub poll_down: Option<i64>,
    pub poll_message_id: Option<String>,
    pub feedback_summary: Option<String>,
    pub collected_at: String,
}

/// Schreibt einen Snapshot und gibt die gespeicherte Zeile zurueck.
pub async fn snapshot_signals(
    pool: &Pool,
    tournament_id: i64,
    input: &SignalSnapshotInput,
) -> AutomatikResult<SignalSnapshot> {
    let collected_at = input.collected_at.clone().unwrap_or_else(now_iso);
    let row = sqlx::query_as::<_, SignalSnapshot>(
        "INSERT INTO tournament_signals \
             (tournament_id, participants, teams, no_shows, poll_up, poll_down, \
              poll_message_id, feedback_summary, collected_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
    )
    .bind(tournament_id)
    .bind(input.participants)
    .bind(input.teams)
    .bind(input.no_shows)
    .bind(input.poll_up)
    .bind(input.poll_down)
    .bind(&input.poll_message_id)
    .bind(&input.feedback_summary)
    .bind(&collected_at)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Laedt einen Snapshot per ID.
pub async fn get_signal(pool: &Pool, signal_id: i64) -> AutomatikResult<Option<SignalSnapshot>> {
    let row = sqlx::query_as::<_, SignalSnapshot>("SELECT * FROM tournament_signals WHERE id = ?")
        .bind(signal_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// Listet alle Snapshots eines Turniers.
pub async fn list_for_tournament(
    pool: &Pool,
    tournament_id: i64,
) -> AutomatikResult<Vec<SignalSnapshot>> {
    let rows = sqlx::query_as::<_, SignalSnapshot>(
        "SELECT * FROM tournament_signals WHERE tournament_id = ? ORDER BY id",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false)
}
