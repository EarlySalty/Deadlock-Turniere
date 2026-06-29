//! Proposal-Persistenz und reine Proposal-State-Machine.

use serde::{Deserialize, Serialize};
use turnier_db::Pool;

use crate::error::{AutomatikError, AutomatikResult};

/// Herkunft eines Vorschlags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum ProposalSource {
    /// Vom Automatik-Loop erzeugt.
    Bot,
    /// Manuell im Admin-Kontext erzeugt.
    Manual,
}

/// Persistierter Proposal-Zustand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum ProposalState {
    Draft,
    PendingApproval,
    Approved,
    Rejected,
    Expired,
}

/// Ereignisse der reinen State-Machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProposalEvent {
    SubmitForApproval,
    Approve,
    Reject,
    Expire,
    Feedback,
}

/// Vote-Entscheidung eines Casters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum VoteDecision {
    Approve,
    Reject,
}

/// DB-Zeile aus `tournament_proposals`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Proposal {
    pub id: i64,
    pub preset_id: Option<i64>,
    pub source: ProposalSource,
    pub proposed_start: Option<String>,
    pub config_json: String,
    pub state: ProposalState,
    pub proposal_message_id: Option<String>,
    pub channel_id: Option<String>,
    pub tournament_id: Option<i64>,
    pub decided_at: Option<String>,
    pub created_at: String,
}

/// DB-Zeile aus `tournament_proposal_votes`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ProposalVote {
    pub id: i64,
    pub proposal_id: i64,
    pub caster_discord_id: String,
    pub decision: VoteDecision,
    pub created_at: String,
}

/// DB-Zeile aus `tournament_proposal_feedback`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ProposalFeedback {
    pub id: i64,
    pub proposal_id: i64,
    pub caster_discord_id: String,
    pub raw_text: String,
    pub applied_change_json: Option<String>,
    pub created_at: String,
}

/// Reine Proposal-State-Machine ohne DB/Discord.
pub fn transition(state: ProposalState, event: ProposalEvent) -> AutomatikResult<ProposalState> {
    let next = match (state, event) {
        (ProposalState::Draft, ProposalEvent::SubmitForApproval) => ProposalState::PendingApproval,
        (ProposalState::PendingApproval, ProposalEvent::Approve) => ProposalState::Approved,
        (ProposalState::PendingApproval, ProposalEvent::Reject) => ProposalState::Rejected,
        (ProposalState::PendingApproval, ProposalEvent::Expire) => ProposalState::Expired,
        (ProposalState::PendingApproval, ProposalEvent::Feedback) => ProposalState::Draft,
        _ => return Err(AutomatikError::InvalidTransition { state, event }),
    };
    Ok(next)
}

/// Legt einen Vorschlag im Zustand `draft` an und liefert die ID.
pub async fn create_proposal(
    pool: &Pool,
    preset_id: Option<i64>,
    source: ProposalSource,
    proposed_start: Option<&str>,
    config_json: &str,
) -> AutomatikResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO tournament_proposals (preset_id, source, proposed_start, config_json) \
         VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(preset_id)
    .bind(source)
    .bind(proposed_start)
    .bind(config_json)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Laedt einen Vorschlag per ID.
pub async fn get_proposal(pool: &Pool, proposal_id: i64) -> AutomatikResult<Option<Proposal>> {
    let row = sqlx::query_as::<_, Proposal>("SELECT * FROM tournament_proposals WHERE id = ?")
        .bind(proposal_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// Listet Vorschlaege, optional nach Zustand gefiltert.
pub async fn list_proposals(
    pool: &Pool,
    state: Option<ProposalState>,
) -> AutomatikResult<Vec<Proposal>> {
    let rows = if let Some(state) = state {
        sqlx::query_as::<_, Proposal>(
            "SELECT * FROM tournament_proposals WHERE state = ? ORDER BY id DESC",
        )
        .bind(state)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, Proposal>("SELECT * FROM tournament_proposals ORDER BY id DESC")
            .fetch_all(pool)
            .await?
    };
    Ok(rows)
}

/// Speichert/aktualisiert den Vote eines Casters. Pro Caster und Proposal gibt
/// es wegen UNIQUE hoechstens eine Zeile; erneuter Vote ueberschreibt die
/// Entscheidung.
pub async fn record_vote(
    pool: &Pool,
    proposal_id: i64,
    caster_id: &str,
    decision: VoteDecision,
) -> AutomatikResult<()> {
    sqlx::query(
        "INSERT INTO tournament_proposal_votes \
             (proposal_id, caster_discord_id, decision) \
         VALUES (?, ?, ?) \
         ON CONFLICT(proposal_id, caster_discord_id) DO UPDATE SET \
             decision = excluded.decision, created_at = datetime('now')",
    )
    .bind(proposal_id)
    .bind(caster_id)
    .bind(decision)
    .execute(pool)
    .await?;
    Ok(())
}

/// Speichert Freitext-Feedback eines Casters.
pub async fn record_feedback(
    pool: &Pool,
    proposal_id: i64,
    caster_id: &str,
    raw_text: &str,
    applied_change_json: Option<&str>,
) -> AutomatikResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO tournament_proposal_feedback \
             (proposal_id, caster_discord_id, raw_text, applied_change_json) \
         VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(proposal_id)
    .bind(caster_id)
    .bind(raw_text)
    .bind(applied_change_json)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Zaehlt aktuelle Approve-Votes eines Vorschlags.
pub async fn approvals_count(pool: &Pool, proposal_id: i64) -> AutomatikResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tournament_proposal_votes \
         WHERE proposal_id = ? AND decision = 'approve'",
    )
    .bind(proposal_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Wendet ein Proposal-Event validiert auf den aktuellen DB-Zustand an.
pub async fn apply_event(
    pool: &Pool,
    proposal_id: i64,
    event: ProposalEvent,
) -> AutomatikResult<ProposalState> {
    let current: (ProposalState,) =
        sqlx::query_as("SELECT state FROM tournament_proposals WHERE id = ?")
            .bind(proposal_id)
            .fetch_one(pool)
            .await?;
    let next = transition(current.0, event)?;
    set_state(pool, proposal_id, current.0, next).await?;
    Ok(next)
}

/// Setzt den persistierten Proposal-Zustand nach vorheriger State-Machine-Pruefung.
/// Terminale Zustaende bekommen `decided_at`, nicht-terminale Zustaende leeren ihn.
async fn set_state(
    pool: &Pool,
    proposal_id: i64,
    current_state: ProposalState,
    next_state: ProposalState,
) -> AutomatikResult<()> {
    let terminal = matches!(
        next_state,
        ProposalState::Approved | ProposalState::Rejected | ProposalState::Expired
    );
    let res = sqlx::query(
        "UPDATE tournament_proposals SET \
             state = ?, decided_at = CASE WHEN ? THEN datetime('now') ELSE NULL END \
         WHERE id = ? AND state = ?",
    )
    .bind(next_state)
    .bind(terminal)
    .bind(proposal_id)
    .bind(current_state)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound.into());
    }
    Ok(())
}

/// Listet Votes eines Vorschlags.
pub async fn list_votes(pool: &Pool, proposal_id: i64) -> AutomatikResult<Vec<ProposalVote>> {
    let rows = sqlx::query_as::<_, ProposalVote>(
        "SELECT * FROM tournament_proposal_votes \
         WHERE proposal_id = ? ORDER BY id",
    )
    .bind(proposal_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Listet Feedback eines Vorschlags.
pub async fn list_feedback(
    pool: &Pool,
    proposal_id: i64,
) -> AutomatikResult<Vec<ProposalFeedback>> {
    let rows = sqlx::query_as::<_, ProposalFeedback>(
        "SELECT * FROM tournament_proposal_feedback \
         WHERE proposal_id = ? ORDER BY id",
    )
    .bind(proposal_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
