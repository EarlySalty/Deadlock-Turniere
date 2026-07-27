use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScrimError {
    #[error("invalid block proposal: {0}")]
    InvalidProposal(String),
    #[error("invalid slot response: {0}")]
    InvalidResponse(String),
    #[error("invalid stored scrim data: {0}")]
    InvalidStoredData(String),
    #[error("actor is not an active coach")]
    CoachUnauthorized,
    #[error("participant does not belong to the requested team")]
    ParticipantUnauthorized,
    #[error("replacement request does not belong to the actor")]
    ReplacementRequestUnauthorized,
    #[error("actor Discord ID is invalid")]
    InvalidActor,
    #[error("scrim runtime is not writable by turniere: mode={mode}, writer={operational_writer}")]
    RuntimeNotWritable {
        mode: String,
        operational_writer: String,
    },
    #[error("idempotency key was already used with a different payload")]
    IdempotencyConflict,
    #[error("idempotent command is still processing")]
    CommandInProgress,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

pub type ScrimResult<T> = Result<T, ScrimError>;
