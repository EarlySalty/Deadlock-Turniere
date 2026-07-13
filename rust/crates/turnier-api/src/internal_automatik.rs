//! Interner Vertrag fuer den Discord-Master-Bot.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use turnier_automatik::presets;
use turnier_automatik::proposals::{self, ProposalState, VoteDecision};
use turnier_automatik::routine::{self, RoutineTournamentPlan};

use crate::error::{WebError, WebResult};
use crate::state::AppState;

const INTERNAL_TOKEN_HEADER: &str = "X-Internal-Token";
const APPROVER_ROLE_IDS: [&str; 2] = ["1337518124647579661", "1401891955931222110"];

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/internal/turnier/v1/proposals/{proposal_id}",
            get(get_proposal),
        )
        .route(
            "/internal/turnier/v1/proposals/{proposal_id}/rendered",
            post(attach_rendered),
        )
        .route(
            "/internal/turnier/v1/proposals/{proposal_id}/planned",
            post(store_planned),
        )
        .route(
            "/internal/turnier/v1/proposals/{proposal_id}/vote",
            post(vote),
        )
        .route(
            "/internal/turnier/v1/proposals/{proposal_id}/revision",
            post(revise),
        )
        .route(
            "/internal/turnier/v1/proposals/{proposal_id}/revision/{revised_id}/activate",
            post(activate_revision),
        )
        .route(
            "/internal/turnier/v1/proposals/{proposal_id}/announcement-rendered",
            post(announcement_rendered),
        )
        .route(
            "/internal/turnier/v1/proposals/{proposal_id}/announcement-planned",
            post(announcement_planned),
        )
}

#[derive(Debug, Deserialize)]
struct RenderedBody {
    config_json: String,
    channel_id: String,
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct PlannedBody {
    config_json: String,
}

#[derive(Debug, Deserialize)]
struct VoteBody {
    actor_id: String,
    role_ids: Vec<String>,
    decision: VoteDecision,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RevisionBody {
    role_ids: Vec<String>,
    config_json: String,
}

#[derive(Debug, Deserialize)]
struct ActivateRevisionBody {
    actor_id: String,
    role_ids: Vec<String>,
    feedback: String,
    channel_id: String,
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct AnnouncementRenderedBody {
    actor_id: String,
    role_ids: Vec<String>,
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct AnnouncementPlannedBody {
    role_ids: Vec<String>,
    draft: String,
}

fn require_internal(headers: &HeaderMap, state: &AppState) -> WebResult<()> {
    let expected = state.config.discord_oauth_internal_api_token.trim();
    let supplied = headers
        .get(INTERNAL_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if expected.is_empty() || supplied != expected {
        return Err(WebError::unauthorized("Interne Authentifizierung fehlt"));
    }
    Ok(())
}

fn require_approver(role_ids: &[String]) -> WebResult<()> {
    if role_ids
        .iter()
        .any(|role| APPROVER_ROLE_IDS.contains(&role.as_str()))
    {
        return Ok(());
    }
    Err(WebError::forbidden(
        "Nur Mods und Community-Mods duerfen abstimmen",
    ))
}

async fn proposal_payload(state: &AppState, proposal_id: i64) -> WebResult<Value> {
    let proposal = proposals::get_proposal(&state.pool, proposal_id)
        .await?
        .ok_or_else(|| WebError::not_found("Vorschlag nicht gefunden"))?;
    let votes = proposals::list_votes(&state.pool, proposal_id).await?;
    let feedback = proposals::list_feedback(&state.pool, proposal_id).await?;
    let learning_feedback = proposals::list_recent_feedback(&state.pool, 20).await?;
    let approvals = proposals::approvals_count(&state.pool, proposal_id).await?;
    let announcement_posted = proposals::announcement_posted(&state.pool, proposal_id).await?;
    Ok(json!({
        "proposal": proposal,
        "votes": votes,
        "feedback": feedback,
        "learning_feedback": learning_feedback,
        "approvals": approvals,
        "required_approvals": proposals::REQUIRED_APPROVALS,
        "announcement_posted": announcement_posted,
    }))
}

async fn get_proposal(
    State(state): State<AppState>,
    Path(proposal_id): Path<i64>,
    headers: HeaderMap,
) -> WebResult<Json<Value>> {
    require_internal(&headers, &state)?;
    let proposal_id = resolve_active_id(&state, proposal_id).await?;
    Ok(Json(proposal_payload(&state, proposal_id).await?))
}

async fn attach_rendered(
    State(state): State<AppState>,
    Path(proposal_id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<RenderedBody>,
) -> WebResult<Json<Value>> {
    require_internal(&headers, &state)?;
    proposals::attach_rendered_message(
        &state.pool,
        proposal_id,
        &body.config_json,
        &body.channel_id,
        &body.message_id,
    )
    .await?;
    Ok(Json(proposal_payload(&state, proposal_id).await?))
}

async fn store_planned(
    State(state): State<AppState>,
    Path(proposal_id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<PlannedBody>,
) -> WebResult<Json<Value>> {
    require_internal(&headers, &state)?;
    proposals::store_planned_config(&state.pool, proposal_id, &body.config_json).await?;
    Ok(Json(proposal_payload(&state, proposal_id).await?))
}

async fn vote(
    State(state): State<AppState>,
    Path(proposal_id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<VoteBody>,
) -> WebResult<Json<Value>> {
    require_internal(&headers, &state)?;
    require_approver(&body.role_ids)?;
    let proposal_id = resolve_active_id(&state, proposal_id).await?;
    let proposal = proposals::get_proposal(&state.pool, proposal_id)
        .await?
        .ok_or_else(|| WebError::not_found("Vorschlag nicht gefunden"))?;
    if let Some(tournament_id) = proposal.tournament_id {
        let mut payload = proposal_payload(&state, proposal_id).await?;
        payload["went_live"] = json!(false);
        payload["tournament_id"] = json!(tournament_id);
        return Ok(Json(payload));
    }
    if !matches!(
        proposal.state,
        ProposalState::PendingApproval | ProposalState::Approved
    ) {
        return Err(WebError::conflict("Vorschlag ist nicht mehr abstimmbar"));
    }

    if body.decision == VoteDecision::Reject {
        let reason = body.reason.as_deref().unwrap_or_default().trim();
        if reason.is_empty() {
            return Err(WebError::bad_request("Ein N braucht einen Grund"));
        }
        proposals::record_feedback(
            &state.pool,
            proposal_id,
            &body.actor_id,
            reason,
            Some(r#"{"kind":"reject"}"#),
        )
        .await?;
    }
    proposals::record_vote(&state.pool, proposal_id, &body.actor_id, body.decision).await?;
    let approvals = proposals::approvals_count(&state.pool, proposal_id).await?;
    let mut went_live = false;
    let mut tournament_id = None;
    if body.decision == VoteDecision::Approve && approvals >= proposals::REQUIRED_APPROVALS {
        let (id, created) = materialize(&state, proposal_id, &body.actor_id).await?;
        tournament_id = Some(id);
        went_live = created;
    }

    let mut payload = proposal_payload(&state, proposal_id).await?;
    payload["went_live"] = json!(went_live);
    payload["tournament_id"] = json!(tournament_id);
    Ok(Json(payload))
}

async fn revise(
    State(state): State<AppState>,
    Path(proposal_id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<RevisionBody>,
) -> WebResult<Json<Value>> {
    require_internal(&headers, &state)?;
    require_approver(&body.role_ids)?;
    let proposal_id = resolve_active_id(&state, proposal_id).await?;
    let revised_id =
        proposals::prepare_revision(&state.pool, proposal_id, &body.config_json).await?;
    Ok(Json(proposal_payload(&state, revised_id).await?))
}

async fn activate_revision(
    State(state): State<AppState>,
    Path((proposal_id, revised_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    Json(body): Json<ActivateRevisionBody>,
) -> WebResult<Json<Value>> {
    require_internal(&headers, &state)?;
    require_approver(&body.role_ids)?;
    let proposal_id = resolve_active_id(&state, proposal_id).await?;
    proposals::activate_prepared_revision(
        &state.pool,
        proposal_id,
        revised_id,
        &body.actor_id,
        &body.feedback,
        &body.channel_id,
        &body.message_id,
    )
    .await?;
    Ok(Json(proposal_payload(&state, revised_id).await?))
}

async fn announcement_rendered(
    State(state): State<AppState>,
    Path(proposal_id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<AnnouncementRenderedBody>,
) -> WebResult<Json<Value>> {
    require_internal(&headers, &state)?;
    require_approver(&body.role_ids)?;
    let proposal_id = resolve_active_id(&state, proposal_id).await?;
    let proposal = proposals::get_proposal(&state.pool, proposal_id)
        .await?
        .ok_or_else(|| WebError::not_found("Vorschlag nicht gefunden"))?;
    if proposal.tournament_id.is_none() {
        return Err(WebError::conflict(
            "Ankündigungsvorlage erst nach der Freigabe möglich",
        ));
    }
    proposals::record_announcement_posted(
        &state.pool,
        proposal_id,
        &body.actor_id,
        &body.message_id,
    )
    .await?;
    Ok(Json(proposal_payload(&state, proposal_id).await?))
}

async fn announcement_planned(
    State(state): State<AppState>,
    Path(proposal_id): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<AnnouncementPlannedBody>,
) -> WebResult<Json<Value>> {
    require_internal(&headers, &state)?;
    require_approver(&body.role_ids)?;
    let proposal_id = resolve_active_id(&state, proposal_id).await?;
    proposals::store_announcement_draft(&state.pool, proposal_id, &body.draft).await?;
    Ok(Json(proposal_payload(&state, proposal_id).await?))
}

async fn resolve_active_id(state: &AppState, proposal_id: i64) -> WebResult<i64> {
    proposals::resolve_active_proposal_id(&state.pool, proposal_id)
        .await?
        .ok_or_else(|| WebError::not_found("Vorschlag nicht gefunden"))
}

async fn materialize(state: &AppState, proposal_id: i64, actor_id: &str) -> WebResult<(i64, bool)> {
    let proposal = proposals::get_proposal(&state.pool, proposal_id)
        .await?
        .ok_or_else(|| WebError::not_found("Vorschlag nicht gefunden"))?;
    if let Some(tournament_id) = proposal.tournament_id {
        return Ok((tournament_id, false));
    }
    let preset_id = proposal
        .preset_id
        .ok_or_else(|| WebError::bad_request("Vorschlag hat kein Preset"))?;
    let preset = presets::get(&state.pool, preset_id)
        .await?
        .ok_or_else(|| WebError::not_found("Preset nicht gefunden"))?;
    let config: Value = serde_json::from_str(&proposal.config_json)
        .map_err(|_| WebError::bad_request("Vorschlagsplan ist ungueltig"))?;
    let plan = RoutineTournamentPlan {
        registration_start: config_time(&config, "registration_start")?,
        registration_end: config_time(&config, "registration_end")?,
        checkin_start: config_time(&config, "checkin_start")?,
        event_start: config_time(&config, "event_start")?,
        bracket_start: config_time(&config, "bracket_start")?,
    };
    let ensured = routine::ensure_routine_tournament(&state.pool, &preset, &plan).await?;
    if ensured.status == "draft" {
        turnier_scheduler::advance_tournament_status(
            &state.pool,
            &state.match_manager,
            &state.notifier,
            ensured.id,
            "draft",
            "registration",
            "discord_approval",
            Some(actor_id),
        )
        .await?;
    } else if ensured.status != "registration" {
        return Err(WebError::conflict(
            "Turnier wurde bereits ueber die Anmeldung hinaus fortgesetzt",
        ));
    }
    proposals::approve_and_attach_tournament(&state.pool, proposal_id, ensured.id).await?;
    Ok((ensured.id, true))
}

fn config_time(config: &Value, key: &str) -> WebResult<DateTime<Utc>> {
    let raw = config
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| WebError::bad_request(format!("Vorschlagsplan ohne {key}")))?;
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| WebError::bad_request(format!("Ungueltiger Zeitpunkt: {key}")))
}
