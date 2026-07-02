//! Fehler-Typ des Rang-Resolvers.
//!
//! Im Python-Original verschluckte ein flächendeckendes `except Exception:
//! return None` jede Ursache (DB gesperrt, Pfad falsch, Schema-Mismatch) — ein
//! echter Fehler war nicht von „kein Rang" zu unterscheiden. Hier sind die
//! Ursachen typisiert; die einzelnen Pfade loggen Fehler per `tracing::warn` und
//! behandeln nur das echte „nicht gefunden" als `None`.

use thiserror::Error;
use turnier_core::DiscordIdParseError;

/// Fehler bei der Rang-Auflösung.
#[derive(Debug, Error)]
pub enum SteamError {
    /// DB-Zugriff (App-DB oder read-only Bridge-DB) fehlgeschlagen.
    #[error(transparent)]
    Db(#[from] sqlx::Error),

    /// Discord-ID konnte nicht in den zentralen BIGINT-DB-Typ konvertiert werden.
    #[error("Ungueltige Discord-ID: {0}")]
    InvalidDiscordId(#[from] DiscordIdParseError),

    /// HTTP-Aufruf an die Discord-REST-API fehlgeschlagen.
    #[error("Discord-REST-Aufruf fehlgeschlagen: {0}")]
    DiscordHttp(String),
}

/// Bequemer Result-Alias des Subsystems.
pub type SteamResult<T> = Result<T, SteamError>;
