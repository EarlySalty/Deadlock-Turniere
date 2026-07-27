//! Loopback-only internal API for the canonical Scrim boundary.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;

use turnier_scrim::decision::validate_match_request_batch;
use turnier_scrim::dto::{
    ActionReceipt, AnnounceTeamRequest, AnnounceTeamResponse, AnnouncementPublicationRequest,
    CreateMatchRequest, CreateTeamRequest, DiscordResyncResponse, DiscordSyncStatus,
    LobbyCodeRequest, MatchIdPatchRequest, MatchIdsRequest, MatchRequestAction,
    MatchRequestDefaults, MatchRequestPatch, MatchRequestResponseRequest, ParticipantPatchRequest,
    ParticipantPatchResponse, PlanningCreateRequest, ReleaseMatchRequest, ReminderRequest,
    ReplacementRequestCreate, ReplacementRequestPatch, ResultFetchRequest, SelfServiceParticipant,
    SignupRequest, StatusPublicationRequest, SubstituteRequest, SubstituteResponse,
    SuggestTeamRequest, TeamMutationResponse, TeamPatchRequest, WeeklyAvailability,
    MATCH_REQUEST_RESPONSE_SCHEMA_VERSION,
};
use turnier_scrim::model::{
    AnnouncementPreview, Coach, LobbyStateMutation, MatchMutation, MatchRequest, MatchRequestBatch,
    MatchRequestBatchInput, MatchRequestTemplate, Participant, ReplacementCandidate,
    ReplacementNeed, ScrimAction, ScrimDay, ScrimMatch, ScrimMe, ScrimReadModel, ScrimSlot, Team,
    TeamBoard, TeamRef, TeamTimeline,
};
use turnier_scrim::repository::{
    DiscordRoleSyncPlan, MutationDispatch, PgScrimReadRepository, RoleOperation,
    ScrimReadRepository, SignupMutation,
};
use turnier_scrim::service::ScrimService;

use crate::error::{WebError, WebResult};
use crate::state::AppState;

const INTERNAL_TOKEN_HEADER: &str = "X-Internal-Token";
const REQUEST_ID_HEADER: &str = "X-Request-Id";
const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";
const ACTOR_DISCORD_ID_HEADER: &str = "X-Actor-Discord-Id";
const ACTOR_DISPLAY_NAME_HEADER: &str = "X-Actor-Display-Name";

// Wortgleich zum bisherigen Weg (Website routes/scrim.rs): dieselben Meldungen im Coach-UI.
const DISCORD_SYNC_NOOP: &str = "Keine Discord-Änderung nötig.";
const DISCORD_SYNC_NOT_CONFIGURED: &str = "Discord-Sync ist nicht konfiguriert.";
const DISCORD_SYNC_SUCCESS: &str = "Discord-Rollen aktualisiert.";
/// Die Datenbankänderung steht bereits, nur die Discord-Zustellung ist gescheitert.
/// Der Text muss das sagen, sonst versucht jemand die ganze Aktion neu statt nur den Versand.
const DISCORD_SYNC_FAILED: &str =
    "Gespeichert, aber die Discord-Nachricht ging nicht raus. Löse denselben Vorgang noch einmal aus, dann wird sie nachgereicht.";
const DISCORD_ROLE_CREATION_FAILED: &str =
    "Team gespeichert, aber die Discord-Rolle konnte nicht erstellt werden. Löse denselben Vorgang noch einmal aus, damit die Rolle nachgereicht wird.";
const DM_NO_ACCOUNT: &str = "No linked Discord account; DM not sent.";
const DM_SUCCESS: &str = "DM sent.";
const DM_FAILED: &str = "DM delivery failed.";
const LOBBY_CODE_DISCORD_TIMEOUT: Duration = Duration::from_secs(25);
const SUBSTITUTE_DISCORD_SYNC_TIMEOUT: Duration = Duration::from_secs(20);
const SCRIM_OPERATIONAL_POLL_INTERVAL: Duration = Duration::from_secs(15);
const SCRIM_RUNTIME_LOCK_A: i32 = 724_060_001;
const SCRIM_RUNTIME_LOCK_B: i32 = 724_060_002;

struct ResultFetchClaim {
    match_id: i32,
    team_a_id: i32,
    team_b_id: i32,
    party_id: Option<String>,
    steam_match_id: Option<i64>,
    result_ref_id: Option<i64>,
}

struct ReminderClaim {
    reminder_id: i64,
    effect_id: i64,
    request_id: i32,
    team_id: i32,
    payload: serde_json::Value,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/turnier/v1/scrims/command-center",
            get(read_command_center),
        )
        .route("/internal/turnier/v1/scrims/me", get(read_me))
        .route(
            "/internal/turnier/v1/scrims/me/availability",
            put(update_my_availability),
        )
        .route("/internal/turnier/v1/scrims/signup", post(signup))
        .route("/internal/turnier/v1/scrims/pool", get(read_pool))
        .route("/internal/turnier/v1/scrims/coaches", get(read_coaches))
        .route(
            "/internal/turnier/v1/scrims/teams",
            get(read_teams).post(create_team),
        )
        .route("/internal/turnier/v1/scrims/teams/{id}", patch(patch_team))
        .route(
            "/internal/turnier/v1/scrims/participants/{id}",
            patch(patch_participant),
        )
        .route(
            "/internal/turnier/v1/scrims/teams/{id}/announce",
            post(announce_team),
        )
        .route(
            "/internal/turnier/v1/scrims/teams/{id}/suggest",
            post(suggest_team),
        )
        .route(
            "/internal/turnier/v1/scrims/teams/{id}/substitute",
            post(substitute),
        )
        .route(
            "/internal/turnier/v1/scrims/participants/{id}/resync-discord",
            post(resync_participant_discord),
        )
        .route(
            "/internal/turnier/v1/scrims/teams/{id}/board",
            get(read_team_board),
        )
        .route(
            "/internal/turnier/v1/scrims/teams/{id}/timeline",
            get(read_team_timeline),
        )
        .route("/internal/turnier/v1/scrims/history", get(read_history))
        .route(
            "/internal/turnier/v1/scrims/match-requests/defaults",
            get(read_match_request_defaults),
        )
        .route(
            "/internal/turnier/v1/scrims/match-request-batches",
            get(read_match_request_batches).post(create_match_request_batch),
        )
        .route(
            "/internal/turnier/v1/scrims/match-request-batches/{id}",
            get(read_match_request_batch),
        )
        .route(
            "/internal/turnier/v1/scrims/match-requests/{id}",
            get(read_match_request).patch(patch_match_request),
        )
        .route(
            "/internal/turnier/v1/scrims/match-requests/{id}/status-preview",
            get(read_match_request_status_preview),
        )
        .route(
            "/internal/turnier/v1/scrims/match-requests/{id}/replacement-needs",
            get(read_match_request_replacement_needs),
        )
        .route(
            "/internal/turnier/v1/scrims/match-requests/{id}/release",
            post(release_match_request),
        )
        .route(
            "/internal/turnier/v1/scrims/match-requests/{id}/reminders",
            post(create_match_request_reminders),
        )
        .route(
            "/internal/turnier/v1/scrims/match-requests/{id}/status-publications",
            post(create_match_request_status_publication),
        )
        .route(
            "/internal/turnier/v1/scrims/replacement-needs/{id}/candidates",
            get(read_replacement_candidates),
        )
        .route(
            "/internal/turnier/v1/scrims/replacement-needs/{id}/requests",
            post(create_replacement_request),
        )
        .route(
            "/internal/turnier/v1/scrims/replacement-requests/{id}",
            patch(patch_replacement_request),
        )
        .route(
            "/internal/turnier/v1/scrims/matches",
            get(read_matches).post(create_match),
        )
        .route("/internal/turnier/v1/scrims/matches/{id}", get(read_match))
        .route(
            "/internal/turnier/v1/scrims/matches/{id}/lobby-code",
            put(set_lobby_code),
        )
        .route(
            "/internal/turnier/v1/scrims/matches/{id}/match-ids",
            post(add_match_ids),
        )
        .route(
            "/internal/turnier/v1/scrims/matches/{id}/result-fetches",
            post(request_result_fetch),
        )
        .route(
            "/internal/turnier/v1/scrims/matches/{id}/result-refs/{ref_id}",
            patch(select_result_ref),
        )
        .route(
            "/internal/turnier/v1/scrims/blocks/{id}/announcement-preview",
            get(read_announcement_preview),
        )
        .route(
            "/internal/turnier/v1/scrims/blocks/{id}/announcement-publications",
            post(create_announcement_publication),
        )
        .route("/internal/turnier/v1/scrims/actions/{id}", get(read_action))
        .route(
            "/internal/turnier/v1/scrims/interactions/match-request-response",
            post(match_request_response),
        )
}

pub fn spawn_substitute_sweep_worker(state: AppState) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(state.config.scrim_substitute_sweep_interval_seconds);
        loop {
            let repository = PgScrimReadRepository::new(state.pool.clone());
            let reserve_role_id = positive_config_id(state.config.scrim_reserve_role_id);
            let signup_role_id = positive_config_id(state.config.scrim_signup_role_id);
            let plans = match repository.sweep_expired_substitutes().await {
                Ok(plans) => plans,
                Err(error) => {
                    tracing::warn!(%error, "Scrim-Aushilfe-Ablauf konnte nicht geprueft werden");
                    Vec::new()
                }
            };
            let count = plans.len();
            // Jeder Durchlauf ist ein eigener Vorgang: sonst wuerde ein spaeterer Ablauf
            // derselben Rolle im Zwischenspeicher des Brokers haengen bleiben.
            let sweep_key = format!("substitute-sweep-{}", Utc::now().timestamp());
            for pending in plans {
                let delivery = match repository
                    .begin_expired_substitute_sync_delivery(
                        &pending,
                        reserve_role_id,
                        signup_role_id,
                    )
                    .await
                {
                    Ok(Some(delivery)) => delivery,
                    Ok(None) => continue,
                    Err(error) => {
                        tracing::warn!(
                            %error,
                            participant_id = pending.participant_id,
                            "Scrim-Aushilfe-Rollen-Sync konnte nicht vorbereitet werden"
                        );
                        continue;
                    }
                };
                let participant_id = delivery.participant_id;
                let subject = delivery.plan.subject.clone();
                let user_id = delivery.plan.discord_user_id;
                let status = match tokio::time::timeout(
                    SUBSTITUTE_DISCORD_SYNC_TIMEOUT,
                    sync_discord_roles(&state, vec![delivery.plan.clone()], &sweep_key),
                )
                .await
                {
                    Ok(status) => status,
                    Err(_) => DiscordSyncStatus {
                        ok: false,
                        detail: "timeout".to_string(),
                    },
                };
                if !status.ok {
                    tracing::warn!(
                        participant_id,
                        subject,
                        user_id = ?user_id,
                        detail = %status.detail,
                        "Scrim-Aushilfe-Rollen-Sync bleibt fuer Retry offen"
                    );
                }
                if let Err(error) = delivery.finish(status.ok).await {
                    tracing::warn!(
                        %error,
                        participant_id,
                        subject,
                        user_id = ?user_id,
                        delivered = status.ok,
                        "Scrim-Aushilfe-Rollen-Sync konnte nicht abgeschlossen werden"
                    );
                }
            }
            tracing::info!(count, "Scrim-Aushilfe-Ablauf geprueft");
            tokio::time::sleep(interval).await;
        }
    });
}

pub fn spawn_scrim_operational_worker(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SCRIM_OPERATIONAL_POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = process_scrim_operational_once(&state).await {
                tracing::warn!(%error, "Scrim-Result/Reminder-Worker fehlgeschlagen");
            }
        }
    });
}

pub async fn process_scrim_operational_once(state: &AppState) -> Result<(), sqlx::Error> {
    let Some(runtime_guard) = begin_turniere_operational_tick(&state.pool).await? else {
        return Ok(());
    };
    if let Some(claim) = claim_result_fetch(&state.pool).await? {
        process_result_fetch(state, claim).await?;
    }
    if let Some(claim) = claim_reminder(&state.pool).await? {
        process_reminder(state, claim).await?;
    }
    runtime_guard.commit().await?;
    Ok(())
}

async fn begin_turniere_operational_tick(
    pool: &sqlx::PgPool,
) -> Result<Option<sqlx::Transaction<'_, sqlx::Postgres>>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if !lock_and_require_turniere_runtime(&mut tx).await? {
        tx.rollback().await?;
        return Ok(None);
    }
    Ok(Some(tx))
}

async fn lock_and_require_turniere_runtime(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<bool, sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(SCRIM_RUNTIME_LOCK_A)
        .bind(SCRIM_RUNTIME_LOCK_B)
        .execute(&mut **tx)
        .await?;
    sqlx::query_scalar(
        "SELECT EXISTS(\
             SELECT 1 FROM scrim.runtime_control \
              WHERE control_key='scrim_runtime' \
                AND mode IN ('draining', 'turniere') \
                AND operational_writer='turniere'\
         )",
    )
    .fetch_one(&mut **tx)
    .await
}

async fn claim_result_fetch(pool: &sqlx::PgPool) -> Result<Option<ResultFetchClaim>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT m.id, m.team_a_id, m.team_b_id, m.party_id, m.steam_match_id \
           FROM scrim.matches m \
          WHERE m.lobby_state='result_requested' \
             OR (m.lobby_state IN ('result_failed', 'result_fetching') \
                 AND m.updated_at <= now() - interval '15 minutes') \
             OR EXISTS(\
                    SELECT 1 FROM scrim.match_result_refs stale_ref \
                     WHERE stale_ref.match_id=m.id \
                       AND stale_ref.fetch_status='fetching' \
                       AND stale_ref.updated_at <= now() - interval '15 minutes'\
                ) \
          ORDER BY m.updated_at, m.id \
          LIMIT 1 FOR UPDATE OF m SKIP LOCKED",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let match_id = row.try_get::<i32, _>("id")?;
    let result_ref = sqlx::query(
        "SELECT id, steam_match_id \
           FROM scrim.match_result_refs \
          WHERE match_id=$1 \
            AND (fetch_status='pending' \
                 OR (fetch_status IN ('failed', 'fetching') \
                     AND updated_at <= now() - interval '15 minutes')) \
          ORDER BY CASE fetch_status WHEN 'pending' THEN 0 WHEN 'failed' THEN 1 ELSE 2 END, \
                   entered_at, id \
          LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .bind(match_id)
    .fetch_optional(&mut *tx)
    .await?;
    let result_ref_id = result_ref
        .as_ref()
        .map(|result_ref| result_ref.try_get::<i64, _>("id"))
        .transpose()?;
    let result_ref_steam_match_id = result_ref
        .as_ref()
        .map(|result_ref| result_ref.try_get::<i64, _>("steam_match_id"))
        .transpose()?;
    if let Some(result_ref_id) = result_ref_id {
        sqlx::query(
            "UPDATE scrim.match_result_refs \
                SET fetch_status='fetching', last_error=NULL, updated_at=now() \
              WHERE id=$1",
        )
        .bind(result_ref_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE scrim.matches SET lobby_state='result_fetching', updated_at=now() WHERE id=$1",
    )
    .bind(match_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Some(ResultFetchClaim {
        match_id,
        team_a_id: row.try_get("team_a_id")?,
        team_b_id: row.try_get("team_b_id")?,
        party_id: row.try_get("party_id")?,
        steam_match_id: result_ref_steam_match_id.or(row.try_get("steam_match_id")?),
        result_ref_id,
    }))
}

async fn process_result_fetch(
    state: &AppState,
    claim: ResultFetchClaim,
) -> Result<(), sqlx::Error> {
    let result = state
        .match_manager
        .fetch_scrim_match_result(claim.steam_match_id, claim.party_id.as_deref())
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| {
            let parse_match_id = |key| match result.get(key) {
                None | Some(serde_json::Value::Null) => Ok(None),
                Some(value) => json_i64(value).map(Some).ok_or_else(|| {
                    format!("Steam-Ergebnis enthält in {key} keine gültige Match-ID.")
                }),
            };
            let match_id = parse_match_id("match_id")?;
            let deadlock_match_id = parse_match_id("deadlock_match_id")?;
            let returned_match_id = match_id.or(deadlock_match_id);
            if let Some(expected) = claim.steam_match_id {
                if let Some(actual) = [match_id, deadlock_match_id]
                    .into_iter()
                    .flatten()
                    .find(|actual| *actual != expected)
                {
                    return Err(format!(
                        "Steam-Ergebnis gehört zu Match {actual}, erwartet wurde Match {expected}."
                    ));
                }
                if returned_match_id.is_none() {
                    return Err(format!(
                        "Steam-Ergebnis enthält keine Match-ID, erwartet wurde Match {expected}."
                    ));
                }
            }
            Ok((result, returned_match_id))
        });
    let (result, returned_match_id) = match result {
        Ok(result) => result,
        Err(error) => {
            let mut tx = state.pool.begin().await?;
            if let Some(result_ref_id) = claim.result_ref_id {
                sqlx::query(
                    "UPDATE scrim.match_result_refs \
                        SET fetch_status='failed', last_error=$2, updated_at=now() WHERE id=$1",
                )
                .bind(result_ref_id)
                .bind(&error)
                .execute(&mut *tx)
                .await?;
            }
            let next_state: String = sqlx::query_scalar(
                "SELECT CASE \
                     WHEN EXISTS(SELECT 1 FROM scrim.match_result_refs \
                                  WHERE match_id=$1 AND fetch_status='pending') \
                         THEN 'result_requested' \
                     WHEN EXISTS(SELECT 1 FROM scrim.match_result_refs \
                                  WHERE match_id=$1 AND fetch_status='fetching') \
                         THEN 'result_fetching' \
                     WHEN result_json IS NOT NULL \
                       OR EXISTS(SELECT 1 FROM scrim.match_result_refs \
                                  WHERE match_id=$1 AND fetch_status='fetched') \
                         THEN 'finished' \
                     ELSE 'result_failed' \
                   END \
                   FROM scrim.matches WHERE id=$1",
            )
            .bind(claim.match_id)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE scrim.matches \
                    SET lobby_state=$2, updated_at=now() WHERE id=$1",
            )
            .bind(claim.match_id)
            .bind(next_state)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            tracing::warn!(
                match_id = claim.match_id,
                result_ref_id = claim.result_ref_id,
                %error,
                "Scrim-Ergebnisabruf fehlgeschlagen; Datenbankstand bleibt für einen neuen Versuch erhalten"
            );
            return Ok(());
        }
    };

    let steam_match_id = returned_match_id.or(claim.steam_match_id);
    let winner_team_id = match result.get("winning_team").and_then(json_i64) {
        Some(0) => Some(claim.team_a_id),
        Some(1) => Some(claim.team_b_id),
        Some(other) => {
            tracing::warn!(
                match_id = claim.match_id,
                winning_team = other,
                "Scrim-Ergebnis enthält kein gültiges Team; Ergebnis wird ohne Sieger gespeichert"
            );
            None
        }
        None => None,
    };
    let mut normalized = serde_json::Map::new();
    if let Some(steam_match_id) = steam_match_id {
        normalized.insert(
            "steam_match_id".to_string(),
            serde_json::json!(steam_match_id),
        );
    }
    if let Some(winner_team_id) = winner_team_id {
        normalized.insert(
            "winner_team_id".to_string(),
            serde_json::json!(winner_team_id),
        );
    }
    for key in [
        "winner",
        "winning_team",
        "match_time",
        "duration",
        "duration_s",
        "teams",
        "players",
        "lineups",
        "substitutes",
        "stats",
    ] {
        if let Some(value) = result.get(key) {
            normalized.insert(key.to_string(), value.clone());
        }
    }

    let mut tx = state.pool.begin().await?;
    if let Some(result_ref_id) = claim.result_ref_id {
        sqlx::query(
            "UPDATE scrim.match_result_refs \
                SET fetch_status='fetched', fetched_at=now(), last_error=NULL, \
                    winner_team_id=$2, raw_result_json=$3, normalized_result_json=$4, \
                    updated_at=now() \
              WHERE id=$1",
        )
        .bind(result_ref_id)
        .bind(winner_team_id)
        .bind(&result)
        .bind(serde_json::Value::Object(normalized))
        .execute(&mut *tx)
        .await?;
    }
    let next_state: String = sqlx::query_scalar(
        "SELECT CASE \
             WHEN EXISTS(SELECT 1 FROM scrim.match_result_refs \
                          WHERE match_id=$1 AND fetch_status='pending') \
                 THEN 'result_requested' \
             WHEN EXISTS(SELECT 1 FROM scrim.match_result_refs \
                          WHERE match_id=$1 AND fetch_status='fetching') \
                 THEN 'result_fetching' \
             ELSE 'finished' \
           END",
    )
    .bind(claim.match_id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE scrim.matches \
            SET steam_match_id=COALESCE(steam_match_id, $2), lobby_state=$3, updated_at=now() \
          WHERE id=$1",
    )
    .bind(claim.match_id)
    .bind(steam_match_id)
    .bind(next_state)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

fn json_i64(value: &serde_json::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str()?.trim().parse().ok())
}

async fn claim_reminder(pool: &sqlx::PgPool) -> Result<Option<ReminderClaim>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT r.id, r.request_id, r.team_id, r.template, r.target_participant_ids, \
                r.target_role_id, r.discord_channel_id, mr.status AS request_status, \
                t.name AS team_name, effect.id AS effect_id, effect.state AS effect_state, \
                effect.payload AS effect_payload \
           FROM scrim.match_request_reminders r \
           JOIN scrim.match_requests mr ON mr.id=r.request_id \
           JOIN scrim.teams t ON t.id=r.team_id \
           LEFT JOIN scrim.match_request_reminder_effects link ON link.reminder_id=r.id \
           LEFT JOIN scrim.outbox_effects effect ON effect.id=link.outbox_effect_id \
          WHERE (r.status='approved' \
                 OR (r.status IN ('failed', 'posting') \
                     AND r.updated_at <= now() - interval '15 minutes')) \
            AND r.scheduled_for <= now() \
          ORDER BY r.scheduled_for, r.id \
          LIMIT 1 FOR UPDATE OF r, mr SKIP LOCKED",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let reminder_id = row.try_get::<i64, _>("id")?;
    let request_id = row.try_get::<i32, _>("request_id")?;
    let team_id = row.try_get::<i32, _>("team_id")?;
    if let (Some(effect_id), Some(effect_state), Some(payload)) = (
        row.try_get::<Option<i64>, _>("effect_id")?,
        row.try_get::<Option<String>, _>("effect_state")?,
        row.try_get::<Option<serde_json::Value>, _>("effect_payload")?,
    ) {
        if effect_state == "delivered" {
            sqlx::query(
                "UPDATE scrim.match_request_reminders \
                    SET status='posted', last_error=NULL, \
                        posted_at=COALESCE(posted_at, now()), updated_at=now() \
                  WHERE id=$1",
            )
            .bind(reminder_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE scrim.match_request_reminder_effects \
                    SET reconciled_at=COALESCE(reconciled_at, now()) \
                  WHERE reminder_id=$1 AND outbox_effect_id=$2",
            )
            .bind(reminder_id)
            .bind(effect_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(None);
        }
        if matches!(effect_state.as_str(), "uncertain" | "dead")
            || payload.get("idempotency_key").is_none()
        {
            if !matches!(effect_state.as_str(), "uncertain" | "dead") {
                sqlx::query(
                    "UPDATE scrim.outbox_effects \
                        SET state='uncertain', lease_owner=NULL, lease_until=NULL, \
                            last_error_code='err_discord_effect_uncertain', updated_at=now() \
                      WHERE id=$1 AND state IN ('pending', 'leased', 'retry')",
                )
                .bind(effect_id)
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query(
                "UPDATE scrim.match_request_reminders \
                    SET status='uncertain', \
                        last_error='Discord-Zustellung ist unklar; prüfe den Zielkanal, bevor du erneut sendest.', \
                        updated_at=now() WHERE id=$1",
            )
            .bind(reminder_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            tracing::warn!(
                reminder_id,
                effect_id,
                request_id,
                team_id,
                effect_state,
                "Scrim-Reminder wird wegen unklarer Discord-Zustellung nicht automatisch erneut gesendet"
            );
            return Ok(None);
        }
        if effect_state == "cancelled" {
            sqlx::query(
                "UPDATE scrim.match_request_reminders \
                    SET status='cancelled', updated_at=now() WHERE id=$1",
            )
            .bind(reminder_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(None);
        }
        sqlx::query(
            "UPDATE scrim.outbox_effects \
                SET state='leased', lease_owner='turnier_bot:scrim_reminder', \
                    lease_until=now() + interval '1 minute', attempts=attempts + 1, \
                    next_attempt_at=NULL, updated_at=now() \
              WHERE id=$1",
        )
        .bind(effect_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE scrim.match_request_reminders \
                SET status='posting', updated_at=now() WHERE id=$1",
        )
        .bind(reminder_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(Some(ReminderClaim {
            reminder_id,
            effect_id,
            request_id,
            team_id,
            payload,
        }));
    }
    let request_status = row.try_get::<String, _>("request_status")?;
    if !matches!(request_status.as_str(), "open" | "post_failed") {
        sqlx::query(
            "UPDATE scrim.match_request_reminders \
                SET status='cancelled', missing_count=0, \
                    target_discord_user_ids='{}', last_error='Terminabfrage geschlossen', \
                    updated_at=now() WHERE id=$1",
        )
        .bind(reminder_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(None);
    }
    let participant_ids = row.try_get::<Vec<i32>, _>("target_participant_ids")?;
    let missing = sqlx::query(
        "SELECT p.id, p.discord_id \
           FROM scrim.participants p \
          WHERE p.id=ANY($1) \
            AND NOT EXISTS(\
                SELECT 1 FROM scrim.match_request_responses response \
                 WHERE response.request_id=$2 AND response.team_id=$3 \
                   AND response.participant_id=p.id\
            ) \
          ORDER BY array_position($1, p.id), p.id",
    )
    .bind(&participant_ids)
    .bind(request_id)
    .bind(team_id)
    .fetch_all(&mut *tx)
    .await?;
    if missing.is_empty() {
        sqlx::query(
            "UPDATE scrim.match_request_reminders \
                SET status='cancelled', missing_count=0, \
                    target_discord_user_ids='{}', last_error='Keine offenen Antworten mehr', \
                    updated_at=now() WHERE id=$1",
        )
        .bind(reminder_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(None);
    }
    let target_participant_ids = missing
        .iter()
        .map(|participant| participant.try_get::<i32, _>("id"))
        .collect::<Result<Vec<_>, _>>()?;
    let discord_ids = missing
        .iter()
        .map(|participant| participant.try_get::<Option<i64>, _>("discord_id"))
        .collect::<Result<Vec<_>, _>>()?;
    let all_have_discord = discord_ids.iter().all(Option::is_some);
    let target_user_ids = if all_have_discord {
        discord_ids.into_iter().flatten().collect()
    } else {
        Vec::new()
    };
    let target_role_id = row.try_get::<Option<i64>, _>("target_role_id")?;
    if !all_have_discord && target_role_id.is_none() {
        sqlx::query(
            "UPDATE scrim.match_request_reminders \
                SET status='failed', target_kind='team', target_participant_ids=$2, \
                    target_discord_user_ids='{}', missing_count=$3, \
                    last_error='Keine pingbare Teamrolle hinterlegt', updated_at=now() \
              WHERE id=$1",
        )
        .bind(reminder_id)
        .bind(&target_participant_ids)
        .bind(i32::try_from(target_participant_ids.len()).unwrap_or(i32::MAX))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::warn!(
            reminder_id,
            request_id,
            team_id,
            "Scrim-Reminder ohne pingbares Ziel nicht gepostet"
        );
        return Ok(None);
    }
    let target = if target_user_ids.is_empty() {
        target_role_id.map_or_else(|| "euch".to_string(), |id| format!("<@&{id}>"))
    } else {
        target_user_ids
            .iter()
            .map(|id| format!("<@{id}>"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let team_name = row.try_get::<String, _>("team_name")?;
    let template = row.try_get::<String, _>("template")?;
    let content = match template.as_str() {
        "antwort_fehlt" => format!(
            "Erinnerung für {team_name}: Es fehlen noch Antworten von {target}.\nBitte stimmt in der Terminabfrage oben ab."
        ),
        "frist_bald" => format!(
            "Erinnerung für {team_name}: Die Frist läuft bald ab und es fehlen noch Antworten von {target}.\nBitte stimmt in der Terminabfrage oben ab, damit der Termin stehen kann."
        ),
        "bestaetigung_offen" => format!(
            "Erinnerung für {team_name}: Es fehlen noch Bestätigungen von {target}.\nBitte gebt oben kurz Bescheid, ob ihr beim Match dabei seid."
        ),
        _ => {
            let error = format!("Unbekannte Scrim-Reminder-Vorlage: {template}");
            sqlx::query(
                "UPDATE scrim.match_request_reminders \
                    SET status='failed', last_error=$2, updated_at=now() WHERE id=$1",
            )
            .bind(reminder_id)
            .bind(&error)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            tracing::warn!(
                reminder_id,
                request_id,
                team_id,
                %error,
                "Scrim-Reminder konnte nicht erstellt werden"
            );
            return Ok(None);
        }
    };
    let payload = serde_json::json!({
        "channel_id": row.try_get::<i64, _>("discord_channel_id")?,
        "content": content,
        "idempotency_key": format!("scrim-reminder:{reminder_id}"),
    });
    let payload_hash = Sha256::digest(
        serde_json::to_vec(&payload).map_err(|error| sqlx::Error::Protocol(error.to_string()))?,
    )
    .to_vec();
    let effect_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.outbox_effects(\
             effect_type, idempotency_key, payload_hash, payload, state, \
             lease_owner, lease_until, attempts, remote_system\
         ) VALUES (\
             'discord_scrim_effect', $1, $2, $3, 'leased', \
             'turnier_bot:scrim_reminder', now() + interval '1 minute', 1, 'discord'\
         ) RETURNING id",
    )
    .bind(format!("scrim_reminder:{reminder_id}"))
    .bind(payload_hash)
    .bind(&payload)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO scrim.match_request_reminder_effects(reminder_id, outbox_effect_id) \
         VALUES ($1, $2)",
    )
    .bind(reminder_id)
    .bind(effect_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE scrim.match_request_reminders \
            SET status='posting', target_kind=$2, target_participant_ids=$3, \
                target_discord_user_ids=$4, missing_count=$5, updated_at=now() \
          WHERE id=$1",
    )
    .bind(reminder_id)
    .bind(if all_have_discord { "members" } else { "team" })
    .bind(&target_participant_ids)
    .bind(&target_user_ids)
    .bind(i32::try_from(target_participant_ids.len()).unwrap_or(i32::MAX))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(ReminderClaim {
        reminder_id,
        effect_id,
        request_id,
        team_id,
        payload,
    }))
}

async fn process_reminder(state: &AppState, claim: ReminderClaim) -> Result<(), sqlx::Error> {
    let result = state
        .notifier
        .broker()
        .post_internal::<serde_json::Value, _>(
            "/internal/master/v1/discord/send-message",
            &claim.payload,
        )
        .await;
    match result {
        Ok(response) => {
            let channel_id = claim.payload.get("channel_id").and_then(json_i64);
            let discord_message_id = response.pointer("/result/message_id").and_then(json_i64);
            let (Some(channel_id), Some(discord_message_id)) = (channel_id, discord_message_id)
            else {
                let error =
                    "Discord hat den Scrim-Reminder ohne Channel- oder Message-ID bestätigt";
                let mut tx = state.pool.begin().await?;
                sqlx::query(
                    "UPDATE scrim.match_request_reminders \
                        SET status='uncertain', last_error=$2, updated_at=now() WHERE id=$1",
                )
                .bind(claim.reminder_id)
                .bind(error)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE scrim.outbox_effects \
                        SET state='uncertain', lease_owner=NULL, lease_until=NULL, \
                            last_error_code='err_discord_effect_uncertain', updated_at=now() \
                      WHERE id=$1",
                )
                .bind(claim.effect_id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                tracing::warn!(
                    reminder_id = claim.reminder_id,
                    effect_id = claim.effect_id,
                    request_id = claim.request_id,
                    team_id = claim.team_id,
                    "Discord-Zustellung des Scrim-Reminders konnte nicht eindeutig bestätigt werden"
                );
                return Ok(());
            };
            let remote_message_id = format!("discord:{channel_id}:{discord_message_id}");
            let payload_hash = Sha256::digest(
                serde_json::to_vec(&claim.payload)
                    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?,
            )
            .to_vec();
            let mut tx = state.pool.begin().await?;
            sqlx::query(
                "UPDATE scrim.match_request_reminders \
                    SET status='posted', discord_message_id=COALESCE($2, discord_message_id), \
                        last_error=NULL, posted_at=now(), updated_at=now() \
                  WHERE id=$1",
            )
            .bind(claim.reminder_id)
            .bind(Some(discord_message_id))
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE scrim.outbox_effects \
                    SET state='delivered', lease_owner=NULL, lease_until=NULL, \
                        next_attempt_at=NULL, remote_message_id=$2, last_error_code=NULL, \
                        delivered_at=now(), updated_at=now() \
                  WHERE id=$1",
            )
            .bind(claim.effect_id)
            .bind(&remote_message_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE scrim.match_request_reminder_effects \
                    SET reconciled_at=now() WHERE reminder_id=$1 AND outbox_effect_id=$2",
            )
            .bind(claim.reminder_id)
            .bind(claim.effect_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO scrim.effect_receipts(\
                     outbox_effect_id, remote_system, remote_message_id, payload_hash, \
                     receipt_payload, status, observed_at\
                 ) VALUES ($1, 'discord', $2, $3, $4, 'confirmed', now()) \
                 ON CONFLICT (remote_system, remote_message_id) \
                     WHERE remote_message_id IS NOT NULL DO NOTHING",
            )
            .bind(claim.effect_id)
            .bind(&remote_message_id)
            .bind(payload_hash)
            .bind(serde_json::json!({
                "channel_id": channel_id.to_string(),
                "message_id": discord_message_id.to_string(),
            }))
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
        Err(error) => {
            let mut tx = state.pool.begin().await?;
            sqlx::query(
                "UPDATE scrim.match_request_reminders \
                    SET status='failed', last_error=$2, updated_at=now() WHERE id=$1",
            )
            .bind(claim.reminder_id)
            .bind(error.to_string())
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE scrim.outbox_effects \
                    SET state='retry', lease_owner=NULL, lease_until=NULL, \
                        next_attempt_at=now() + interval '15 minutes', \
                        last_error_code='err_discord_send', updated_at=now() \
                  WHERE id=$1",
            )
            .bind(claim.effect_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            tracing::warn!(
                reminder_id = claim.reminder_id,
                request_id = claim.request_id,
                team_id = claim.team_id,
                %error,
                "Scrim-Reminder blieb gespeichert, aber die Discord-Nachricht ging nicht raus"
            );
        }
    }
    Ok(())
}

async fn read_command_center(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<ScrimReadModel>> {
    Ok(Json(operator_model(&state, peer, &headers).await?))
}

async fn read_me(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<ScrimMe>> {
    require_internal_boundary(peer, &headers, &state)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    let model = service.read_model().await?;
    let participant = model
        .participants
        .iter()
        .find(|participant| participant.discord_id.as_deref() == Some(actor.discord_id))
        .cloned();
    let team = participant.as_ref().and_then(|participant| {
        model
            .teams
            .iter()
            .find(|team| {
                team.members
                    .iter()
                    .any(|member| member.participant_id == participant.id)
            })
            .cloned()
    });
    let next_match = team
        .as_ref()
        .and_then(|team| next_match_for_team(&model.matches, team.id));
    Ok(Json(ScrimMe {
        participant,
        team,
        next_match,
    }))
}

async fn read_pool(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<Vec<Participant>>> {
    Ok(Json(
        operator_model(&state, peer, &headers).await?.participants,
    ))
}

async fn read_coaches(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<Vec<Coach>>> {
    require_operator(&state, peer, &headers).await?;
    Ok(Json(service(&state).repository().coaches().await?))
}

async fn read_teams(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<Vec<Team>>> {
    Ok(Json(operator_model(&state, peer, &headers).await?.teams))
}

async fn read_team_board(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<TeamBoard>> {
    let id = parse_db_id(&id, "team_id")?;
    let model = operator_model(&state, peer, &headers).await?;
    let team = find_team(&model, id)?;
    Ok(Json(TeamBoard { team }))
}

async fn read_team_timeline(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<TeamTimeline>> {
    let id = parse_db_id(&id, "team_id")?;
    let model = operator_model(&state, peer, &headers).await?;
    let team = find_team(&model, id)?;
    Ok(Json(TeamTimeline {
        team: TeamRef {
            id: team.id,
            name: team.name,
        },
        matches: model
            .matches
            .into_iter()
            .filter(|scrim_match| match_has_team(scrim_match, id))
            .collect(),
        lagebild_refs: model
            .lagebild_refs
            .into_iter()
            .filter(|snapshot| snapshot.team_id == id)
            .collect(),
    }))
}

async fn read_history(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<Vec<ScrimMatch>>> {
    require_operator(&state, peer, &headers).await?;
    Ok(Json(service(&state).history().await?))
}

async fn read_match_request_defaults(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<MatchRequestDefaults>> {
    require_operator(&state, peer, &headers).await?;
    Ok(Json(MatchRequestDefaults {
        default_deadline_hours: 48,
        min_slots: 2,
        max_slots: 5,
        templates: vec![
            MatchRequestTemplate::RegularScrim,
            MatchRequestTemplate::Testmatch,
            MatchRequestTemplate::Training,
        ],
        preset_slots: vec![
            ScrimSlot {
                day: ScrimDay::Saturday,
                from: 1_200,
                to: 1_320,
            },
            ScrimSlot {
                day: ScrimDay::Sunday,
                from: 1_200,
                to: 1_320,
            },
        ],
    }))
}

async fn read_match_request_batch(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<MatchRequestBatch>> {
    let id = parse_db_id(&id, "batch_id")?;
    let model = operator_model(&state, peer, &headers).await?;
    let batch = model
        .match_request_batches
        .into_iter()
        .find(|batch| batch.id == id)
        .ok_or_else(|| WebError::not_found("Match-Request-Batch nicht gefunden"))?;
    Ok(Json(batch))
}

async fn read_match_request_batches(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<Vec<MatchRequestBatch>>> {
    Ok(Json(
        operator_model(&state, peer, &headers)
            .await?
            .match_request_batches,
    ))
}

async fn read_match_request(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<MatchRequest>> {
    let id = parse_db_id(&id, "request_id")?;
    let model = operator_model(&state, peer, &headers).await?;
    Ok(Json(find_match_request(model, id)?))
}

async fn read_match_request_status_preview(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<MatchRequest>> {
    let id = parse_db_id(&id, "request_id")?;
    let model = operator_model(&state, peer, &headers).await?;
    Ok(Json(find_match_request(model, id)?))
}

async fn read_match_request_replacement_needs(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<Vec<ReplacementNeed>>> {
    let id = parse_db_id(&id, "request_id")?;
    let model = operator_model(&state, peer, &headers).await?;
    Ok(Json(find_match_request(model, id)?.facts.replacement_needs))
}

async fn read_matches(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<Json<Vec<ScrimMatch>>> {
    Ok(Json(operator_model(&state, peer, &headers).await?.matches))
}

async fn read_match(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<ScrimMatch>> {
    let id = parse_db_id(&id, "match_id")?;
    let model = operator_model(&state, peer, &headers).await?;
    let scrim_match = model
        .matches
        .into_iter()
        .find(|scrim_match| scrim_match.id == id)
        .ok_or_else(|| WebError::not_found("Scrim-Match nicht gefunden"))?;
    Ok(Json(scrim_match))
}

async fn create_match_request_batch(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PlanningCreateRequest>,
) -> WebResult<(StatusCode, Json<ActionReceipt>)> {
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let payload = serde_json::to_value(&body)
        .map_err(|_| WebError::internal("Scrim-Payload konnte nicht serialisiert werden"))?;
    let batch = planning_batch(body)?;
    let validated = validate_match_request_batch(&batch, Utc::now(), &BTreeSet::new())?;
    let receipt = service
        .repository()
        .create_match_request_batch(
            mutation.idempotency_key,
            mutation.request_id,
            &payload,
            actor.discord_id,
            actor.display_name,
            &validated,
        )
        .await?;
    Ok((StatusCode::OK, Json(receipt)))
}

async fn release_match_request(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ReleaseMatchRequest>,
) -> WebResult<(StatusCode, Json<ActionReceipt>)> {
    let id = parse_db_id(&id, "request_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    if body
        .reason
        .as_ref()
        .is_some_and(|reason| reason.chars().count() > 1_000)
    {
        return Err(WebError::bad_request("reason ist zu lang"));
    }
    let payload = json_with_target("request_id", id, &body)?;
    let receipt = service
        .repository()
        .release_match_request(
            mutation.idempotency_key,
            mutation.request_id,
            &payload,
            id,
            &body,
            (actor.discord_id, actor.display_name),
        )
        .await?;
    Ok((StatusCode::OK, Json(receipt)))
}

async fn create_match(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<CreateMatchRequest>,
) -> WebResult<Json<MatchMutation>> {
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    Ok(Json(
        service.create_match(mutation.idempotency_key, body).await?,
    ))
}

async fn set_lobby_code(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<LobbyCodeRequest>,
) -> WebResult<Json<MatchMutation>> {
    let id = parse_db_id(&id, "match_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation_headers = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let mutation = service
        .set_lobby_code(
            mutation_headers.idempotency_key,
            id,
            actor.discord_id,
            actor.display_name,
            body,
        )
        .await?;
    let delivery_ok = match service
        .repository()
        .begin_lobby_code_delivery(mutation_headers.idempotency_key, &mutation)
        .await
    {
        Ok(Some(mut delivery)) => {
            let (delivered_channel_ids, discord_ok) = distribute_lobby_code(
                &state,
                &mutation,
                &delivery.delivered_channel_ids,
                mutation_headers.idempotency_key,
            )
            .await;
            delivery.delivered_channel_ids.extend(delivered_channel_ids);
            let delivered_channel_count = delivery.delivered_channel_ids.len();
            match delivery.finish().await {
                Ok(()) => discord_ok,
                Err(error) => {
                    tracing::warn!(
                        match_id = mutation.scrim_match.id,
                        delivered_channel_count,
                        idempotency_key = mutation_headers.idempotency_key,
                        %error,
                        "Scrim-Lobbycode-Discord-Zustellstatus konnte nicht gespeichert werden; ein Replay bleibt möglich"
                    );
                    false
                }
            }
        }
        Ok(None) => true,
        Err(error) => {
            tracing::warn!(
                match_id = mutation.scrim_match.id,
                idempotency_key = mutation_headers.idempotency_key,
                %error,
                "Scrim-Lobbycode-Discord-Versand konnte nicht vorbereitet werden; Datenbankstand bleibt gespeichert"
            );
            false
        }
    };
    if !delivery_ok {
        return Err(WebError::new(StatusCode::BAD_GATEWAY, DISCORD_SYNC_FAILED));
    }
    Ok(Json(mutation))
}

async fn add_match_ids(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<MatchIdsRequest>,
) -> WebResult<Json<MatchMutation>> {
    let id = parse_db_id(&id, "match_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    Ok(Json(
        service
            .add_match_ids(
                mutation.idempotency_key,
                id,
                actor.discord_id,
                actor.display_name,
                body,
            )
            .await?,
    ))
}

async fn request_result_fetch(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ResultFetchRequest>,
) -> WebResult<Json<LobbyStateMutation>> {
    let id = parse_db_id(&id, "match_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    Ok(Json(
        service
            .request_result_fetch(mutation.idempotency_key, id, body)
            .await?,
    ))
}

async fn select_result_ref(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((id, ref_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<MatchIdPatchRequest>,
) -> WebResult<Json<MatchMutation>> {
    let id = parse_db_id(&id, "match_id")?;
    let ref_id = parse_i64_db_id(&ref_id, "result_ref_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let selected = service
        .select_result_ref(
            mutation.idempotency_key,
            id,
            ref_id,
            actor.discord_id,
            actor.display_name,
            body,
        )
        .await?;
    notify_selected_result(&state, &selected).await;
    Ok(Json(selected))
}

async fn notify_selected_result(state: &AppState, mutation: &MatchMutation) {
    let scrim_match = &mutation.scrim_match;
    let Some(selected) = scrim_match.selected_result.as_ref() else {
        tracing::warn!(
            match_id = scrim_match.id,
            "Scrim-Ergebnisauswahl wurde gespeichert, enthält aber kein ausgewähltes Ergebnis"
        );
        return;
    };
    let winner = [scrim_match.team_a.as_ref(), scrim_match.team_b.as_ref()]
        .into_iter()
        .flatten()
        .find(|team| Some(team.id) == selected.winner_team_id);
    let Some(winner) = winner else {
        tracing::warn!(
            match_id = scrim_match.id,
            result_ref_id = selected.result_ref_id,
            winner_team_id = selected.winner_team_id,
            "Scrim-Ergebnisauswahl wurde gespeichert, aber der Sieger gehört nicht zum Match"
        );
        return;
    };
    let channels = match sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
        "SELECT ta.discord_channel_id AS team_a_channel_id, \
                tb.discord_channel_id AS team_b_channel_id \
           FROM scrim.matches m \
           JOIN scrim.match_result_selections selection \
             ON selection.match_id=m.id AND selection.result_ref_id=$2 \
           LEFT JOIN scrim.teams ta ON ta.id=m.team_a_id \
           LEFT JOIN scrim.teams tb ON tb.id=m.team_b_id \
          WHERE m.id=$1",
    )
    .bind(scrim_match.id)
    .bind(selected.result_ref_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(channels)) => channels,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(
                match_id = scrim_match.id,
                result_ref_id = selected.result_ref_id,
                %error,
                "Scrim-Ergebnisauswahl wurde gespeichert, aber die Teamkanäle konnten nicht geladen werden"
            );
            return;
        }
    };
    let channels = [channels.0, channels.1]
        .into_iter()
        .flatten()
        .filter(|channel_id| *channel_id > 0)
        .collect::<BTreeSet<_>>();
    let content = format!(
        "Scrim beendet. Das Ergebnis ist eingetragen. Sieger: {}.",
        winner.name
    );
    for channel_id in channels {
        if let Err(error) = state
            .notifier
            .broker()
            .post_internal::<serde_json::Value, _>(
                "/internal/master/v1/discord/send-message",
                &serde_json::json!({
                    "channel_id": channel_id,
                    "content": content,
                    "idempotency_key": format!(
                        "scrim-selected-result:{}:{}:{channel_id}",
                        scrim_match.id, selected.result_ref_id
                    ),
                }),
            )
            .await
        {
            tracing::warn!(
                match_id = scrim_match.id,
                result_ref_id = selected.result_ref_id,
                winner_team_id = selected.winner_team_id,
                channel_id,
                %error,
                "Scrim-Ergebnisauswahl ist gespeichert, aber die Discord-Nachricht ging nicht raus"
            );
        }
    }
}

async fn read_announcement_preview(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<AnnouncementPreview>> {
    require_operator(&state, peer, &headers).await?;
    Ok(Json(service(&state).announcement_preview(&id).await?))
}

async fn create_announcement_publication(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<AnnouncementPublicationRequest>,
) -> WebResult<Json<AnnouncementPreview>> {
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let publication = service
        .create_announcement_publication(
            &id,
            mutation.idempotency_key,
            actor.discord_id,
            actor.display_name,
            body,
        )
        .await?;
    if !publish_announcement(&state, &publication, mutation.idempotency_key).await {
        return Err(WebError::new(StatusCode::BAD_GATEWAY, DISCORD_SYNC_FAILED));
    }
    Ok(Json(service.announcement_preview(&id).await?))
}

async fn read_action(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<ScrimAction>> {
    let id = parse_i64_db_id(&id, "action_id")?;
    require_operator(&state, peer, &headers).await?;
    Ok(Json(service(&state).action(id).await?))
}

async fn distribute_lobby_code(
    state: &AppState,
    mutation: &MatchMutation,
    delivered_channel_ids: &BTreeSet<String>,
    request_idempotency_key: &str,
) -> (BTreeSet<String>, bool) {
    let Some(code) = mutation.scrim_match.join_code.as_deref() else {
        tracing::warn!(
            match_id = mutation.scrim_match.id,
            idempotency_key = request_idempotency_key,
            "Scrim-Lobbycode-Zustellung hat keinen gespeicherten Code; Zustellung bleibt für einen Retry offen"
        );
        return (BTreeSet::new(), false);
    };
    let team_ids = [
        mutation.scrim_match.team_a.as_ref().map(|team| team.id),
        mutation.scrim_match.team_b.as_ref().map(|team| team.id),
    ];
    let model = match service(state).read_model().await {
        Ok(model) => model,
        Err(error) => {
            tracing::warn!(
                match_id = mutation.scrim_match.id,
                idempotency_key = request_idempotency_key,
                %error,
                "Scrim-Lobbycode-Kanaele konnten nicht geladen werden; Discord-Sync fail-open"
            );
            return (BTreeSet::new(), false);
        }
    };
    let match_id = mutation.scrim_match.id;
    let mut pending_channels = BTreeSet::new();
    let mut complete = true;
    for team_id in team_ids.into_iter().flatten() {
        let Some(channel_id) = model
            .teams
            .iter()
            .find(|team| team.id == team_id)
            .and_then(|team| team.discord_channel_id.as_deref())
        else {
            tracing::warn!(
                match_id,
                team_id,
                idempotency_key = request_idempotency_key,
                "Scrim-Lobbycode-Teamkanal fehlt; Zustellung bleibt für einen Retry offen"
            );
            complete = false;
            continue;
        };
        if !delivered_channel_ids.contains(channel_id) {
            pending_channels.insert(channel_id);
        }
    }
    let mut channels = pending_channels.into_iter();
    let Some(first) = channels.next() else {
        return (BTreeSet::new(), complete);
    };
    let Some(second) = channels.next() else {
        let delivered =
            distribute_lobby_code_to_channel(state, match_id, first, code, request_idempotency_key)
                .await;
        return (
            BTreeSet::from_iter(delivered.then(|| first.to_string())),
            complete && delivered,
        );
    };
    debug_assert!(channels.next().is_none(), "a match has at most two teams");
    let (first_delivered, second_delivered) = tokio::join!(
        distribute_lobby_code_to_channel(state, match_id, first, code, request_idempotency_key),
        distribute_lobby_code_to_channel(state, match_id, second, code, request_idempotency_key)
    );
    let delivered = [
        first_delivered.then(|| first.to_string()),
        second_delivered.then(|| second.to_string()),
    ]
    .into_iter()
    .flatten()
    .collect();
    (delivered, complete && first_delivered && second_delivered)
}

async fn distribute_lobby_code_to_channel(
    state: &AppState,
    match_id: i32,
    channel_id: &str,
    code: &str,
    request_idempotency_key: &str,
) -> bool {
    let operation = format!("{match_id}\0{channel_id}\0{code}\0{request_idempotency_key}");
    let idempotency_key = format!(
        "scrim-lobby-code-{:x}",
        Sha256::digest(operation.as_bytes())
    );
    match tokio::time::timeout(
        LOBBY_CODE_DISCORD_TIMEOUT,
        state
            .notifier
            .broker()
            .post_internal::<serde_json::Value, _>(
                "/internal/master/v1/discord/send-message",
                &serde_json::json!({
                    "channel_id": channel_id,
                    "content": format!("Lobby Code: {code}"),
                    "idempotency_key": idempotency_key,
                }),
            ),
    )
    .await
    {
        Ok(Ok(_)) => true,
        Ok(Err(error)) => {
            tracing::warn!(
                match_id,
                channel_id,
                idempotency_key = request_idempotency_key,
                %error,
                "Scrim-Lobbycode-Discord-Sync fail-open"
            );
            false
        }
        Err(_) => {
            tracing::warn!(
                match_id,
                channel_id,
                idempotency_key = request_idempotency_key,
                timeout_seconds = LOBBY_CODE_DISCORD_TIMEOUT.as_secs(),
                "Scrim-Lobbycode-Discord-Versand hat das Zeitlimit erreicht; Zustellung bleibt für einen Retry offen"
            );
            false
        }
    }
}

async fn publish_announcement(
    state: &AppState,
    publication: &AnnouncementPreview,
    idempotency_key: &str,
) -> bool {
    let (Some(announcement_id), Some(channel_id)) =
        (publication.id, publication.channel_id.as_deref())
    else {
        tracing::warn!(
            announcement_id = ?publication.id,
            channel_id = ?publication.channel_id,
            idempotency_key,
            "Scrim-Ankündigung hat kein vollständiges Discord-Ziel; Zustellung bleibt für einen Retry offen"
        );
        return false;
    };
    // Wurde der Block schon veroeffentlicht, nicht erneut posten. Bei gleichem
    // Idempotenzschluessel liefert die Datenbank denselben Entwurf zurueck; die
    // Absicherung des Brokers gegen Doppelausfuehrung haelt nur begrenzte Zeit vor,
    // ein spaeter Wiederholungsversuch wuerde die Ankuendigung sonst doppelt posten.
    if publication.published_at.is_some() {
        tracing::debug!(
            announcement_id,
            "Scrim-Ankuendigung bereits veroeffentlicht, kein erneuter Versand"
        );
        return true;
    }
    let result = state
        .notifier
        .broker()
        .post_internal::<serde_json::Value, _>(
            "/internal/master/v1/discord/send-message",
            &serde_json::json!({
                "channel_id": channel_id,
                "content": publication.message,
                "idempotency_key": idempotency_key,
            }),
        )
        .await;
    match result {
        Ok(response) => {
            // Der Broker antwortet als {"ok": true, "result": {...}} — die uebrigen Pfade
            // sind nur Rueckfalloptionen, damit eine abweichende Antwortform nichts verliert.
            let remote_message_id = response
                .pointer("/result/message_id")
                .or_else(|| response.pointer("/data/message_id"))
                .or_else(|| response.get("message_id"))
                .and_then(|value| {
                    value
                        .as_str()
                        .map(ToOwned::to_owned)
                        .or_else(|| value.as_u64().map(|id| id.to_string()))
                })
                .map(|message_id| format!("discord:{channel_id}:{message_id}"));
            if let Err(error) = service(state)
                .repository()
                .mark_announcement_published(announcement_id, remote_message_id.as_deref())
                .await
            {
                tracing::warn!(
                    announcement_id,
                    channel_id,
                    idempotency_key,
                    %error,
                    "Scrim-Ankuendigungsstatus konnte nach Discord-Versand nicht aktualisiert werden"
                );
                return false;
            }
            true
        }
        Err(error) => {
            tracing::warn!(
                announcement_id,
                channel_id,
                idempotency_key,
                %error,
                "Scrim-Ankuendigungs-Discord-Versand fail-open"
            );
            false
        }
    }
}

async fn patch_match_request(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<MatchRequestPatch>,
) -> WebResult<(StatusCode, Json<ActionReceipt>)> {
    let id = parse_db_id(&id, "request_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let payload = json_with_target("request_id", id, &body)?;
    let receipt = service
        .patch_match_request(
            mutation.idempotency_key,
            mutation.request_id,
            &payload,
            id,
            &body,
            (actor.discord_id, actor.display_name),
        )
        .await?;
    Ok((StatusCode::OK, Json(receipt)))
}

async fn create_match_request_reminders(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ReminderRequest>,
) -> WebResult<(StatusCode, Json<ActionReceipt>)> {
    let id = parse_db_id(&id, "request_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let payload = json_with_target("request_id", id, &body)?;
    let receipt = service
        .create_match_request_reminders(
            mutation.idempotency_key,
            mutation.request_id,
            &payload,
            id,
            &body,
            (actor.discord_id, actor.display_name),
        )
        .await?;
    Ok((StatusCode::OK, Json(receipt)))
}

async fn create_match_request_status_publication(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<StatusPublicationRequest>,
) -> WebResult<(StatusCode, Json<ActionReceipt>)> {
    let id = parse_db_id(&id, "request_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let payload = json_with_target("request_id", id, &body)?;
    let dispatch = service
        .create_status_publication(
            mutation.idempotency_key,
            mutation.request_id,
            &payload,
            id,
            &body,
            (actor.discord_id, actor.display_name),
        )
        .await?;
    let delivered = dispatch_discord(
        &state,
        "match_request_status_publication",
        mutation.idempotency_key,
        &dispatch,
    )
    .await;
    if !delivered {
        return Err(WebError::new(StatusCode::BAD_GATEWAY, DISCORD_SYNC_FAILED));
    }
    Ok((StatusCode::OK, Json(dispatch.receipt)))
}

async fn read_replacement_candidates(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<Vec<ReplacementCandidate>>> {
    let id = parse_db_i64(&id, "replacement_need_id")?;
    require_operator(&state, peer, &headers).await?;
    Ok(Json(service(&state).replacement_candidates(id).await?))
}

async fn create_replacement_request(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ReplacementRequestCreate>,
) -> WebResult<(StatusCode, Json<ActionReceipt>)> {
    let id = parse_db_i64(&id, "replacement_need_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let payload = json_with_i64_target("replacement_need_id", id, &body)?;
    let receipt = service
        .create_replacement_request(
            mutation.idempotency_key,
            mutation.request_id,
            &payload,
            id,
            &body,
            (actor.discord_id, actor.display_name),
        )
        .await?;
    Ok((StatusCode::OK, Json(receipt)))
}

async fn patch_replacement_request(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ReplacementRequestPatch>,
) -> WebResult<(StatusCode, Json<ActionReceipt>)> {
    let id = parse_db_i64(&id, "replacement_request_id")?;
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    let payload = json_with_i64_target("replacement_request_id", id, &body)?;
    let request_key = mutation.idempotency_key;
    let response = service
        .patch_replacement_request(
            request_key,
            mutation.request_id,
            &payload,
            id,
            &body,
            (actor.discord_id, actor.display_name),
        )
        .await?;
    if response
        .sync_plans
        .iter()
        .any(|plan| !plan.actions.is_empty())
        && !sync_discord_roles(&state, response.sync_plans, request_key)
            .await
            .ok
    {
        return Err(WebError::new(StatusCode::BAD_GATEWAY, DISCORD_SYNC_FAILED));
    }
    Ok((StatusCode::OK, Json(response.receipt)))
}

async fn signup(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<SignupRequest>,
) -> WebResult<Json<SelfServiceParticipant>> {
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let signup = service(&state)
        .signup(
            actor.discord_id,
            actor.display_name,
            body,
            positive_config_id(state.config.scrim_signup_role_id),
            positive_config_id(state.config.scrim_reserve_role_id),
        )
        .await?;
    if !sync_signup_roles(&state, &signup, mutation.idempotency_key).await {
        return Err(WebError::new(StatusCode::BAD_GATEWAY, DISCORD_SYNC_FAILED));
    }
    Ok(Json(signup.participant))
}

async fn update_my_availability(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<WeeklyAvailability>,
) -> WebResult<Json<SelfServiceParticipant>> {
    require_internal_boundary(peer, &headers, &state)?;
    require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    Ok(Json(
        service(&state)
            .update_availability(actor.discord_id, body)
            .await?,
    ))
}

async fn create_team(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<CreateTeamRequest>,
) -> WebResult<Json<TeamMutationResponse>> {
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let team = service.create_team(mutation.idempotency_key, body).await?;
    // Bei einer Wiederholung liegt die Rolle schon an. Der Idempotenzschutz des Brokers
    // laeuft ueber eine Zwischenspeicherung mit Ablaufzeit, taugt also nicht als
    // Dauerschutz — sonst legte ein spaeter Wiederholungsversuch eine zweite Rolle an
    // und ueberschriebe die hinterlegte ID.
    let existing_role_id = service.roster_team(team.id).await?.discord_role_id;
    let discord_role_id = match existing_role_id {
        Some(role_id) => Some(role_id),
        None => create_team_discord_role(&state, &team, mutation.idempotency_key).await,
    };
    let request_key = mutation.idempotency_key;
    let mutation = service
        .finish_team_creation(team.id, discord_role_id)
        .await?;
    let discord_sync = if discord_role_id.is_some() {
        sync_discord_roles(&state, mutation.sync_plans, request_key).await
    } else {
        DiscordSyncStatus {
            ok: false,
            detail: DISCORD_ROLE_CREATION_FAILED.to_string(),
        }
    };
    Ok(Json(TeamMutationResponse {
        team: mutation.team,
        discord_sync,
    }))
}

async fn patch_team(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<TeamPatchRequest>,
) -> WebResult<Json<TeamMutationResponse>> {
    require_internal_boundary(peer, &headers, &state)?;
    let request_key = require_mutation_headers(&headers)?.idempotency_key;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let mutation = service
        .patch_team(request_key, parse_db_id(&id, "team_id")?, body)
        .await?;
    let discord_sync = sync_discord_roles(&state, mutation.sync_plans, request_key).await;
    Ok(Json(TeamMutationResponse {
        team: mutation.team,
        discord_sync,
    }))
}

async fn patch_participant(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ParticipantPatchRequest>,
) -> WebResult<Json<ParticipantPatchResponse>> {
    require_internal_boundary(peer, &headers, &state)?;
    let request_key = require_mutation_headers(&headers)?.idempotency_key;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let mutation = service
        .patch_participant(
            parse_db_id(&id, "participant_id")?,
            body,
            positive_config_id(state.config.scrim_reserve_role_id),
            positive_config_id(state.config.scrim_signup_role_id),
        )
        .await?;
    let discord_sync = sync_discord_roles(&state, vec![mutation.sync_plan], request_key).await;
    Ok(Json(ParticipantPatchResponse {
        participant: mutation.participant,
        discord_sync,
    }))
}

async fn announce_team(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<AnnounceTeamRequest>,
) -> WebResult<Json<AnnounceTeamResponse>> {
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    if body
        .note
        .as_ref()
        .is_some_and(|note| note.chars().count() > 500)
    {
        return Err(WebError::bad_request(
            "Die Notiz ist zu lang — höchstens 500 Zeichen.",
        ));
    }
    let team = service.roster_team(parse_db_id(&id, "team_id")?).await?;
    Ok(Json(
        post_team_announcement(&state, &team, body.note, mutation.idempotency_key).await,
    ))
}

async fn suggest_team(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<SuggestTeamRequest>,
) -> WebResult<Json<turnier_scrim::dto::RosterSuggestResponse>> {
    require_internal_boundary(peer, &headers, &state)?;
    require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    Ok(Json(
        service
            .suggest_roster(parse_db_id(&id, "team_id")?, body)
            .await?,
    ))
}

async fn substitute(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<SubstituteRequest>,
) -> WebResult<Json<SubstituteResponse>> {
    require_internal_boundary(peer, &headers, &state)?;
    let request = require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let window = body.window.clone();
    let mutation = service
        .substitute(
            request.idempotency_key,
            parse_db_id(&id, "team_id")?,
            body.participant_id,
            body.window,
            positive_config_id(state.config.scrim_reserve_role_id),
            positive_config_id(state.config.scrim_signup_role_id),
        )
        .await?;
    let discord_sync =
        sync_discord_roles(&state, vec![mutation.sync_plan], request.idempotency_key).await;
    let dm = send_substitute_dm(
        &state,
        mutation.participant.id,
        mutation.discord_user_id,
        &mutation.team_name,
        window,
        request.idempotency_key,
    )
    .await;
    Ok(Json(SubstituteResponse {
        participant: mutation.participant,
        discord_sync,
        dm,
    }))
}

async fn resync_participant_discord(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> WebResult<Json<DiscordResyncResponse>> {
    require_internal_boundary(peer, &headers, &state)?;
    let request_key = require_mutation_headers(&headers)?.idempotency_key;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let plan = service
        .participant_resync_plan(
            parse_db_id(&id, "participant_id")?,
            positive_config_id(state.config.scrim_reserve_role_id),
            positive_config_id(state.config.scrim_signup_role_id),
        )
        .await?;
    let discord_sync = sync_discord_roles(&state, vec![plan], request_key).await;
    Ok(Json(DiscordResyncResponse { discord_sync }))
}

async fn create_team_discord_role(
    state: &AppState,
    team: &turnier_scrim::dto::RosterTeam,
    idempotency_key: &str,
) -> Option<i64> {
    let Some(guild_id) = positive_config_id(Some(state.config.scrim_guild_id)) else {
        tracing::warn!(
            team_id = team.id,
            team_name = %team.name,
            "Scrim-Team-Rolle konnte ohne gültige Guild-ID nicht erstellt werden; Team bleibt gespeichert"
        );
        return None;
    };
    let response = state
        .notifier
        .broker()
        .post_internal::<serde_json::Value, _>(
            "/internal/master/v1/discord/role/create",
            &serde_json::json!({
                "guild_id": guild_id,
                "name": team.name,
                "mentionable": false,
                "reason": "scrim team role",
                "idempotency_key": idempotency_key,
            }),
        )
        .await;
    match response {
        Ok(response) => response
            .pointer("/result/role_id")
            // Discord-IDs kommen als Zahl oder als Zeichenkette — in JSON ist die
            // Zeichenkette die uebliche Form, weil die IDs groesser als 2^53 werden.
            // Nur Zahlen zu akzeptieren hiesse: Rolle angelegt, aber nicht gemerkt.
            .and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
            })
            .and_then(|role_id| i64::try_from(role_id).ok())
            .filter(|role_id| *role_id > 0)
            .or_else(|| {
                tracing::warn!(
                    team_id = team.id,
                    team_name = %team.name,
                    "Scrim-Team-Rollen-Antwort war ungültig; Team bleibt gespeichert"
                );
                None
            }),
        Err(error) => {
            tracing::warn!(
                team_id = team.id,
                team_name = %team.name,
                %error,
                "Scrim-Team-Rolle konnte nicht erstellt werden; Team bleibt gespeichert"
            );
            None
        }
    }
}

/// `operation_key` benennt den ausloesenden Vorgang und geht in den Idempotenzschluessel ein.
///
/// Der Broker merkt sich erfolgreiche Rollenaktionen fuer eine begrenzte Zeit. Ohne
/// Vorgangsbezug waere "Rolle entziehen und kurz darauf neu vergeben" innerhalb dieser
/// Zeitspanne ein Treffer im Zwischenspeicher: die Datenbank fuehrt die Rolle wieder,
/// Discord hat sie nicht. Mit Vorgangsbezug greift die Absicherung weiterhin bei einer
/// Wiederholung desselben Aufrufs, aber nicht mehr ueber verschiedene Vorgaenge hinweg.
async fn sync_discord_roles(
    state: &AppState,
    plans: Vec<DiscordRoleSyncPlan>,
    operation_key: &str,
) -> DiscordSyncStatus {
    let Some(guild_id) = positive_config_id(Some(state.config.scrim_guild_id)) else {
        tracing::warn!("Scrim-Rollen-Sync ohne gueltige Guild-ID");
        return DiscordSyncStatus {
            ok: false,
            detail: DISCORD_SYNC_NOT_CONFIGURED.to_string(),
        };
    };
    let mut ok = true;
    let mut changed = false;
    let mut tasks = tokio::task::JoinSet::new();
    for plan in plans {
        let Some(discord_user_id) = plan.discord_user_id else {
            continue;
        };
        for action in plan.actions {
            changed = true;
            let (path, operation) = match action.operation {
                RoleOperation::Add => ("/internal/master/v1/discord/member/add-role", "add"),
                RoleOperation::Remove => {
                    ("/internal/master/v1/discord/member/remove-role", "remove")
                }
            };
            let broker = state.notifier.broker().clone();
            let subject = plan.subject.clone();
            let role_id = action.role_id;
            let payload = serde_json::json!({
                "guild_id": guild_id,
                "user_id": discord_user_id,
                "role_id": role_id,
                "reason": format!("scrim {subject} {operation} role {role_id}"),
                "idempotency_key": format!(
                    "scrim-{operation_key}-{}-{}-{operation}",
                    subject, role_id
                ),
            });
            tasks.spawn(async move {
                match broker
                    .post_internal::<serde_json::Value, _>(path, &payload)
                    .await
                {
                    Ok(_) => true,
                    Err(error) => {
                        tracing::warn!(
                            subject,
                            participant_id = subject,
                            user_id = discord_user_id,
                            role_id,
                            operation,
                            %error,
                            "Scrim-Discord-Rollen-Sync fail-open"
                        );
                        false
                    }
                }
            });
        }
    }
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(action_ok) => ok &= action_ok,
            Err(error) => {
                ok = false;
                tracing::warn!(%error, "Scrim-Discord-Rollen-Sync-Task fehlgeschlagen");
            }
        }
    }
    DiscordSyncStatus {
        ok: ok || !changed,
        detail: if !changed {
            DISCORD_SYNC_NOOP.to_string()
        } else if ok {
            DISCORD_SYNC_SUCCESS.to_string()
        } else {
            DISCORD_SYNC_FAILED.to_string()
        },
    }
}

async fn post_team_announcement(
    state: &AppState,
    team: &turnier_scrim::dto::RosterTeam,
    note: Option<String>,
    idempotency_key: &str,
) -> AnnounceTeamResponse {
    let Some(channel_id) = positive_config_id(Some(state.config.scrim_announce_channel_id)) else {
        tracing::warn!(team_id = team.id, "Scrim-Ankündigung ohne gültigen Kanal");
        return AnnounceTeamResponse {
            message_id: None,
            ok: false,
            detail: "Kein Ankündigungskanal konfiguriert — es wurde nichts gepostet.".to_string(),
        };
    };
    let allowed_role_ids = positive_config_id(state.config.scrim_signup_role_id)
        .into_iter()
        .collect::<Vec<_>>();
    // Wortgleich zum bisherigen Weg (Website routes/scrim.rs, build_team_announcement):
    // Die Community soll nach der Umstellung dieselbe Nachricht sehen.
    let content = allowed_role_ids
        .first()
        .map(|role_id| format!("<@&{role_id}>"))
        .unwrap_or_default();
    let description = match (team.default_from, team.default_to) {
        (Some(from), Some(to)) => format!(
            "Das Team spielt üblicherweise **{}**. Wenn du zu der Zeit kannst und Lust hast, reagier hier mit ✅ — wir melden uns bei dir.",
            format_team_window(from, to)
        ),
        _ => "Wenn du Lust hast, in diesem Team zu spielen, reagier hier mit ✅ — wir melden uns bei dir."
            .to_string(),
    };
    let response = state
        .notifier
        .broker()
        .post_internal::<serde_json::Value, _>(
            "/internal/master/v1/discord/send-rich-message",
            &serde_json::json!({
                "channel_id": channel_id,
                "content": content,
                "embed": {
                    "color": 0x00C8_A86B,
                    "title": format!("{} sucht Verstärkung", team.name),
                    "description": description,
                    "fields": note.map(|value| serde_json::json!({
                        "name": "Dazu noch",
                        "value": value,
                        "inline": false,
                    })).into_iter().collect::<Vec<_>>(),
                    "footer": {"text": "Deutsche Deadlock Community"},
                },
                "allowed_role_ids": allowed_role_ids,
                "idempotency_key": idempotency_key,
            }),
        )
        .await;
    let message_id = match response {
        Ok(response) => {
            // Zahl oder Zeichenkette akzeptieren: ohne Message-ID wird die Reaktion nicht
            // gesetzt, und dann kann sich niemand auf die Ankuendigung melden.
            let message_id = response
                .pointer("/result/message_id")
                .and_then(|value| {
                    value
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| value.as_u64().map(|id| id.to_string()))
                })
                .filter(|message_id| !message_id.is_empty());
            if message_id.is_none() {
                tracing::warn!(
                    team_id = team.id,
                    "Scrim-Ankündigungsantwort enthält keine Message-ID"
                );
            }
            message_id
        }
        Err(error) => {
            tracing::warn!(
                team_id = team.id,
                %error,
                "Scrim-Ankündigung konnte nicht gepostet werden"
            );
            None
        }
    };
    let Some(message_id) = message_id else {
        return AnnounceTeamResponse {
            message_id: None,
            ok: false,
            detail: "Die Ankündigung konnte nicht gepostet werden. Versuch es gleich noch mal."
                .to_string(),
        };
    };
    let detail = match state
        .notifier
        .broker()
        .post_internal::<serde_json::Value, _>(
            "/internal/master/v1/discord/add-reaction",
            &serde_json::json!({
                "channel_id": channel_id,
                "message_id": message_id,
                "emoji": "✅",
                "idempotency_key": format!("{idempotency_key}:reaction"),
            }),
        )
        .await
    {
        Ok(_) => "Ankündigung ist gepostet.".to_string(),
        Err(error) => {
            tracing::warn!(
                team_id = team.id,
                channel_id,
                message_id,
                %error,
                "Scrim-Ankündigungsreaktion konnte nicht gesetzt werden"
            );
            "Der Aufruf steht im Scrim-Kanal, aber der ✅-Haken konnte nicht gesetzt werden. Setz ihn bitte einmal selbst darunter.".to_string()
        }
    };
    AnnounceTeamResponse {
        message_id: Some(message_id),
        ok: true,
        detail,
    }
}

/// DM an eine Aushilfe, wortgleich zum bisherigen Weg (Website routes/scrim.rs).
/// Sagt explizit, dass der Auswechselspieler-Status bleibt — sonst denken Leute,
/// sie waeren fest im Team.
fn substitute_dm_content(team_name: &str, window: &ScrimSlot) -> String {
    let day = match window.day {
        ScrimDay::Monday => "Montag",
        ScrimDay::Tuesday => "Dienstag",
        ScrimDay::Wednesday => "Mittwoch",
        ScrimDay::Thursday => "Donnerstag",
        ScrimDay::Friday => "Freitag",
        ScrimDay::Saturday => "Samstag",
        ScrimDay::Sunday => "Sonntag",
    };
    let format_minutes = |minutes: u16| format!("{:02}:{:02}", minutes / 60, minutes % 60);
    let time = format!(
        "{day}, {}–{} Uhr",
        format_minutes(window.from),
        format_minutes(window.to)
    );
    format!(
        "Hey! 👋 Du springst für **{team_name}** ein — **{time}**.\n\nDie Team-Rolle hast du gerade bekommen, damit siehst du den Team-Kanal und wirst bei Pings mitgenommen. Du bleibst weiterhin Auswechselspieler.\n\nWenn's doch nicht klappt, sag bitte kurz im Team-Kanal Bescheid, damit wir Ersatz finden. Viel Spaß! 🎮"
    )
}

/// Stammzeit als Text, wortgleich zum bisherigen Weg (Website routes/scrim.rs).
/// `1440` steht fuer offenes Ende, deshalb "ab 20:00 Uhr" statt "20:00–24:00 Uhr".
fn format_team_window(default_from: i32, default_to: i32) -> String {
    let format_minutes = |minutes: i32| format!("{:02}:{:02}", minutes / 60, minutes % 60);
    if default_to == 1440 {
        format!("ab {} Uhr", format_minutes(default_from))
    } else {
        format!(
            "{}–{} Uhr",
            format_minutes(default_from),
            format_minutes(default_to)
        )
    }
}

async fn send_substitute_dm(
    state: &AppState,
    participant_id: i32,
    discord_user_id: Option<u64>,
    team_name: &str,
    window: ScrimSlot,
    idempotency_key: &str,
) -> DiscordSyncStatus {
    let Some(user_id) = discord_user_id else {
        return DiscordSyncStatus {
            ok: false,
            detail: DM_NO_ACCOUNT.to_string(),
        };
    };
    let result = state
        .notifier
        .broker()
        .post_internal::<serde_json::Value, _>(
            "/internal/master/v1/discord/send-dm",
            &serde_json::json!({
                "user_id": user_id,
                "content": substitute_dm_content(team_name, &window),
                "team_name": team_name,
                "window": window,
                "idempotency_key": format!("{idempotency_key}:dm"),
            }),
        )
        .await;
    if let Err(error) = result {
        tracing::warn!(
            participant_id,
            user_id,
            %error,
            "Scrim-Aushilfe-DM fail-open"
        );
        return DiscordSyncStatus {
            ok: false,
            detail: DM_FAILED.to_string(),
        };
    }
    DiscordSyncStatus {
        ok: true,
        detail: DM_SUCCESS.to_string(),
    }
}

async fn sync_signup_roles(
    state: &AppState,
    signup: &SignupMutation,
    idempotency_key: &str,
) -> bool {
    if signup.role_ids.is_empty() {
        return true;
    }
    let Some(discord_user_id) = signup.discord_user_id else {
        tracing::warn!(
            participant_id = signup.participant.id,
            idempotency_key,
            "Scrim-Signup-Discord-Sync hat kein Discord-Ziel"
        );
        return false;
    };
    let Some(guild_id) = positive_config_id(Some(state.config.scrim_guild_id)) else {
        tracing::warn!(
            participant_id = signup.participant.id,
            idempotency_key,
            "SCRIM_GUILD_ID ist ungültig; Scrim-Signup-Rollen bleiben für Retry offen"
        );
        return false;
    };
    let mut delivered = true;
    for role_id in &signup.role_ids {
        let reason = format!("scrim {} add role {}", signup.participant.id, role_id);
        let role_idempotency_key = format!("scrim-{}-{}-add", signup.participant.id, role_id);
        if let Err(error) = state
            .notifier
            .broker()
            .post_internal::<serde_json::Value, _>(
                "/internal/master/v1/discord/member/add-role",
                &serde_json::json!({
                    "guild_id": guild_id,
                    "user_id": discord_user_id,
                    "role_id": role_id,
                    "reason": reason,
                    "idempotency_key": role_idempotency_key,
                }),
            )
            .await
        {
            delivered = false;
            tracing::warn!(
                participant_id = signup.participant.id,
                idempotency_key,
                user_id = discord_user_id,
                role_id,
                %error,
                "Scrim-Signup-Discord-Sync fail-open"
            );
        }
    }
    delivered
}

async fn dispatch_discord(
    state: &AppState,
    scope: &str,
    idempotency_key: &str,
    dispatch: &MutationDispatch,
) -> bool {
    let mut delivered = Vec::new();
    let mut failed = false;
    for target in &dispatch.discord {
        let result = if let Some(user_id) = target.user_id {
            state
                .notifier
                .broker()
                .post_internal::<serde_json::Value, _>(
                    "/internal/master/v1/discord/send-dm",
                    &serde_json::json!({
                        "user_id": user_id,
                        "content": target.content,
                        "idempotency_key": format!("scrim_dispatch:{}:{}:user:{}", target.kind, target.record_id, user_id),
                    }),
                )
                .await
        } else if let Some(channel_id) = target.channel_id {
            state
                .notifier
                .broker()
                .post_internal::<serde_json::Value, _>(
                    "/internal/master/v1/discord/send-message",
                    &serde_json::json!({
                        "channel_id": channel_id,
                        "content": target.content,
                        "idempotency_key": format!("scrim_dispatch:{}:{}:channel:{}", target.kind, target.record_id, channel_id),
                    }),
                )
                .await
        } else {
            failed = true;
            tracing::warn!(
                scope,
                idempotency_key,
                record_id = target.record_id,
                "Scrim-Discord-Versand ohne Ziel fail-open"
            );
            continue;
        };
        match result {
            Ok(_) => delivered.push(target.clone()),
            Err(error) => {
                failed = true;
                tracing::warn!(
                    scope,
                    idempotency_key,
                    record_id = target.record_id,
                    user_id = target.user_id,
                    channel_id = target.channel_id,
                    %error,
                    "Scrim-Discord-Versand fail-open"
                );
            }
        }
    }
    if let Err(error) = PgScrimReadRepository::new(state.pool.clone())
        .mark_dispatches_delivered(scope, idempotency_key, &delivered)
        .await
    {
        failed = true;
        tracing::warn!(
            scope,
            idempotency_key,
            %error,
            "Zugestellte Scrim-Discord-Nachrichten konnten nicht vermerkt werden"
        );
    }
    !failed
}

fn positive_config_id(value: Option<i64>) -> Option<u64> {
    value
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
}

async fn match_request_response(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<MatchRequestResponseRequest>,
) -> WebResult<(StatusCode, Json<ActionReceipt>)> {
    require_internal_boundary(peer, &headers, &state)?;
    let mutation = require_mutation_headers(&headers)?;
    validate_interaction(&body, mutation.idempotency_key)?;
    let payload = serde_json::to_value(&body)
        .map_err(|_| WebError::internal("Scrim-Payload konnte nicht serialisiert werden"))?;
    let receipt = service(&state)
        .repository()
        .record_match_request_response(
            mutation.idempotency_key,
            mutation.request_id,
            &payload,
            &body,
        )
        .await?;
    Ok((StatusCode::OK, Json(receipt)))
}

async fn operator_model(
    state: &AppState,
    peer: SocketAddr,
    headers: &HeaderMap,
) -> WebResult<ScrimReadModel> {
    require_operator(state, peer, headers).await?;
    Ok(service(state).read_model().await?)
}

async fn require_operator<'a>(
    state: &AppState,
    peer: SocketAddr,
    headers: &'a HeaderMap,
) -> WebResult<BffActor<'a>> {
    require_internal_boundary(peer, headers, state)?;
    let service = service(state);
    let actor = require_bff_actor(headers)?;
    service.authorize_operator(actor.discord_id).await?;
    Ok(actor)
}

fn find_team(model: &ScrimReadModel, id: i32) -> WebResult<Team> {
    model
        .teams
        .iter()
        .find(|team| team.id == id)
        .cloned()
        .ok_or_else(|| WebError::not_found("Scrim-Team nicht gefunden"))
}

fn find_match_request(model: ScrimReadModel, id: i32) -> WebResult<MatchRequest> {
    model
        .match_request_batches
        .into_iter()
        .flat_map(|batch| batch.requests)
        .find(|request| request.id == id)
        .ok_or_else(|| WebError::not_found("Match-Request nicht gefunden"))
}

fn match_has_team(scrim_match: &ScrimMatch, team_id: i32) -> bool {
    scrim_match
        .team_a
        .as_ref()
        .is_some_and(|team| team.id == team_id)
        || scrim_match
            .team_b
            .as_ref()
            .is_some_and(|team| team.id == team_id)
}

fn next_match_for_team(matches: &[ScrimMatch], team_id: i32) -> Option<ScrimMatch> {
    let now = Utc::now();
    matches
        .iter()
        .filter(|scrim_match| is_next_match_candidate(scrim_match, team_id, now))
        .min_by(compare_next_match)
        .cloned()
}

fn is_next_match_candidate(scrim_match: &ScrimMatch, team_id: i32, now: DateTime<Utc>) -> bool {
    if !match_has_team(scrim_match, team_id) || scrim_match.selected_result.is_some() {
        return false;
    }
    let status = scrim_match.status.to_ascii_lowercase();
    if matches!(
        status.as_str(),
        "cancelled" | "canceled" | "finished" | "completed" | "played" | "closed"
    ) {
        return false;
    }
    if scrim_match.lobby_state.as_deref().is_some_and(|state| {
        matches!(
            state.to_ascii_lowercase().as_str(),
            "finished" | "cancelled" | "canceled"
        )
    }) {
        return false;
    }
    scrim_match
        .scheduled_at
        .is_none_or(|scheduled_at| scheduled_at >= now || is_active_match_status(&status))
}

fn is_active_match_status(status: &str) -> bool {
    matches!(
        status,
        "active" | "running" | "live" | "in_progress" | "ongoing" | "started"
    )
}

fn compare_next_match(left: &&ScrimMatch, right: &&ScrimMatch) -> Ordering {
    match (left.scheduled_at, right.scheduled_at) {
        (Some(left_at), Some(right_at)) => left_at
            .cmp(&right_at)
            .then_with(|| left.created_at.cmp(&right.created_at))
            .then_with(|| left.id.cmp(&right.id)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left
            .created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id)),
    }
}

fn planning_batch(body: PlanningCreateRequest) -> WebResult<MatchRequestBatchInput> {
    let slots = body.slots.map(|slots| {
        slots
            .into_iter()
            .map(|slot| ScrimSlot {
                day: slot.day,
                from: slot.from_minute,
                to: slot.to_minute,
            })
            .collect::<Vec<_>>()
    });
    let matches = body
        .pairings
        .into_iter()
        .map(|pairing| {
            let slots = pairing.slots.map(|slots| {
                slots
                    .into_iter()
                    .map(|slot| ScrimSlot {
                        day: slot.day,
                        from: slot.from_minute,
                        to: slot.to_minute,
                    })
                    .collect::<Vec<_>>()
            });
            Ok(turnier_scrim::model::MatchRequestPairingInput {
                team_a_id: parse_db_id(&pairing.team_a_id, "team_a_id")?,
                team_b_id: pairing
                    .team_b_id
                    .as_deref()
                    .map(|id| parse_db_id(id, "team_b_id"))
                    .transpose()?,
                slots,
            })
        })
        .collect::<WebResult<Vec<_>>>()?;
    Ok(MatchRequestBatchInput {
        template: planning_template(body.technical_template_key.as_deref())?,
        deadline_at: body.deadline_at,
        slots,
        matches,
    })
}

fn service(state: &AppState) -> ScrimService<PgScrimReadRepository> {
    ScrimService::new(PgScrimReadRepository::new(state.pool.clone()))
}

fn require_internal_boundary(
    peer: SocketAddr,
    headers: &HeaderMap,
    state: &AppState,
) -> WebResult<()> {
    if !peer.ip().is_loopback() {
        return Err(WebError::forbidden(
            "Scrim Internal API ist nur lokal erreichbar",
        ));
    }
    let expected = state.config.turnier_internal_api_token.trim();
    let supplied = header(headers, INTERNAL_TOKEN_HEADER).unwrap_or_default();
    if expected.is_empty() || supplied != expected {
        return Err(WebError::unauthorized("Interne Authentifizierung fehlt"));
    }
    Ok(())
}

struct MutationHeaders<'a> {
    request_id: &'a str,
    idempotency_key: &'a str,
}

fn require_mutation_headers(headers: &HeaderMap) -> WebResult<MutationHeaders<'_>> {
    let request_id = required_header(headers, REQUEST_ID_HEADER)?;
    require_domain_ref(request_id, REQUEST_ID_HEADER)?;
    let idempotency_key = required_header(headers, IDEMPOTENCY_KEY_HEADER)?;
    require_domain_ref(idempotency_key, IDEMPOTENCY_KEY_HEADER)?;
    Ok(MutationHeaders {
        request_id,
        idempotency_key,
    })
}

struct BffActor<'a> {
    discord_id: &'a str,
    display_name: &'a str,
}

fn require_bff_actor(headers: &HeaderMap) -> WebResult<BffActor<'_>> {
    let discord_id = required_header(headers, ACTOR_DISCORD_ID_HEADER)?;
    require_positive_decimal(discord_id, "actor Discord ID")?;
    let display_name = required_header(headers, ACTOR_DISPLAY_NAME_HEADER)?;
    Ok(BffActor {
        discord_id,
        display_name,
    })
}

fn validate_interaction(
    body: &MatchRequestResponseRequest,
    idempotency_key: &str,
) -> WebResult<()> {
    if body.schema_version != MATCH_REQUEST_RESPONSE_SCHEMA_VERSION {
        return Err(WebError::bad_request(
            "Nicht unterstützte Scrim-Interaction-Version",
        ));
    }
    if body.idempotency != idempotency_key || body.event != body.idempotency {
        return Err(WebError::bad_request(
            "Idempotency-Key stimmt nicht überein",
        ));
    }
    require_domain_ref(&body.event, "event")?;
    require_domain_ref(&body.idempotency, "idempotency")?;
    for (value, name) in [
        (&body.request, "request"),
        (&body.team, "team"),
        (&body.interaction, "interaction"),
        (&body.guild, "guild"),
        (&body.channel, "channel"),
        (&body.actor, "actor"),
    ] {
        require_positive_decimal(value, name)?;
    }
    if let Some(message_id) = &body.message {
        require_positive_decimal(message_id, "message")?;
    }
    if body
        .actor_role_ids
        .iter()
        .any(|role_id| require_positive_decimal(role_id, "actor_role_id").is_err())
    {
        return Err(WebError::bad_request("actor_role_id ist ungültig"));
    }
    match (body.action, body.slot) {
        (MatchRequestAction::Slot, Some(_)) | (MatchRequestAction::None, None) => Ok(()),
        _ => Err(WebError::bad_request(
            "action und slot passen nicht zusammen",
        )),
    }
}

fn required_header<'a>(headers: &'a HeaderMap, name: &str) -> WebResult<&'a str> {
    header(headers, name)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| WebError::bad_request(format!("{name} fehlt")))
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
}

fn require_positive_decimal(value: &str, name: &str) -> WebResult<()> {
    if !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|id| id > 0)
    {
        Ok(())
    } else {
        Err(WebError::bad_request(format!("{name} ist ungültig")))
    }
}

fn require_domain_ref(value: &str, name: &str) -> WebResult<()> {
    let Some((prefix, rest)) = value.split_once(':') else {
        return Err(WebError::bad_request(format!("{name} ist ungültig")));
    };
    let prefix_valid = (2..=32).contains(&prefix.len())
        && prefix.bytes().enumerate().all(|(index, byte)| {
            matches!(
                (index, byte),
                (0, b'a'..=b'z') | (_, b'a'..=b'z' | b'0'..=b'9' | b'_')
            )
        });
    let mut rest_bytes = rest.bytes();
    let rest_valid = (1..=96).contains(&rest.len())
        && rest_bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && rest_bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-'));
    if prefix_valid && rest_valid {
        Ok(())
    } else {
        Err(WebError::bad_request(format!("{name} ist ungültig")))
    }
}

fn parse_db_id(value: &str, name: &str) -> WebResult<i32> {
    require_positive_decimal(value, name)?;
    value
        .parse::<i32>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| WebError::bad_request(format!("{name} ist zu groß")))
}

fn parse_i64_db_id(value: &str, name: &str) -> WebResult<i64> {
    require_positive_decimal(value, name)?;
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| WebError::bad_request(format!("{name} ist zu groß")))
}

fn parse_db_i64(value: &str, name: &str) -> WebResult<i64> {
    require_positive_decimal(value, name)?;
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| WebError::bad_request(format!("{name} ist zu groß")))
}

fn planning_template(key: Option<&str>) -> WebResult<MatchRequestTemplate> {
    match key.map(str::trim).filter(|key| !key.is_empty()) {
        None | Some("regular_scrim" | "regular-scrim" | "bo3-default") => {
            Ok(MatchRequestTemplate::RegularScrim)
        }
        Some("testmatch") => Ok(MatchRequestTemplate::Testmatch),
        Some("training") => Ok(MatchRequestTemplate::Training),
        Some(_) => Err(WebError::bad_request(
            "technical_template_key ist unbekannt",
        )),
    }
}

fn json_with_target<T: serde::Serialize>(
    name: &str,
    id: i32,
    body: &T,
) -> WebResult<serde_json::Value> {
    let body = serde_json::to_value(body)
        .map_err(|_| WebError::internal("Scrim-Payload konnte nicht serialisiert werden"))?;
    Ok(serde_json::json!({
        name: id.to_string(),
        "body": body,
    }))
}

fn json_with_i64_target<T: serde::Serialize>(
    name: &str,
    id: i64,
    body: &T,
) -> WebResult<serde_json::Value> {
    let body = serde_json::to_value(body)
        .map_err(|_| WebError::internal("Scrim-Payload konnte nicht serialisiert werden"))?;
    Ok(serde_json::json!({
        name: id.to_string(),
        "body": body,
    }))
}
