//! Auth-Router: delegierter Discord-OAuth-Flow (portiert `auth/discord_oauth.py`).
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
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::error::{WebError, WebResult};
use crate::extract::OptionalUser;
use crate::state::AppState;

/// Name des Session-Cookies (identisch zum Original).
const SESSION_COOKIE: &str = "session_token";

/// Router der Auth-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/guest", post(guest_login))
        .route("/auth/discord/login", get(login))
        .route("/auth/discord/complete", get(complete))
        .route("/auth/discord/logout", get(logout))
}

/// `GET /auth/discord/login`: Redirect auf die Discord-Authorize-URL (307).
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

/// `GET /auth/discord/complete`: Session anlegen, Cookie setzen, aufs Frontend (302).
async fn complete(
    State(state): State<AppState>,
    Query(query): Query<CompleteQuery>,
) -> WebResult<Response> {
    let state_id = query.state_id.unwrap_or_default();
    let identity = state.oauth.complete_login(&state_id).await?;

    let token = turnier_auth::create_session_with_lifetime(
        &state.pool,
        &identity.discord_id,
        &identity.discord_name,
        &identity.discord_avatar,
        &identity.roles,
        state.config.limits.session_lifetime_days,
    )
    .await?;

    let session_max_age_seconds = state.config.limits.session_lifetime_days * 24 * 60 * 60;
    let cookie = format!(
        "{SESSION_COOKIE}={token}; HttpOnly; Secure; SameSite=Lax; Max-Age={session_max_age_seconds}; Path=/"
    );
    Ok(redirect(
        StatusCode::FOUND,
        &state.config.frontend_url,
        Some(cookie),
    ))
}

/// `GET /auth/discord/logout`: Session + Cookie löschen, aufs Frontend (302).
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

#[derive(Deserialize)]
struct GuestLogin {
    name: String,
}

fn guest_name(raw: &str) -> WebResult<&str> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 32 || name.chars().any(char::is_control) {
        return Err(WebError::bad_request(
            "Bitte gib einen Namen mit 1 bis 32 Zeichen ohne Steuerzeichen ein",
        ));
    }
    Ok(name)
}

/// Creates an unprivileged guest identity. A name never grants access to an
/// existing participant; only the opaque browser cookie identifies its owner.
async fn guest_login(
    State(state): State<AppState>,
    OptionalUser(user): OptionalUser,
    Json(input): Json<GuestLogin>,
) -> WebResult<Response> {
    if user.is_some() {
        return Err(WebError::conflict("Du bist bereits angemeldet"));
    }
    let name = guest_name(&input.name)?;
    // Reuse the signup identity sequence for concurrency safe allocation.
    // Negative IDs cannot collide with Discord snowflakes. Sequence gaps
    // are expected, including when the guest cancels before signing up.
    let guest_id: i64 = sqlx::query_scalar(
        "SELECT -nextval(pg_get_serial_sequence('turnier.tournament_signups', 'id'))",
    )
    .fetch_one(&state.pool)
    .await?;
    let token = turnier_auth::create_session_with_lifetime(
        &state.pool,
        &guest_id.to_string(),
        name,
        "",
        &[],
        state.config.limits.session_lifetime_days,
    )
    .await?;
    let max_age = state.config.limits.session_lifetime_days * 24 * 60 * 60;
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header(SET_COOKIE, format!("{SESSION_COOKIE}={token}; HttpOnly; Secure; SameSite=Lax; Max-Age={max_age}; Path=/"))
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .expect("valid guest response"))
}

#[cfg(test)]
mod guest_tests {
    use super::guest_name;

    #[test]
    fn validates_guest_names() {
        assert_eq!(guest_name("  Jörg  ").unwrap(), "Jörg");
        for name in ["", "  ", "a\nb", "a\u{7f}b", &"ü".repeat(33)] {
            assert!(guest_name(name).is_err());
        }
        assert!(guest_name(&"ü".repeat(32)).is_ok());
    }
}
