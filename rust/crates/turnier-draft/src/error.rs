//! Fehler-Typ des Draft-Subsystems.
//!
//! Im Python-Original warf die Engine durchweg `ValueError` (unbekannter Held,
//! Doppel-Pick, fehlende/abgeschlossene Session), das die Route in HTTP 400/404
//! übersetzte. Hier sind die fachlichen Fälle typisiert, damit turnier-api sie gezielt
//! auf Status-Codes abbilden kann; diese Crate hängt selbst nicht von axum ab.

use thiserror::Error;

/// Fehler der Draft-Engine.
#[derive(Debug, Error)]
pub enum DraftError {
    /// Held ist nicht in der Stammdaten-Liste (Original: `ValueError("Unbekannter
    /// Held: …")`). turnier-api → HTTP 400.
    #[error("Unbekannter Held: {0}")]
    UnknownHero(String),

    /// Session existiert nicht (Original: `ValueError("Session nicht gefunden")`).
    /// turnier-api → HTTP 404.
    #[error("Session nicht gefunden")]
    SessionNotFound,

    /// Session existiert nicht oder ist nicht mehr `in_progress` (Original:
    /// `ValueError("Draft-Session nicht gefunden oder bereits abgeschlossen")`).
    /// turnier-api → HTTP 400.
    #[error("Draft-Session nicht gefunden oder bereits abgeschlossen")]
    SessionNotActive,

    /// Der Held wurde in dieser Session bereits gebannt oder gepickt (Original:
    /// `ValueError("{hero} wurde bereits gebannt oder gepickt")`). turnier-api → HTTP 400.
    #[error("{0} wurde bereits gebannt oder gepickt")]
    HeroAlreadyTaken(String),

    /// Optimistic-Concurrency: `current_action_index` wurde zwischen Lesen und
    /// Schreiben parallel verändert (Compare-and-Swap betraf 0 Zeilen). Im
    /// Python-Original gab es diesen Schutz nicht (Race, last-write-wins); der
    /// Rust-Port macht den Konflikt explizit. turnier-api → HTTP 409.
    #[error("Draft-Aktion kollidierte mit einer parallelen Aktion")]
    ActionConflict,

    /// Ungueltige Discord-ID fuer eine BIGINT-Spalte.
    #[error("ungueltige Discord-ID: {0}")]
    InvalidDiscordId(String),

    /// Persistenz-Fehler (Pool/Query).
    #[error(transparent)]
    Db(#[from] turnier_db::DbError),
}

/// Direkte Konvertierung aus `sqlx::Error`, damit `?` an Query-Aufrufen ohne
/// manuelles Mapping funktioniert — über [`turnier_db::DbError`], damit es genau eine
/// Persistenz-Fehler-Repräsentation gibt.
impl From<sqlx::Error> for DraftError {
    fn from(err: sqlx::Error) -> Self {
        DraftError::Db(turnier_db::DbError::from(err))
    }
}

/// Bequemer Result-Alias des Subsystems.
pub type DraftResult<T> = Result<T, DraftError>;
