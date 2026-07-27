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
    Ok(Json(
        service
            .select_result_ref(
                mutation.idempotency_key,
                id,
                ref_id,
                actor.discord_id,
                actor.display_name,
                body,
            )
            .await?,
    ))
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
    let receipt = service
        .patch_replacement_request(
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
        .patch_team(parse_db_id(&id, "team_id")?, body)
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
