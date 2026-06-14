//! Consent-Router (Stub) — portiert `tournament/consent_routes.py`
//! (Datenschutz-Einwilligung). Wird in Welle 5b implementiert.

use axum::Router;

use crate::state::AppState;

/// Router der Einwilligungs-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
}
