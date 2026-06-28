//! Auth-Extractoren: lösen das Session-Token aus Header oder Cookie auf und
//! liefern die [`UserSession`] (bzw. erzwingen Mod-/Admin-Rechte).
//!
//! Token-Quelle wie im Original (`middleware.py`): zuerst `Authorization: Bearer
//! <token>`, sonst das Cookie `session_token`. Fehlt das Token oder ist die
//! Session ungültig/abgelaufen → 401. Mod-/Admin-Gates → 403.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;

use turnier_core::UserSession;

use crate::error::WebError;
use crate::state::AppState;

/// Name des Session-Cookies (identisch zum Python-Original).
const SESSION_COOKIE: &str = "session_token";

/// Liest das Session-Token: `Authorization: Bearer` hat Vorrang vor dem Cookie.
fn extract_token(parts: &Parts) -> Option<String> {
    if let Some(value) = parts.headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(token) = value.strip_prefix("Bearer ") {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    let jar = CookieJar::from_headers(&parts.headers);
    jar.get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .filter(|t| !t.is_empty())
}

/// Ein authentifizierter Nutzer (eingeloggt, Rolle egal).
#[derive(Debug, Clone)]
pub struct AuthUser(pub UserSession);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = extract_token(parts)
            .ok_or_else(|| WebError::unauthorized("Nicht authentifiziert"))?;
        let session = turnier_auth::resolve_session(&state.pool, &token, &state.role_sets).await?;
        Ok(AuthUser(session))
    }
}

/// Ein Nutzer mit mindestens Moderator-Rechten.
#[derive(Debug, Clone)]
pub struct ModUser(pub UserSession);

impl FromRequestParts<AppState> for ModUser {
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_mod {
            return Err(WebError::forbidden("Moderator-Berechtigung erforderlich"));
        }
        Ok(ModUser(user))
    }
}

/// Ein Nutzer mit Admin-Rechten.
#[derive(Debug, Clone)]
pub struct AdminUser(pub UserSession);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = WebError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(WebError::forbidden("Admin-Berechtigung erforderlich"));
        }
        Ok(AdminUser(user))
    }
}

/// Optionaler Nutzer-Kontext für öffentliche Routen, die sich für eingeloggte
/// Nutzer anders verhalten. Schlägt nie fehl.
#[derive(Debug, Clone)]
pub struct OptionalUser(pub Option<UserSession>);

impl FromRequestParts<AppState> for OptionalUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(OptionalUser(
            AuthUser::from_request_parts(parts, state).await.ok().map(|a| a.0),
        ))
    }
}
