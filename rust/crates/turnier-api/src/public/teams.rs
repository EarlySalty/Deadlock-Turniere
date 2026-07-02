//! Team-Lifecycle: Erstellen, Beitreten (mit Auto-Austritt), Verlassen, Kicken,
//! Recruiting-Status setzen, Captain-Direkt-Einladung.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Postgres;

use turnier_core::{Team, TeamMember};
use turnier_discord::NotificationEvent;

use crate::db;
use crate::error::{map_unique_conflict, WebError, WebResult};
use crate::extract::AuthUser;
use crate::state::AppState;

use super::helpers::{self, RankInput};

/// Router der Team-Lifecycle-Routen.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tournaments/{tournament_id}/teams", post(create_team))
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/join",
            post(join_team),
        )
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/recruiting",
            patch(update_team_recruiting),
        )
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/leave",
            delete(leave_team),
        )
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}",
            delete(kick_team_member),
        )
        .route(
            "/api/tournaments/{tournament_id}/teams/{team_id}/invite/{target_discord_id}",
            post(invite_to_team),
        )
}

/// Body von `create_team`.
#[derive(Debug, Deserialize)]
struct CreateTeamBody {
    #[serde(default)]
    name: Option<String>,
}

/// `POST /api/tournaments/{tournament_id}/teams` — Team erstellen (201).
/// Der aktuelle User wird Captain.
async fn create_team(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(tournament_id): Path<i64>,
    Json(body): Json<CreateTeamBody>,
) -> WebResult<(StatusCode, Json<Team>)> {
    let name = body.name.unwrap_or_default().trim().to_string();
    let name_len = name.chars().count();
    if !(2..=32).contains(&name_len) {
        return Err(WebError::bad_request(
            "Team-Name muss zwischen 2 und 32 Zeichen lang sein",
        ));
    }
    let name_key = turnier_engine::name_key(&name);

    let pool = &state.pool;
    helpers::ensure_consent(pool, &user.discord_id).await?;
    let user_discord_id = db::parse_discord_id(&user.discord_id)?;

    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;

    // Name-Einzigartigkeit (casefold).
    let name_taken: Option<(i64,)> = sqlx::query_as(
        r#"SELECT id FROM turnier."teams" WHERE tournament_id = $1 AND name_key = $2"#,
    )
    .bind(tournament_id)
    .bind(&name_key)
    .fetch_optional(pool)
    .await?;
    if name_taken.is_some() {
        return Err(WebError::conflict(
            "Ein Team mit diesem Namen existiert bereits",
        ));
    }

    // Bereits in einem Team dieses Turniers?
    let already_in_team: Option<(i64,)> = sqlx::query_as(
        r#"SELECT tm.id FROM turnier."team_members" tm JOIN turnier."teams" t ON tm.team_id = t.id
         WHERE t.tournament_id = $1 AND tm.discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(user_discord_id)
    .fetch_optional(pool)
    .await?;
    if already_in_team.is_some() {
        return Err(WebError::conflict(
            "Du bist bereits in einem Team dieses Turniers",
        ));
    }

    let rank = helpers::load_rank_input(&state, &user.discord_id).await;

    let mut tx = pool.begin().await?;
    let team_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO turnier."teams"
         (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status)
         VALUES ($1, $2, $3, $4, now(), 'open') RETURNING id"#,
    )
    .bind(tournament_id)
    .bind(&name)
    .bind(&name_key)
    .bind(user_discord_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|err| map_unique_conflict(err, "Ein Team mit diesem Namen existiert bereits"))?;

    sqlx::query(
        r#"INSERT INTO turnier."team_members"
         (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'captain', now())"#,
    )
    .bind(team_id)
    .bind(user_discord_id)
    .bind(user.discord_name.as_deref())
    .bind(&rank.steam_id)
    .bind(&rank.rank)
    .bind(rank.rank_score)
    .execute(&mut *tx)
    .await?;

    helpers::upsert_signup(
        &mut tx,
        tournament_id,
        &user.discord_id,
        user.discord_name.as_deref(),
        &rank,
        Some(team_id),
    )
    .await?;

    tx.commit().await?;

    let team = helpers::load_team_response(&state, team_id).await?;
    Ok((StatusCode::CREATED, Json(team)))
}

/// `POST /api/tournaments/{tournament_id}/teams/{team_id}/join` — Team beitreten.
/// Tritt automatisch aus dem alten Team aus (Captain nur wenn allein).
async fn join_team(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
) -> WebResult<Json<TeamMember>> {
    let pool = &state.pool;
    helpers::ensure_consent(pool, &user.discord_id).await?;
    let user_discord_id = db::parse_discord_id(&user.discord_id)?;

    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;
    helpers::load_team_or_404(pool, tournament_id, team_id).await?;

    let rank = helpers::load_rank_input(&state, &user.discord_id).await;

    let mut tx = pool.begin().await?;

    // Team-Größe prüfen.
    helpers::ensure_team_has_capacity(&mut *tx, team_id, t.team_size).await?;

    // Bereits in einem Team dieses Turniers? → automatisch austreten.
    let existing: Option<(i64, i64, i64)> = sqlx::query_as(
        r#"SELECT tm.id, tm.team_id, t.captain_discord_id FROM turnier."team_members" tm
         JOIN turnier."teams" t ON tm.team_id = t.id
         WHERE t.tournament_id = $1 AND tm.discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(user_discord_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some((_, old_team_id, captain_discord_id)) = existing {
        let is_captain = user_discord_id == captain_discord_id;
        let old_member_count = helpers::count_team_members(&mut *tx, old_team_id).await?;

        if is_captain && old_member_count > 1 {
            return Err(WebError::bad_request(
                "Übergib zuerst die Captain-Rolle oder löse das Team auf",
            ));
        }

        if is_captain && old_member_count == 1 {
            // Mitglied-Daten VOR dem Löschen sichern.
            let old_captain: Option<OldMemberRow> = sqlx::query_as(
                r#"SELECT discord_name, steam_id, rank, rank_score
                 FROM turnier."team_members" WHERE team_id = $1 AND discord_id = $2"#,
            )
            .bind(old_team_id)
            .bind(user_discord_id)
            .fetch_optional(&mut *tx)
            .await?;

            sqlx::query(r#"DELETE FROM turnier."team_members" WHERE team_id = $1"#)
                .bind(old_team_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(r#"DELETE FROM turnier."team_applications" WHERE team_id = $1"#)
                .bind(old_team_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(r#"DELETE FROM turnier."team_invitations" WHERE team_id = $1"#)
                .bind(old_team_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(r#"DELETE FROM turnier."teams" WHERE id = $1"#)
                .bind(old_team_id)
                .execute(&mut *tx)
                .await?;

            reset_or_create_solo_signup(
                &mut tx,
                tournament_id,
                &user.discord_id,
                old_captain.as_ref(),
            )
            .await?;
        } else {
            // Normales Verlassen.
            sqlx::query(
                r#"DELETE FROM turnier."team_members" WHERE team_id = $1 AND discord_id = $2"#,
            )
            .bind(old_team_id)
            .bind(user_discord_id)
            .execute(&mut *tx)
            .await?;
            let has_signup: Option<(i64,)> = sqlx::query_as(
                r#"SELECT id FROM turnier."tournament_signups"
                 WHERE tournament_id = $1 AND discord_id = $2"#,
            )
            .bind(tournament_id)
            .bind(user_discord_id)
            .fetch_optional(&mut *tx)
            .await?;
            if has_signup.is_some() {
                sqlx::query(
                    r#"UPDATE turnier."tournament_signups" SET team_id = NULL
                     WHERE tournament_id = $1 AND discord_id = $2"#,
                )
                .bind(tournament_id)
                .bind(user_discord_id)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    // Mitglied hinzufügen (Insert direkt, danach lastrowid).
    let member_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO turnier."team_members"
         (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'member', now()) RETURNING id"#,
    )
    .bind(team_id)
    .bind(user_discord_id)
    .bind(user.discord_name.as_deref())
    .bind(&rank.steam_id)
    .bind(&rank.rank)
    .bind(rank.rank_score)
    .fetch_one(&mut *tx)
    .await?;

    helpers::upsert_signup(
        &mut tx,
        tournament_id,
        &user.discord_id,
        user.discord_name.as_deref(),
        &rank,
        Some(team_id),
    )
    .await?;

    let member: TeamMember = load_team_member_by_id(&mut tx, member_id).await?;

    tx.commit().await?;

    Ok(Json(member))
}

/// Body von `update_team_recruiting`.
#[derive(Debug, Deserialize)]
struct RecruitingBody {
    #[serde(default)]
    recruitment_status: Option<String>,
}

/// `PATCH /api/tournaments/{tournament_id}/teams/{team_id}/recruiting` —
/// Recruiting-Status setzen (Captain oder Mod).
async fn update_team_recruiting(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
    Json(body): Json<RecruitingBody>,
) -> WebResult<Json<Team>> {
    let raw = body.recruitment_status.unwrap_or_default();
    if !matches!(raw.as_str(), "open" | "application" | "closed") {
        return Err(WebError::bad_request("Ungültiger Recruiting-Status"));
    }

    let pool = &state.pool;
    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    helpers::ensure_registration_open(&t.status)?;
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;
    helpers::ensure_captain_or_mod(&user, &team.captain_discord_id)?;

    sqlx::query(r#"UPDATE turnier."teams" SET recruitment_status = $1 WHERE id = $2"#)
        .bind(&raw)
        .bind(team_id)
        .execute(pool)
        .await?;

    Ok(Json(helpers::load_team_response(&state, team_id).await?))
}

/// `DELETE /api/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}` —
/// Captain kickt ein Mitglied (nur `status = 'registration'`).
async fn kick_team_member(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id, discord_id)): Path<(i64, i64, String)>,
) -> WebResult<Json<Value>> {
    // Discord-ID-Regex ^\d{17,19}$ — BEWUSST 1:1 erhalten (Original-Smell
    // „behavior-change": inkonsistent zu _looks_like_discord_id 16–21).
    if !is_strict_discord_id(&discord_id) {
        return Err(WebError::bad_request("Ungültige Discord-ID"));
    }
    let target_discord_id = db::parse_discord_id(&discord_id)?;

    let pool = &state.pool;
    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    if t.status != "registration" {
        return Err(WebError::bad_request("Anmeldung ist nicht geöffnet"));
    }
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;

    if user.discord_id != team.captain_discord_id {
        return Err(WebError::forbidden(
            "Nur der Captain darf Mitglieder entfernen",
        ));
    }
    if discord_id == user.discord_id {
        return Err(WebError::bad_request("Du kannst dich nicht selbst kicken"));
    }

    let member: Option<OldMemberRow> = sqlx::query_as(
        r#"SELECT discord_name, steam_id, rank, rank_score
         FROM turnier."team_members" WHERE team_id = $1 AND discord_id = $2"#,
    )
    .bind(team_id)
    .bind(target_discord_id)
    .fetch_optional(pool)
    .await?;
    let Some(member) = member else {
        return Err(WebError::not_found("Mitglied nicht gefunden"));
    };

    let mut tx = pool.begin().await?;
    sqlx::query(r#"DELETE FROM turnier."team_members" WHERE team_id = $1 AND discord_id = $2"#)
        .bind(team_id)
        .bind(target_discord_id)
        .execute(&mut *tx)
        .await?;
    reset_or_create_solo_signup(&mut tx, tournament_id, &discord_id, Some(&member)).await?;
    tx.commit().await?;

    Ok(Json(json!({
        "status": "mitglied_entfernt",
        "discord_id": discord_id,
        "team_id": team_id,
    })))
}

/// `DELETE /api/tournaments/{tournament_id}/teams/{team_id}/leave` — Team verlassen.
/// Captain kann nur austreten, wenn er allein ist (→ Team-Auflösung).
async fn leave_team(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let user_discord_id = db::parse_discord_id(&user.discord_id)?;
    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    if t.status != "registration" {
        return Err(WebError::bad_request("Anmeldung ist nicht geöffnet"));
    }
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;

    let member: Option<OldMemberRow> = sqlx::query_as(
        r#"SELECT discord_name, steam_id, rank, rank_score
         FROM turnier."team_members" WHERE team_id = $1 AND discord_id = $2"#,
    )
    .bind(team_id)
    .bind(user_discord_id)
    .fetch_optional(pool)
    .await?;
    let Some(member) = member else {
        return Err(WebError::not_found("Du bist kein Mitglied dieses Teams"));
    };

    let mut tx = pool.begin().await?;
    let member_count = helpers::count_team_members(&mut *tx, team_id).await?;
    let is_captain = user.discord_id == team.captain_discord_id;

    if is_captain && member_count > 1 {
        return Err(WebError::bad_request(
            "Übergib zuerst die Captain-Rolle oder löse das Team auf",
        ));
    }

    if is_captain && member_count == 1 {
        sqlx::query(r#"DELETE FROM turnier."team_members" WHERE team_id = $1"#)
            .bind(team_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"DELETE FROM turnier."team_applications" WHERE team_id = $1"#)
            .bind(team_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"DELETE FROM turnier."team_invitations" WHERE team_id = $1"#)
            .bind(team_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"DELETE FROM turnier."teams" WHERE id = $1"#)
            .bind(team_id)
            .execute(&mut *tx)
            .await?;
        // Bei vorhandenem Signup: team_id zurücksetzen; sonst NEUEN Eintrag mit
        // user.discord_name (NICHT member.discord_name) — 1:1 zum Original.
        let has_signup: Option<(i64,)> = sqlx::query_as(
            r#"SELECT id FROM turnier."tournament_signups"
             WHERE tournament_id = $1 AND discord_id = $2"#,
        )
        .bind(tournament_id)
        .bind(user_discord_id)
        .fetch_optional(&mut *tx)
        .await?;
        if has_signup.is_some() {
            sqlx::query(
                r#"UPDATE turnier."tournament_signups" SET team_id = NULL
                 WHERE tournament_id = $1 AND discord_id = $2"#,
            )
            .bind(tournament_id)
            .bind(user_discord_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                r#"INSERT INTO turnier."tournament_signups"
                 (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NULL, now())"#,
            )
            .bind(tournament_id)
            .bind(user_discord_id)
            .bind(user.discord_name.as_deref())
            .bind(&member.steam_id)
            .bind(&member.rank)
            .bind(member.rank_score)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        return Ok(Json(
            json!({ "status": "team_aufgeloest", "team_id": team_id }),
        ));
    }

    // Normales Verlassen.
    sqlx::query(r#"DELETE FROM turnier."team_members" WHERE team_id = $1 AND discord_id = $2"#)
        .bind(team_id)
        .bind(user_discord_id)
        .execute(&mut *tx)
        .await?;
    reset_or_create_solo_signup(&mut tx, tournament_id, &user.discord_id, Some(&member)).await?;
    tx.commit().await?;

    Ok(Json(
        json!({ "status": "team_verlassen", "team_id": team_id }),
    ))
}

/// `POST /api/tournaments/{tournament_id}/teams/{team_id}/invite/{target_discord_id}`
/// — Captain fügt einen solo-angemeldeten Spieler SOFORT ins Team ein
/// (KEIN team_invitations-Eintrag — BEWUSST 1:1 erhalten, „needs-decision").
async fn invite_to_team(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, team_id, target_discord_id)): Path<(i64, i64, String)>,
) -> WebResult<Json<Team>> {
    if !is_strict_discord_id(&target_discord_id) {
        return Err(WebError::bad_request("Ungültige Discord-ID"));
    }
    let target_discord_id_i64 = db::parse_discord_id(&target_discord_id)?;

    let pool = &state.pool;
    let t = helpers::load_tournament_or_404(pool, tournament_id).await?;
    if t.status != "registration" {
        return Err(WebError::bad_request("Anmeldung ist nicht geöffnet"));
    }
    let team = helpers::load_team_or_404(pool, tournament_id, team_id).await?;

    if user.discord_id != team.captain_discord_id {
        return Err(WebError::forbidden("Nur der Captain darf Spieler einladen"));
    }

    // Spieler nicht bereits in einem Team.
    let in_team: Option<(i64,)> = sqlx::query_as(
        r#"SELECT tm.id FROM turnier."team_members" tm JOIN turnier."teams" t ON tm.team_id = t.id
         WHERE t.tournament_id = $1 AND tm.discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(target_discord_id_i64)
    .fetch_optional(pool)
    .await?;
    if in_team.is_some() {
        return Err(WebError::conflict("Spieler ist bereits in einem Team"));
    }

    // Ziel-Spieler ist solo angemeldet (team_id IS NULL).
    let solo: Option<SoloSignupRow> = sqlx::query_as(
        r#"SELECT steam_id, rank, rank_score FROM turnier."tournament_signups"
         WHERE tournament_id = $1 AND discord_id = $2 AND team_id IS NULL"#,
    )
    .bind(tournament_id)
    .bind(target_discord_id_i64)
    .fetch_optional(pool)
    .await?;
    let Some(solo) = solo else {
        return Err(WebError::bad_request(
            "Spieler ist nicht als Solo-Spieler angemeldet",
        ));
    };

    // discord_name aus früherer team_members-Mitgliedschaft — BEWUSST ad-hoc
    // (ohne ORDER BY/Fallback-Kette, „safe"-Smell 1:1 erhalten).
    let name_row: Option<(Option<String>,)> = sqlx::query_as(
        r#"SELECT discord_name FROM turnier."team_members"
         WHERE discord_id = $1 AND discord_name != '' LIMIT 1"#,
    )
    .bind(target_discord_id_i64)
    .fetch_optional(pool)
    .await?;
    let discord_name = name_row
        .and_then(|(n,)| n)
        .unwrap_or_else(|| target_discord_id.clone());

    let mut tx = pool.begin().await?;

    // Team-Größe prüfen (innerhalb der Tx).
    helpers::ensure_team_has_capacity(&mut *tx, team_id, t.team_size).await?;

    sqlx::query(
        r#"INSERT INTO turnier."team_members"
         (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'member', now())"#,
    )
    .bind(team_id)
    .bind(target_discord_id_i64)
    .bind(&discord_name)
    .bind(&solo.steam_id)
    .bind(&solo.rank)
    .bind(solo.rank_score)
    .execute(&mut *tx)
    .await?;

    let rank = RankInput {
        steam_id: solo.steam_id.clone(),
        rank: solo.rank.clone(),
        rank_score: solo.rank_score.unwrap_or(0),
    };
    helpers::upsert_signup(
        &mut tx,
        tournament_id,
        &target_discord_id,
        Some(&discord_name),
        &rank,
        Some(team_id),
    )
    .await?;

    tx.commit().await?;

    // Notify als Side-Effect nach Commit (Felder vorher kopiert; is_test unterdrückt).
    if !t.is_test {
        let message = format!("Du wurdest zu `{}` für `{}` eingeladen.", team.name, t.name);
        if let Err(err) = state
            .notifier
            .notify_users(
                std::slice::from_ref(&target_discord_id),
                NotificationEvent::TeamInvite,
                &message,
            )
            .await
        {
            tracing::error!(
                tournament_id, team_id, target = %target_discord_id, error = %err,
                "Team-Invite-Benachrichtigung fehlgeschlagen"
            );
        }
    }

    Ok(Json(helpers::load_team_response(&state, team_id).await?))
}

// ---------------------------------------------------------------------------
// Geteilte Helfer dieses Moduls
// ---------------------------------------------------------------------------

/// Member-Daten, die vor einem Löschen für den Solo-Signup gesichert werden.
#[derive(sqlx::FromRow)]
struct OldMemberRow {
    discord_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: Option<i64>,
}

/// Rangfelder eines Solo-Signups.
#[derive(sqlx::FromRow)]
struct SoloSignupRow {
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: Option<i64>,
}

/// Setzt `team_id` eines vorhandenen Solo-Signups auf NULL oder legt — falls
/// keiner existiert — einen neuen NULL-Team-Signup mit den gesicherten
/// Member-Daten an. Geteilte Auto-Leave-Routine (Befund „safe": Duplikation).
async fn reset_or_create_solo_signup(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    discord_id: &str,
    member: Option<&OldMemberRow>,
) -> WebResult<()> {
    let discord_id_i64 = db::parse_discord_id(discord_id)?;
    let existing: Option<(i64,)> = sqlx::query_as(
        r#"SELECT id FROM turnier."tournament_signups" WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(discord_id_i64)
    .fetch_optional(&mut **tx)
    .await?;
    if existing.is_some() {
        sqlx::query(
            r#"UPDATE turnier."tournament_signups" SET team_id = NULL
             WHERE tournament_id = $1 AND discord_id = $2"#,
        )
        .bind(tournament_id)
        .bind(discord_id_i64)
        .execute(&mut **tx)
        .await?;
    } else if let Some(member) = member {
        sqlx::query(
            r#"INSERT INTO turnier."tournament_signups"
             (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at)
             VALUES ($1, $2, $3, $4, $5, $6, NULL, now())"#,
        )
        .bind(tournament_id)
        .bind(discord_id_i64)
        .bind(&member.discord_name)
        .bind(&member.steam_id)
        .bind(&member.rank)
        .bind(member.rank_score)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Lädt EIN Team-Mitglied per ID als DTO (für die `join`-Response).
async fn load_team_member_by_id(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    member_id: i64,
) -> WebResult<TeamMember> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i64,
        team_id: i64,
        discord_id: i64,
        discord_name: Option<String>,
        steam_id: Option<String>,
        rank: Option<String>,
        rank_score: Option<i64>,
        role: turnier_core::TeamRole,
        joined_at: DateTime<Utc>,
    }
    let row: Row = sqlx::query_as(
        r#"SELECT id, team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at
         FROM turnier."team_members" WHERE id = $1"#,
    )
    .bind(member_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(TeamMember {
        id: row.id,
        team_id: row.team_id,
        discord_id: db::discord_id_to_string(row.discord_id),
        discord_name: row.discord_name,
        steam_id: row.steam_id,
        rank: row.rank,
        rank_score: row.rank_score.unwrap_or(0),
        role: row.role,
        joined_at: db::ts_to_string(row.joined_at),
    })
}

/// Strenge Discord-ID-Prüfung `^\d{17,19}$` (kick/invite — 1:1 zum Original).
fn is_strict_discord_id(value: &str) -> bool {
    let len = value.len();
    (17..=19).contains(&len) && value.chars().all(|c| c.is_ascii_digit())
}
