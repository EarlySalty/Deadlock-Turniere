//! Team- und Teilnehmer-Verwaltung: Teams anlegen/umbenennen/löschen,
//! Recruiting-Status, Bewerbungen listen/annehmen/ablehnen, Captain wechseln,
//! Mitglieder entfernen/verschieben/direkt hinzufügen, Solo-Signups zuweisen/löschen.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Row, Sqlite, Transaction};

use tb_core::{ApplicationStatus, RecruitmentStatus, Team, TeamApplication, TournamentSignup};

use crate::error::{WebError, WebResult};
use crate::extract::{AdminUser, ModUser};
use crate::state::AppState;

use super::helpers::{
    audit, count_team_members, ensure_participant_management_allowed, ensure_team_has_capacity,
    ensure_team_not_locked, load_team_application_or_404, load_team_or_404, load_tournament_or_404,
    reassign_or_clear_captain, upsert_signup_for_team, upsert_signup_from_member, MemberSnapshot,
};
use super::loaders::load_team_detail;

/// Router der Team-/Teilnehmer-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/tournaments/{tournament_id}/teams", post(create_team))
        .route("/api/admin/tournaments/{tournament_id}/teams/{team_id}", put(rename_team).delete(delete_team))
        .route("/api/admin/tournaments/{tournament_id}/teams/{team_id}/recruiting", patch(update_recruiting))
        .route("/api/admin/tournaments/{tournament_id}/teams/{team_id}/applications", get(list_applications))
        .route(
            "/api/admin/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept",
            post(accept_application),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject",
            post(reject_application),
        )
        .route("/api/admin/tournaments/{tournament_id}/teams/{team_id}/captain", put(change_captain))
        .route(
            "/api/admin/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}",
            delete(remove_member),
        )
        .route("/api/admin/tournaments/{tournament_id}/teams/{team_id}/members/move", post(move_member))
        .route("/api/admin/tournaments/{tournament_id}/teams/{team_id}/signups/assign", post(assign_signup))
        .route("/api/admin/tournaments/{tournament_id}/teams/{team_id}/add-member", post(add_member))
        .route("/api/admin/tournaments/{tournament_id}/signups/{signup_id}", delete(delete_signup))
}

/// Body mit einem `name`-Feld (create/rename).
#[derive(Debug, Deserialize)]
struct NameBody {
    #[serde(default)]
    name: Option<String>,
}

/// Validiert einen Team-Namen (2–32 Zeichen nach Trim).
fn validate_team_name(raw: Option<&str>) -> WebResult<String> {
    let name = raw.unwrap_or("").trim().to_string();
    let len = name.chars().count();
    if !(2..=32).contains(&len) {
        return Err(WebError::bad_request("Team-Name muss zwischen 2 und 32 Zeichen lang sein"));
    }
    Ok(name)
}

/// Bestimmt die Rolle eines neuen Mitglieds (`captain`, wenn Team leer oder ohne
/// Captain). Portiert die 4× duplizierte Heuristik.
fn member_role(member_count: i64, captain_discord_id: &str) -> &'static str {
    if member_count == 0 || captain_discord_id.is_empty() {
        "captain"
    } else {
        "member"
    }
}

/// `POST /api/admin/tournaments/{id}/teams` — leeres Team anlegen (201).
async fn create_team(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
    Json(body): Json<NameBody>,
) -> WebResult<(StatusCode, Json<Team>)> {
    let name = validate_team_name(body.name.as_deref())?;

    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    let name_key = tb_tournament::name_key(&name);

    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM teams WHERE tournament_id = ? AND name_key = ?")
            .bind(tournament_id)
            .bind(&name_key)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_some() {
        return Err(WebError::conflict("Ein Team mit diesem Namen existiert bereits"));
    }

    let team_id: i64 = sqlx::query(
        "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, '') RETURNING id",
    )
    .bind(tournament_id)
    .bind(&name)
    .bind(&name_key)
    .fetch_one(&mut *tx)
    .await?
    .get("id");

    audit(
        &mut *tx,
        "team_create_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_id": team_id, "name": name }),
    )
    .await?;
    tx.commit().await?;

    let team = load_team_detail(&state, team_id).await?;
    Ok((StatusCode::CREATED, Json(team)))
}

/// `PUT /api/admin/tournaments/{id}/teams/{team_id}` — Team umbenennen.
async fn rename_team(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
    Json(body): Json<NameBody>,
) -> WebResult<Json<Team>> {
    let name = validate_team_name(body.name.as_deref())?;

    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    load_team_or_404(&mut *tx, tournament_id, team_id).await?;
    let name_key = tb_tournament::name_key(&name);

    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM teams WHERE tournament_id = ? AND name_key = ? AND id != ?",
    )
    .bind(tournament_id)
    .bind(&name_key)
    .bind(team_id)
    .fetch_optional(&mut *tx)
    .await?;
    if exists.is_some() {
        return Err(WebError::conflict("Ein Team mit diesem Namen existiert bereits"));
    }

    sqlx::query("UPDATE teams SET name = ?, name_key = ? WHERE id = ?")
        .bind(&name)
        .bind(&name_key)
        .bind(team_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut *tx,
        "team_rename_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_id": team_id, "name": name }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(load_team_detail(&state, team_id).await?))
}

/// Body von `recruiting` (`recruitment_status` Pflicht).
#[derive(Debug, Deserialize)]
struct RecruitingBody {
    #[serde(default)]
    recruitment_status: Option<String>,
}

/// `PATCH .../teams/{team_id}/recruiting` — Recruiting-Status setzen.
async fn update_recruiting(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
    Json(body): Json<RecruitingBody>,
) -> WebResult<Json<Value>> {
    let raw = body
        .recruitment_status
        .as_deref()
        .ok_or_else(|| WebError::bad_request("recruitment_status ist erforderlich"))?;
    let status: RecruitmentStatus = serde_json::from_value(json!(raw))
        .map_err(|_| WebError::bad_request("Ungültiger recruitment_status"))?;
    let status_str = match status {
        RecruitmentStatus::Open => "open",
        RecruitmentStatus::Application => "application",
        RecruitmentStatus::Closed => "closed",
    };

    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    load_team_or_404(&mut *tx, tournament_id, team_id).await?;

    sqlx::query("UPDATE teams SET recruitment_status = ? WHERE id = ?")
        .bind(status_str)
        .bind(team_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut *tx,
        "team_recruitment_status_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_id": team_id, "recruitment_status": status_str }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "status": "ok", "team_id": team_id, "recruitment_status": status_str })))
}

/// `GET .../teams/{team_id}/applications` — Bewerbungen eines Teams listen.
async fn list_applications(
    State(state): State<AppState>,
    _mod: ModUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
) -> WebResult<Json<Vec<TeamApplication>>> {
    let tournament = load_tournament_or_404(&state.pool, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    load_team_or_404(&state.pool, tournament_id, team_id).await?;

    #[derive(sqlx::FromRow)]
    struct AppRow {
        id: i64,
        team_id: i64,
        discord_name: String,
        status: String,
        created_at: String,
    }
    let rows: Vec<AppRow> = sqlx::query_as(
        "SELECT id, team_id, discord_name, status, created_at FROM team_applications \
         WHERE team_id = ? ORDER BY created_at DESC, id DESC",
    )
    .bind(team_id)
    .fetch_all(&state.pool)
    .await?;
    let mut apps = Vec::with_capacity(rows.len());
    for r in rows {
        let status: ApplicationStatus = serde_json::from_value(json!(r.status))
            .map_err(|_| WebError::internal("Ungültiger Bewerbungsstatus in der DB"))?;
        apps.push(TeamApplication {
            id: r.id,
            team_id: r.team_id,
            discord_name: r.discord_name,
            status,
            created_at: r.created_at,
        });
    }
    Ok(Json(apps))
}

/// `POST .../applications/{app_id}/accept` — Bewerbung annehmen, Spieler ins Team.
async fn accept_application(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id, app_id)): Path<(i64, i64, i64)>,
) -> WebResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    let team_size: i64 = tournament.get("team_size");
    let target_team = load_team_or_404(&mut *tx, tournament_id, team_id).await?;
    let captain: String = target_team.get("captain_discord_id");
    let application = load_team_application_or_404(&mut *tx, team_id, app_id).await?;

    let app_status: String = application.get("status");
    if app_status != "pending" {
        return Err(WebError::bad_request("Nur ausstehende Bewerbungen können angenommen werden"));
    }
    let app_discord_id: String = application.get("discord_id");
    let app_discord_name: Option<String> = application.get("discord_name");

    ensure_team_has_capacity(&mut *tx, team_id, team_size).await?;

    // Bereits in einem Team dieses Turniers?
    let already: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM team_members tm JOIN teams t ON tm.team_id = t.id \
         WHERE t.tournament_id = ? AND tm.discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&app_discord_id)
    .fetch_optional(&mut *tx)
    .await?;
    if already.is_some() {
        return Err(WebError::conflict("Spieler ist bereits Mitglied in einem Team dieses Turniers"));
    }

    // Signup-Daten (falls vorhanden) übernehmen.
    let signup = sqlx::query(
        "SELECT discord_name, steam_id, rank, rank_score, team_id \
         FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&app_discord_id)
    .fetch_optional(&mut *tx)
    .await?;

    let mut effective_name = app_discord_name.clone();
    let mut steam_id: Option<String> = None;
    let mut rank: Option<String> = None;
    let mut rank_score: i64 = 0;
    if let Some(s) = &signup {
        let s_team_id: Option<i64> = s.get("team_id");
        // Bereits einem ANDEREN Team zugeordnet (None oder eigenes Team sind ok).
        if s_team_id.is_some() && s_team_id != Some(team_id) {
            return Err(WebError::conflict("Spieler ist bereits einem anderen Team zugeordnet"));
        }
        let s_name: Option<String> = s.get("discord_name");
        if let Some(n) = s_name {
            if !n.is_empty() {
                effective_name = Some(n);
            }
        }
        steam_id = s.get("steam_id");
        rank = s.get("rank");
        rank_score = s.get::<Option<i64>, _>("rank_score").unwrap_or(0);
    }

    let count = count_team_members(&mut *tx, team_id).await?;
    let role = member_role(count, &captain);

    insert_member(&mut tx, team_id, &app_discord_id, effective_name.as_deref(), steam_id.as_deref(), rank.as_deref(), rank_score, role).await?;
    upsert_signup_for_team(
        &mut tx,
        tournament_id,
        &app_discord_id,
        effective_name.as_deref(),
        steam_id.as_deref(),
        rank.as_deref(),
        rank_score,
        team_id,
    )
    .await?;
    if role == "captain" {
        set_captain(&mut tx, team_id, &app_discord_id).await?;
    }
    sqlx::query("UPDATE team_applications SET status = ? WHERE id = ?")
        .bind("accepted")
        .bind(app_id)
        .execute(&mut *tx)
        .await?;

    audit(
        &mut *tx,
        "team_application_accept_admin",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "team_id": team_id,
            "application_id": app_id,
            "discord_id": app_discord_id,
        }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(json!({
        "status": ApplicationStatus::Accepted,
        "application_id": app_id,
        "team_id": team_id,
    })))
}

/// `POST .../applications/{app_id}/reject` — Bewerbung ablehnen.
async fn reject_application(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id, app_id)): Path<(i64, i64, i64)>,
) -> WebResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    load_team_or_404(&mut *tx, tournament_id, team_id).await?;
    let application = load_team_application_or_404(&mut *tx, team_id, app_id).await?;
    let app_status: String = application.get("status");
    if app_status != "pending" {
        return Err(WebError::bad_request("Nur ausstehende Bewerbungen können abgelehnt werden"));
    }
    let app_discord_id: String = application.get("discord_id");

    sqlx::query("UPDATE team_applications SET status = ? WHERE id = ?")
        .bind("rejected")
        .bind(app_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut *tx,
        "team_application_reject_admin",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "team_id": team_id,
            "application_id": app_id,
            "discord_id": app_discord_id,
        }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(json!({
        "status": ApplicationStatus::Rejected,
        "application_id": app_id,
        "team_id": team_id,
    })))
}

/// `DELETE .../teams/{team_id}` — Team löschen (nur ohne Turnier-Historie).
async fn delete_team(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
) -> WebResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    let team = load_team_or_404(&mut *tx, tournament_id, team_id).await?;
    let team_name: String = team.get("name");
    ensure_team_not_locked(&mut tx, team_id).await?;

    // Mitglieder als Solo-Signups zurücklegen.
    let members = sqlx::query(
        "SELECT discord_id, discord_name, steam_id, rank, rank_score FROM team_members \
         WHERE team_id = ? ORDER BY joined_at",
    )
    .bind(team_id)
    .fetch_all(&mut *tx)
    .await?;
    for m in &members {
        let snap = MemberSnapshot {
            discord_id: m.get("discord_id"),
            discord_name: m.get("discord_name"),
            steam_id: m.get("steam_id"),
            rank: m.get("rank"),
            rank_score: m.get::<Option<i64>, _>("rank_score").unwrap_or(0),
        };
        upsert_signup_from_member(&mut tx, tournament_id, &snap).await?;
    }

    sqlx::query("UPDATE tournament_signups SET team_id = NULL WHERE tournament_id = ? AND team_id = ?")
        .bind(tournament_id)
        .bind(team_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM team_members WHERE team_id = ?")
        .bind(team_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM teams WHERE id = ?")
        .bind(team_id)
        .execute(&mut *tx)
        .await?;

    audit(
        &mut *tx,
        "team_delete_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_id": team_id, "name": team_name }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "status": "gelöscht", "team_id": team_id })))
}

/// Body mit `discord_id` (captain wechseln).
#[derive(Debug, Deserialize)]
struct DiscordIdBody {
    #[serde(default)]
    discord_id: Option<String>,
}

/// `PUT .../teams/{team_id}/captain` — Captain innerhalb des Teams wechseln.
async fn change_captain(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
    Json(body): Json<DiscordIdBody>,
) -> WebResult<Json<Team>> {
    let discord_id = body.discord_id.unwrap_or_default().trim().to_string();
    if discord_id.is_empty() {
        return Err(WebError::bad_request("discord_id ist erforderlich"));
    }

    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    load_team_or_404(&mut *tx, tournament_id, team_id).await?;

    let is_member: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM team_members WHERE team_id = ? AND discord_id = ?")
            .bind(team_id)
            .bind(&discord_id)
            .fetch_optional(&mut *tx)
            .await?;
    if is_member.is_none() {
        return Err(WebError::not_found("Mitglied nicht im Team gefunden"));
    }

    sqlx::query("UPDATE team_members SET role = 'member' WHERE team_id = ?")
        .bind(team_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE team_members SET role = 'captain' WHERE team_id = ? AND discord_id = ?")
        .bind(team_id)
        .bind(&discord_id)
        .execute(&mut *tx)
        .await?;
    set_captain(&mut tx, team_id, &discord_id).await?;
    audit(
        &mut *tx,
        "team_change_captain_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_id": team_id, "discord_id": discord_id }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(load_team_detail(&state, team_id).await?))
}

/// `DELETE .../teams/{team_id}/members/{discord_id}` — Mitglied entfernen.
async fn remove_member(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id, discord_id)): Path<(i64, i64, String)>,
) -> WebResult<Json<Team>> {
    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    let team = load_team_or_404(&mut *tx, tournament_id, team_id).await?;
    let captain: String = team.get("captain_discord_id");

    let member = sqlx::query("SELECT * FROM team_members WHERE team_id = ? AND discord_id = ?")
        .bind(team_id)
        .bind(&discord_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| WebError::not_found("Mitglied nicht gefunden"))?;

    let snap = MemberSnapshot {
        discord_id: member.get("discord_id"),
        discord_name: member.get("discord_name"),
        steam_id: member.get("steam_id"),
        rank: member.get("rank"),
        rank_score: member.get::<Option<i64>, _>("rank_score").unwrap_or(0),
    };
    upsert_signup_from_member(&mut tx, tournament_id, &snap).await?;
    sqlx::query("DELETE FROM team_members WHERE team_id = ? AND discord_id = ?")
        .bind(team_id)
        .bind(&discord_id)
        .execute(&mut *tx)
        .await?;
    if captain == discord_id {
        reassign_or_clear_captain(&mut tx, team_id).await?;
    }

    audit(
        &mut *tx,
        "team_remove_member_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_id": team_id, "discord_id": discord_id }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(load_team_detail(&state, team_id).await?))
}

/// Body von `members/move` (`from_team_id` + `discord_id`).
#[derive(Debug, Deserialize)]
struct MoveBody {
    #[serde(default)]
    from_team_id: Option<i64>,
    #[serde(default)]
    discord_id: Option<String>,
}

/// `POST .../teams/{team_id}/members/move` — Mitglied zwischen Teams verschieben.
async fn move_member(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
    Json(body): Json<MoveBody>,
) -> WebResult<Json<Team>> {
    let discord_id = body.discord_id.unwrap_or_default().trim().to_string();
    let Some(from_team_id) = body.from_team_id else {
        return Err(WebError::bad_request("from_team_id und discord_id sind erforderlich"));
    };
    if discord_id.is_empty() {
        return Err(WebError::bad_request("from_team_id und discord_id sind erforderlich"));
    }
    if from_team_id == team_id {
        return Err(WebError::bad_request("Quelle und Ziel dürfen nicht identisch sein"));
    }

    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    let team_size: i64 = tournament.get("team_size");
    let source_team = load_team_or_404(&mut *tx, tournament_id, from_team_id).await?;
    let target_team = load_team_or_404(&mut *tx, tournament_id, team_id).await?;
    ensure_team_has_capacity(&mut *tx, team_id, team_size).await?;

    let in_source: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM team_members WHERE team_id = ? AND discord_id = ?")
            .bind(from_team_id)
            .bind(&discord_id)
            .fetch_optional(&mut *tx)
            .await?;
    if in_source.is_none() {
        return Err(WebError::not_found("Mitglied nicht im Quell-Team gefunden"));
    }

    let in_target: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM team_members WHERE team_id = ? AND discord_id = ?")
            .bind(team_id)
            .bind(&discord_id)
            .fetch_optional(&mut *tx)
            .await?;
    if in_target.is_some() {
        return Err(WebError::conflict("Spieler ist bereits im Ziel-Team"));
    }

    let count = count_team_members(&mut *tx, team_id).await?;
    let target_captain: String = target_team.get("captain_discord_id");
    let new_role = member_role(count, &target_captain);

    sqlx::query("UPDATE team_members SET team_id = ?, role = ? WHERE team_id = ? AND discord_id = ?")
        .bind(team_id)
        .bind(new_role)
        .bind(from_team_id)
        .bind(&discord_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE tournament_signups SET team_id = ? WHERE tournament_id = ? AND discord_id = ?")
        .bind(team_id)
        .bind(tournament_id)
        .bind(&discord_id)
        .execute(&mut *tx)
        .await?;
    if new_role == "captain" {
        set_captain(&mut tx, team_id, &discord_id).await?;
    }
    let source_captain: String = source_team.get("captain_discord_id");
    if source_captain == discord_id {
        reassign_or_clear_captain(&mut tx, from_team_id).await?;
    }

    audit(
        &mut *tx,
        "team_move_member_admin",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "discord_id": discord_id,
            "from_team_id": from_team_id,
            "to_team_id": team_id,
        }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(load_team_detail(&state, team_id).await?))
}

/// Body von `signups/assign` (`signup_id`).
#[derive(Debug, Deserialize)]
struct AssignBody {
    #[serde(default)]
    signup_id: Option<i64>,
}

/// `POST .../teams/{team_id}/signups/assign` — Solo-Signup einem Team zuweisen.
async fn assign_signup(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
    Json(body): Json<AssignBody>,
) -> WebResult<Json<Team>> {
    let Some(signup_id) = body.signup_id else {
        return Err(WebError::bad_request("signup_id ist erforderlich"));
    };

    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;
    let team_size: i64 = tournament.get("team_size");
    let target_team = load_team_or_404(&mut *tx, tournament_id, team_id).await?;
    let captain: String = target_team.get("captain_discord_id");
    ensure_team_has_capacity(&mut *tx, team_id, team_size).await?;

    let signup = sqlx::query("SELECT * FROM tournament_signups WHERE id = ? AND tournament_id = ?")
        .bind(signup_id)
        .bind(tournament_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| WebError::not_found("Signup nicht gefunden"))?;
    let s_team_id: Option<i64> = signup.get("team_id");
    if s_team_id.is_some() {
        return Err(WebError::bad_request("Signup ist bereits einem Team zugewiesen"));
    }
    let s_discord_id: String = signup.get("discord_id");

    let already: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM team_members tm JOIN teams t ON tm.team_id = t.id \
         WHERE t.tournament_id = ? AND tm.discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&s_discord_id)
    .fetch_optional(&mut *tx)
    .await?;
    if already.is_some() {
        return Err(WebError::conflict("Spieler ist bereits Mitglied in einem Team dieses Turniers"));
    }

    let count = count_team_members(&mut *tx, team_id).await?;
    let role = member_role(count, &captain);
    let s_name: Option<String> = signup.get("discord_name");
    let s_steam: Option<String> = signup.get("steam_id");
    let s_rank: Option<String> = signup.get("rank");
    let s_score: i64 = signup.get::<Option<i64>, _>("rank_score").unwrap_or(0);

    insert_member(&mut tx, team_id, &s_discord_id, s_name.as_deref(), s_steam.as_deref(), s_rank.as_deref(), s_score, role).await?;
    sqlx::query("UPDATE tournament_signups SET team_id = ? WHERE id = ?")
        .bind(team_id)
        .bind(signup_id)
        .execute(&mut *tx)
        .await?;
    if role == "captain" {
        set_captain(&mut tx, team_id, &s_discord_id).await?;
    }

    audit(
        &mut *tx,
        "team_assign_signup_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_id": team_id, "signup_id": signup_id }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(load_team_detail(&state, team_id).await?))
}

/// Body von `add-member` (`discord_id` + `discord_name`).
#[derive(Debug, Deserialize)]
struct AddMemberBody {
    #[serde(default)]
    discord_id: Option<String>,
    #[serde(default)]
    discord_name: Option<String>,
}

/// `POST .../teams/{team_id}/add-member` — Ersatzspieler direkt hinzufügen (Admin,
/// nur ab group_phase/bracket).
async fn add_member(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path((tournament_id, team_id)): Path<(i64, i64)>,
    Json(body): Json<AddMemberBody>,
) -> WebResult<Json<Team>> {
    let discord_id = body.discord_id.unwrap_or_default().trim().to_string();
    let discord_name = body.discord_name.unwrap_or_default().trim().to_string();
    if discord_id.is_empty() || discord_name.is_empty() {
        return Err(WebError::bad_request("discord_id und discord_name sind erforderlich"));
    }

    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    let status: String = tournament.get("status");
    if !matches!(status.as_str(), "group_phase" | "bracket") {
        return Err(WebError::bad_request(
            "Ersatzspieler können erst ab der Gruppenphase hinzugefügt werden",
        ));
    }
    let team_size: i64 = tournament.get("team_size");
    let target_team = load_team_or_404(&mut *tx, tournament_id, team_id).await?;
    let captain: String = target_team.get("captain_discord_id");
    ensure_team_has_capacity(&mut *tx, team_id, team_size).await?;

    let already: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM team_members tm JOIN teams t ON tm.team_id = t.id \
         WHERE t.tournament_id = ? AND tm.discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&discord_id)
    .fetch_optional(&mut *tx)
    .await?;
    if already.is_some() {
        return Err(WebError::conflict("Spieler ist bereits Mitglied in einem Team dieses Turniers"));
    }

    let existing_signup = sqlx::query(
        "SELECT discord_name, steam_id, rank, rank_score, team_id \
         FROM tournament_signups WHERE tournament_id = ? AND discord_id = ?",
    )
    .bind(tournament_id)
    .bind(&discord_id)
    .fetch_optional(&mut *tx)
    .await?;

    let mut steam_id: Option<String> = None;
    let mut rank: Option<String> = None;
    let mut rank_score: i64 = 0;
    let mut signup_name: Option<String> = None;
    if let Some(s) = &existing_signup {
        let s_team_id: Option<i64> = s.get("team_id");
        if s_team_id.is_some() && s_team_id != Some(team_id) {
            return Err(WebError::conflict("Spieler ist bereits einem anderen Team zugeordnet"));
        }
        steam_id = s.get("steam_id");
        rank = s.get("rank");
        rank_score = s.get::<Option<i64>, _>("rank_score").unwrap_or(0);
        signup_name = s.get("discord_name");
    }

    // effective_name: discord_name (immer gesetzt) hat Vorrang vor Signup-Name.
    let effective_name = if !discord_name.is_empty() {
        Some(discord_name.clone())
    } else {
        signup_name.or_else(|| Some(discord_id.clone()))
    };

    let count = count_team_members(&mut *tx, team_id).await?;
    let role = member_role(count, &captain);

    insert_member(&mut tx, team_id, &discord_id, effective_name.as_deref(), steam_id.as_deref(), rank.as_deref(), rank_score, role).await?;
    upsert_signup_for_team(
        &mut tx,
        tournament_id,
        &discord_id,
        effective_name.as_deref(),
        steam_id.as_deref(),
        rank.as_deref(),
        rank_score,
        team_id,
    )
    .await?;
    if role == "captain" {
        set_captain(&mut tx, team_id, &discord_id).await?;
    }

    audit(
        &mut *tx,
        "team_add_member_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_id": team_id, "discord_id": discord_id }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(load_team_detail(&state, team_id).await?))
}

/// `DELETE .../signups/{signup_id}` — nicht zugewiesenes Solo-Signup löschen.
///
/// Gibt das gelöschte Signup zurück (Befund admin_routes.py:2096 — 1:1 erhalten,
/// behavior-change: ein nicht mehr existierendes Objekt mit ID).
async fn delete_signup(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, signup_id)): Path<(i64, i64)>,
) -> WebResult<Json<TournamentSignup>> {
    let mut tx = state.pool.begin().await?;
    let tournament = load_tournament_or_404(&mut *tx, tournament_id).await?;
    ensure_participant_management_allowed(tournament.get::<String, _>("status").as_str())?;

    let signup = sqlx::query(
        "SELECT id, tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at \
         FROM tournament_signups WHERE id = ? AND tournament_id = ?",
    )
    .bind(signup_id)
    .bind(tournament_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| WebError::not_found("Signup nicht gefunden"))?;

    let s_team_id: Option<i64> = signup.get("team_id");
    if s_team_id.is_some() {
        return Err(WebError::bad_request("Nur nicht zugewiesene Solo-Signups können gelöscht werden"));
    }
    let s_discord_id: String = signup.get("discord_id");

    let dto = TournamentSignup {
        id: signup.get("id"),
        tournament_id: signup.get("tournament_id"),
        discord_id: s_discord_id.clone(),
        discord_name: signup.get("discord_name"),
        steam_id: signup.get("steam_id"),
        rank: signup.get("rank"),
        rank_score: signup.get::<Option<i64>, _>("rank_score").unwrap_or(0),
        team_id: s_team_id,
        signed_up_at: signup.get("signed_up_at"),
    };

    sqlx::query("DELETE FROM tournament_signups WHERE id = ?")
        .bind(signup_id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut *tx,
        "signup_delete_admin",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "signup_id": signup_id, "discord_id": s_discord_id }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(dto))
}

// --- gemeinsame Mutations-Helfer (innerhalb der Transaktion) ----------------

/// Fügt ein Mitglied mit gegebener Rolle ein.
#[allow(clippy::too_many_arguments)]
async fn insert_member(
    tx: &mut Transaction<'_, Sqlite>,
    team_id: i64,
    discord_id: &str,
    discord_name: Option<&str>,
    steam_id: Option<&str>,
    rank: Option<&str>,
    rank_score: i64,
    role: &str,
) -> WebResult<()> {
    sqlx::query(
        "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(team_id)
    .bind(discord_id)
    .bind(discord_name)
    .bind(steam_id)
    .bind(rank)
    .bind(rank_score)
    .bind(role)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Setzt den Captain eines Teams.
async fn set_captain(
    tx: &mut Transaction<'_, Sqlite>,
    team_id: i64,
    discord_id: &str,
) -> WebResult<()> {
    sqlx::query("UPDATE teams SET captain_discord_id = ? WHERE id = ?")
        .bind(discord_id)
        .bind(team_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
