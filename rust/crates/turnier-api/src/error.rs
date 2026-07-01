//! Einheitlicher Web-Fehler und seine Übersetzung in eine HTTP-Response.
//!
//! Alle Routen geben `WebResult<T>` zurück; Domänenfehler aus den tb-*-Crates
//! werden über `?` in einen [`WebError`] mit passendem HTTP-Status übersetzt. Die
//! Response-Form entspricht exakt FastAPI: `{"detail": "<text>"}` plus Statuscode.
//!
//! Die hier gewählten Status sind sinnvolle Defaults der Domänenfehler. Wo eine
//! Route im Python-Original einen abweichenden Status verlangt, konstruiert sie
//! den `WebError` explizit (z. B. [`WebError::bad_request`]).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Fehler einer HTTP-Route: Status + Detailmeldung.
#[derive(Debug, Clone)]
pub struct WebError {
    pub status: StatusCode,
    pub detail: String,
}

/// Bequemer Result-Alias für Handler.
pub type WebResult<T> = Result<T, WebError>;

pub(crate) fn map_unique_conflict(err: sqlx::Error, detail: &'static str) -> WebError {
    match &err {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505") => {
            WebError::conflict(detail)
        }
        _ => err.into(),
    }
}

impl WebError {
    /// Erzeugt einen Fehler mit explizitem Status und Meldung.
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self {
            status,
            detail: detail.into(),
        }
    }

    /// 400 Bad Request.
    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, detail)
    }
    /// 401 Unauthorized.
    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, detail)
    }
    /// 403 Forbidden.
    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, detail)
    }
    /// 404 Not Found.
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, detail)
    }
    /// 409 Conflict.
    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, detail)
    }
    /// 422 Unprocessable Entity (FastAPIs Validierungs-Status).
    pub fn unprocessable(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, detail)
    }
    /// 500 Internal Server Error.
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, detail)
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "detail": self.detail }))).into_response()
    }
}

fn status_from_u16(code: u16) -> StatusCode {
    StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

impl From<turnier_auth::AuthError> for WebError {
    fn from(err: turnier_auth::AuthError) -> Self {
        Self::new(status_from_u16(err.status_code()), err.to_string())
    }
}

impl From<turnier_db::DbError> for WebError {
    fn from(err: turnier_db::DbError) -> Self {
        tracing::error!(error = %err, "DB-Fehler in Route");
        Self::internal("Interner Datenbankfehler")
    }
}

impl From<sqlx::Error> for WebError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(error = %err, "DB-Fehler in Route");
        Self::internal("Interner Datenbankfehler")
    }
}

impl From<turnier_engine::TournamentError> for WebError {
    fn from(err: turnier_engine::TournamentError) -> Self {
        use turnier_engine::TournamentError::*;
        match err {
            Validation(msg) => Self::bad_request(msg),
            SnapshotMismatch => Self::conflict("Daten wurden zwischenzeitlich geändert"),
            StatusConflict => Self::conflict("Statuswechsel kollidiert"),
            Steam(_) => Self::internal("Rang-/Steam-Fehler"),
            Db(e) => e.into(),
        }
    }
}

impl From<turnier_match::MatchError> for WebError {
    fn from(err: turnier_match::MatchError) -> Self {
        use turnier_match::MatchError::*;
        match err {
            NotFound(msg) => Self::not_found(msg),
            State(msg) => Self::conflict(msg),
            Invalid(msg) => Self::bad_request(msg),
            Db(e) => e.into(),
        }
    }
}

impl From<turnier_match::SteamTaskError> for WebError {
    fn from(err: turnier_match::SteamTaskError) -> Self {
        use turnier_match::SteamTaskError::*;
        match err {
            Timeout { .. } => Self::new(StatusCode::GATEWAY_TIMEOUT, err.to_string()),
            Failed(msg) => Self::new(StatusCode::BAD_GATEWAY, msg),
            State(msg) => Self::conflict(msg),
            NotFound(msg) => Self::not_found(msg),
            Db(e) => e.into(),
            Bridge(_) => Self::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Steam-Bridge nicht verfügbar",
            ),
            InvalidResult(msg) => Self::new(StatusCode::BAD_GATEWAY, msg),
        }
    }
}

impl From<turnier_draft::DraftError> for WebError {
    fn from(err: turnier_draft::DraftError) -> Self {
        use turnier_draft::DraftError::*;
        match err {
            SessionNotFound => Self::not_found("Draft-Session nicht gefunden"),
            UnknownHero(msg) => Self::bad_request(msg),
            SessionNotActive => Self::bad_request("Draft ist nicht aktiv"),
            HeroAlreadyTaken(msg) => Self::bad_request(msg),
            ActionConflict => Self::conflict("Draft-Aktion kollidiert"),
            InvalidDiscordId(_) => Self::bad_request("Ungueltige Discord-ID"),
            Db(e) => e.into(),
        }
    }
}

impl From<turnier_scheduler::SchedulerError> for WebError {
    fn from(err: turnier_scheduler::SchedulerError) -> Self {
        use turnier_scheduler::SchedulerError::*;
        match err {
            InvalidTransition(msg) => Self::bad_request(msg),
            StatusConflict => Self::conflict("Turnierstatus wurde parallel geändert"),
            InvalidActorId(_) => Self::bad_request("Ungueltige Actor-Discord-ID"),
            Tournament(e) => e.into(),
            Db(e) => e.into(),
        }
    }
}

impl From<turnier_automatik::AutomatikError> for WebError {
    fn from(err: turnier_automatik::AutomatikError) -> Self {
        use turnier_automatik::AutomatikError::*;
        match err {
            InvalidTransition { .. } => Self::conflict("Dieser Statuswechsel ist nicht möglich"),
            MissingApproval { .. } => {
                Self::conflict("Für diesen Vorschlag liegt noch keine Caster-Freigabe vor.")
            }
            InvalidNumericId(_) => Self::bad_request("Ungueltige numerische ID"),
            Json(_) => Self::bad_request("Ungueltiges JSON"),
            Time(_) => Self::bad_request("Ungueltiger Zeitstempel"),
            Db(e) => e.into(),
        }
    }
}

impl From<turnier_steam::SteamError> for WebError {
    fn from(err: turnier_steam::SteamError) -> Self {
        tracing::error!(error = %err, "Steam-/Rang-Fehler in Route");
        Self::internal("Rang-/Steam-Fehler")
    }
}
