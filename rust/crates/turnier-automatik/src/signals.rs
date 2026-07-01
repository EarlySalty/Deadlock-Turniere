//! Signal-Snapshots fuer spaetere Auswertung.

use chrono::{DateTime, Utc};
use turnier_core::{discord_id_to_string, now_utc, parse_discord_id};
use turnier_db::Pool;

use crate::error::{AutomatikError, AutomatikResult};

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

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct SignalSnapshotRow {
    pub id: i64,
    pub tournament_id: i64,
    pub participants: Option<i64>,
    pub teams: Option<i64>,
    pub no_shows: Option<i64>,
    pub poll_up: Option<i64>,
    pub poll_down: Option<i64>,
    pub poll_message_id: Option<i64>,
    pub feedback_summary: Option<String>,
    pub collected_at: DateTime<Utc>,
}

impl From<SignalSnapshotRow> for SignalSnapshot {
    fn from(row: SignalSnapshotRow) -> Self {
        Self {
            id: row.id,
            tournament_id: row.tournament_id,
            participants: row.participants,
            teams: row.teams,
            no_shows: row.no_shows,
            poll_up: row.poll_up,
            poll_down: row.poll_down,
            poll_message_id: row.poll_message_id.map(discord_id_to_string),
            feedback_summary: row.feedback_summary,
            collected_at: row.collected_at.to_rfc3339(),
        }
    }
}

/// Schreibt einen Snapshot und gibt die gespeicherte Zeile zurueck.
pub async fn snapshot_signals(
    pool: &Pool,
    tournament_id: i64,
    input: &SignalSnapshotInput,
) -> AutomatikResult<SignalSnapshot> {
    let collected_at = match input.collected_at.as_deref() {
        Some(value) => DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc),
        None => now_utc(),
    };
    let poll_message_id = input
        .poll_message_id
        .as_deref()
        .map(parse_numeric_id)
        .transpose()?;
    let row = sqlx::query_as::<_, SignalSnapshotRow>(
        "INSERT INTO turnier.tournament_signals \
             (tournament_id, participants, teams, no_shows, poll_up, poll_down, \
              poll_message_id, feedback_summary, collected_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING *",
    )
    .bind(tournament_id)
    .bind(input.participants)
    .bind(input.teams)
    .bind(input.no_shows)
    .bind(input.poll_up)
    .bind(input.poll_down)
    .bind(poll_message_id)
    .bind(&input.feedback_summary)
    .bind(collected_at)
    .fetch_one(pool)
    .await?;
    Ok(row.into())
}

/// Laedt einen Snapshot per ID.
pub async fn get_signal(pool: &Pool, signal_id: i64) -> AutomatikResult<Option<SignalSnapshot>> {
    let row = sqlx::query_as::<_, SignalSnapshotRow>(
        "SELECT * FROM turnier.tournament_signals WHERE id = $1",
    )
    .bind(signal_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// Listet alle Snapshots eines Turniers.
pub async fn list_for_tournament(
    pool: &Pool,
    tournament_id: i64,
) -> AutomatikResult<Vec<SignalSnapshot>> {
    let rows = sqlx::query_as::<_, SignalSnapshotRow>(
        "SELECT * FROM turnier.tournament_signals WHERE tournament_id = $1 ORDER BY id",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

fn parse_numeric_id(value: &str) -> AutomatikResult<i64> {
    parse_discord_id(value).map_err(|_| AutomatikError::InvalidNumericId(value.to_string()))
}
