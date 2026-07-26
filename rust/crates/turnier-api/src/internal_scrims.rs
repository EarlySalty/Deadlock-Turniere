//! Loopback-only internal API for the canonical Scrim boundary.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};

use turnier_scrim::decision::validate_match_request_batch;
use turnier_scrim::dto::{
    ActionReceipt, AnnounceTeamRequest, AnnounceTeamResponse, CapabilityReceipt, CreateTeamRequest,
    DiscordResyncResponse, DiscordSyncStatus, MatchRequestAction, MatchRequestDefaults,
    MatchRequestResponseRequest, ParticipantPatchRequest, ParticipantPatchResponse,
    PlanningCreateRequest, ReleaseMatchRequest, SelfServiceParticipant, SignupRequest,
    SubstituteRequest, SubstituteResponse, SuggestTeamRequest, TeamMutationResponse,
    TeamPatchRequest, WeeklyAvailability, MATCH_REQUEST_RESPONSE_SCHEMA_VERSION,
};
use turnier_scrim::model::{
    Coach, MatchRequest, MatchRequestBatch, MatchRequestBatchInput, MatchRequestTemplate,
    Participant, ReplacementNeed, ScrimDay, ScrimMatch, ScrimMe, ScrimReadModel, ScrimSlot, Team,
    TeamBoard, TeamRef, TeamTimeline,
};
use turnier_scrim::repository::{
    DiscordRoleSyncPlan, PgScrimReadRepository, RoleOperation, ScrimReadRepository, SignupMutation,
};
use turnier_scrim::service::ScrimService;

use crate::error::{WebError, WebResult};
use crate::state::AppState;

const INTERNAL_TOKEN_HEADER: &str = "X-Internal-Token";
const REQUEST_ID_HEADER: &str = "X-Request-Id";
const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";
const ACTOR_DISCORD_ID_HEADER: &str = "X-Actor-Discord-Id";
const ACTOR_DISPLAY_NAME_HEADER: &str = "X-Actor-Display-Name";
const CAPABILITY_DISABLED: &str = "Scrim-Funktion ist während des Cutovers noch deaktiviert.";

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
            get(read_match_request).patch(disabled_operator_path_mutation),
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
            post(disabled_operator_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/match-requests/{id}/status-publications",
            post(disabled_operator_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/replacement-needs/{id}/candidates",
            get(disabled_operator_read),
        )
        .route(
            "/internal/turnier/v1/scrims/replacement-needs/{id}/requests",
            post(disabled_operator_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/replacement-requests/{id}",
            patch(disabled_operator_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/matches",
            get(read_matches).post(disabled_operator_mutation),
        )
        .route("/internal/turnier/v1/scrims/matches/{id}", get(read_match))
        .route(
            "/internal/turnier/v1/scrims/matches/{id}/lobby-code",
            put(disabled_operator_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/matches/{id}/match-ids",
            post(disabled_operator_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/matches/{id}/result-fetches",
            post(disabled_operator_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/matches/{id}/result-refs/{ref_id}",
            patch(disabled_operator_two_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/blocks/{id}/announcement-preview",
            get(disabled_operator_read),
        )
        .route(
            "/internal/turnier/v1/scrims/blocks/{id}/announcement-publications",
            post(disabled_operator_path_mutation),
        )
        .route(
            "/internal/turnier/v1/scrims/actions/{id}",
            get(disabled_operator_read),
        )
        .route(
            "/internal/turnier/v1/scrims/interactions/match-request-response",
            post(match_request_response),
        )
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

async fn signup(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<SignupRequest>,
) -> WebResult<Json<SelfServiceParticipant>> {
    require_internal_boundary(peer, &headers, &state)?;
    require_mutation_headers(&headers)?;
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
    sync_signup_roles(&state, &signup).await;
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
    let discord_role_id = create_team_discord_role(&state, &team, mutation.idempotency_key).await;
    let mutation = service
        .finish_team_creation(team.id, discord_role_id)
        .await?;
    let discord_sync = sync_discord_roles(&state, mutation.sync_plans).await;
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
    require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    let service = service(&state);
    service.authorize_operator(actor.discord_id).await?;
    let mutation = service
        .patch_team(parse_db_id(&id, "team_id")?, body)
        .await?;
    let discord_sync = sync_discord_roles(&state, mutation.sync_plans).await;
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
    require_mutation_headers(&headers)?;
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
    let discord_sync = sync_discord_roles(&state, vec![mutation.sync_plan]).await;
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
        return Err(WebError::bad_request("Platzhalter"));
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
    let discord_sync = sync_discord_roles(&state, vec![mutation.sync_plan]).await;
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
    require_mutation_headers(&headers)?;
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
    let discord_sync = sync_discord_roles(&state, vec![plan]).await;
    Ok(Json(DiscordResyncResponse { discord_sync }))
}

async fn create_team_discord_role(
    state: &AppState,
    team: &turnier_scrim::dto::RosterTeam,
    idempotency_key: &str,
) -> Option<i64> {
    let Some(guild_id) = positive_config_id(Some(state.config.scrim_guild_id)) else {
        tracing::warn!(team_id = team.id, "Scrim-Team-Rolle ohne gueltige Guild-ID");
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
            .and_then(serde_json::Value::as_u64)
            .and_then(|role_id| i64::try_from(role_id).ok())
            .or_else(|| {
                tracing::warn!(
                    team_id = team.id,
                    "Scrim-Team-Rollen-Antwort war ungueltig; DB-Mutation bleibt bestehen"
                );
                None
            }),
        Err(error) => {
            tracing::warn!(
                team_id = team.id,
                %error,
                "Scrim-Team-Rolle konnte nicht erstellt werden; DB-Mutation bleibt bestehen"
            );
            None
        }
    }
}

async fn sync_discord_roles(
    state: &AppState,
    plans: Vec<DiscordRoleSyncPlan>,
) -> DiscordSyncStatus {
    let Some(guild_id) = positive_config_id(Some(state.config.scrim_guild_id)) else {
        tracing::warn!("Scrim-Rollen-Sync ohne gueltige Guild-ID");
        return DiscordSyncStatus {
            ok: false,
            detail: "Platzhalter".to_string(),
        };
    };
    let mut ok = true;
    let mut changed = false;
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
            let result = state
                .notifier
                .broker()
                .post_internal::<serde_json::Value, _>(
                    path,
                    &serde_json::json!({
                        "guild_id": guild_id,
                        "user_id": discord_user_id,
                        "role_id": action.role_id,
                        "reason": format!("scrim {} {operation} role {}", plan.subject, action.role_id),
                        "idempotency_key": format!(
                            "scrim-{}-{}-{operation}",
                            plan.subject, action.role_id
                        ),
                    }),
                )
                .await;
            if let Err(error) = result {
                ok = false;
                tracing::warn!(
                    subject = %plan.subject,
                    user_id = discord_user_id,
                    role_id = action.role_id,
                    operation,
                    %error,
                    "Scrim-Discord-Rollen-Sync fail-open"
                );
            }
        }
    }
    DiscordSyncStatus {
        ok: ok || !changed,
        detail: "Platzhalter".to_string(),
    }
}

async fn post_team_announcement(
    state: &AppState,
    team: &turnier_scrim::dto::RosterTeam,
    note: Option<String>,
    idempotency_key: &str,
) -> AnnounceTeamResponse {
    let Some(channel_id) = positive_config_id(Some(state.config.scrim_announce_channel_id)) else {
        tracing::warn!(team_id = team.id, "Scrim-Ankuendigung ohne gueltigen Kanal");
        return AnnounceTeamResponse {
            message_id: None,
            ok: false,
            detail: "Platzhalter".to_string(),
        };
    };
    let allowed_role_ids = positive_config_id(state.config.scrim_signup_role_id)
        .into_iter()
        .collect::<Vec<_>>();
    let response = state
        .notifier
        .broker()
        .post_internal::<serde_json::Value, _>(
            "/internal/master/v1/discord/send-rich-message",
            &serde_json::json!({
                "channel_id": channel_id,
                "content": "Platzhalter",
                "embed": {
                    "title": "Platzhalter",
                    "description": "Platzhalter",
                    "fields": note.map(|value| serde_json::json!({
                        "name": "Platzhalter",
                        "value": value,
                        "inline": false,
                    })).into_iter().collect::<Vec<_>>(),
                    "footer": {"text": "Platzhalter"},
                },
                "allowed_role_ids": allowed_role_ids,
                "idempotency_key": idempotency_key,
            }),
        )
        .await;
    let message_id = match response {
        Ok(response) => {
            let message_id = response
                .pointer("/result/message_id")
                .and_then(serde_json::Value::as_str)
                .filter(|message_id| !message_id.is_empty())
                .map(str::to_string);
            if message_id.is_none() {
                tracing::warn!(
                    team_id = team.id,
                    "Scrim-Ankuendigungsantwort enthaelt keine Message-ID"
                );
            }
            message_id
        }
        Err(error) => {
            tracing::warn!(
                team_id = team.id,
                %error,
                "Scrim-Ankuendigung konnte nicht gepostet werden"
            );
            None
        }
    };
    let Some(message_id) = message_id else {
        return AnnounceTeamResponse {
            message_id: None,
            ok: false,
            detail: "Platzhalter".to_string(),
        };
    };
    if let Err(error) = state
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
        tracing::warn!(
            team_id = team.id,
            %error,
            "Scrim-Ankuendigungsreaktion konnte nicht gesetzt werden"
        );
    }
    AnnounceTeamResponse {
        message_id: Some(message_id),
        ok: true,
        detail: "Platzhalter".to_string(),
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
            detail: "Platzhalter".to_string(),
        };
    };
    let result = state
        .notifier
        .broker()
        .post_internal::<serde_json::Value, _>(
            "/internal/master/v1/discord/send-dm",
            &serde_json::json!({
                "user_id": user_id,
                "content": "Platzhalter",
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
            detail: "Platzhalter".to_string(),
        };
    }
    DiscordSyncStatus {
        ok: true,
        detail: "Platzhalter".to_string(),
    }
}

async fn sync_signup_roles(state: &AppState, signup: &SignupMutation) {
    let Some(discord_user_id) = signup.discord_user_id else {
        return;
    };
    let Some(guild_id) = positive_config_id(Some(state.config.scrim_guild_id)) else {
        tracing::warn!("SCRIM_GUILD_ID ist ungueltig; Scrim-Rollen-Sync deaktiviert");
        return;
    };
    for role_id in &signup.role_ids {
        let reason = format!("scrim {} add role {}", signup.participant.id, role_id);
        let idempotency_key = format!("scrim-{}-{}-add", signup.participant.id, role_id);
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
                    "idempotency_key": idempotency_key,
                }),
            )
            .await
        {
            tracing::warn!(
                participant_id = signup.participant.id,
                role_id,
                %error,
                "Scrim-Signup-Discord-Sync fail-open"
            );
        }
    }
}

fn positive_config_id(value: Option<i64>) -> Option<u64> {
    value
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
}

async fn disabled_operator_mutation(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> WebResult<(StatusCode, Json<CapabilityReceipt>)> {
    require_internal_boundary(peer, &headers, &state)?;
    require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    service(&state).authorize_operator(actor.discord_id).await?;
    Ok(disabled_capability())
}

async fn disabled_operator_path_mutation(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(_id): Path<String>,
    headers: HeaderMap,
) -> WebResult<(StatusCode, Json<CapabilityReceipt>)> {
    require_internal_boundary(peer, &headers, &state)?;
    require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    service(&state).authorize_operator(actor.discord_id).await?;
    Ok(disabled_capability())
}

async fn disabled_operator_two_path_mutation(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path((_id, _ref_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> WebResult<(StatusCode, Json<CapabilityReceipt>)> {
    require_internal_boundary(peer, &headers, &state)?;
    require_mutation_headers(&headers)?;
    let actor = require_bff_actor(&headers)?;
    service(&state).authorize_operator(actor.discord_id).await?;
    Ok(disabled_capability())
}

async fn disabled_operator_read(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(_id): Path<String>,
    headers: HeaderMap,
) -> WebResult<(StatusCode, Json<CapabilityReceipt>)> {
    require_operator(&state, peer, &headers).await?;
    Ok(disabled_capability())
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

fn disabled_capability() -> (StatusCode, Json<CapabilityReceipt>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(CapabilityReceipt {
            available: false,
            verified: false,
            message: CAPABILITY_DISABLED.to_string(),
        }),
    )
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
