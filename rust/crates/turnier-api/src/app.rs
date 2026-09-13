//! Zusammenbau des HTTP-Routers: alle Router-Module mergen, die Querschnitt-
//! Middleware (CORS, TrustedHost) anlegen und den `AppState` anhängen.

use axum::extract::{Request, State};
use axum::http::header::HOST;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

use turnier_core::UserSession;

use crate::error::WebError;
use crate::extract::AuthUser;
use crate::state::AppState;
use crate::{
    account, admin, auth, consent, draft, internal_automatik, internal_scrims, leaderboard,
    observer, operations, public, test_mode,
};

/// Baut den vollständigen axum-Router inkl. State und Middleware.
pub fn build_router(state: AppState) -> Router {
    let cors = build_cors(&state);

    Router::new()
        .route("/api/me", get(me))
        .route("/api/health", get(health))
        .merge(auth::router())
        .merge(account::router())
        .merge(public::router())
        .merge(admin::router())
        .merge(operations::router())
        .merge(consent::router())
        .merge(leaderboard::router())
        .merge(draft::router())
        .merge(observer::router())
        .merge(internal_automatik::router())
        .merge(internal_scrims::router())
        .merge(test_mode::router(&state.config))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            host_guard,
        ))
        .layer(cors)
        .with_state(state)
}

/// `GET /api/me` — die aktuelle Nutzer-Session.
async fn me(AuthUser(user): AuthUser) -> Json<UserSession> {
    Json(user)
}

/// `GET /api/health` — Health-Check.
async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "service": "deadlock-turniere" }))
}

/// Baut den CORS-Layer aus den konfigurierten Origins. Mit `allow_credentials`
/// werden Methoden/Header gespiegelt (Äquivalent zu Pythons `["*"]`).
fn build_cors(state: &AppState) -> CorsLayer {
    let origins: Vec<HeaderValue> = state
        .config
        .cors_allowed_origins()
        .into_iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true)
}

/// TrustedHost-Äquivalent: lässt nur Requests mit erlaubtem `Host`-Header durch
/// (`config.allowed_hosts()`), sonst 400 — wie Starlettes `TrustedHostMiddleware`.
async fn host_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let allowed = state.config.allowed_hosts();
    let host = req
        .headers()
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .map(host_only);

    match host {
        Some(h) if allowed.iter().any(|a| a == &h) => next.run(req).await,
        _ => WebError::bad_request("Invalid host header").into_response(),
    }
}

/// Extrahiert den reinen Hostnamen (ohne Port) aus einem `Host`-Header.
fn host_only(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('[') {
        // IPv6 in Klammern: bis zur schließenden Klammer.
        if let Some(end) = rest.find(']') {
            return rest[..end].to_lowercase();
        }
    }
    raw.split(':').next().unwrap_or(raw).to_lowercase()
}
