//! Fehler-Typ der Turnier-Engine.
//!
//! Die Varianten bilden die fachlichen Fehlersituationen des Python-Originals ab
//! (dort `ValueError`, `RuntimeError`, `CheckinSnapshotMismatchError`). turnier-api
//! übersetzt sie später in HTTP-Status; diese Crate hängt selbst nicht von axum
//! ab.

use thiserror::Error;

/// Fehler der Turnier-Engine.
#[derive(Debug, Error)]
pub enum TournamentError {
    /// Fachliche Vorbedingung verletzt (entspricht `ValueError` im Original):
    /// z. B. „Turnier nicht gefunden", „Mindestens 2 Teams benötigt",
    /// „Check-in kann nur in der Check-in-Phase abgeschlossen werden".
    #[error("{0}")]
    Validation(String),

    /// Der Confirm-Snapshot weicht vom Dry-Run ab (parallele Datenänderung).
    /// Entspricht `CheckinSnapshotMismatchError`.
    #[error(
        "Check-in-Daten haben sich seit der Vorschau geändert. Bitte Dry-Run erneut ausführen."
    )]
    SnapshotMismatch,

    /// Optimistic-Concurrency: der Turnierstatus wurde parallel geändert
    /// (entspricht dem `RuntimeError` bei `rowcount == 0`).
    #[error("Turnierstatus wurde parallel geändert")]
    StatusConflict,

    /// Rang-Resolver-Fehler (Steam-/Discord-Lookup) beim Solo-Team-Bilden.
    #[cfg(feature = "persist")]
    #[error(transparent)]
    Steam(#[from] turnier_steam::SteamError),

    /// Persistenz-Fehler (Pool/Query/Migration).
    #[cfg(feature = "persist")]
    #[error(transparent)]
    Db(#[from] turnier_db::DbError),
}

/// Direkte Konvertierung aus `sqlx::Error`, damit `?` an Query-Aufrufen ohne
/// manuelles Mapping funktioniert — über [`turnier_db::DbError`], damit es genau eine
/// Persistenz-Fehler-Repräsentation gibt.
#[cfg(feature = "persist")]
impl From<sqlx::Error> for TournamentError {
    fn from(err: sqlx::Error) -> Self {
        TournamentError::Db(turnier_db::DbError::from(err))
    }
}

impl TournamentError {
    /// Kurzform für einen Validierungsfehler aus einem statischen Text.
    pub fn validation(msg: impl Into<String>) -> Self {
        TournamentError::Validation(msg.into())
    }
}

/// Bequemer Result-Alias der Turnier-Engine.
pub type TournamentResult<T> = Result<T, TournamentError>;
