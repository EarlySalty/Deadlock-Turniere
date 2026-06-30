//! Fehler-Typ der Turnier-Automatik.

use thiserror::Error;

use crate::proposals::{ProposalEvent, ProposalState};

/// Fehler der Automatik-Bibliothek.
#[derive(Debug, Error)]
pub enum AutomatikError {
    /// Proposal-State-Machine: der angeforderte Uebergang ist fachlich verboten.
    #[error("ungueltiger Proposal-Uebergang von {state:?} via {event:?}")]
    InvalidTransition {
        /// Ausgangszustand.
        state: ProposalState,
        /// Angefordertes Ereignis.
        event: ProposalEvent,
    },

    /// Proposal-Approval ohne gespeicherte Caster-Freigabe.
    #[error("proposal approval missing for proposal {proposal_id}")]
    MissingApproval {
        /// Proposal-ID.
        proposal_id: i64,
    },

    /// Persistenz-Fehler (Pool/Query).
    #[error(transparent)]
    Db(#[from] turnier_db::DbError),
}

impl From<sqlx::Error> for AutomatikError {
    fn from(err: sqlx::Error) -> Self {
        AutomatikError::Db(turnier_db::DbError::from(err))
    }
}

/// Bequemer Result-Alias des Subsystems.
pub type AutomatikResult<T> = Result<T, AutomatikError>;
