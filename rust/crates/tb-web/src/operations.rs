//! Operations-Router (Stub) — portiert `tournament/operations_routes.py`
//! (Ergebnis-Selbstmeldung, No-Show etc.). Wird in Welle 5b implementiert.

use axum::Router;

use crate::state::AppState;

/// Router der Operations-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
}
