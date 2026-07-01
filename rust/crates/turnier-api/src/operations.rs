//! Operations-Router — Off-Stream-Ergebnis-Selbstmeldung, Admin-Bestätigung,
//! Leitstand und Stream-Marker (portiert `tournament/operations_routes.py`).
//! Arbeitet ausschließlich auf Bracket-Matches.

use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use turnier_core::{MatchResultReport, MatchResultReportCreate};
use turnier_match::{ApplyBracketParams, MatchError};

use crate::db;
use crate::error::{WebError, WebResult};
use crate::extract::{AuthUser, ModUser};
use crate::state::AppState;

const OPEN_REPORT_STATUS: &str = "pending";
const FINISHED_MATCH_STATUSES: [&str; 3] = ["completed", "cancelled", "forfeit"];

/// Router der Operations-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/tournaments/{tournament_id}/matches/{match_id}/report-result",
            post(report_match_result),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/action-items",
            get(get_action_items),
        )
        .route(
            "/api/admin/result-reports/{report_id}/confirm",
            post(confirm_result_report),
        )
        .route(
            "/api/admin/result-reports/{report_id}/reject",
            post(reject_result_report),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/matches/{match_id}/stream",
            patch(set_match_stream_flag),
        )
}

/// Die für die Operationen relevanten Felder eines Bracket-Matches.
#[derive(sqlx::FromRow)]
struct BracketMatchRow {
    team1_id: Option<i64>,
    team2_id: Option<i64>,
    status: String,
}

/// Lädt ein Bracket-Match oder liefert 404.
async fn load_bracket_match(
    pool: &turnier_db::Pool,
    tournament_id: i64,
    match_id: i64,
) -> WebResult<BracketMatchRow> {
    sqlx::query_as::<_, BracketMatchRow>(
        r#"SELECT team1_id, team2_id, status FROM turnier."bracket_matches"
         WHERE id = $1 AND tournament_id = $2"#,
    )
    .bind(match_id)
    .bind(tournament_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| WebError::not_found("Match nicht gefunden"))
}

/// Schreibt einen Audit-Log-Eintrag (innerhalb der aktuellen Verbindung/Tx).
async fn audit(
    pool: &turnier_db::Pool,
    action: &str,
    user_id: &str,
    details: Value,
) -> WebResult<()> {
    sqlx::query(
        r#"INSERT INTO turnier."audit_log" (action, user_id, details, created_at)
         VALUES ($1, $2, $3, now())"#,
    )
    .bind(action)
    .bind(db::parse_actor_id(user_id)?)
    .bind(details)
    .execute(pool)
    .await?;
    Ok(())
}

/// Die Captain-Discord-IDs der angegebenen Teams.
async fn team_captains(pool: &turnier_db::Pool, team_ids: &[i64]) -> WebResult<Vec<String>> {
    let mut captains = Vec::new();
    for &team_id in team_ids {
        let row: Option<(i64,)> =
            sqlx::query_as(r#"SELECT captain_discord_id FROM turnier."teams" WHERE id = $1"#)
                .bind(team_id)
                .fetch_optional(pool)
                .await?;
        if let Some((captain,)) = row {
            captains.push(db::discord_id_to_string(captain));
        }
    }
    Ok(captains)
}

/// Die Discord-IDs aller Mitglieder eines Teams.
async fn team_member_ids(pool: &turnier_db::Pool, team_id: i64) -> WebResult<Vec<String>> {
    let rows: Vec<(i64,)> =
        sqlx::query_as(r#"SELECT discord_id FROM turnier."team_members" WHERE team_id = $1"#)
            .bind(team_id)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|r| db::discord_id_to_string(r.0))
        .collect())
}

/// Eine vollständige `match_result_reports`-Zeile.
#[derive(sqlx::FromRow)]
struct ReportRow {
    id: i64,
    match_type: String,
    match_id: i64,
    tournament_id: i64,
    reported_by: i64,
    winner_team_id: Option<i64>,
    deadlock_match_id: Option<String>,
    is_no_show: bool,
    no_show_team_id: Option<i64>,
    status: String,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    resolved_by: Option<i64>,
}

impl ReportRow {
    fn into_dto(self) -> MatchResultReport {
        MatchResultReport {
            id: self.id,
            match_type: self.match_type,
            match_id: self.match_id,
            tournament_id: self.tournament_id,
            reported_by: db::discord_id_to_string(self.reported_by),
            winner_team_id: self.winner_team_id,
            deadlock_match_id: self.deadlock_match_id,
            is_no_show: self.is_no_show,
            no_show_team_id: self.no_show_team_id,
            status: self.status,
            created_at: db::ts_to_string(self.created_at),
            resolved_at: self.resolved_at.map(db::ts_to_string),
            resolved_by: self.resolved_by.map(db::discord_id_to_string),
        }
    }
}

/// `POST /api/tournaments/{tournament_id}/matches/{match_id}/report-result` —
/// ein Captain (oder Mod) meldet Sieger/No-Show; bleibt `pending` bis Bestätigung.
async fn report_match_result(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
    Json(body): Json<MatchResultReportCreate>,
) -> WebResult<(axum::http::StatusCode, Json<MatchResultReport>)> {
    let pool = &state.pool;
    let user_discord_id = db::parse_discord_id(&user.discord_id)?;
    let m = load_bracket_match(pool, tournament_id, match_id).await?;

    if FINISHED_MATCH_STATUSES.contains(&m.status.as_str()) {
        return Err(WebError::bad_request(format!(
            "Match {match_id} ist bereits abgeschlossen ({})",
            m.status
        )));
    }
    let (Some(team1_id), Some(team2_id)) = (m.team1_id, m.team2_id) else {
        return Err(WebError::bad_request(
            "Match hat noch nicht beide Teams gesetzt",
        ));
    };

    let captains = team_captains(pool, &[team1_id, team2_id]).await?;
    let is_captain = captains.iter().any(|c| c == &user.discord_id);
    let is_mod = user.is_mod || user.is_admin;
    if !is_captain && !is_mod {
        return Err(WebError::forbidden(
            "Nur ein Team-Captain dieses Matches darf ein Ergebnis melden",
        ));
    }

    if body.is_no_show {
        if !matches!(body.no_show_team_id, Some(t) if t == team1_id || t == team2_id) {
            return Err(WebError::bad_request(
                "no_show_team_id muss eines der beiden Teams sein",
            ));
        }
    } else if !matches!(body.winner_team_id, Some(t) if t == team1_id || t == team2_id) {
        return Err(WebError::bad_request(
            "winner_team_id muss eines der beiden Teams sein",
        ));
    }

    // Vorherige offene Meldung desselben Melders ersetzen.
    sqlx::query(
        r#"DELETE FROM turnier."match_result_reports"
         WHERE match_type = 'bracket' AND match_id = $1 AND reported_by = $2 AND status = $3"#,
    )
    .bind(match_id)
    .bind(user_discord_id)
    .bind(OPEN_REPORT_STATUS)
    .execute(pool)
    .await?;

    let report_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO turnier."match_result_reports"
         (match_type, match_id, tournament_id, reported_by, winner_team_id,
          deadlock_match_id, is_no_show, no_show_team_id, status, created_at)
         VALUES ('bracket', $1, $2, $3, $4, $5, $6, $7, 'pending', now()) RETURNING id"#,
    )
    .bind(match_id)
    .bind(tournament_id)
    .bind(user_discord_id)
    .bind(body.winner_team_id)
    .bind(&body.deadlock_match_id)
    .bind(body.is_no_show)
    .bind(body.no_show_team_id)
    .fetch_one(pool)
    .await?;

    audit(
        pool,
        "match_result_reported",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "match_id": match_id,
            "report_id": report_id,
            "is_no_show": body.is_no_show,
        }),
    )
    .await?;

    let missing_ids = if body.is_no_show {
        match body.no_show_team_id {
            Some(team) => team_member_ids(pool, team).await?,
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let report_row: ReportRow =
        sqlx::query_as(r#"SELECT * FROM turnier."match_result_reports" WHERE id = $1"#)
            .bind(report_id)
            .fetch_one(pool)
            .await?;

    // No-Show-Hinweis-DM an das säumige Team (best-effort).
    if !missing_ids.is_empty() {
        if let Err(err) = state
            .notifier
            .notify_users(
                &missing_ids,
                turnier_discord::NotificationEvent::MatchStart,
                "Hey, dein Gegner wartet im Turnier auf dich — komm bitte in die Lobby, \
                 sonst wird das Match als Walkover gewertet.",
            )
            .await
        {
            tracing::error!(match_id, error = %err, "No-Show-Hinweis-DM fehlgeschlagen");
        }
    }

    Ok((axum::http::StatusCode::CREATED, Json(report_row.into_dto())))
}

/// `GET /api/admin/tournaments/{tournament_id}/action-items` — offene Meldungen
/// für den Admin-Leitstand inkl. No-Show-Schonfrist-Status.
async fn get_action_items(
    State(state): State<AppState>,
    _mod: ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;

    let grace: Option<(i64,)> =
        sqlx::query_as(r#"SELECT no_show_grace_minutes FROM turnier."tournaments" WHERE id = $1"#)
            .bind(tournament_id)
            .fetch_optional(pool)
            .await?;
    let Some((grace_minutes,)) = grace else {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    };

    let rows: Vec<ActionItemRow> = sqlx::query_as(
        r#"SELECT r.id, r.match_id, r.reported_by, r.winner_team_id, r.deadlock_match_id,
                r.is_no_show, r.no_show_team_id, r.created_at,
                bm.round AS match_round, bm.on_stream,
                t1.name AS team1_name, t2.name AS team2_name,
                tw.name AS winner_name, tn.name AS no_show_name
         FROM turnier."match_result_reports" r
         JOIN turnier."bracket_matches" bm ON bm.id = r.match_id
         LEFT JOIN turnier."teams" t1 ON t1.id = bm.team1_id
         LEFT JOIN turnier."teams" t2 ON t2.id = bm.team2_id
         LEFT JOIN turnier."teams" tw ON tw.id = r.winner_team_id
         LEFT JOIN turnier."teams" tn ON tn.id = r.no_show_team_id
         WHERE r.tournament_id = $1 AND r.match_type = 'bracket' AND r.status = 'pending'
         ORDER BY r.created_at"#,
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;

    let now = Utc::now();
    let items: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let is_no_show = row.is_no_show;
            let grace_expired =
                is_no_show && now >= row.created_at + Duration::minutes(grace_minutes);
            json!({
                "report_id": row.id,
                "match_id": row.match_id,
                "match_round": row.match_round,
                "on_stream": row.on_stream,
                "team1_name": row.team1_name,
                "team2_name": row.team2_name,
                "reported_by": db::discord_id_to_string(row.reported_by),
                "is_no_show": is_no_show,
                "winner_team_id": row.winner_team_id,
                "winner_name": row.winner_name,
                "no_show_team_id": row.no_show_team_id,
                "no_show_name": row.no_show_name,
                "deadlock_match_id": row.deadlock_match_id,
                "created_at": db::ts_to_string(row.created_at),
                "grace_minutes": grace_minutes,
                "grace_expired": grace_expired,
            })
        })
        .collect();

    Ok(Json(json!({
        "pending_reports": items,
        "no_show_grace_minutes": grace_minutes,
    })))
}

/// Zeile der Action-Items-Query.
#[derive(sqlx::FromRow)]
struct ActionItemRow {
    id: i64,
    match_id: i64,
    reported_by: i64,
    winner_team_id: Option<i64>,
    deadlock_match_id: Option<String>,
    is_no_show: bool,
    no_show_team_id: Option<i64>,
    created_at: DateTime<Utc>,
    match_round: i64,
    on_stream: bool,
    team1_name: Option<String>,
    team2_name: Option<String>,
    winner_name: Option<String>,
    no_show_name: Option<String>,
}

/// Lädt eine Meldung oder liefert 404.
async fn load_report(pool: &turnier_db::Pool, report_id: i64) -> WebResult<ReportRow> {
    sqlx::query_as::<_, ReportRow>(r#"SELECT * FROM turnier."match_result_reports" WHERE id = $1"#)
        .bind(report_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| WebError::not_found("Meldung nicht gefunden"))
}

/// `POST /api/admin/result-reports/{report_id}/confirm` — Meldung bestätigen und
/// das Match werten.
async fn confirm_result_report(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(report_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let report = load_report(pool, report_id).await?;
    if report.status != "pending" {
        return Err(WebError::bad_request(format!(
            "Meldung ist bereits {}",
            report.status
        )));
    }
    let m = load_bracket_match(pool, report.tournament_id, report.match_id).await?;
    let (team1_id, team2_id) = (m.team1_id, m.team2_id);

    let (winner_id, result_source, force) = if report.is_no_show {
        let winner = if report.no_show_team_id == team2_id {
            team1_id
        } else {
            team2_id
        };
        (winner, "no_show", true)
    } else {
        (report.winner_team_id, "self_report", false)
    };

    if !matches!((winner_id, team1_id, team2_id), (Some(w), t1, t2) if Some(w) == t1 || Some(w) == t2)
    {
        return Err(WebError::bad_request(
            "Sieger der Meldung passt nicht mehr zu den Teams des Matches",
        ));
    }
    let winner_id = winner_id.expect("oben gegen team1/team2 geprüft");

    // Match werten. Fehler-Mapping wie im Original (NotFound->404, sonst 400).
    let outcome = state
        .match_manager
        .apply_bracket_match_result(
            report.tournament_id,
            report.match_id,
            ApplyBracketParams {
                winner_id: Some(winner_id),
                source: result_source.to_string(),
                force,
                ..Default::default()
            },
        )
        .await
        .map_err(|err| match err {
            MatchError::NotFound(msg) => WebError::not_found(msg),
            other => WebError::bad_request(other.to_string()),
        })?;

    // Diese Meldung bestätigen, konkurrierende verwerfen, ggf. deadlock_match_id setzen.
    let actor_id = db::parse_actor_id(&user.discord_id)?;
    sqlx::query(
        r#"UPDATE turnier."match_result_reports"
         SET status = 'confirmed', resolved_at = now(), resolved_by = $1 WHERE id = $2"#,
    )
    .bind(actor_id)
    .bind(report_id)
    .execute(pool)
    .await?;
    sqlx::query(
        r#"UPDATE turnier."match_result_reports"
         SET status = 'rejected', resolved_at = now(), resolved_by = $1
         WHERE match_type = 'bracket' AND match_id = $2 AND status = 'pending' AND id != $3"#,
    )
    .bind(actor_id)
    .bind(report.match_id)
    .bind(report_id)
    .execute(pool)
    .await?;
    if let Some(deadlock_match_id) = &report.deadlock_match_id {
        sqlx::query(r#"UPDATE turnier."bracket_matches" SET deadlock_match_id = $1 WHERE id = $2"#)
            .bind(deadlock_match_id)
            .bind(report.match_id)
            .execute(pool)
            .await?;
    }
    audit(
        pool,
        "match_result_report_confirmed",
        &user.discord_id,
        json!({
            "report_id": report_id,
            "match_id": report.match_id,
            "winner_id": winner_id,
            "source": result_source,
        }),
    )
    .await?;

    Ok(Json(json!({
        "status": "ok",
        "report_id": report_id,
        "match_id": outcome.match_id,
        "winner_id": outcome.winner_id,
        "winning_team": outcome.winning_team,
        "source": result_source,
    })))
}

/// `POST /api/admin/result-reports/{report_id}/reject` — Meldung verwerfen.
async fn reject_result_report(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(report_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    let report = load_report(pool, report_id).await?;
    if report.status != "pending" {
        return Err(WebError::bad_request(format!(
            "Meldung ist bereits {}",
            report.status
        )));
    }
    let actor_id = db::parse_actor_id(&user.discord_id)?;
    sqlx::query(
        r#"UPDATE turnier."match_result_reports"
         SET status = 'rejected', resolved_at = now(), resolved_by = $1 WHERE id = $2"#,
    )
    .bind(actor_id)
    .bind(report_id)
    .execute(pool)
    .await?;
    audit(
        pool,
        "match_result_report_rejected",
        &user.discord_id,
        json!({ "report_id": report_id, "match_id": report.match_id }),
    )
    .await?;
    Ok(Json(json!({ "status": "ok", "report_id": report_id })))
}

/// Body von `set_match_stream_flag` (`{"on_stream": true}`).
#[derive(Debug, Deserialize)]
struct StreamFlagBody {
    on_stream: bool,
}

/// `PATCH /api/admin/tournaments/{tournament_id}/matches/{match_id}/stream` —
/// Stream-Marker eines Matches umschalten.
async fn set_match_stream_flag(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, match_id)): Path<(i64, i64)>,
    Json(body): Json<StreamFlagBody>,
) -> WebResult<Json<Value>> {
    let pool = &state.pool;
    load_bracket_match(pool, tournament_id, match_id).await?;
    sqlx::query(
        r#"UPDATE turnier."bracket_matches" SET on_stream = $1 WHERE id = $2 AND tournament_id = $3"#,
    )
        .bind(body.on_stream)
        .bind(match_id)
        .bind(tournament_id)
        .execute(pool)
        .await?;
    audit(
        pool,
        "match_stream_flag",
        &user.discord_id,
        json!({ "match_id": match_id, "on_stream": body.on_stream }),
    )
    .await?;
    Ok(Json(
        json!({ "status": "ok", "match_id": match_id, "on_stream": body.on_stream }),
    ))
}
