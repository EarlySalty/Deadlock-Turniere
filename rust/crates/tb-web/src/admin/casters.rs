//! Caster-Verwaltung: verfügbare Caster aus der Discord-Rolle, Zuweisung/Entzug
//! auf Turnier-Ebene. Die Match-Ebene-Routen sind deprecated (410 Gone bzw. ein
//! Alias auf die Turnier-Caster), wie im Original.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{WebError, WebResult};
use crate::extract::ModUser;
use crate::state::AppState;

use super::helpers::{audit, ensure_bracket_match_exists, load_tournament_or_404};

/// Ausgabe-Form eines Casters (`MatchCasterOut`).
#[derive(Debug, Clone, Serialize)]
pub struct MatchCasterOut {
    pub discord_id: String,
    // Kein skip_serializing_if: FastAPI gibt fehlende Felder als `null` aus
    // (kein exclude_none), nicht weggelassen.
    pub display_name: Option<String>,
    pub assigned_at: Option<String>,
    pub assigned_by: Option<String>,
}

/// Router der Caster-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/casters", get(list_available_casters))
        .route(
            "/api/admin/tournaments/{tournament_id}/casters",
            get(list_tournament_casters).post(assign_tournament_caster),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/casters/{discord_id}",
            delete(remove_tournament_caster),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/casters",
            get(list_match_casters).post(assign_match_caster_gone),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/casters/{discord_id}",
            delete(remove_match_caster_gone),
        )
}

/// Prüft, ob ein String wie eine rohe Discord-ID aussieht (16–21 Ziffern).
fn looks_like_discord_id(value: &str) -> bool {
    let s = value.trim();
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) && (16..=21).contains(&s.len())
}

/// `_preferred_discord_name`-Variante für Caster (display>global>username).
fn caster_display_name(member: &Value, discord_id: &str) -> Option<String> {
    for key in ["display_name", "global_name", "username"] {
        if let Some(candidate) = member.get(key).and_then(Value::as_str) {
            let stripped = candidate.trim();
            if stripped.is_empty() || stripped == discord_id || looks_like_discord_id(stripped) {
                continue;
            }
            return Some(stripped.to_string());
        }
    }
    None
}

/// Lädt die Mitglieder der Caster-Rolle als `discord_id -> display_name`-Map.
///
/// `strict`: bei Discord-Fehler 503 (statt leere Map). Portiert
/// `_load_caster_role_members`.
async fn load_caster_role_members(
    state: &AppState,
    strict: bool,
) -> WebResult<HashMap<String, Option<String>>> {
    let guild_id: i64 = state.config.discord_guild_id.parse().unwrap_or(0);
    let role_id = state.config.discord_caster_role_id;

    let members = match state.notifier.get_role_members(guild_id, role_id).await {
        Ok(members) => members,
        Err(err) => {
            if strict {
                return Err(WebError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Caster-Rollenmitglieder konnten nicht geladen werden",
                ));
            }
            tracing::error!(error = %err, "Caster-Rollenmitglieder konnten nicht geladen werden");
            return Ok(HashMap::new());
        }
    };

    let mut out = HashMap::new();
    for member in &members {
        let discord_id = member
            .get("user_id")
            .and_then(Value::as_str)
            .or_else(|| member.get("id").and_then(Value::as_str))
            .unwrap_or("")
            .trim()
            .to_string();
        if discord_id.is_empty() {
            continue;
        }
        let name = caster_display_name(member, &discord_id);
        out.insert(discord_id, name);
    }
    Ok(out)
}

/// Lädt die einem Turnier zugewiesenen Caster (mit aufgelösten Namen). Portiert
/// `_load_tournament_casters`.
async fn load_tournament_casters_inner(
    state: &AppState,
    tournament_id: i64,
    display_names: Option<HashMap<String, Option<String>>>,
) -> WebResult<Vec<MatchCasterOut>> {
    let display_names = match display_names {
        Some(d) => d,
        None => load_caster_role_members(state, false).await?,
    };

    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT discord_id, assigned_at, assigned_by FROM tournament_casters \
         WHERE tournament_id = ? ORDER BY assigned_at, discord_id",
    )
    .bind(tournament_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter(|(id, _, _)| !id.is_empty())
        .map(|(discord_id, assigned_at, assigned_by)| {
            let display_name = display_names.get(&discord_id).cloned().flatten();
            MatchCasterOut { discord_id, display_name, assigned_at, assigned_by }
        })
        .collect())
}

/// `GET /api/admin/casters` — alle verfügbaren Caster aus der Discord-Rolle (strict).
async fn list_available_casters(
    State(state): State<AppState>,
    _mod: ModUser,
) -> WebResult<Json<Vec<MatchCasterOut>>> {
    let display_names = load_caster_role_members(&state, true).await?;
    let mut casters: Vec<MatchCasterOut> = display_names
        .into_iter()
        .map(|(discord_id, display_name)| MatchCasterOut {
            discord_id,
            display_name,
            assigned_at: None,
            assigned_by: None,
        })
        .collect();
    // Sortierung nach (display_name, discord_id) wie im Original.
    casters.sort_by(|a, b| {
        let an = a.display_name.clone().unwrap_or_default();
        let bn = b.display_name.clone().unwrap_or_default();
        an.cmp(&bn).then_with(|| a.discord_id.cmp(&b.discord_id))
    });
    Ok(Json(casters))
}

/// `GET .../tournaments/{id}/casters` — zugewiesene Turnier-Caster listen.
async fn list_tournament_casters(
    State(state): State<AppState>,
    _mod: ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Vec<MatchCasterOut>>> {
    load_tournament_or_404(&state.pool, tournament_id).await?;
    Ok(Json(load_tournament_casters_inner(&state, tournament_id, None).await?))
}

/// Body von `assign_tournament_caster` (`discord_id`).
#[derive(Debug, Deserialize)]
struct CasterAssignBody {
    discord_id: String,
}

/// `POST .../tournaments/{id}/casters` — Caster zuweisen (muss Rollenmitglied sein).
async fn assign_tournament_caster(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
    Json(body): Json<CasterAssignBody>,
) -> WebResult<Json<Vec<MatchCasterOut>>> {
    let display_names = load_caster_role_members(&state, true).await?;
    if !display_names.contains_key(&body.discord_id) {
        return Err(WebError::bad_request("Discord-User ist kein Caster"));
    }

    load_tournament_or_404(&state.pool, tournament_id).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO tournament_casters (tournament_id, discord_id, assigned_by) VALUES (?, ?, ?)",
    )
    .bind(tournament_id)
    .bind(&body.discord_id)
    .bind(&user.discord_id)
    .execute(&state.pool)
    .await?;
    audit(
        &state.pool,
        "tournament_caster_assign",
        &user.discord_id,
        serde_json::json!({ "tournament_id": tournament_id, "discord_id": body.discord_id }),
    )
    .await?;

    Ok(Json(load_tournament_casters_inner(&state, tournament_id, Some(display_names)).await?))
}

/// `DELETE .../tournaments/{id}/casters/{discord_id}` — Turnier-Caster entfernen.
async fn remove_tournament_caster(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, discord_id)): Path<(i64, String)>,
) -> WebResult<Json<Vec<MatchCasterOut>>> {
    load_tournament_or_404(&state.pool, tournament_id).await?;
    sqlx::query("DELETE FROM tournament_casters WHERE tournament_id = ? AND discord_id = ?")
        .bind(tournament_id)
        .bind(&discord_id)
        .execute(&state.pool)
        .await?;
    audit(
        &state.pool,
        "tournament_caster_remove",
        &user.discord_id,
        serde_json::json!({ "tournament_id": tournament_id, "discord_id": discord_id }),
    )
    .await?;

    Ok(Json(load_tournament_casters_inner(&state, tournament_id, None).await?))
}

/// `GET .../matches/{match_id}/casters` — Match-Caster (gibt faktisch die
/// Turnier-Caster zurück; Befund admin_routes.py:2970 — 1:1 erhalten).
async fn list_match_casters(
    State(state): State<AppState>,
    _mod: ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Vec<MatchCasterOut>>> {
    ensure_bracket_match_exists(&state.pool, tournament_id, match_id).await?;
    Ok(Json(load_tournament_casters_inner(&state, tournament_id, None).await?))
}

/// `POST .../matches/{match_id}/casters` — 410 Gone (deprecated).
async fn assign_match_caster_gone(
    _mod: ModUser,
    Path((_tournament_id, _match_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    Err(WebError::new(
        StatusCode::GONE,
        "Caster werden auf Turnier-Ebene verwaltet — siehe /admin/tournaments/{id}/casters",
    ))
}

/// `DELETE .../matches/{match_id}/casters/{discord_id}` — 410 Gone (deprecated).
async fn remove_match_caster_gone(
    _mod: ModUser,
    Path((_tournament_id, _match_id, _discord_id)): Path<(i64, i64, String)>,
) -> WebResult<Json<Value>> {
    Err(WebError::new(
        StatusCode::GONE,
        "Caster werden auf Turnier-Ebene verwaltet — siehe /admin/tournaments/{id}/casters",
    ))
}
