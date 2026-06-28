//! Fehler-Typ der Broker-Schicht.
//!
//! Bewusst ENTKOPPELT von FastAPI/axum: das Python-Original warf in den
//! Helfern teils `HTTPException` (Konfig fehlt) und teils `RuntimeError`
//! (Transport/HTTP) — beides wird hier auf einen klaren Enum reduziert.

use thiserror::Error;

/// Fehler beim Aufruf des Discord-Master-Brokers.
#[derive(Debug, Error)]
pub enum BrokerError {
    /// Basis-URL oder Token sind nicht konfiguriert. Entspricht dem
    /// `HTTP_503`-Pfad im Python-Original (`_broker_base_url`/`_broker_headers`),
    /// nur ohne FastAPI-Kopplung.
    #[error("{0}")]
    Unconfigured(&'static str),

    /// Der Broker war nicht erreichbar (Transport-/Verbindungsfehler).
    /// Im Python-Original: `RuntimeError("Discord-Broker nicht erreichbar: …")`.
    #[error("Discord-Broker nicht erreichbar: {0}")]
    Unreachable(#[source] reqwest::Error),

    /// Der Broker antwortete mit einem Nicht-200-Status. `detail` ist exakt der
    /// im Original ermittelte Text (JSON-`error`/`detail`, sonst Response-Body,
    /// sonst Default).
    #[error("{detail}")]
    Http { status: u16, detail: String },

    /// Die Antwort war kein (Objekt-)JSON. Im Original zwei getrennte
    /// `RuntimeError`-Meldungen — hier ein Variant mit der jeweiligen Meldung.
    #[error("{0}")]
    BadJson(&'static str),
}

/// Bequemer Result-Alias der Broker-Schicht.
pub type BrokerResult<T> = Result<T, BrokerError>;

/// Ungültige (nicht numerische) Snowflake-ID — verursacht KEINE Panic, sondern
/// landet deterministisch in der `failed`-Bucket der jeweiligen Operation.
pub const INVALID_ID_ERROR: &str = "Ungültige Discord-ID";
