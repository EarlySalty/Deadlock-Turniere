//! Leaderboard-Router (Stub) — portiert `tournament/leaderboard_routes.py`
//! (Rangliste, Spielerprofile). Wird in Welle 5b implementiert.

use axum::Router;

use crate::state::AppState;

/// Router der Leaderboard-/Profil-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
}
