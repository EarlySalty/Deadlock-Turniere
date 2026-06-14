//! Admin-Router (Stub) — portiert `tournament/admin_routes.py` (der grösste
//! Monolith). Wird in Welle 5b in thematische Submodule aufgeteilt
//! (tournaments/phases/teams/brackets/matches/group_matches/casters/voice).

use axum::Router;

use crate::state::AppState;

/// Router der Admin-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
}
