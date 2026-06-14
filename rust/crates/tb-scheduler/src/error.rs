//! Fehler-Typ des Schedulers.
//!
//! [`advance_tournament_status`](crate::advance_tournament_status) bildet die
//! beiden fachlichen Fehlersituationen des Python-Originals ab: ungültiger
//! Status-Übergang (`ValueError`) und paralleler Statuswechsel (`RuntimeError`
//! bei `rowcount == 0`). Alles andere kommt aus den Unter-Crates durch.

use thiserror::Error;

/// Fehler der Scheduler-Orchestrierung.
#[derive(Debug, Error)]
pub enum SchedulerError {
    /// Ungültiger Status-Übergang. Entspricht dem `ValueError` im Original
    /// (`advance_tournament_status`, Z.140-144): die erlaubten Folge-Status
    /// werden im Text mitgeliefert.
    #[error("{0}")]
    InvalidTransition(String),

    /// Optimistic-Concurrency: der Turnierstatus wurde zwischen Lesen und
    /// Schreiben parallel geändert (`UPDATE ... WHERE status = current` traf
    /// keine Zeile). Entspricht dem `RuntimeError` im Original (Z.169).
    #[error("Turnierstatus wurde parallel geändert")]
    StatusConflict,

    /// Fehler aus der Turnier-Engine (Generierung Gruppen/Matches/Bracket,
    /// Punkte-Neuberechnung).
    #[error(transparent)]
    Tournament(#[from] tb_tournament::TournamentError),

    /// Persistenz-Fehler (Pool/Query).
    #[error(transparent)]
    Db(#[from] tb_db::DbError),
}

/// Direkte Konvertierung aus `sqlx::Error`, damit `?` an Query-Aufrufen ohne
/// manuelles Mapping funktioniert — über [`tb_db::DbError`], damit es genau eine
/// Persistenz-Fehler-Repräsentation gibt.
impl From<sqlx::Error> for SchedulerError {
    fn from(err: sqlx::Error) -> Self {
        SchedulerError::Db(tb_db::DbError::from(err))
    }
}

/// Bequemer Result-Alias des Schedulers.
pub type SchedulerResult<T> = Result<T, SchedulerError>;
