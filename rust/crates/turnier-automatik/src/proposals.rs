//! Proposal-Persistenz und reine Proposal-State-Machine.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use turnier_core::{discord_id_to_string, json::jsonb_to_wire_string, now_utc, parse_discord_id};
use turnier_db::Pool;

use crate::error::{AutomatikError, AutomatikResult};

pub const REQUIRED_APPROVALS: i64 = 2;

/// Herkunft eines Vorschlags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum ProposalSource {
    /// Vom Automatik-Loop erzeugt.
    Bot,
    /// Manuell im Admin-Kontext erzeugt.
    Manual,
}

/// Persistierter Proposal-Zustand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
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
#[sqlx(type_name = "text", rename_all = "snake_case")]
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

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct ProposalRow {
    pub id: i64,
    pub preset_id: Option<i64>,
    pub source: ProposalSource,
    pub proposed_start: Option<DateTime<Utc>>,
    pub config_json: Value,
    pub state: ProposalState,
    pub proposal_message_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub tournament_id: Option<i64>,
    pub decided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<ProposalRow> for Proposal {
    fn from(row: ProposalRow) -> Self {
        Self {
            id: row.id,
            preset_id: row.preset_id,
            source: row.source,
            proposed_start: row.proposed_start.map(|value| value.to_rfc3339()),
            config_json: row.config_json.to_string(),
            state: row.state,
            proposal_message_id: row.proposal_message_id.map(discord_id_to_string),
            channel_id: row.channel_id.map(discord_id_to_string),
            tournament_id: row.tournament_id,
            decided_at: row.decided_at.map(|value| value.to_rfc3339()),
            created_at: row.created_at.to_rfc3339(),
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct ProposalVoteRow {
    pub id: i64,
    pub proposal_id: i64,
    pub caster_discord_id: i64,
    pub decision: VoteDecision,
    pub created_at: DateTime<Utc>,
}

impl From<ProposalVoteRow> for ProposalVote {
    fn from(row: ProposalVoteRow) -> Self {
        Self {
            id: row.id,
            proposal_id: row.proposal_id,
            caster_discord_id: discord_id_to_string(row.caster_discord_id),
            decision: row.decision,
            created_at: row.created_at.to_rfc3339(),
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct ProposalFeedbackRow {
    pub id: i64,
    pub proposal_id: i64,
    pub caster_discord_id: i64,
    pub raw_text: String,
    pub applied_change_json: Option<Value>,
    pub created_at: DateTime<Utc>,
}

impl From<ProposalFeedbackRow> for ProposalFeedback {
    fn from(row: ProposalFeedbackRow) -> Self {
        Self {
            id: row.id,
            proposal_id: row.proposal_id,
            caster_discord_id: discord_id_to_string(row.caster_discord_id),
            raw_text: row.raw_text,
            applied_change_json: jsonb_to_wire_string(row.applied_change_json),
            created_at: row.created_at.to_rfc3339(),
        }
    }
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
    let proposed_start = parse_optional_utc(proposed_start)?;
    let config_json = serde_json::from_str::<Value>(config_json)?;
    let now = now_utc();
    let id = sqlx::query_scalar(
        "INSERT INTO turnier.tournament_proposals \
             (preset_id, source, proposed_start, config_json, state, created_at) \
         VALUES ($1, $2, $3, $4, 'draft', $5) RETURNING id",
    )
    .bind(preset_id)
    .bind(source)
    .bind(proposed_start)
    .bind(config_json)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Ersetzt einen offenen Vorschlag atomar durch eine neue, unbestimmte Version.
/// Votes bleiben am alten Proposal und zählen deshalb nicht weiter.
pub async fn create_revision(
    pool: &Pool,
    proposal_id: i64,
    caster_id: &str,
    feedback: &str,
    config_json: &str,
) -> AutomatikResult<i64> {
    let caster_id = parse_numeric_id(caster_id)?;
    let feedback = feedback.trim();
    if feedback.is_empty() {
        return Err(AutomatikError::MissingFeedback);
    }
    let config_json = serde_json::from_str::<Value>(config_json)?;
    let now = now_utc();
    let mut tx = pool.begin().await?;
    let parent = sqlx::query_as::<_, ProposalRow>(
        "SELECT * FROM turnier.tournament_proposals WHERE id = $1 FOR UPDATE",
    )
    .bind(proposal_id)
    .fetch_one(&mut *tx)
    .await?;
    if parent.state != ProposalState::PendingApproval {
        return Err(AutomatikError::InvalidTransition {
            state: parent.state,
            event: ProposalEvent::Feedback,
        });
    }

    sqlx::query(
        "INSERT INTO turnier.tournament_proposal_feedback \
             (proposal_id, caster_discord_id, raw_text, applied_change_json, created_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(proposal_id)
    .bind(caster_id)
    .bind(feedback)
    .bind(&config_json)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE turnier.tournament_proposals SET state = 'expired', decided_at = $1 \
         WHERE id = $2 AND state = 'pending_approval'",
    )
    .bind(now)
    .bind(proposal_id)
    .execute(&mut *tx)
    .await?;
    let revised_id = sqlx::query_scalar(
        "INSERT INTO turnier.tournament_proposals \
             (preset_id, source, proposed_start, config_json, state, created_at) \
         VALUES ($1, $2, $3, $4, 'pending_approval', $5) RETURNING id",
    )
    .bind(parent.preset_id)
    .bind(parent.source)
    .bind(parent.proposed_start)
    .bind(config_json)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(revised_id)
}

/// Laedt einen Vorschlag per ID.
pub async fn get_proposal(pool: &Pool, proposal_id: i64) -> AutomatikResult<Option<Proposal>> {
    let row = sqlx::query_as::<_, ProposalRow>(
        "SELECT * FROM turnier.tournament_proposals WHERE id = $1",
    )
    .bind(proposal_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// Listet Vorschlaege, optional nach Zustand gefiltert.
pub async fn list_proposals(
    pool: &Pool,
    state: Option<ProposalState>,
) -> AutomatikResult<Vec<Proposal>> {
    let rows = if let Some(state) = state {
        sqlx::query_as::<_, ProposalRow>(
            "SELECT * FROM turnier.tournament_proposals WHERE state = $1 ORDER BY id DESC",
        )
        .bind(state)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, ProposalRow>(
            "SELECT * FROM turnier.tournament_proposals ORDER BY id DESC",
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows.into_iter().map(Into::into).collect())
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
    let caster_id = parse_numeric_id(caster_id)?;
    let now = now_utc();
    sqlx::query(
        "INSERT INTO turnier.tournament_proposal_votes \
             (proposal_id, caster_discord_id, decision, created_at) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (proposal_id, caster_discord_id) DO UPDATE SET \
             decision = EXCLUDED.decision, created_at = EXCLUDED.created_at",
    )
    .bind(proposal_id)
    .bind(caster_id)
    .bind(decision)
    .bind(now)
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
    let caster_id = parse_numeric_id(caster_id)?;
    let applied_change_json = applied_change_json
        .map(serde_json::from_str::<Value>)
        .transpose()?;
    let now = now_utc();
    let id = sqlx::query_scalar(
        "INSERT INTO turnier.tournament_proposal_feedback \
             (proposal_id, caster_discord_id, raw_text, applied_change_json, created_at) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(proposal_id)
    .bind(caster_id)
    .bind(raw_text)
    .bind(applied_change_json)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Zaehlt aktuelle Approve-Votes eines Vorschlags.
pub async fn approvals_count(pool: &Pool, proposal_id: i64) -> AutomatikResult<i64> {
    let count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.tournament_proposal_votes \
         WHERE proposal_id = $1 AND decision = 'approve'",
    )
    .bind(proposal_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// Wendet ein Proposal-Event validiert auf den aktuellen DB-Zustand an.
pub async fn apply_event(
    pool: &Pool,
    proposal_id: i64,
    event: ProposalEvent,
) -> AutomatikResult<ProposalState> {
    let current: (ProposalState,) =
        sqlx::query_as("SELECT state FROM turnier.tournament_proposals WHERE id = $1")
            .bind(proposal_id)
            .fetch_one(pool)
            .await?;
    let next = transition(current.0, event)?;
    if matches!(next, ProposalState::Approved)
        && approvals_count(pool, proposal_id).await? < REQUIRED_APPROVALS
    {
        return Err(AutomatikError::MissingApproval { proposal_id });
    }
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
    let decided_at = terminal.then(now_utc);
    let res = sqlx::query(
        "UPDATE turnier.tournament_proposals SET state = $1, decided_at = $2 \
         WHERE id = $3 AND state = $4",
    )
    .bind(next_state)
    .bind(decided_at)
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
    let rows = sqlx::query_as::<_, ProposalVoteRow>(
        "SELECT * FROM turnier.tournament_proposal_votes \
         WHERE proposal_id = $1 ORDER BY id",
    )
    .bind(proposal_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Listet Feedback eines Vorschlags.
pub async fn list_feedback(
    pool: &Pool,
    proposal_id: i64,
) -> AutomatikResult<Vec<ProposalFeedback>> {
    let rows = sqlx::query_as::<_, ProposalFeedbackRow>(
        "SELECT * FROM turnier.tournament_proposal_feedback \
         WHERE proposal_id = $1 ORDER BY id",
    )
    .bind(proposal_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

fn parse_numeric_id(value: &str) -> AutomatikResult<i64> {
    parse_discord_id(value).map_err(|_| AutomatikError::InvalidNumericId(value.to_string()))
}

fn parse_optional_utc(value: Option<&str>) -> AutomatikResult<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value).map(|timestamp| timestamp.with_timezone(&Utc))
        })
        .transpose()
        .map_err(Into::into)
}
