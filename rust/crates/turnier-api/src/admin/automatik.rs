//! Admin-Routen der Turnier-Automatik: Presets und Proposals.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use turnier_automatik::presets::{
    self, Category, NewPreset, Preset, PresetConfig, PresetUpdate,
};
use turnier_automatik::proposals::{
    self, Proposal, ProposalEvent, ProposalFeedback, ProposalSource, ProposalState, ProposalVote,
    VoteDecision,
};
use turnier_core::{BracketFormat, InviteMode, TournamentGameMode, TournamentMode, UserSession};

use crate::error::{WebError, WebResult};
use crate::extract::ModUser;
use crate::state::AppState;

use super::helpers::audit;

const PH_PRESET_NOT_FOUND: &str = "Preset nicht gefunden";
const PH_PROPOSAL_NOT_FOUND: &str = "Vorschlag nicht gefunden";
const PH_INVALID_PROPOSAL_STATE: &str = "Ungültiger Vorschlags-Status";
const PH_INVALID_PROPOSAL_EVENT: &str = "Ungültige Vorschlags-Aktion";
const PH_INVALID_VOTE_DECISION: &str = "Ungültige Vote-Entscheidung";
const PH_CONFIG_JSON_ERROR: &str = "Konfiguration konnte nicht erzeugt werden";
const PH_CASTER_ROLE_REQUIRED: &str = "Platzhalter";

/// Router fuer `/api/admin/presets` und `/api/admin/proposals`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/presets", get(list_presets).post(create_preset))
        .route(
            "/api/admin/presets/{id}",
            get(get_preset).put(update_preset).delete(delete_preset),
        )
        .route("/api/admin/presets/{id}/active", patch(set_preset_active))
        .route("/api/admin/proposals", get(list_proposals).post(create_manual_proposal))
        .route("/api/admin/proposals/{id}", get(get_proposal_detail))
        .route("/api/admin/proposals/{id}/event", post(apply_proposal_event))
        .route("/api/admin/proposals/{id}/votes", post(record_proposal_vote))
        .route(
            "/api/admin/proposals/{id}/feedback",
            post(record_proposal_feedback),
        )
}

#[derive(Debug, Clone, Serialize)]
struct PresetDto {
    id: i64,
    name: String,
    category: Category,
    team_size: i64,
    bracket_format: BracketFormat,
    series_format: i64,
    final_series_format: Option<i64>,
    tournament_mode: TournamentMode,
    tournament_game_mode: TournamentGameMode,
    match_objective: String,
    invite_mode: InviteMode,
    reminder_offsets: Option<String>,
    start_reminder_offsets: Option<String>,
    rules: Option<String>,
    description_template: Option<String>,
    active: bool,
    created_by: String,
    created_at: String,
    updated_at: String,
}

impl From<Preset> for PresetDto {
    fn from(preset: Preset) -> Self {
        Self {
            id: preset.id,
            name: preset.name,
            category: preset.category,
            team_size: preset.team_size,
            bracket_format: preset.bracket_format,
            series_format: preset.series_format,
            final_series_format: preset.final_series_format,
            tournament_mode: preset.tournament_mode,
            tournament_game_mode: preset.tournament_game_mode,
            match_objective: preset.match_objective,
            invite_mode: preset.invite_mode,
            reminder_offsets: preset.reminder_offsets,
            start_reminder_offsets: preset.start_reminder_offsets,
            rules: preset.rules,
            description_template: preset.description_template,
            active: preset.active,
            created_by: preset.created_by,
            created_at: preset.created_at,
            updated_at: preset.updated_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PresetConfigBody {
    team_size: i64,
    bracket_format: BracketFormat,
    series_format: i64,
    #[serde(default)]
    final_series_format: Option<i64>,
    tournament_mode: TournamentMode,
    tournament_game_mode: TournamentGameMode,
    match_objective: String,
    invite_mode: InviteMode,
    #[serde(default)]
    reminder_offsets: Option<String>,
    #[serde(default)]
    start_reminder_offsets: Option<String>,
    #[serde(default)]
    rules: Option<String>,
    #[serde(default)]
    description_template: Option<String>,
}

impl PresetConfigBody {
    fn into_config(self) -> PresetConfig {
        PresetConfig {
            team_size: self.team_size,
            bracket_format: self.bracket_format,
            series_format: self.series_format,
            final_series_format: self.final_series_format,
            tournament_mode: self.tournament_mode,
            tournament_game_mode: self.tournament_game_mode,
            match_objective: self.match_objective,
            invite_mode: self.invite_mode,
            reminder_offsets: self.reminder_offsets,
            start_reminder_offsets: self.start_reminder_offsets,
            rules: self.rules,
            description_template: self.description_template,
        }
    }
}

#[derive(Debug, Deserialize)]
struct NewPresetBody {
    name: String,
    category: Category,
    config: PresetConfigBody,
    #[serde(default = "default_true")]
    active: bool,
}

#[derive(Debug, Deserialize)]
struct PresetUpdateBody {
    name: String,
    category: Category,
    config: PresetConfigBody,
}

#[derive(Debug, Deserialize)]
struct ActiveBody {
    active: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ProposalDto {
    id: i64,
    preset_id: Option<i64>,
    source: ProposalSource,
    proposed_start: Option<String>,
    config_json: String,
    state: ProposalState,
    proposal_message_id: Option<String>,
    channel_id: Option<String>,
    tournament_id: Option<i64>,
    decided_at: Option<String>,
    created_at: String,
}

impl From<Proposal> for ProposalDto {
    fn from(proposal: Proposal) -> Self {
        Self {
            id: proposal.id,
            preset_id: proposal.preset_id,
            source: proposal.source,
            proposed_start: proposal.proposed_start,
            config_json: proposal.config_json,
            state: proposal.state,
            proposal_message_id: proposal.proposal_message_id,
            channel_id: proposal.channel_id,
            tournament_id: proposal.tournament_id,
            decided_at: proposal.decided_at,
            created_at: proposal.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ProposalVoteDto {
    id: i64,
    proposal_id: i64,
    caster_discord_id: String,
    decision: VoteDecision,
    created_at: String,
}

impl From<ProposalVote> for ProposalVoteDto {
    fn from(vote: ProposalVote) -> Self {
        Self {
            id: vote.id,
            proposal_id: vote.proposal_id,
            caster_discord_id: vote.caster_discord_id,
            decision: vote.decision,
            created_at: vote.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ProposalFeedbackDto {
    id: i64,
    proposal_id: i64,
    caster_discord_id: String,
    raw_text: String,
    applied_change_json: Option<String>,
    created_at: String,
}

impl From<ProposalFeedback> for ProposalFeedbackDto {
    fn from(feedback: ProposalFeedback) -> Self {
        Self {
            id: feedback.id,
            proposal_id: feedback.proposal_id,
            caster_discord_id: feedback.caster_discord_id,
            raw_text: feedback.raw_text,
            applied_change_json: feedback.applied_change_json,
            created_at: feedback.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct ProposalDetailDto {
    proposal: ProposalDto,
    votes: Vec<ProposalVoteDto>,
    feedback: Vec<ProposalFeedbackDto>,
    approvals: i64,
}

#[derive(Debug, Deserialize)]
struct ProposalListQuery {
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ManualProposalBody {
    preset_id: i64,
    name: String,
    #[serde(default)]
    proposed_start: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProposalEventBody {
    event: String,
}

#[derive(Debug, Serialize)]
struct ProposalStateDto {
    state: ProposalState,
}

#[derive(Debug, Deserialize)]
struct VoteBody {
    caster_id: String,
    decision: String,
}

#[derive(Debug, Deserialize)]
struct FeedbackBody {
    raw_text: String,
}

#[derive(Debug, Serialize)]
struct FeedbackCreatedDto {
    id: i64,
}

fn default_true() -> bool {
    true
}

fn parse_wire_enum<T>(value: &str, detail: &'static str) -> WebResult<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(Value::String(value.to_string()))
        .map_err(|_| WebError::bad_request(detail))
}

fn parse_event(value: &str) -> WebResult<ProposalEvent> {
    match value {
        "submit" => Ok(ProposalEvent::SubmitForApproval),
        "approve" => Ok(ProposalEvent::Approve),
        "reject" => Ok(ProposalEvent::Reject),
        "expire" => Ok(ProposalEvent::Expire),
        _ => Err(WebError::bad_request(PH_INVALID_PROPOSAL_EVENT)),
    }
}

fn actor_has_caster_role(state: &AppState, user: &UserSession) -> bool {
    let caster_role_id = state.config.discord_caster_role_id.to_string();
    user.roles.iter().any(|role| role == &caster_role_id)
}

async fn load_preset(pool: &turnier_db::Pool, preset_id: i64) -> WebResult<Preset> {
    presets::get(pool, preset_id)
        .await?
        .ok_or_else(|| WebError::not_found(PH_PRESET_NOT_FOUND))
}

async fn load_proposal(pool: &turnier_db::Pool, proposal_id: i64) -> WebResult<Proposal> {
    proposals::get_proposal(pool, proposal_id)
        .await?
        .ok_or_else(|| WebError::not_found(PH_PROPOSAL_NOT_FOUND))
}

fn manual_config_json(
    preset: &Preset,
    name: &str,
    proposed_start: Option<String>,
) -> WebResult<String> {
    let value = json!({
        "name": name,
        "category": preset.category,
        "team_size": preset.team_size,
        "bracket_format": preset.bracket_format,
        "series_format": preset.series_format,
        "final_series_format": preset.final_series_format,
        "tournament_mode": preset.tournament_mode,
        "tournament_game_mode": preset.tournament_game_mode,
        "match_objective": &preset.match_objective,
        "invite_mode": preset.invite_mode,
        "reminder_offsets": &preset.reminder_offsets,
        "start_reminder_offsets": &preset.start_reminder_offsets,
        "rules": &preset.rules,
        "description_template": &preset.description_template,
        "proposed_start": proposed_start,
        "preset_id": preset.id,
    });
    serde_json::to_string(&value).map_err(|_| WebError::internal(PH_CONFIG_JSON_ERROR))
}

async fn list_presets(
    State(state): State<AppState>,
    _mod: ModUser,
) -> WebResult<Json<Vec<PresetDto>>> {
    let presets = presets::list(&state.pool).await?;
    Ok(Json(presets.into_iter().map(PresetDto::from).collect()))
}

async fn create_preset(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Json(body): Json<NewPresetBody>,
) -> WebResult<Json<PresetDto>> {
    let preset = presets::create(
        &state.pool,
        &NewPreset {
            name: body.name,
            category: body.category,
            config: body.config.into_config(),
            active: body.active,
            created_by: user.discord_id.clone(),
        },
    )
    .await?;
    audit(
        &state.pool,
        "preset_create",
        &user.discord_id,
        json!({ "preset_id": preset.id }),
    )
    .await?;
    Ok(Json(preset.into()))
}

async fn get_preset(
    State(state): State<AppState>,
    _mod: ModUser,
    Path(id): Path<i64>,
) -> WebResult<Json<PresetDto>> {
    Ok(Json(load_preset(&state.pool, id).await?.into()))
}

async fn update_preset(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(id): Path<i64>,
    Json(body): Json<PresetUpdateBody>,
) -> WebResult<Json<PresetDto>> {
    let preset = presets::update(
        &state.pool,
        id,
        &PresetUpdate {
            name: body.name,
            category: body.category,
            config: body.config.into_config(),
        },
    )
    .await?
    .ok_or_else(|| WebError::not_found(PH_PRESET_NOT_FOUND))?;
    audit(
        &state.pool,
        "preset_update",
        &user.discord_id,
        json!({ "preset_id": id }),
    )
    .await?;
    Ok(Json(preset.into()))
}

async fn set_preset_active(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(id): Path<i64>,
    Json(body): Json<ActiveBody>,
) -> WebResult<Json<PresetDto>> {
    if !presets::set_active(&state.pool, id, body.active).await? {
        return Err(WebError::not_found(PH_PRESET_NOT_FOUND));
    }
    audit(
        &state.pool,
        "preset_set_active",
        &user.discord_id,
        json!({ "preset_id": id, "active": body.active }),
    )
    .await?;
    Ok(Json(load_preset(&state.pool, id).await?.into()))
}

async fn delete_preset(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(id): Path<i64>,
) -> WebResult<StatusCode> {
    if !presets::delete(&state.pool, id).await? {
        return Err(WebError::not_found(PH_PRESET_NOT_FOUND));
    }
    audit(
        &state.pool,
        "preset_delete",
        &user.discord_id,
        json!({ "preset_id": id }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_proposals(
    State(state): State<AppState>,
    _mod: ModUser,
    Query(query): Query<ProposalListQuery>,
) -> WebResult<Json<Vec<ProposalDto>>> {
    let filter = match query.state {
        Some(state) => Some(parse_wire_enum(&state, PH_INVALID_PROPOSAL_STATE)?),
        None => None,
    };
    let proposals = proposals::list_proposals(&state.pool, filter).await?;
    Ok(Json(proposals.into_iter().map(ProposalDto::from).collect()))
}

async fn get_proposal_detail(
    State(state): State<AppState>,
    _mod: ModUser,
    Path(id): Path<i64>,
) -> WebResult<Json<ProposalDetailDto>> {
    let proposal = load_proposal(&state.pool, id).await?;
    let votes = proposals::list_votes(&state.pool, id).await?;
    let feedback = proposals::list_feedback(&state.pool, id).await?;
    let approvals = proposals::approvals_count(&state.pool, id).await?;
    Ok(Json(ProposalDetailDto {
        proposal: proposal.into(),
        votes: votes.into_iter().map(ProposalVoteDto::from).collect(),
        feedback: feedback.into_iter().map(ProposalFeedbackDto::from).collect(),
        approvals,
    }))
}

async fn create_manual_proposal(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Json(body): Json<ManualProposalBody>,
) -> WebResult<Json<ProposalDto>> {
    let preset = load_preset(&state.pool, body.preset_id).await?;
    let config_json = manual_config_json(&preset, &body.name, body.proposed_start.clone())?;
    let proposal_id = proposals::create_proposal(
        &state.pool,
        Some(body.preset_id),
        ProposalSource::Manual,
        body.proposed_start.as_deref(),
        &config_json,
    )
    .await?;
    audit(
        &state.pool,
        "proposal_create_manual",
        &user.discord_id,
        json!({ "proposal_id": proposal_id, "preset_id": body.preset_id }),
    )
    .await?;
    Ok(Json(load_proposal(&state.pool, proposal_id).await?.into()))
}

async fn apply_proposal_event(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(id): Path<i64>,
    Json(body): Json<ProposalEventBody>,
) -> WebResult<Json<ProposalStateDto>> {
    load_proposal(&state.pool, id).await?;
    let event = parse_event(&body.event)?;
    let next = proposals::apply_event(&state.pool, id, event).await?;
    audit(
        &state.pool,
        "proposal_event",
        &user.discord_id,
        json!({ "proposal_id": id, "event": body.event }),
    )
    .await?;
    Ok(Json(ProposalStateDto { state: next }))
}

async fn record_proposal_vote(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(id): Path<i64>,
    Json(body): Json<VoteBody>,
) -> WebResult<StatusCode> {
    if !actor_has_caster_role(&state, &user) {
        return Err(WebError::forbidden(PH_CASTER_ROLE_REQUIRED));
    }
    load_proposal(&state.pool, id).await?;
    let actor_id = user.discord_id.clone();
    let requested_caster_id = body.caster_id;
    let decision_text = body.decision;
    let decision = parse_wire_enum(&decision_text, PH_INVALID_VOTE_DECISION)?;
    proposals::record_vote(&state.pool, id, &actor_id, decision).await?;
    audit(
        &state.pool,
        "proposal_vote",
        &actor_id,
        json!({
            "proposal_id": id,
            "caster_id": &actor_id,
            "requested_caster_id": requested_caster_id,
            "decision": decision_text
        }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn record_proposal_feedback(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(id): Path<i64>,
    Json(body): Json<FeedbackBody>,
) -> WebResult<Json<FeedbackCreatedDto>> {
    load_proposal(&state.pool, id).await?;
    let feedback_id = proposals::record_feedback(
        &state.pool,
        id,
        &user.discord_id,
        &body.raw_text,
        None,
    )
    .await?;
    audit(
        &state.pool,
        "proposal_feedback",
        &user.discord_id,
        json!({ "proposal_id": id, "feedback_id": feedback_id }),
    )
    .await?;
    Ok(Json(FeedbackCreatedDto { id: feedback_id }))
}
