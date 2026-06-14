//! Einladungs- und Bewerbungs-Flow: Captain lädt per signup_id ein, User
//! akzeptiert/lehnt ab; Bewerbung (apply/list/accept/reject).

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use tb_core::{TeamApplication, TeamInvitation};
use tb_discord::NotificationEvent;

use crate::error::{WebError, WebResult};
use crate::extract::AuthUser;
use crate::state::AppState;

use super::helpers::{self, RankInput};

/// Router der Einladungs-/Bewerbungs-Routen.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/invite-by-signup/{signup_id}",
            post(invite_to_team_by_signup),
        )
        .route("/api/tournaments/{tournament_id}/my-invitations", get(get_my_invitations))
        .route(
            "/api/tournaments/{tournament_id}/invitations/{invite_id}/accept",
            post(accept_team_invitation),
        )
        .route(
            "/api/tournaments/{tournament_id}/invitations/{invite_id}/reject",
            post(reject_team_invitation),
        )
        .route("/api/tournaments/{tournament_id}/teams/{team_id}/apply", post(apply_to_team))
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/applications",
            get(get_team_applications),
        )
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept",
            post(accept_team_application),
        )
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject",
            post(reject_team_application),
        )
}

/// Eine `tournament_signups`-Zeile, die für eine Einladung gebraucht wird.
#[derive(sqlx::FromRow)]
struct SignupRow {
    discord_id: String,
    discord_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: Option<i64>,
    team_id: Option<i64>,
}

/// `POST .../invite-by-signup/{signup_id}` — Captain lädt einen Solo-Signup ein.
/// Respektiert invite_mode/-window und invite_auto_accept.
async fn invite_to_team_by_signup(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id, signup_id)): Path<(i64, i64, i64)>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;
    helpers::ensure_captain(&user, &team.captain_discord_id)?;
    helpers::ensure_team_has_capacity(pool, team_id, t.team_size).await?;

    let signup: Option<SignupRow> = sqlx::query_as(
        "SELECT discord_id, discord_name, steam_id, rank, rank_score, team_id \
         FROM tournament_signups WHERE id = ? AND tournament_id = ?",
    )
    .bind(signup_id)
    .bind(tournament_id)
    .fetch_optional(pool)
    .await?;
    let Some(signup) = signup else {
        return Err(WebError::not_found("Signup nicht gefunden"));
    };
    if signup.team_id.is_some() {
        return Err(WebError::conflict("Spieler ist bereits einem Team zugeordnet"));
    }

    helpers::ensure_user_not_in_tournament_team(pool, tournament_id, &signup.discord_id).await?;
    let expires_at = helpers::ensure_invites_enabled(&t)?;

    let auto_accept: bool = {
        let profile: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT invite_auto_accept FROM user_profiles WHERE discord_id = ?")
                .bind(&signup.discord_id)
                .fetch_optional(pool)
                .await?;
        matches!(profile, Some((Some(v),)) if v != 0)
    };

    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, status FROM team_invitations WHERE team_id = ? AND discord_id = ?",
    )
    .bind(team_id)
    .bind(&signup.discord_id)
    .fetch_optional(pool)
    .await?;

    let target_status = if auto_accept { "accepted" } else { "pending" };

    if let Some((_, ref status)) = existing {
        if status == "pending" && !auto_accept {
            return Err(WebError::conflict(
                "Für diesen Spieler existiert bereits eine offene Einladung",
            ));
        }
    }

    let mut tx = pool.begin().await?;

    let invite_id: i64 = if let Some((existing_id, _)) = existing {
        sqlx::query(
            "UPDATE team_invitations SET tournament_id = ?, signup_id = ?, status = ?, \
             created_at = datetime('now'), expires_at = ? WHERE id = ?",
        )
        .bind(tournament_id)
        .bind(signup_id)
        .bind(target_status)
        .bind(&expires_at)
        .bind(existing_id)
        .execute(&mut *tx)
        .await?;
        existing_id
    } else {
        sqlx::query_scalar(
            "INSERT INTO team_invitations \
             (tournament_id, team_id, discord_id, signup_id, status, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(tournament_id)
        .bind(team_id)
        .bind(&signup.discord_id)
        .bind(signup_id)
        .bind(target_status)
        .bind(&expires_at)
        .fetch_one(&mut *tx)
        .await?
    };

    if auto_accept {
        let rank = RankInput {
            steam_id: signup.steam_id.clone(),
            rank: signup.rank.clone(),
            rank_score: signup.rank_score.unwrap_or(0),
        };
        helpers::add_user_to_team(
            pool,
            &mut tx,
            tournament_id,
            team_id,
            &signup.discord_id,
            signup.discord_name.as_deref(),
            &rank,
        )
        .await?;
        sqlx::query("UPDATE team_invitations SET status = 'accepted' WHERE id = ?")
            .bind(invite_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(Json(json!({ "status": "auto_accepted" })));
    }

    tx.commit().await?;

    // Notify als Side-Effect nach Commit; is_test unterdrückt.
    if !t.is_test {
        let message = format!(
            "Du wurdest von `{}` für `{}` eingeladen.",
            team.name, t.name
        );
        if let Err(err) = state
            .notifier
            .notify_users(
                std::slice::from_ref(&signup.discord_id),
                NotificationEvent::TeamInvite,
                &message,
            )
            .await
        {
            tracing::error!(
                tournament_id, team_id, signup_id, error = %err,
                "Signup-Invite-Benachrichtigung fehlgeschlagen"
            );
        }
    }

    Ok(Json(json!({ "status": "invited" })))
}

/// `GET /api/tournaments/{tournament_id}/my-invitations` — offene Einladungen.
async fn get_my_invitations(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Vec<TeamInvitation>>> {
    let pool = &state.pool;
    helpers::load_tournament_or_404(pool, tournament_id).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        tournament_id: i64,
        team_id: i64,
        team_name: Option<String>,
        status: tb_core::InvitationStatus,
        created_at: String,
        expires_at: Option<String>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT ti.id, ti.tournament_id, ti.team_id, t.name AS team_name, \
                ti.status, ti.created_at, ti.expires_at \
         FROM team_invitations ti JOIN teams t ON t.id = ti.team_id \
         WHERE ti.tournament_id = ? AND ti.discord_id = ? AND ti.status = 'pending' \
         ORDER BY ti.created_at DESC",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_all(pool)
    .await?;

    let invitations = rows
        .into_iter()
        .map(|r| TeamInvitation {
            id: r.id,
            tournament_id: r.tournament_id,
            team_id: r.team_id,
            team_name: r.team_name,
            status: r.status,
            created_at: r.created_at,
            expires_at: r.expires_at,
        })
        .collect();
    Ok(Json(invitations))
}

/// Eine `team_invitations`-Zeile für accept.
#[derive(sqlx::FromRow)]
struct InvitationRow {
    discord_id: String,
    team_id: i64,
    status: String,
}

/// `POST .../invitations/{invite_id}/accept` — Einladung annehmen.
async fn accept_team_invitation(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, invite_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;

    let invitation: Option<InvitationRow> = sqlx::query_as(
        "SELECT discord_id, team_id, status \
         FROM team_invitations WHERE id = ? AND tournament_id = ?",
    )
    .bind(invite_id)
    .bind(tournament_id)
    .fetch_optional(pool)
    .await?;
    let Some(invitation) = invitation else {
        return Err(WebError::not_found("Einladung nicht gefunden"));
    };
    if invitation.discord_id != user.discord_id {
        return Err(WebError::forbidden("Diese Einladung gehört nicht zu dir"));
    }
    if invitation.status != "pending" {
        return Err(WebError::bad_request("Einladung ist nicht mehr offen"));
    }

    helpers::load_team_or_404(pool, tournament_id, invitation.team_id).await?;
    helpers::ensure_team_has_capacity(pool, invitation.team_id, t.team_size).await?;
    helpers::ensure_user_not_in_tournament_team(pool, tournament_id, &user.discord_id).await?;

    // Rangdaten aus vorhandenem Signup, sonst frisch vom Resolver.
    let signup: Option<SignupRow> = sqlx::query_as(
        "SELECT discord_id, discord_name, steam_id, rank, rank_score, team_id \
         FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&user.discord_id)
    .fetch_optional(pool)
    .await?;

    let (rank, discord_name) = if let Some(signup) = signup {
        // `user.discord_name or signup.discord_name` — leerer String fällt zurück
        // (Python-`or`-Semantik 1:1).
        let discord_name = user
            .discord_name
            .clone()
            .filter(|n| !n.is_empty())
            .or(signup.discord_name);
        (
            RankInput {
                steam_id: signup.steam_id,
                rank: signup.rank,
                rank_score: signup.rank_score.unwrap_or(0),
            },
            discord_name,
        )
    } else {
        (
            helpers::load_rank_input(&state, &user.discord_id).await,
            user.discord_name.clone(),
        )
    };

    let mut tx = pool.begin().await?;
    helpers::add_user_to_team(
        pool,
        &mut tx,
        tournament_id,
        invitation.team_id,
        &user.discord_id,
        discord_name.as_deref(),
        &rank,
    )
    .await?;
    sqlx::query("UPDATE team_invitations SET status = 'accepted' WHERE id = ?")
        .bind(invite_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Json(json!({ "status": "accepted", "invite_id": invite_id })))
}

/// `POST .../invitations/{invite_id}/reject` — Einladung ablehnen.
/// KEIN `_ensure_registration_open`-Gate (anders als accept — 1:1 erhalten).
async fn reject_team_invitation(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, invite_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    helpers::load_tournament_or_404(pool, tournament_id).await?;

    let invitation: Option<(String, String)> = sqlx::query_as(
        "SELECT discord_id, status FROM team_invitations WHERE id = ? AND tournament_id = ?",
    )
    .bind(invite_id)
    .bind(tournament_id)
    .fetch_optional(pool)
    .await?;
    let Some((discord_id, status)) = invitation else {
        return Err(WebError::not_found("Einladung nicht gefunden"));
    };
    if discord_id != user.discord_id {
        return Err(WebError::forbidden("Diese Einladung gehört nicht zu dir"));
    }
    if status != "pending" {
        return Err(WebError::bad_request("Einladung ist nicht mehr offen"));
    }

    sqlx::query("UPDATE team_invitations SET status = 'rejected' WHERE id = ?")
        .bind(invite_id)
        .execute(pool)
        .await?;

    Ok(Json(json!({ "status": "rejected", "invite_id": invite_id })))
}

/// `POST /api/tournaments/{tournament_id}/teams/{team_id}/apply` — auf Team
/// bewerben (nur `recruitment_status = 'application'`).
async fn apply_to_team(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;
    if team.recruitment_status != "application" {
        return Err(WebError::forbidden("Dieses Team nimmt aktuell keine Bewerbungen an"));
    }

    helpers::ensure_user_not_in_tournament_team(pool, tournament_id, &user.discord_id).await?;

    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, status FROM team_applications WHERE team_id = ? AND discord_id = ?",
    )
    .bind(team_id)
    .bind(&user.discord_id)
    .fetch_optional(pool)
    .await?;

    let discord_name =
        helpers::resolve_discord_name(pool, &user.discord_id, user.discord_name.as_deref()).await?;

    if let Some((_, ref status)) = existing {
        if status == "pending" {
            return Err(WebError::conflict(
                "Für dieses Team existiert bereits eine offene Bewerbung",
            ));
        }
    }

    if let Some((id, _)) = existing {
        sqlx::query(
            "UPDATE team_applications SET discord_name = ?, status = 'pending', \
             created_at = datetime('now') WHERE id = ?",
        )
        .bind(&discord_name)
        .bind(id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO team_applications (team_id, discord_id, discord_name, status, created_at) \
             VALUES (?, ?, ?, 'pending', datetime('now'))",
        )
        .bind(team_id)
        .bind(&user.discord_id)
        .bind(&discord_name)
        .execute(pool)
        .await?;
    }

    Ok(Json(json!({ "status": "applied", "team_id": team_id })))
}

/// `GET .../teams/{team_id}/applications` — Bewerbungen eines Teams (Captain|Mod).
async fn get_team_applications(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
) -> WebResult<Json<Vec<TeamApplication>>> {
    let pool = &state.pool;
    helpers::load_tournament_or_404(pool, tournament_id).await?;
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;
    helpers::ensure_captain_or_mod(&user, &team.captain_discord_id)?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        team_id: i64,
        discord_name: String,
        status: tb_core::ApplicationStatus,
        created_at: String,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, team_id, discord_name, status, created_at \
         FROM team_applications WHERE team_id = ? ORDER BY created_at DESC",
    )
    .bind(team_id)
    .fetch_all(pool)
    .await?;

    let applications = rows
        .into_iter()
        .map(|r| TeamApplication {
            id: r.id,
            team_id: r.team_id,
            discord_name: r.discord_name,
            status: r.status,
            created_at: r.created_at,
        })
        .collect();
    Ok(Json(applications))
}

/// Eine `team_applications`-Zeile für accept/reject.
#[derive(sqlx::FromRow)]
struct ApplicationRow {
    discord_id: String,
    discord_name: String,
    status: String,
}

/// `POST .../applications/{app_id}/accept` — Bewerbung annehmen (Captain|Mod).
async fn accept_team_application(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id, app_id)): Path<(i64, i64, i64)>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;
    helpers::ensure_captain_or_mod(&user, &team.captain_discord_id)?;

    let application: Option<ApplicationRow> = sqlx::query_as(
        "SELECT discord_id, discord_name, status FROM team_applications \
         WHERE id = ? AND team_id = ?",
    )
    .bind(app_id)
    .bind(team_id)
    .fetch_optional(pool)
    .await?;
    let Some(application) = application else {
        return Err(WebError::not_found("Bewerbung nicht gefunden"));
    };
    if application.status != "pending" {
        return Err(WebError::bad_request("Bewerbung ist nicht mehr offen"));
    }

    helpers::ensure_team_has_capacity(pool, team_id, t.team_size).await?;
    helpers::ensure_user_not_in_tournament_team(pool, tournament_id, &application.discord_id)
        .await?;

    // Rangdaten aus vorhandenem Signup, sonst frisch vom Resolver.
    let signup: Option<SignupRow> = sqlx::query_as(
        "SELECT discord_id, discord_name, steam_id, rank, rank_score, team_id \
         FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&application.discord_id)
    .fetch_optional(pool)
    .await?;

    let (rank, discord_name) = if let Some(signup) = signup {
        let discord_name = signup
            .discord_name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| application.discord_name.clone());
        (
            RankInput {
                steam_id: signup.steam_id,
                rank: signup.rank,
                rank_score: signup.rank_score.unwrap_or(0),
            },
            discord_name,
        )
    } else {
        (
            helpers::load_rank_input(&state, &application.discord_id).await,
            application.discord_name.clone(),
        )
    };

    let mut tx = pool.begin().await?;
    helpers::add_user_to_team(
        pool,
        &mut tx,
        tournament_id,
        team_id,
        &application.discord_id,
        Some(&discord_name),
        &rank,
    )
    .await?;
    sqlx::query("UPDATE team_applications SET status = 'accepted' WHERE id = ?")
        .bind(app_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Json(json!({ "status": "accepted", "application_id": app_id })))
}

/// `POST .../applications/{app_id}/reject` — Bewerbung ablehnen (Captain|Mod).
/// KEIN registration-Gate (anders als accept — 1:1 erhalten).
async fn reject_team_application(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id, app_id)): Path<(i64, i64, i64)>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    helpers::load_tournament_or_404(pool, tournament_id).await?;
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;
    helpers::ensure_captain_or_mod(&user, &team.captain_discord_id)?;

    let application: Option<(String,)> =
        sqlx::query_as("SELECT status FROM team_applications WHERE id = ? AND team_id = ?")
            .bind(app_id)
            .bind(team_id)
            .fetch_optional(pool)
            .await?;
    let Some((status,)) = application else {
        return Err(WebError::not_found("Bewerbung nicht gefunden"));
    };
    if status != "pending" {
        return Err(WebError::bad_request("Bewerbung ist nicht mehr offen"));
    }

    sqlx::query("UPDATE team_applications SET status = 'rejected' WHERE id = ?")
        .bind(app_id)
        .execute(pool)
        .await?;

    Ok(Json(json!({ "status": "rejected", "application_id": app_id })))
}
