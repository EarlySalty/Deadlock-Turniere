//! Solo-Anmeldung, Abmeldung und Spieler-Check-in.

use axum::extract::{Path, State};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::db;
use crate::error::{map_unique_conflict, WebError, WebResult};
use crate::extract::AuthUser;
use crate::state::AppState;

use super::helpers;

/// Router der Solo-Signup-/Check-in-Routen.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/tournaments/{tournament_id}/signup",
            post(solo_signup).delete(cancel_solo_signup),
        )
        .route(
            "/api/tournaments/{tournament_id}/checkin",
            post(checkin_player),
        )
}

/// `POST /api/tournaments/{tournament_id}/signup`: Solo-Anmeldung (201).
/// Bei `team_size == 1` wird automatisch ein 1-Mann-Team angelegt, außer im
/// Modus `solo_only`, in dem das Orga-Team die Teams bildet.
async fn solo_signup(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<(axum::http::StatusCode, Json<Value>)> {
    let pool = &state.pool;
    helpers::ensure_consent(pool, &user.discord_id).await?;
    let user_discord_id = db::parse_participant_id(&user.discord_id)?;
    let rank = helpers::load_rank_input(&state, &user.discord_id).await;

    let mut tx = pool.begin().await?;
    let t = helpers::lock_tournament_or_404(&mut tx, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;

    // Bereits angemeldet?
    let signup_exists: Option<(i64,)> = sqlx::query_as(
        r#"SELECT id FROM turnier."tournament_signups" WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(user_discord_id)
    .fetch_optional(&mut *tx)
    .await?;
    if signup_exists.is_some() {
        return Err(WebError::conflict(
            "Du bist bereits für dieses Turnier angemeldet",
        ));
    }

    // Bereits in einem Team?
    let in_team: Option<(i64,)> = sqlx::query_as(
        r#"SELECT tm.id FROM turnier."team_members" tm JOIN turnier."teams" t ON tm.team_id = t.id
         WHERE t.tournament_id = $1 AND tm.discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(user_discord_id)
    .fetch_optional(&mut *tx)
    .await?;
    if in_team.is_some() {
        return Err(WebError::conflict(
            "Du bist bereits in einem Team dieses Turniers",
        ));
    }

    let creates_solo_team = t.team_size == 1
        && !t
            .invite_mode
            .as_deref()
            .is_some_and(turnier_core::InviteMode::is_solo_only);

    helpers::ensure_guest_name_free(
        &mut tx,
        tournament_id,
        user_discord_id,
        user.discord_name.as_deref(),
    )
    .await?;

    sqlx::query(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, signed_up_at)
         VALUES ($1, $2, $3, $4, $5, $6, now())"#,
    )
    .bind(tournament_id)
    .bind(user_discord_id)
    .bind(user.discord_name.as_deref())
    .bind(&rank.steam_id)
    .bind(&rank.rank)
    .bind(rank.rank_score)
    .execute(&mut *tx)
    .await
    .map_err(|err| map_unique_conflict(err, "Du bist bereits für dieses Turnier angemeldet"))?;

    if creates_solo_team {
        let resolved_name =
            helpers::sanitize_discord_name(user.discord_name.as_deref(), Some(&user.discord_id))
                .unwrap_or_else(|| user.discord_id.clone());

        let mut team_name = truncate_chars(&resolved_name, 32);
        let base_key = team_name.to_lowercase();
        let mut name_key = base_key.clone();
        let mut suffix = 1;
        loop {
            let exists: Option<(i64,)> = sqlx::query_as(
                r#"SELECT id FROM turnier."teams" WHERE tournament_id = $1 AND name_key = $2"#,
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
            r#"INSERT INTO turnier."teams"
             (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status)
             VALUES ($1, $2, $3, $4, now(), 'open') RETURNING id"#,
        )
        .bind(tournament_id)
        .bind(&team_name)
        .bind(&name_key)
        .bind(user_discord_id)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            r#"INSERT INTO turnier."team_members"
             (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
             VALUES ($1, $2, $3, $4, $5, $6, 'captain', now())"#,
        )
        .bind(team_id)
        .bind(user_discord_id)
        .bind(&resolved_name)
        .bind(&rank.steam_id)
        .bind(&rank.rank)
        .bind(rank.rank_score)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"UPDATE turnier."tournament_signups" SET team_id = $1
             WHERE tournament_id = $2 AND discord_id = $3"#,
        )
        .bind(team_id)
        .bind(tournament_id)
        .bind(user_discord_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(json!({ "status": "angemeldet", "tournament_id": tournament_id })),
    ))
}

/// `DELETE /api/tournaments/{tournament_id}/signup`: Solo-Anmeldung zurückziehen.
async fn cancel_solo_signup(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let user_discord_id = db::parse_participant_id(&user.discord_id)?;

    let mut tx = pool.begin().await?;
    let status = helpers::lock_tournament_status(&mut tx, tournament_id).await?;
    if !helpers::registration_window_open(&status) {
        return Err(WebError::bad_request(
            "Austragen ist nur bis zum Ende des Check-ins möglich",
        ));
    }
    let signup: Option<(Option<i64>,)> = sqlx::query_as(
        r#"SELECT team_id FROM turnier."tournament_signups"
         WHERE tournament_id = $1 AND discord_id = $2 FOR UPDATE"#,
    )
    .bind(tournament_id)
    .bind(user_discord_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((team_id,)) = signup else {
        return Err(WebError::not_found("Keine Anmeldung gefunden"));
    };
    if team_id.is_some() {
        return Err(WebError::bad_request(
            "Du bist bereits in einem Team. Verlasse zuerst das Team",
        ));
    }

    helpers::remove_signup_and_checkin(&mut tx, tournament_id, user_discord_id).await?;
    helpers::audit(
        &mut *tx,
        "signup_withdraw",
        Some(&user.discord_id),
        &json!({ "tournament_id": tournament_id, "discord_id": user.discord_id, "status": status }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(
        json!({ "status": "abgemeldet", "tournament_id": tournament_id }),
    ))
}

/// `POST /api/tournaments/{tournament_id}/checkin`: Spieler-Check-in.
/// Nur bei `status = 'checkin'`. Erzeugt fehlenden Signup für Team-Mitglieder.
async fn checkin_player(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let user_discord_id = db::parse_participant_id(&user.discord_id)?;

    let mut tx = pool.begin().await?;
    let status = helpers::lock_tournament_status(&mut tx, tournament_id).await?;
    if status != "checkin" {
        return Err(WebError::bad_request("Check-in ist aktuell nicht geöffnet"));
    }

    if !helpers::ensure_checkin_signup(
        &mut tx,
        tournament_id,
        user_discord_id,
        user.discord_name.as_deref(),
    )
    .await?
    {
        return Err(WebError::forbidden(
            "Du bist für dieses Turnier nicht angemeldet",
        ));
    }
    let already_checked_in =
        !helpers::insert_checkin(&mut tx, tournament_id, user_discord_id).await?;

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

    Ok(Json(
        json!({ "checked_in": true, "already_checked_in": already_checked_in }),
    ))
}

/// Kürzt einen String auf höchstens `max` Zeichen (Codepoints), wie das
/// Python-Slicing `name[:32]`.
fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
