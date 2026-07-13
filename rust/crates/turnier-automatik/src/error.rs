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

    /// Revision oder Ablehnung ohne verwertbaren menschlichen Grund.
    #[error("feedback darf nicht leer sein")]
    MissingFeedback,

    /// Fuer denselben Vorschlag wird bereits eine neue Version vorbereitet.
    #[error("fuer diesen Vorschlag wird bereits eine Revision vorbereitet")]
    RevisionInProgress,

    /// Der vorbereitete Entwurf gehoert zu einem anderen Elternvorschlag.
    #[error("revisionsentwurf gehoert nicht zum angegebenen vorschlag")]
    RevisionParentMismatch,

    /// Ein sichtbarer oder bereits bewerteter Plan darf nicht still ersetzt werden.
    #[error("der Vorschlagsplan ist bereits gesperrt")]
    PlanLocked,

    /// Strukturierter Vorschlagsplan ist kein JSON-Objekt.
    #[error("vorschlagsplan muss ein JSON-Objekt sein")]
    InvalidProposalConfig,

    /// Ungueltige Discord-/Message-ID fuer eine BIGINT-Spalte.
    #[error("ungueltige numerische ID: {0}")]
    InvalidNumericId(String),

    /// Ungueltiges JSON fuer eine JSONB-Spalte.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// Ungueltiger Zeitstempel fuer eine TIMESTAMPTZ-Spalte.
    #[error(transparent)]
    Time(#[from] chrono::ParseError),

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
