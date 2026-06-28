//! Solo-Anmeldung, Abmeldung und Spieler-Check-in.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::error::{WebError, WebResult};
use crate::extract::AuthUser;
use crate::state::AppState;

use super::helpers::{self, RankInput};

/// Router der Solo-Signup-/Check-in-Routen.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/tournaments/{tournament_id}/signup",
            post(solo_signup).delete(cancel_solo_signup),
        )
        .route("/api/tournaments/{tournament_id}/checkin", post(checkin_player))
}

/// `POST /api/tournaments/{tournament_id}/signup` — Solo-Anmeldung (201).
/// Bei `team_size == 1` wird automatisch ein 1-Mann-Team angelegt.
async fn solo_signup(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<(axum::http::StatusCode, Json<Value>)> {
    let pool = &state.pool;
    helpers::ensure_consent(pool, &user.discord_id).await?;

    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;

    // Bereits angemeldet?
    let signup_exists: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(pool)
    .await?;
    if signup_exists.is_some() {
        return Err(WebError::conflict("Du bist bereits für dieses Turnier angemeldet"));
    }

    // Bereits in einem Team?
    let in_team: Option<(i64,)> = sqlx::query_as(
        "SELECT tm.id FROM team_members tm JOIN teams t ON tm.team_id = t.id \
         WHERE t.tournament_id = ? AND tm.discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(pool)
    .await?;
    if in_team.is_some() {
        return Err(WebError::conflict("Du bist bereits in einem Team dieses Turniers"));
    }

    let rank = helpers::load_rank_input(&state, &user.discord_id).await;
    let team_size = t.team_size;

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO tournament_signups \
         (tournament_id, discord_id, discord_name, steam_id, rank, rank_score) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .bind(user.discord_name.as_deref())
    .bind(&rank.steam_id)
    .bind(&rank.rank)
    .bind(rank.rank_score)
    .execute(&mut *tx)
    .await?;

    if team_size == 1 {
        // Bei 1vs1 ist jeder Teilnehmer sein eigenes Team — direkt anlegen.
        // BEWUSST 1:1 erhalten: Such-Schleife für eindeutigen name_key statt
        // UNIQUE-Retry (Original-Smell „behavior-change", routes.py:1119-1150).
        let resolved_name = helpers::sanitize_discord_name(
            user.discord_name.as_deref(),
            Some(&user.discord_id),
        )
        .unwrap_or_else(|| user.discord_id.clone());

        let mut team_name = truncate_chars(&resolved_name, 32);
        let base_key = team_name.to_lowercase();
        let mut name_key = base_key.clone();
        let mut suffix = 1;
        loop {
            let exists: Option<(i64,)> = sqlx::query_as(
                "SELECT id FROM teams WHERE tournament_id = ? AND name_key = ?",
            )
            .bind(tournament_id)
            .bind(&name_key)
            .fetch_optional(&mut *tx)
            .await?;
            if exists.is_none() {
                break;
            }
            suffix += 1;
            name_key = format!("{base_key}{suffix}");
            team_name = truncate_chars(&format!("{resolved_name}{suffix}"), 32);
        }

        let team_id: i64 = sqlx::query_scalar(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) \
             VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(tournament_id)
        .bind(&team_name)
        .bind(&name_key)
        .bind(&user.discord_id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO team_members \
             (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) \
             VALUES (?, ?, ?, ?, ?, ?, 'captain')",
        )
        .bind(team_id)
        .bind(&user.discord_id)
        .bind(&resolved_name)
        .bind(&rank.steam_id)
        .bind(&rank.rank)
        .bind(rank.rank_score)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE tournament_signups SET team_id = ? WHERE tournament_id = ? AND discord_id = ?",
        )
        .bind(team_id)
        .bind(tournament_id)
        .bind(&user.discord_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(json!({ "status": "angemeldet", "tournament_id": tournament_id })),
    ))
}

/// `DELETE /api/tournaments/{tournament_id}/signup` — Solo-Anmeldung zurückziehen.
/// Nur bei `status = 'registration'` und wenn `team_id` NULL ist.
async fn cancel_solo_signup(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;

    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    if t.status != "registration" {
        return Err(WebError::bad_request("Anmeldung ist nicht geöffnet"));
    }

    let signup: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT team_id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(pool)
    .await?;
    let Some((team_id,)) = signup else {
        return Err(WebError::not_found("Keine Anmeldung gefunden"));
    };
    if team_id.is_some() {
        return Err(WebError::bad_request(
            "Du bist bereits in einem Team — verlasse zuerst das Team",
        ));
    }

    sqlx::query("DELETE FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?")
        .bind(tournament_id)
        .bind(&user.discord_id)
        .execute(pool)
        .await?;

    Ok(Json(json!({ "status": "abgemeldet", "tournament_id": tournament_id })))
}

/// `POST /api/tournaments/{tournament_id}/checkin` — Spieler-Check-in.
/// Nur bei `status = 'checkin'`. Erzeugt fehlenden Signup für Team-Mitglieder.
async fn checkin_player(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;

    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    if t.status != "checkin" {
        return Err(WebError::bad_request("Check-in ist aktuell nicht geöffnet"));
    }

    let mut tx = pool.begin().await?;

    let signup_exists: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(&mut *tx)
    .await?;

    if signup_exists.is_none() {
        // Lazy-Signup für Team-Mitglieder, die noch keinen Signup-Eintrag haben.
        let membership: Option<MembershipRow> = sqlx::query_as(
            "SELECT tm.discord_name, tm.steam_id, tm.rank, tm.rank_score, tm.team_id \
             FROM team_members tm JOIN teams t ON t.id = tm.team_id \
             WHERE t.tournament_id = ? AND tm.discord_id = ?",
        )
        .bind(tournament_id)
        .bind(&user.discord_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(m) = membership else {
            return Err(WebError::forbidden("Du bist für dieses Turnier nicht angemeldet"));
        };
        // `user.discord_name or membership.discord_name` — leerer String fällt
        // zurück (Python-`or`-Semantik 1:1).
        let discord_name = user
            .discord_name
            .clone()
            .filter(|n| !n.is_empty())
            .or(m.discord_name);
        let rank = RankInput {
            steam_id: m.steam_id,
            rank: m.rank,
            rank_score: m.rank_score.unwrap_or(0),
        };
        helpers::upsert_signup(
            &mut tx,
            tournament_id,
            &user.discord_id,
            discord_name.as_deref(),
            &rank,
            Some(m.team_id),
        )
        .await?;
    }

    let existing_checkin: Option<(String,)> = sqlx::query_as(
        "SELECT checked_in_at FROM tournament_checkins WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(&mut *tx)
    .await?;
    let already_checked_in = existing_checkin.is_some();
    if !already_checked_in {
        sqlx::query("INSERT INTO tournament_checkins (tournament_id, discord_id) VALUES (?, ?)")
            .bind(tournament_id)
            .bind(&user.discord_id)
            .execute(&mut *tx)
            .await?;
    }

    helpers::audit(
        &mut *tx,
        "tournament_checkin",
        Some(&user.discord_id),
        &json!({
            "tournament_id": tournament_id,
            "discord_id": user.discord_id,
            "already_checked_in": already_checked_in,
        }),
    )
    .await?;

    tx.commit().await?;

    Ok(Json(json!({ "checked_in": true, "already_checked_in": already_checked_in })))
}

/// Mitgliedszeile für den Lazy-Signup beim Check-in.
#[derive(sqlx::FromRow)]
struct MembershipRow {
    discord_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: Option<i64>,
    team_id: i64,
}

/// Kürzt einen String auf höchstens `max` Zeichen (Codepoints), wie das
/// Python-Slicing `name[:32]`.
fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
