//! Phasen-Steuerung: Check-in finalisieren und Solo-Spieler zufällig verteilen.
//! Delegiert an [`turnier_engine::finalize_checkin`] /
//! [`turnier_engine::assign_random_teams`].

use std::collections::HashSet;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use turnier_engine::{
    assign_random_teams, finalize_checkin, FinalizeCheckinParams, FinalizeCheckinResult,
    SoloShuffler,
};

use crate::error::{WebError, WebResult};
use crate::extract::ModUser;
use crate::state::AppState;

use super::helpers::load_tournament_or_404;

/// Router der Phasen-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/tournaments/{tournament_id}/finalize-checkin",
            post(finalize_checkin_route),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/checkins",
            get(list_checkins),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/checkins/{discord_id}",
            post(checkin_participant),
        )
        .route(
            "/api/admin/tournaments/{tournament_id}/assign-random",
            post(assign_random),
        )
}

async fn list_checkins(
    State(state): State<AppState>,
    ModUser(_user): ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    load_tournament_or_404(&state.pool, tournament_id).await?;
    let rows: Vec<(i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"SELECT discord_id, checked_in_at FROM turnier."tournament_checkins"
         WHERE tournament_id = $1 ORDER BY checked_in_at, id"#,
    )
    .bind(tournament_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(Value::Array(
        rows.into_iter()
            .map(|(discord_id, checked_in_at)| {
                json!({
                    "discord_id": crate::db::discord_id_to_string(discord_id),
                    "checked_in_at": crate::db::ts_to_string(checked_in_at),
                })
            })
            .collect(),
    )))
}

async fn checkin_participant(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path((tournament_id, discord_id)): Path<(i64, String)>,
) -> WebResult<Json<Value>> {
    let participant_id = crate::db::parse_participant_id(&discord_id)?;
    let mut tx = state.pool.begin().await?;
    let status = crate::public::helpers::lock_tournament_status(&mut tx, tournament_id).await?;
    crate::public::helpers::ensure_checkin_open(status)?;
    if !crate::public::helpers::ensure_checkin_signup(&mut tx, tournament_id, participant_id, None)
        .await?
    {
        return Err(WebError::not_found(
            "Spieler ist in diesem Turnier nicht angemeldet",
        ));
    }
    let already_checked_in =
        !crate::public::helpers::insert_checkin(&mut tx, tournament_id, participant_id).await?;
    crate::public::helpers::audit(
        &mut *tx,
        "checkin_admin",
        Some(&user.discord_id),
        &json!({
            "tournament_id": tournament_id,
            "discord_id": crate::db::discord_id_to_string(participant_id),
            "already_checked_in": already_checked_in,
        }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({ "checked_in": true, "already_checked_in": already_checked_in }),
    ))
}

/// Query-Parameter `?confirm=bool` für die Check-in-Finalisierung.
#[derive(Debug, Deserialize)]
struct ConfirmQuery {
    #[serde(default)]
    confirm: bool,
}

/// Body von `finalize-checkin` (`allowed_team_ids` + `snapshot_token`, beide opt.).
#[derive(Debug, Default, Deserialize)]
struct FinalizeBody {
    #[serde(default)]
    allowed_team_ids: Option<Vec<Value>>,
    #[serde(default)]
    snapshot_token: Option<String>,
    #[serde(default)]
    fill_from_solos: bool,
}

pub(crate) async fn finalize_checkin_and_plan_lobbies(
    state: &AppState,
    tournament_id: i64,
    params: FinalizeCheckinParams<'_>,
) -> WebResult<FinalizeCheckinResult> {
    let result = finalize_checkin(&state.pool, tournament_id, params).await?;
    if let Some(entered) = result.entered_status {
        turnier_scheduler::schedule_lobbies_best_effort(
            &state.match_manager,
            tournament_id,
            entered,
        )
        .await;
    }
    Ok(result)
}

/// `POST /api/admin/tournaments/{id}/finalize-checkin?confirm=bool`: Check-in
/// abschließen, Teams bereinigen, optional Gruppenphase starten.
async fn finalize_checkin_route(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
    Query(query): Query<ConfirmQuery>,
    body: Option<Json<FinalizeBody>>,
) -> WebResult<Json<Value>> {
    let body = body.map(|Json(b)| b).unwrap_or_default();

    // allowed_team_ids: int ODER numerischer String (wie das Original).
    let mut allowed_team_ids: HashSet<i64> = HashSet::new();
    if let Some(list) = &body.allowed_team_ids {
        for item in list {
            if let Some(id) = item.as_i64() {
                allowed_team_ids.insert(id);
            } else if let Some(s) = item.as_str() {
                if let Ok(id) = s.parse::<i64>() {
                    allowed_team_ids.insert(id);
                }
            }
        }
    }
    let snapshot_token = body
        .snapshot_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let result = finalize_checkin_and_plan_lobbies(
        &state,
        tournament_id,
        FinalizeCheckinParams {
            confirm: query.confirm,
            allowed_team_ids,
            actor_id: Some(&user.discord_id),
            expected_snapshot_token: snapshot_token.as_deref(),
            advance_to_group_phase: query.confirm,
            fill_from_solos: body.fill_from_solos,
        },
    )
    .await?;

    let mut out = json!({
        "warnings": result.warnings.iter().map(|w| json!({
            "team_id": w.team_id,
            "team_name": w.team_name,
            "current": w.current,
            "required": w.required,
        })).collect::<Vec<_>>(),
        "removed_players": result.removed_players.iter().map(|p| json!({
            "team_id": p.team_id,
            "team_name": p.team_name,
            "discord_id": p.discord_id,
            "discord_name": p.discord_name,
        })).collect::<Vec<_>>(),
        "added_players": result.added_players.iter().map(|p| json!({
            "team_id": p.team_id,
            "team_name": p.team_name,
            "discord_id": p.discord_id,
            "discord_name": p.discord_name,
            "source": p.source,
        })).collect::<Vec<_>>(),
        "created_teams": result.created_teams.iter().map(|t| json!({
            "team_id": t.team_id,
            "team_name": t.team_name,
            "members": t.members.iter().map(|m| json!({
                "discord_id": m.discord_id,
                "discord_name": m.discord_name,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "team_plans": result.team_plans.iter().map(|t| json!({
            "team_id": t.team_id,
            "team_name": t.team_name,
            "checked_in": t.checked_in,
            "required": t.required,
            "outcome": t.outcome.as_str(),
            "roster": t.roster.iter().map(|m| json!({
                "discord_id": m.discord_id,
                "discord_name": m.discord_name,
                "checked_in": m.checked_in,
                "is_bench": m.is_bench,
                "change": m.change.as_str(),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "solo_plans": result.solo_plans.iter().map(|p| json!({
            "discord_id": p.discord_id,
            "discord_name": p.discord_name,
            "checked_in": p.checked_in,
            "outcome": p.outcome.as_str(),
            "team_name": p.team_name,
        })).collect::<Vec<_>>(),
        "auto_team_formation": result.auto_team_formation,
        "can_fill_from_solos": result.can_fill_from_solos,
        "bracket_format": result.bracket_format,
        "effective_bracket_format": result.effective_bracket_format,
        "double_elimination_rule": turnier_engine::engine::double_elim::DOUBLE_ELIMINATION_RULE,
        "bracket_entrants": result.bracket_entrants,
        "final_team_count": result.final_team_count,
        "tournament_mode": result.tournament_mode,
        "tournament_mode_auto": result.tournament_mode_auto,
        "auto_group_stage_threshold": turnier_engine::AUTO_GROUP_STAGE_THRESHOLD,
        "bracket_startable": turnier_engine::engine::double_elim::bracket_startable(result.bracket_entrants),
        "min_bracket_entrants": turnier_engine::engine::double_elim::MIN_BRACKET_ENTRANTS,
        "deleted_team_ids": result.deleted_team_ids,
        "remaining_solo_players": result.remaining_solo_players.iter().map(|p| json!({
            "discord_id": p.discord_id,
            "discord_name": p.discord_name,
        })).collect::<Vec<_>>(),
        "dry_run": result.dry_run,
        "snapshot_token": result.snapshot_token,
    });

    // Vorrück-Felder nur bei confirm + advance (wie das Original).
    if let Value::Object(map) = &mut out {
        if let Some(groups_created) = result.groups_created {
            map.insert("groups_created".into(), json!(groups_created));
        }
        if let Some(matches_created) = result.matches_created {
            map.insert("matches_created".into(), json!(matches_created));
        }
        if let Some(advanced) = result.advanced_to_group_phase {
            map.insert("advanced_to_group_phase".into(), json!(advanced));
        }
        if let Some(advanced) = result.advanced_to_bracket {
            map.insert("advanced_to_bracket".into(), json!(advanced));
        }
    }

    Ok(Json(out))
}

/// Selbstständiger Fisher-Yates-Shuffler ohne `rand`-Crate (xorshift64*, aus der
/// Systemzeit geseedet). Erfüllt den `SoloShuffler`-Vertrag; die Engine nutzt
/// produktiv `RngShuffler<StdRng>` (siehe contract_assumptions).
struct TimeSeededShuffler {
    state: u64,
}

impl TimeSeededShuffler {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl SoloShuffler for TimeSeededShuffler {
    fn permutation(&mut self, len: usize) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..len).collect();
        for i in (1..len).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            indices.swap(i, j);
        }
        indices
    }
}

/// `POST /api/admin/tournaments/{id}/assign-random`: Solo-Anmeldungen zufällig
/// auf Teams verteilen (nur in der Registration).
async fn assign_random(
    State(state): State<AppState>,
    _mod: ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    load_tournament_or_404(&state.pool, tournament_id).await?;

    let mut shuffler = TimeSeededShuffler::new();
    let teams_created = assign_random_teams(
        &state.pool,
        state.rank_resolver.as_ref(),
        tournament_id,
        &mut shuffler,
    )
    .await?;

    Ok(Json(
        json!({ "status": "ok", "teams_created": teams_created }),
    ))
}
