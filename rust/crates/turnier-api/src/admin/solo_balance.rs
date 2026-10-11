use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Postgres, Row, Transaction};
use turnier_core::RankProfile;
use turnier_engine::balance::{balance_players, BalancePlayer, BalancedPlayer};

use crate::db;
use crate::error::{WebError, WebResult};
use crate::extract::AdminUser;
use crate::state::AppState;

use super::helpers::{audit, ensure_team_not_locked, upsert_signup_for_team};
use super::teams::{insert_member, set_captain};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/tournaments/{tournament_id}/solo-balance/preview",
            post(preview),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/solo-balance/apply",
            post(apply),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/solo-balance/waitlist",
            get(waitlist),
        )
}

#[derive(Deserialize)]
struct PreviewBody {
    team_count: Option<usize>,
}

#[derive(Deserialize)]
struct ApplyBody {
    team_count: usize,
    preview_token: String,
}

#[derive(Clone, Serialize, sqlx::FromRow)]
struct Signup {
    id: i64,
    discord_id: i64,
    discord_name: Option<String>,
    steam_id: Option<String>,
    team_id: Option<i64>,
    signed_up_at: DateTime<Utc>,
    priority: bool,
}

#[derive(Serialize)]
struct PlayerDto {
    signup_id: i64,
    discord_id: String,
    discord_name: Option<String>,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_score: i64,
    score: f64,
    priority: bool,
    used_median: bool,
}

#[derive(Serialize)]
struct TeamDto {
    name: String,
    players: Vec<PlayerDto>,
    score_sum: f64,
}

#[derive(Serialize)]
struct PreviewDto {
    teams: Vec<TeamDto>,
    waitlist: Vec<PlayerDto>,
    median_score: f64,
    gap: f64,
    team_count: usize,
    team_size: usize,
    preview_token: String,
}

struct Inputs {
    signups: Vec<Signup>,
    singleton_ids: Vec<i64>,
    team_size: usize,
    is_test: bool,
}

async fn refreshed_profiles(
    state: &AppState,
    tournament_id: i64,
) -> WebResult<HashMap<String, RankProfile>> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT discord_id FROM turnier.tournament_signups WHERE tournament_id = $1 ORDER BY signed_up_at, id")
        .bind(tournament_id).fetch_all(&state.pool).await?;
    state
        .rank_resolver
        .rank_profiles(
            &ids.into_iter()
                .map(db::discord_id_to_string)
                .collect::<Vec<_>>(),
        )
        .await
        .map_err(|_| {
            tracing::warn!(tournament_id, "Solo balance rank refresh failed");
            WebError::internal("Ränge konnten nicht geladen werden. Bitte erneut versuchen.")
        })
}

async fn preview(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(tournament_id): Path<i64>,
    Json(body): Json<PreviewBody>,
) -> WebResult<Json<PreviewDto>> {
    let profiles = refreshed_profiles(&state, tournament_id).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let inputs = load_inputs(&mut tx, tournament_id).await?;
    let dto = build_preview(&inputs, &profiles, body.team_count)?;
    tx.rollback().await?;
    Ok(Json(dto))
}

async fn load_inputs(conn: &mut PgConnection, tournament_id: i64) -> WebResult<Inputs> {
    let tournament =
        sqlx::query("SELECT status, team_size, is_test FROM turnier.tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))?;
    if tournament.get::<String, _>("status") != "checkin" {
        return Err(WebError::bad_request(
            "Teams können nur nach der Anmeldung während des Check-ins ausgeglichen werden.",
        ));
    }
    let team_size = usize::try_from(tournament.get::<i64, _>("team_size"))
        .map_err(|_| WebError::bad_request("Ungültige Teamgröße"))?;
    if team_size <= 1 {
        return Err(WebError::bad_request(
            "Der Teamausgleich benötigt eine Teamgröße größer als 1.",
        ));
    }
    let applied: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM turnier.solo_balance_runs WHERE tournament_id = $1)",
    )
    .bind(tournament_id)
    .fetch_one(&mut *conn)
    .await?;
    if applied {
        return Err(WebError::conflict(
            "Die ausgeglichenen Teams wurden bereits übernommen.",
        ));
    }
    let teams = sqlx::query("SELECT t.id, count(tm.id) AS members FROM turnier.teams t LEFT JOIN turnier.team_members tm ON tm.team_id = t.id WHERE t.tournament_id = $1 GROUP BY t.id ORDER BY t.id")
        .bind(tournament_id).fetch_all(&mut *conn).await?;
    if teams.iter().any(|team| team.get::<i64, _>("members") != 1) {
        return Err(WebError::conflict("Es existieren bereits Teams. Der Teamausgleich ist nur ohne Teams oder mit einzelnen 1-Personen-Teams möglich."));
    }
    let singleton_ids: Vec<i64> = teams.iter().map(|team| team.get("id")).collect();
    let invalid_singletons: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM turnier.team_members tm JOIN turnier.teams t ON t.id = tm.team_id WHERE t.tournament_id = $1 AND NOT EXISTS (SELECT 1 FROM turnier.tournament_signups s WHERE s.tournament_id = $1 AND s.discord_id = tm.discord_id AND s.team_id = tm.team_id)) OR EXISTS (SELECT 1 FROM turnier.team_applications WHERE team_id = ANY($2)) OR EXISTS (SELECT 1 FROM turnier.team_invitations WHERE team_id = ANY($2))")
        .bind(tournament_id).bind(&singleton_ids).fetch_one(&mut *conn).await?;
    if invalid_singletons {
        return Err(WebError::conflict(
            "Vorhandene Teams gehören nicht ausschließlich zu Solo-Anmeldungen.",
        ));
    }
    let is_test: bool = tournament.get("is_test");
    let signups: Vec<Signup> = sqlx::query_as(
        "WITH previous AS (SELECT r.tournament_id FROM turnier.solo_balance_runs r JOIN turnier.tournaments t ON t.id = r.tournament_id WHERE r.tournament_id != $1 AND t.is_test = $2 ORDER BY r.balanced_at DESC, r.tournament_id DESC LIMIT 1)
         SELECT s.id, s.discord_id, s.discord_name, s.steam_id, s.team_id, s.signed_up_at,
         EXISTS (SELECT 1 FROM turnier.solo_balance_waitlist w JOIN previous p ON p.tournament_id = w.tournament_id JOIN turnier.tournament_signups old ON old.id = w.signup_id WHERE w.discord_id = s.discord_id AND old.team_id IS NULL AND NOT EXISTS (SELECT 1 FROM turnier.team_members tm JOIN turnier.teams t ON t.id = tm.team_id WHERE t.tournament_id = w.tournament_id AND tm.discord_id = w.discord_id)) AS priority
         FROM turnier.tournament_signups s WHERE s.tournament_id = $1 ORDER BY s.signed_up_at, s.id")
        .bind(tournament_id).bind(is_test).fetch_all(&mut *conn).await?;
    if signups.iter().any(|signup| {
        signup
            .team_id
            .is_some_and(|id| !singleton_ids.contains(&id))
    }) {
        return Err(WebError::conflict(
            "Solo-Anmeldungen sind bereits Teams zugewiesen.",
        ));
    }
    Ok(Inputs {
        signups,
        singleton_ids,
        team_size,
        is_test,
    })
}

fn profile_score(profile: &RankProfile) -> i64 {
    let tier = profile.rank_tier.or_else(|| {
        profile
            .rank
            .as_deref()
            .and_then(turnier_steam::tier_for_rank_name)
    });
    turnier_steam::rank_score(tier.filter(|tier| (1..=11).contains(tier)), profile.subrank)
}

fn build_preview(
    inputs: &Inputs,
    profiles: &HashMap<String, RankProfile>,
    requested_count: Option<usize>,
) -> WebResult<PreviewDto> {
    let team_count = requested_count.unwrap_or(inputs.signups.len() / inputs.team_size);
    if team_count == 0 || team_count > inputs.signups.len() / inputs.team_size {
        return Err(WebError::bad_request(
            "Für die gewählte Teamanzahl sind nicht genug Solo-Anmeldungen vorhanden.",
        ));
    }
    let players = inputs
        .signups
        .iter()
        .enumerate()
        .map(|(signup_order, signup)| BalancePlayer {
            id: signup.id,
            score: profiles
                .get(&db::discord_id_to_string(signup.discord_id))
                .map(profile_score),
            priority: signup.priority,
            signup_order,
        })
        .collect();
    let result =
        balance_players(players, team_count, inputs.team_size).map_err(WebError::internal)?;
    let signups: HashMap<_, _> = inputs
        .signups
        .iter()
        .map(|signup| (signup.id, signup))
        .collect();
    let player_dto = |player: &BalancedPlayer| -> WebResult<PlayerDto> {
        let signup = signups
            .get(&player.id)
            .ok_or_else(|| WebError::internal("Solo-Anmeldung fehlt im Vorschlag"))?;
        let discord_id = db::discord_id_to_string(signup.discord_id);
        let profile = profiles.get(&discord_id);
        Ok(PlayerDto {
            signup_id: signup.id,
            discord_id,
            discord_name: signup.discord_name.clone(),
            steam_id: profile
                .and_then(|profile| profile.steam_id.clone())
                .or_else(|| signup.steam_id.clone()),
            rank: profile.and_then(|profile| profile.rank.clone()),
            rank_score: profile.map(profile_score).unwrap_or(0),
            score: player.score,
            priority: player.priority,
            used_median: player.used_median,
        })
    };
    let teams = result
        .teams
        .iter()
        .enumerate()
        .map(|(index, team)| {
            Ok(TeamDto {
                name: format!("Team {}", index + 1),
                players: team
                    .players
                    .iter()
                    .map(&player_dto)
                    .collect::<WebResult<_>>()?,
                score_sum: team.score_sum,
            })
        })
        .collect::<WebResult<Vec<_>>>()?;
    let mut dto = PreviewDto {
        teams,
        waitlist: result
            .waitlist
            .iter()
            .map(player_dto)
            .collect::<WebResult<_>>()?,
        median_score: result.median_score,
        gap: result.gap,
        team_count,
        team_size: inputs.team_size,
        preview_token: String::new(),
    };
    let snapshot = serde_json::to_vec(&json!({ "preview": dto, "signups": inputs.signups, "singleton_ids": inputs.singleton_ids, "is_test": inputs.is_test }))
        .map_err(|_| WebError::internal("Vorschlag konnte nicht geprüft werden"))?;
    dto.preview_token = format!("{:x}", Sha256::digest(snapshot));
    Ok(dto)
}

async fn apply(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(tournament_id): Path<i64>,
    Json(body): Json<ApplyBody>,
) -> WebResult<Json<PreviewDto>> {
    let profiles = refreshed_profiles(&state, tournament_id).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT id FROM turnier.tournaments WHERE id = $1 FOR UPDATE")
        .bind(tournament_id)
        .fetch_optional(&mut *tx)
        .await?;
    sqlx::query("LOCK TABLE turnier.teams, turnier.team_members, turnier.tournament_signups IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    let inputs = load_inputs(&mut tx, tournament_id).await?;
    let dto = build_preview(&inputs, &profiles, Some(body.team_count))?;
    if dto.preview_token != body.preview_token {
        return Err(WebError::conflict(
            "Anmeldungen oder Ränge haben sich geändert. Bitte den Vorschlag neu laden.",
        ));
    }
    remove_singletons(&mut tx, &inputs.singleton_ids).await?;
    sqlx::query("INSERT INTO turnier.solo_balance_runs (tournament_id, team_count, team_size) VALUES ($1, $2, $3)")
        .bind(tournament_id).bind(dto.team_count as i64).bind(dto.team_size as i64).execute(&mut *tx).await?;
    for team in &dto.teams {
        create_balanced_team(&mut tx, tournament_id, team).await?;
    }
    for (index, player) in dto.waitlist.iter().enumerate() {
        sqlx::query("UPDATE turnier.tournament_signups SET steam_id = $1, rank = $2, rank_score = $3 WHERE id = $4")
            .bind(&player.steam_id).bind(&player.rank).bind(player.rank_score).bind(player.signup_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO turnier.solo_balance_waitlist (tournament_id, signup_id, discord_id, position) VALUES ($1, $2, $3, $4)")
            .bind(tournament_id).bind(player.signup_id).bind(db::parse_discord_id(&player.discord_id)?).bind(index as i64 + 1).execute(&mut *tx).await?;
    }
    audit(&mut *tx, "solo_balance_apply", &user.discord_id,
        json!({ "tournament_id": tournament_id, "team_count": dto.team_count, "waitlist_count": dto.waitlist.len(), "gap": dto.gap, "median_score": dto.median_score })).await?;
    tx.commit().await?;
    Ok(Json(dto))
}

async fn remove_singletons(tx: &mut Transaction<'_, Postgres>, ids: &[i64]) -> WebResult<()> {
    for id in ids {
        ensure_team_not_locked(tx, *id).await?;
    }
    sqlx::query("UPDATE turnier.tournament_signups SET team_id = NULL WHERE team_id = ANY($1)")
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM turnier.team_members WHERE team_id = ANY($1)")
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM turnier.teams WHERE id = ANY($1)")
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn create_balanced_team(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    team: &TeamDto,
) -> WebResult<()> {
    let team_id: i64 = sqlx::query_scalar("INSERT INTO turnier.teams (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status) VALUES ($1, $2, $3, 0, now(), 'closed') RETURNING id")
        .bind(tournament_id).bind(&team.name).bind(turnier_engine::name_key(&team.name)).fetch_one(&mut **tx).await?;
    for (index, player) in team.players.iter().enumerate() {
        let role = if index == 0 { "captain" } else { "member" };
        insert_member(
            tx,
            team_id,
            &player.discord_id,
            player.discord_name.as_deref(),
            player.steam_id.as_deref(),
            player.rank.as_deref(),
            player.rank_score,
            role,
        )
        .await?;
        upsert_signup_for_team(
            tx,
            tournament_id,
            &player.discord_id,
            player.discord_name.as_deref(),
            player.steam_id.as_deref(),
            player.rank.as_deref(),
            player.rank_score,
            team_id,
        )
        .await?;
        if index == 0 {
            set_captain(tx, team_id, &player.discord_id).await?;
        }
    }
    Ok(())
}

async fn waitlist(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Vec<serde_json::Value>>> {
    super::helpers::load_tournament_or_404(&state.pool, tournament_id).await?;
    let rows = sqlx::query("SELECT s.id, s.discord_name, w.position FROM turnier.solo_balance_waitlist w JOIN turnier.tournament_signups s ON s.id = w.signup_id WHERE w.tournament_id = $1 AND s.team_id IS NULL ORDER BY w.position")
        .bind(tournament_id).fetch_all(&state.pool).await?;
    Ok(Json(rows.iter().map(|row| json!({ "signup_id": row.get::<i64, _>("id"), "discord_name": row.get::<Option<String>, _>("discord_name"), "position": row.get::<i64, _>("position") })).collect()))
}
