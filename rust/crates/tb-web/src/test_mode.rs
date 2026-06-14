//! Test-Mode-Router (Stub) — portiert `admin/test_mode.py`. Nur aktiv, wenn der
//! Test-Modus in der Config freigeschaltet ist. Wird in Welle 5b implementiert.

use axum::Router;

use tb_config::Config;

use crate::state::AppState;

/// Router der Test-Modus-Endpunkte. Liefert einen leeren Router, wenn der
/// Test-Modus nicht freigeschaltet ist (Gate wird in 5b verfeinert).
pub fn router(_config: &Config) -> Router<AppState> {
    Router::new()
}
