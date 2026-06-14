//! Fehler-Typen des Match-Subsystems.
//!
//! Das Python-Original führt zwei getrennte Familien:
//! - [`MatchResultError`] (`RuntimeError`) mit den Unterfällen `MatchNotFoundError`
//!   und `MatchStateError` — fachliche Ergebnis-/Statusfehler.
//! - [`SteamTaskError`] (`RuntimeError`, separat) — ungültige/unvollständige
//!   GC-Ergebnisdaten oder fehlgeschlagene Steam-Tasks.
//!
//! Hier 1:1 als zwei thiserror-Enums abgebildet. Persistenz- und Discord-Fehler
//! werden über `From` eingebettet, damit `?` an Query-/Broker-Aufrufen ohne
//! manuelles Mapping funktioniert.

use thiserror::Error;

/// Fachlicher Fehler bei der Ergebnisverarbeitung oder Statusprüfung eines Matches.
///
/// Entspricht `match.result_processor.MatchResultError` samt der beiden
/// Spezialisierungen `MatchNotFoundError`/`MatchStateError`. Die Unterscheidung
/// `NotFound`/`State`/`Invalid` ersetzt die Python-Subklassen; tb-web übersetzt
/// sie später in HTTP-Status.
#[derive(Debug, Error)]
pub enum MatchError {
    /// Das betroffene Match (oder Turnier) existiert nicht
    /// (`MatchNotFoundError`).
    #[error("{0}")]
    NotFound(String),

    /// Das Match befindet sich nicht in einem erlaubten Status
    /// (`MatchStateError`).
    #[error("{0}")]
    State(String),

    /// Ein übergebenes Ergebnis ist fachlich ungültig (`MatchResultError`
    /// direkt, z. B. „winner_id gehört nicht zum Match").
    #[error("{0}")]
    Invalid(String),

    /// Persistenz-Fehler (Pool/Query) auf der Haupt-DB.
    #[error(transparent)]
    Db(#[from] tb_db::DbError),

    /// Fehler aus der Turnier-Engine (Propagation, Mini-Group-Abschluss).
    #[error(transparent)]
    Tournament(#[from] tb_tournament::TournamentError),
}

impl MatchError {
    /// Kurzform für [`MatchError::NotFound`].
    pub fn not_found(msg: impl Into<String>) -> Self {
        MatchError::NotFound(msg.into())
    }

    /// Kurzform für [`MatchError::State`].
    pub fn state(msg: impl Into<String>) -> Self {
        MatchError::State(msg.into())
    }

    /// Kurzform für [`MatchError::Invalid`].
    pub fn invalid(msg: impl Into<String>) -> Self {
        MatchError::Invalid(msg.into())
    }
}

impl From<sqlx::Error> for MatchError {
    fn from(err: sqlx::Error) -> Self {
        MatchError::Db(tb_db::DbError::from(err))
    }
}

/// Bequemer Result-Alias für die Ergebnis-/Status-Pfade.
pub type MatchResult<T> = Result<T, MatchError>;

/// Fehler bei einem Steam-/GC-Task oder bei ungültigen GC-Ergebnisdaten.
///
/// Entspricht `match.result_processor.SteamTaskError`. Separat von [`MatchError`],
/// weil der Lobby-/Start-/Result-Fetch-Pfad im Original eigene Fehlersemantik hat
/// (Timeout schlägt durch, fehlgeschlagener Task wird übersetzt).
#[derive(Debug, Error)]
pub enum SteamTaskError {
    /// Der Steam-Task ist innerhalb des Timeouts nicht fertig geworden
    /// (entspricht dem durchgereichten `TimeoutError`).
    #[error("Steam-Task {task_id} hat innerhalb von {timeout_s}s nicht geantwortet")]
    Timeout { task_id: i64, timeout_s: f64 },

    /// Der Task wurde mit Fehlerstatus abgeschlossen oder die Aktion ist
    /// fehlgeschlagen (`"{action} fehlgeschlagen: …"`).
    #[error("{0}")]
    Failed(String),

    /// Eine fachliche Statusverletzung auf dem Lobby-/Start-Pfad
    /// (`MatchStateError` aus dem Manager, dort RuntimeError-kompatibel).
    #[error("{0}")]
    State(String),

    /// Das Match (oder Turnier) wurde nicht gefunden (`MatchNotFoundError`).
    #[error("{0}")]
    NotFound(String),

    /// Persistenz-Fehler auf der Haupt-DB.
    #[error(transparent)]
    Db(#[from] tb_db::DbError),

    /// Persistenz-/Konfigurationsfehler auf der externen Steam-Bridge-DB.
    #[error(transparent)]
    Bridge(#[from] crate::steam_bridge::BridgeError),

    /// Ein bei der Ergebnis-Übernahme entstandener [`MatchError`] — im Original
    /// fängt `_fetch_match_result_for_match` `MatchResultError` und verpackt ihn
    /// als `SteamTaskError(f"Ungueltige Steam-Ergebnisdaten: {exc}")`.
    #[error("Ungueltige Steam-Ergebnisdaten: {0}")]
    InvalidResult(String),
}

impl SteamTaskError {
    /// Kurzform für [`SteamTaskError::Failed`].
    pub fn failed(msg: impl Into<String>) -> Self {
        SteamTaskError::Failed(msg.into())
    }

    /// Kurzform für [`SteamTaskError::State`].
    pub fn state(msg: impl Into<String>) -> Self {
        SteamTaskError::State(msg.into())
    }

    /// Kurzform für [`SteamTaskError::NotFound`].
    pub fn not_found(msg: impl Into<String>) -> Self {
        SteamTaskError::NotFound(msg.into())
    }
}

impl From<sqlx::Error> for SteamTaskError {
    fn from(err: sqlx::Error) -> Self {
        SteamTaskError::Db(tb_db::DbError::from(err))
    }
}

/// `MatchError` → `SteamTaskError`: bildet `_fetch_match_result_for_match`
/// nach, das `MatchResultError` zu `SteamTaskError("Ungueltige Steam-Ergebnisdaten: …")`
/// wandelt. NotFound/State/Invalid landen alle in [`SteamTaskError::InvalidResult`]
/// — exakt wie das Original `except MatchResultError as exc` jede der drei
/// Subklassen fängt. DB-/Tournament-Fehler werden hingegen durchgereicht
/// (im Original schlugen diese ebenfalls nicht über den `except`-Zweig durch).
impl From<MatchError> for SteamTaskError {
    fn from(err: MatchError) -> Self {
        match err {
            MatchError::NotFound(m) | MatchError::State(m) | MatchError::Invalid(m) => {
                SteamTaskError::InvalidResult(m)
            }
            MatchError::Db(e) => SteamTaskError::Db(e),
            MatchError::Tournament(e) => {
                SteamTaskError::Failed(format!("Turnier-Engine-Fehler: {e}"))
            }
        }
    }
}

/// Bequemer Result-Alias für die Steam-/Lobby-Pfade.
pub type SteamTaskResult<T> = Result<T, SteamTaskError>;
