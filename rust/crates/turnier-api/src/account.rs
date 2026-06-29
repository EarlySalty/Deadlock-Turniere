//! Account-Self-Service-Routen.

use axum::extract::{Path, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use turnier_automatik::optout::{self, Scope};

use crate::error::{WebError, WebResult};
use crate::extract::AuthUser;
use crate::state::AppState;

const PH_INVALID_SCOPE: &str = "Ungültiger DM-Bereich";

/// Router fuer eigene Account-Aktionen.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/me/dm-optout", get(list_dm_optout).put(set_dm_optout))
        .route("/api/me/dm-optout/{scope}", delete(clear_dm_optout))
}

#[derive(Debug, Serialize)]
struct DmOptOutResponse {
    scopes: Vec<Scope>,
}

#[derive(Debug, Deserialize)]
struct DmOptOutBody {
    scope: String,
}

fn parse_scope(value: &str) -> WebResult<Scope> {
    match value {
        "fun" => Ok(Scope::Fun),
        "comp" => Ok(Scope::Comp),
        "all" => Ok(Scope::All),
        _ => Err(WebError::bad_request(PH_INVALID_SCOPE)),
    }
}

async fn current_optouts(pool: &turnier_db::Pool, discord_id: &str) -> WebResult<DmOptOutResponse> {
    let optouts = optout::list_optouts(pool, discord_id).await?;
    Ok(DmOptOutResponse {
        scopes: optouts.into_iter().map(|entry| entry.scope).collect(),
    })
}

async fn list_dm_optout(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> WebResult<Json<DmOptOutResponse>> {
    Ok(Json(current_optouts(&state.pool, &user.discord_id).await?))
}

async fn set_dm_optout(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<DmOptOutBody>,
) -> WebResult<Json<DmOptOutResponse>> {
    let scope = parse_scope(&body.scope)?;
    optout::set_optout(&state.pool, &user.discord_id, scope).await?;
    Ok(Json(current_optouts(&state.pool, &user.discord_id).await?))
}

async fn clear_dm_optout(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(scope): Path<String>,
) -> WebResult<Json<DmOptOutResponse>> {
    let scope = parse_scope(&scope)?;
    optout::clear_optout(&state.pool, &user.discord_id, scope).await?;
    Ok(Json(current_optouts(&state.pool, &user.discord_id).await?))
}
