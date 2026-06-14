//! Draft-Router — Pick/Ban-Endpunkte (portiert `draft/routes.py`).
//!
//! - `GET  /api/draft/heroes` — Heldenliste (öffentlich).
//! - `POST /api/draft/matches/{match_id}/start` — Draft starten (Admin).
//! - `GET  /api/draft/sessions/{session_id}` — Zustand lesen (Admin).
//! - `POST /api/draft/sessions/{session_id}/action` — Aktion ausführen (Admin).
//!
//! `taken_by` wird wie im Original aus dem Request-Body übernommen (nicht aus der
//! Session — Parität, siehe `docs/known-issues.md` KI-DR01).

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{WebError, WebResult};
use crate::extract::AdminUser;
use crate::state::AppState;

/// Router der Draft-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/draft/heroes", get(list_heroes))
        .route("/api/draft/matches/{match_id}/start", post(start_match_draft))
        .route("/api/draft/sessions/{session_id}", get(get_session))
        .route("/api/draft/sessions/{session_id}/action", post(submit_action))
}

/// `GET /api/draft/heroes` — die geordnete Heldenliste.
async fn list_heroes() -> Json<Value> {
    Json(json!({ "heroes": tb_draft::DEADLOCK_HEROES }))
}

/// `POST /api/draft/matches/{match_id}/start` — Draft für ein Bracket-Match starten.
async fn start_match_draft(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(match_id): Path<i64>,
) -> WebResult<Json<Value>> {
    // Existenz des Bracket-Matches vorab prüfen (wie im Original; FK ist Backstop).
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM bracket_matches WHERE id = ?")
        .bind(match_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(WebError::not_found(format!(
            "Bracket-Match {match_id} nicht gefunden"
        )));
    }

    let session_id = tb_draft::start_draft(&state.pool, match_id, &user.discord_id).await?;
    let draft_state = tb_draft::get_draft_state(&state.pool, session_id).await?;
    Ok(Json(serde_json::to_value(draft_state).unwrap_or_else(|_| json!({}))))
}

/// `GET /api/draft/sessions/{session_id}` — den Draft-Zustand lesen.
async fn get_session(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(session_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let draft_state = tb_draft::get_draft_state(&state.pool, session_id).await?;
    Ok(Json(serde_json::to_value(draft_state).unwrap_or_else(|_| json!({}))))
}

/// Request-Body von `submit_action`.
#[derive(Debug, Deserialize)]
struct DraftActionRequest {
    hero_name: String,
    taken_by: String,
    #[serde(default)]
    force: bool,
}

/// `POST /api/draft/sessions/{session_id}/action` — eine Ban/Pick-Aktion ausführen.
///
/// Antwort ist die flache Verschmelzung aus Draft-Zustand und Aktions-Ergebnis
/// (`{**state, **result}` im Original).
async fn submit_action(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(session_id): Path<i64>,
    Json(body): Json<DraftActionRequest>,
) -> WebResult<Json<Value>> {
    let outcome =
        tb_draft::take_action(&state.pool, session_id, &body.hero_name, &body.taken_by, body.force)
            .await?;
    let draft_state = tb_draft::get_draft_state(&state.pool, session_id).await?;

    let mut merged = serde_json::to_value(draft_state).unwrap_or_else(|_| json!({}));
    if let (Value::Object(target), Ok(Value::Object(extra))) =
        (&mut merged, serde_json::to_value(outcome))
    {
        target.extend(extra);
    }
    Ok(Json(merged))
}
