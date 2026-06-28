//! Fehler-Typ der Auth-Schicht.
//!
//! Die Varianten bilden exakt die HTTP-Semantik des Python-Originals ab
//! (FastAPI-`HTTPException`-Statuscodes), damit turnier-api sie 1:1 in Responses
//! übersetzen kann — ohne dass diese Crate selbst von axum abhängt.

use thiserror::Error;

/// Auth-/RBAC-/Broker-Fehler mit ihrem zugehörigen HTTP-Status.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Kein/ungültiges/abgelaufenes Session-Token (HTTP 401).
    #[error("{0}")]
    Unauthorized(&'static str),

    /// Eingeloggt, aber Berechtigung fehlt (HTTP 403).
    #[error("{0}")]
    Forbidden(&'static str),

    /// Pflicht-Eingabe fehlt, z. B. `state_id` (HTTP 400).
    #[error("{0}")]
    BadRequest(&'static str),

    /// Der Master-Broker / OAuth-Service ist nicht (vollständig) konfiguriert
    /// (HTTP 503).
    #[error("{0}")]
    ServiceUnavailable(&'static str),

    /// Fehler beim Sprechen mit dem Broker: nicht erreichbar, Non-200, kein
    /// JSON, ungültiges Payload (HTTP 502). Die Nachricht ist dynamisch, weil
    /// sie die Fehlermeldung des Brokers durchreichen kann.
    #[error("{0}")]
    BadGateway(String),

    /// Persistenz-Fehler (Pool/Query). Wird auf HTTP 500 abgebildet.
    #[error(transparent)]
    Db(#[from] turnier_db::DbError),
}

/// Direkte Konvertierung aus `sqlx::Error`, damit `?` an Query-Aufrufen ohne
/// manuelles Mapping funktioniert. Geht über [`turnier_db::DbError`], damit es genau
/// eine Persistenz-Fehler-Repräsentation gibt.
impl From<sqlx::Error> for AuthError {
    fn from(err: sqlx::Error) -> Self {
        AuthError::Db(turnier_db::DbError::from(err))
    }
}

/// HTTP-Statuscode, der zu jeder Variante gehört. turnier-api nutzt das beim
/// Übersetzen in eine `Response`.
impl AuthError {
    pub fn status_code(&self) -> u16 {
        match self {
            AuthError::Unauthorized(_) => 401,
            AuthError::Forbidden(_) => 403,
            AuthError::BadRequest(_) => 400,
            AuthError::ServiceUnavailable(_) => 503,
            AuthError::BadGateway(_) => 502,
            AuthError::Db(_) => 500,
        }
    }
}

/// Bequemer Result-Alias der Auth-Schicht.
pub type AuthResult<T> = Result<T, AuthError>;
