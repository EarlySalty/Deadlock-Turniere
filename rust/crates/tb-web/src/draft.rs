//! Draft-Router (Stub) — portiert `draft/routes.py` (Pick/Ban-Endpunkte über
//! `tb_draft`). Wird in Welle 5b implementiert.

use axum::Router;

use crate::state::AppState;

/// Router der Draft-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
}
