//! Auth-Router — delegierter Discord-OAuth-Flow (portiert `auth/discord_oauth.py`).
//!
//! - `GET /auth/discord/login` → holt die Authorize-URL vom Broker und leitet
//!   den Browser dorthin (307).
//! - `GET /auth/discord/complete?state_id=…` → löst das Ergebnis beim Broker ein,
//!   legt eine Session an, setzt das `session_token`-Cookie und leitet auf das
//!   Frontend (302).
//! - `GET /auth/discord/logout` → löscht die Session und das Cookie, leitet aufs
//!   Frontend (302).

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::error::WebResult;
use crate::state::AppState;

/// Name des Session-Cookies (identisch zum Original).
const SESSION_COOKIE: &str = "session_token";
/// Cookie-Lebensdauer in Sekunden (7 Tage), wie `SESSION_LIFETIME` im Original.
const SESSION_MAX_AGE_SECONDS: i64 = 7 * 24 * 60 * 60;

/// Router der Auth-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/discord/login", get(login))
        .route("/auth/discord/complete", get(complete))
        .route("/auth/discord/logout", get(logout))
}

/// `GET /auth/discord/login` — Redirect auf die Discord-Authorize-URL (307).
async fn login(State(state): State<AppState>) -> WebResult<Response> {
    let authorize_url = state.oauth.initiate_login().await?;
    Ok(redirect(
        StatusCode::TEMPORARY_REDIRECT,
        &authorize_url,
        None,
    ))
}

/// Query-Parameter von `/auth/discord/complete`.
#[derive(Debug, Deserialize)]
struct CompleteQuery {
    #[serde(default)]
    state_id: Option<String>,
}

/// `GET /auth/discord/complete` — Session anlegen, Cookie setzen, aufs Frontend (302).
async fn complete(
    State(state): State<AppState>,
    Query(query): Query<CompleteQuery>,
) -> WebResult<Response> {
    let state_id = query.state_id.unwrap_or_default();
    let identity = state.oauth.complete_login(&state_id).await?;

    let token = turnier_auth::create_session(
        &state.pool,
        &identity.discord_id,
        &identity.discord_name,
        &identity.discord_avatar,
        &identity.roles,
    )
    .await?;

    let cookie = format!(
        "{SESSION_COOKIE}={token}; HttpOnly; Secure; SameSite=Lax; Max-Age={SESSION_MAX_AGE_SECONDS}; Path=/"
    );
    Ok(redirect(
        StatusCode::FOUND,
        &state.config.frontend_url,
        Some(cookie),
    ))
}

/// `GET /auth/discord/logout` — Session + Cookie löschen, aufs Frontend (302).
async fn logout(State(state): State<AppState>, jar: CookieJar) -> WebResult<Response> {
    if let Some(token) = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()) {
        if !token.is_empty() {
            // Fehler beim Löschen sind unkritisch (Cookie wird ohnehin entfernt).
            let _ = turnier_auth::delete_session(&state.pool, &token).await;
        }
    }
    let clear = format!("{SESSION_COOKIE}=; Max-Age=0; Path=/");
    Ok(redirect(
        StatusCode::FOUND,
        &state.config.frontend_url,
        Some(clear),
    ))
}

/// Baut eine Redirect-Response mit optionalem `Set-Cookie`-Header.
fn redirect(status: StatusCode, location: &str, set_cookie: Option<String>) -> Response {
    let mut builder = Response::builder()
        .status(status)
        .header(LOCATION, location);
    if let Some(cookie) = set_cookie {
        builder = builder.header(SET_COOKIE, cookie);
    }
    builder
        .body(Body::empty())
        .expect("statische Redirect-Response ist immer gültig")
}
