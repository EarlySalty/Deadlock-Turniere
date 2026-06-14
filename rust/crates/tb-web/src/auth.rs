//! Auth-Router (Stub): `/auth/discord/login`, `/auth/discord/complete`,
//! `/auth/discord/logout`. Wird in Welle 5b implementiert.

use axum::Router;

use crate::state::AppState;

/// Router für den delegierten Discord-OAuth-Flow.
pub fn router() -> Router<AppState> {
    Router::new()
}
