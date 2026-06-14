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

impl WebError {
    /// Erzeugt einen Fehler mit explizitem Status und Meldung.
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        Self { status, detail: detail.into() }
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

impl From<tb_auth::AuthError> for WebError {
    fn from(err: tb_auth::AuthError) -> Self {
        Self::new(status_from_u16(err.status_code()), err.to_string())
    }
}

impl From<tb_db::DbError> for WebError {
    fn from(err: tb_db::DbError) -> Self {
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

impl From<tb_tournament::TournamentError> for WebError {
    fn from(err: tb_tournament::TournamentError) -> Self {
        use tb_tournament::TournamentError::*;
        match err {
            Validation(msg) => Self::bad_request(msg),
            SnapshotMismatch => Self::conflict("Daten wurden zwischenzeitlich geändert"),
            StatusConflict => Self::conflict("Statuswechsel kollidiert"),
            Steam(_) => Self::internal("Rang-/Steam-Fehler"),
            Db(e) => e.into(),
        }
    }
}

impl From<tb_match::MatchError> for WebError {
    fn from(err: tb_match::MatchError) -> Self {
        use tb_match::MatchError::*;
        match err {
            NotFound(msg) => Self::not_found(msg),
            State(msg) => Self::conflict(msg),
            Invalid(msg) => Self::bad_request(msg),
            Db(e) => e.into(),
            Tournament(e) => e.into(),
        }
    }
}

impl From<tb_match::SteamTaskError> for WebError {
    fn from(err: tb_match::SteamTaskError) -> Self {
        use tb_match::SteamTaskError::*;
        match err {
            Timeout { .. } => Self::new(StatusCode::GATEWAY_TIMEOUT, err.to_string()),
            Failed(msg) => Self::new(StatusCode::BAD_GATEWAY, msg),
            State(msg) => Self::conflict(msg),
            NotFound(msg) => Self::not_found(msg),
            Db(e) => e.into(),
            Bridge(_) => Self::new(StatusCode::SERVICE_UNAVAILABLE, "Steam-Bridge nicht verfügbar"),
            InvalidResult(msg) => Self::new(StatusCode::BAD_GATEWAY, msg),
        }
    }
}

impl From<tb_draft::DraftError> for WebError {
    fn from(err: tb_draft::DraftError) -> Self {
        use tb_draft::DraftError::*;
        match err {
            SessionNotFound => Self::not_found("Draft-Session nicht gefunden"),
            UnknownHero(msg) => Self::bad_request(msg),
            SessionNotActive => Self::bad_request("Draft ist nicht aktiv"),
            HeroAlreadyTaken(msg) => Self::bad_request(msg),
            ActionConflict => Self::conflict("Draft-Aktion kollidiert"),
            Db(e) => e.into(),
        }
    }
}

impl From<tb_scheduler::SchedulerError> for WebError {
    fn from(err: tb_scheduler::SchedulerError) -> Self {
        use tb_scheduler::SchedulerError::*;
        match err {
            InvalidTransition(msg) => Self::bad_request(msg),
            StatusConflict => Self::conflict("Turnierstatus wurde parallel geändert"),
            Tournament(e) => e.into(),
            Db(e) => e.into(),
        }
    }
}

impl From<tb_steam::SteamError> for WebError {
    fn from(err: tb_steam::SteamError) -> Self {
        tracing::error!(error = %err, "Steam-/Rang-Fehler in Route");
        Self::internal("Rang-/Steam-Fehler")
    }
}
