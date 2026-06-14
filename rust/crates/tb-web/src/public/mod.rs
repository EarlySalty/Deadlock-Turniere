//! Öffentlicher/Teilnehmer-Router (Stub) — portiert `tournament/routes.py`.
//! Wird in Welle 5b implementiert (intern in Submodule aufgeteilt).

use axum::Router;

use crate::state::AppState;

/// Router der öffentlichen + Teilnehmer-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
}
