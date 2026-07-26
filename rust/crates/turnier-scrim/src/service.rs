use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::decision::{is_scrim_history_entry, validate_match_request_batch};
use crate::dto::{
    AnnouncementPublicationRequest, CreateMatchRequest, LobbyCodeRequest, MatchIdPatchRequest,
    MatchIdsRequest, ResultFetchRequest, SelfServiceParticipant, SignupRequest, WeeklyAvailability,
};
use crate::model::{
    wire_id, AnnouncementPreview, AvailabilitySlot, AvailabilityStatus, LobbyStateMutation,
    MatchMutation, MatchRequestBatchInput, ScrimAction, ScrimMatch, ScrimReadModel,
    ValidatedMatchRequestBatch,
};
use crate::repository::{PgScrimReadRepository, ScrimReadRepository, SignupMutation};
use crate::{ScrimError, ScrimResult};

pub struct ScrimService<R> {
    repository: R,
}

impl<R> ScrimService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }
}

impl<R: ScrimReadRepository> ScrimService<R> {
    pub async fn read_model(&self) -> ScrimResult<ScrimReadModel> {
        self.repository.read_model().await
    }

    pub async fn history(&self) -> ScrimResult<Vec<ScrimMatch>> {
        Ok(self
            .read_model()
            .await?
            .matches
            .into_iter()
            .filter(|scrim_match| {
                is_scrim_history_entry(
                    &scrim_match.status,
                    scrim_match.lobby_state.as_deref(),
                    scrim_match.selected_result.is_some(),
                )
            })
            .collect())
    }

    pub async fn authorize_operator(&self, actor_discord_id: &str) -> ScrimResult<()> {
        let actor_discord_id = parse_discord_id(actor_discord_id)?;
        if self.repository.is_active_coach(actor_discord_id).await? {
            Ok(())
        } else {
            Err(ScrimError::CoachUnauthorized)
        }
    }

    pub async fn validate_batch(
        &self,
        input: &MatchRequestBatchInput,
        now: DateTime<Utc>,
    ) -> ScrimResult<ValidatedMatchRequestBatch> {
        let initially_validated = validate_match_request_batch(input, now, &BTreeSet::new())?;
        let team_ids = initially_validated
            .matches
            .iter()
            .flat_map(|pairing| [Some(pairing.team_a_id), pairing.team_b_id])
            .flatten()
            .collect::<BTreeSet<_>>();
        if self.repository.existing_team_ids(&team_ids).await? != team_ids {
            return Err(ScrimError::InvalidProposal(
                "At least one team was not found".to_string(),
            ));
        }
        let active_team_ids = self.repository.active_request_team_ids(&team_ids).await?;
        if active_team_ids.is_empty() {
            Ok(initially_validated)
        } else {
            validate_match_request_batch(input, now, &active_team_ids)
        }
    }
}

impl ScrimService<PgScrimReadRepository> {
    pub async fn create_match(
        &self,
        idempotency_key: &str,
        request: CreateMatchRequest,
    ) -> ScrimResult<MatchMutation> {
        if request
            .note
            .as_ref()
            .is_some_and(|note| !note.trim().is_empty())
        {
            return Err(ScrimError::InvalidProposal(
                "note is not supported by the canonical match record".to_string(),
            ));
        }
        let model = self.read_model().await?;
        let request_teams = request
            .match_request_id
            .as_deref()
            .map(parse_id)
            .transpose()?
            .map(|request_id| {
                model
                    .match_request_batches
                    .iter()
                    .flat_map(|batch| &batch.requests)
                    .find(|item| item.id == request_id)
                    .map(|item| (item.team_a.id, item.team_b.as_ref().map(|team| team.id)))
                    .ok_or_else(|| {
                        ScrimError::InvalidProposal("match_request_id was not found".to_string())
                    })
            })
            .transpose()?;
        let explicit_a = request.team_a_id.as_deref().map(parse_id).transpose()?;
        let explicit_b = request.team_b_id.as_deref().map(parse_id).transpose()?;
        if let Some((request_a, request_b)) = request_teams {
            if explicit_a.is_some_and(|team_id| team_id != request_a)
                || explicit_b.is_some_and(|team_id| Some(team_id) != request_b)
            {
                return Err(ScrimError::InvalidProposal(
                    "match_request_id does not match the supplied teams".to_string(),
                ));
            }
        }
        let team_a_id = explicit_a
            .or_else(|| request_teams.map(|teams| teams.0))
            .ok_or_else(|| ScrimError::InvalidProposal("team_a_id is required".to_string()))?;
        let team_b_id = explicit_b
            .or_else(|| request_teams.and_then(|teams| teams.1))
            .ok_or_else(|| ScrimError::InvalidProposal("team_b_id is required".to_string()))?;
        if team_a_id == team_b_id {
            return Err(ScrimError::InvalidProposal(
                "team_a_id and team_b_id must differ".to_string(),
            ));
        }
        let coach_spectator_discord_id = request
            .coach_spectator_discord_id
            .as_deref()
            .map(parse_i64_id)
            .transpose()?;
        self.repository
            .create_match(
                idempotency_key,
                team_a_id,
                team_b_id,
                request.scheduled_at,
                coach_spectator_discord_id,
            )
            .await
    }

    pub async fn set_lobby_code(
        &self,
        idempotency_key: &str,
        match_id: i32,
        actor_user_id: &str,
        actor_display_name: &str,
        request: LobbyCodeRequest,
    ) -> ScrimResult<MatchMutation> {
        let code = request.lobby_code.trim();
        if code.chars().count() != 5 || !code.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            return Err(ScrimError::InvalidProposal(
                "lobby_code must be exactly 5 letters or numbers".to_string(),
            ));
        }
        self.repository
            .set_lobby_code(
                idempotency_key,
                match_id,
                &code.to_ascii_uppercase(),
                actor_user_id,
                actor_display_name,
            )
            .await
    }

    pub async fn add_match_ids(
        &self,
        idempotency_key: &str,
        match_id: i32,
        actor_user_id: &str,
        actor_display_name: &str,
        request: MatchIdsRequest,
    ) -> ScrimResult<MatchMutation> {
        if request.match_ids.is_empty() {
            return Err(ScrimError::InvalidProposal(
                "match_ids must not be empty".to_string(),
            ));
        }
        let match_ids = request
            .match_ids
            .iter()
            .map(|value| parse_i64_id(value))
            .collect::<ScrimResult<Vec<_>>>()?;
        if match_ids.iter().collect::<BTreeSet<_>>().len() != match_ids.len() {
            return Err(ScrimError::InvalidProposal(
                "match_ids must be unique".to_string(),
            ));
        }
        self.repository
            .add_match_ids(
                idempotency_key,
                match_id,
                &match_ids,
                actor_user_id,
                actor_display_name,
            )
            .await
    }

    pub async fn request_result_fetch(
        &self,
        idempotency_key: &str,
        match_id: i32,
        request: ResultFetchRequest,
    ) -> ScrimResult<LobbyStateMutation> {
        let result_ref_id = request
            .match_id_ref
            .as_deref()
            .map(parse_i64_id)
            .transpose()?;
        if let Some(winner_team_id) = request
            .winner_team_id
            .as_deref()
            .map(parse_id)
            .transpose()?
        {
            let is_match_team = self
                .read_model()
                .await?
                .matches
                .into_iter()
                .find(|item| item.id == match_id)
                .is_some_and(|item| {
                    item.team_a
                        .as_ref()
                        .is_some_and(|team| team.id == winner_team_id)
                        || item
                            .team_b
                            .as_ref()
                            .is_some_and(|team| team.id == winner_team_id)
                });
            if !is_match_team {
                return Err(ScrimError::InvalidProposal(
                    "winner_team_id does not belong to the match".to_string(),
                ));
            }
        }
        validate_optional_text(request.score.as_deref(), "score", 100)?;
        validate_optional_text(request.notes.as_deref(), "notes", 1_000)?;
        self.repository
            .request_result_fetch(idempotency_key, match_id, result_ref_id)
            .await
    }

    pub async fn select_result_ref(
        &self,
        idempotency_key: &str,
        match_id: i32,
        result_ref_id: i64,
        actor_user_id: &str,
        actor_display_name: &str,
        request: MatchIdPatchRequest,
    ) -> ScrimResult<MatchMutation> {
        validate_required_text(&request.message, "message", 1_000)?;
        let selection_reason = selection_reason(&request.message);
        self.repository
            .select_result_ref(
                idempotency_key,
                match_id,
                result_ref_id,
                actor_user_id,
                actor_display_name,
                &selection_reason,
            )
            .await
    }

    pub async fn announcement_preview(&self, block_id: &str) -> ScrimResult<AnnouncementPreview> {
        validate_block_id(block_id)?;
        self.repository.announcement_preview(block_id).await
    }

    pub async fn create_announcement_publication(
        &self,
        block_id: &str,
        idempotency_key: &str,
        actor_user_id: &str,
        actor_display_name: &str,
        request: AnnouncementPublicationRequest,
    ) -> ScrimResult<AnnouncementPreview> {
        validate_block_id(block_id)?;
        validate_required_text(&request.message, "message", 4_000)?;
        validate_optional_text(request.title.as_deref(), "title", 200)?;
        let channel_id = request.channel_id.as_deref().ok_or_else(|| {
            ScrimError::InvalidProposal("channel_id is required for publication".to_string())
        })?;
        parse_i64_id(channel_id)?;
        self.repository
            .create_announcement_publication(
                block_id,
                idempotency_key,
                actor_user_id,
                actor_display_name,
                &request,
            )
            .await
    }

    pub async fn action(&self, id: i64) -> ScrimResult<ScrimAction> {
        self.repository.action(id).await
    }

    pub async fn signup(
        &self,
        discord_id: &str,
        display_name: &str,
        mut request: SignupRequest,
        signup_role_id: Option<u64>,
        reserve_role_id: Option<u64>,
    ) -> ScrimResult<SignupMutation> {
        let discord_id = parse_discord_id(discord_id)?;
        if let Some(slots) = request.availability_slots.take() {
            let slots = canonicalize_weekly_availability(slots)?;
            request.availability = Some(render_legacy_availability(&slots)?);
            request.availability_slots = Some(slots);
        }
        self.repository
            .signup(
                discord_id,
                display_name,
                &request,
                signup_role_id,
                reserve_role_id,
            )
            .await
    }

    pub async fn update_availability(
        &self,
        discord_id: &str,
        availability: WeeklyAvailability,
    ) -> ScrimResult<SelfServiceParticipant> {
        let discord_id = parse_discord_id(discord_id)?;
        let availability = canonicalize_weekly_availability(availability)?;
        let legacy_availability = render_legacy_availability(&availability)?;
        self.repository
            .update_availability(discord_id, &availability, &legacy_availability)
            .await
    }
}

fn parse_id(value: &str) -> ScrimResult<i32> {
    wire_id::parse_i32(value.trim()).map_err(ScrimError::InvalidProposal)
}

fn parse_i64_id(value: &str) -> ScrimResult<i64> {
    wire_id::parse_i64(value.trim()).map_err(ScrimError::InvalidProposal)
}

fn validate_required_text(value: &str, field: &str, max_chars: usize) -> ScrimResult<()> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(ScrimError::InvalidProposal(format!(
            "{field} must contain between 1 and {max_chars} characters"
        )));
    }
    Ok(())
}

fn validate_optional_text(value: Option<&str>, field: &str, max_chars: usize) -> ScrimResult<()> {
    if let Some(value) = value {
        validate_required_text(value, field, max_chars)?;
    }
    Ok(())
}

fn validate_block_id(value: &str) -> ScrimResult<()> {
    let valid = if let Some((prefix, reference)) = value.split_once(':') {
        (2..=32).contains(&prefix.len())
            && prefix.bytes().enumerate().all(|(index, byte)| {
                matches!(
                    (index, byte),
                    (0, b'a'..=b'z') | (_, b'a'..=b'z' | b'0'..=b'9' | b'_')
                )
            })
            && (1..=96).contains(&reference.len())
            && reference
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphanumeric())
            && reference.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b':' | b'-')
            })
    } else {
        (1..=96).contains(&value.len())
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
    };
    if !valid {
        return Err(ScrimError::InvalidProposal(
            "block_id is invalid".to_string(),
        ));
    }
    Ok(())
}

fn selection_reason(message: &str) -> String {
    let mut reason = String::new();
    for byte in message.bytes() {
        let next = if byte.is_ascii_alphanumeric() {
            byte.to_ascii_lowercase() as char
        } else {
            '_'
        };
        if next != '_' || !reason.ends_with('_') {
            reason.push(next);
        }
        if reason.len() == 64 {
            break;
        }
    }
    let reason = reason.trim_matches('_');
    if reason.len() >= 3
        && reason
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
    {
        reason.to_string()
    } else {
        "operator_selection".to_string()
    }
}

fn parse_discord_id(value: &str) -> ScrimResult<i64> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ScrimError::InvalidActor);
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or(ScrimError::InvalidActor)
}

fn canonicalize_weekly_availability(
    mut weekly: WeeklyAvailability,
) -> ScrimResult<WeeklyAvailability> {
    for slot in [
        &mut weekly.mon,
        &mut weekly.tue,
        &mut weekly.wed,
        &mut weekly.thu,
        &mut weekly.fri,
        &mut weekly.sat,
        &mut weekly.sun,
    ] {
        canonicalize_availability_slot(slot)?;
    }
    Ok(weekly)
}

fn canonicalize_availability_slot(slot: &mut AvailabilitySlot) -> ScrimResult<()> {
    if slot.status != AvailabilityStatus::Available {
        slot.from = None;
        slot.to = None;
        return Ok(());
    }
    if slot.from.is_some_and(|from| from > 1_440)
        || slot.to.is_some_and(|to| to > 1_440)
        || matches!((slot.from, slot.to), (Some(from), Some(to)) if from >= to)
    {
        return Err(ScrimError::InvalidProposal(
            "Ungültige Zeitangabe: Start muss vor Ende liegen, beide innerhalb eines Tages."
                .to_string(),
        ));
    }
    Ok(())
}

fn render_legacy_availability(weekly: &WeeklyAvailability) -> ScrimResult<String> {
    #[derive(Serialize)]
    struct LegacyAvailability<'a> {
        mo: &'a str,
        di: &'a str,
        mi: &'a str,
        r#do: &'a str,
        fr: &'a str,
        sa: &'a str,
        so: &'a str,
    }

    let rendered = [
        render_legacy_slot(&weekly.mon),
        render_legacy_slot(&weekly.tue),
        render_legacy_slot(&weekly.wed),
        render_legacy_slot(&weekly.thu),
        render_legacy_slot(&weekly.fri),
        render_legacy_slot(&weekly.sat),
        render_legacy_slot(&weekly.sun),
    ];
    serde_json::to_string(&LegacyAvailability {
        mo: &rendered[0],
        di: &rendered[1],
        mi: &rendered[2],
        r#do: &rendered[3],
        fr: &rendered[4],
        sa: &rendered[5],
        so: &rendered[6],
    })
    .map_err(|_| {
        ScrimError::InvalidProposal(
            "Deine Verfügbarkeit ließ sich nicht verarbeiten. Bitte trag die Zeiten erneut ein."
                .to_string(),
        )
    })
}

fn render_legacy_slot(slot: &AvailabilitySlot) -> String {
    match slot.status {
        AvailabilityStatus::Unavailable => "Geht nicht".to_string(),
        AvailabilityStatus::Unknown => String::new(),
        AvailabilityStatus::Available => match (slot.from, slot.to) {
            (None, None) => "Flexibel".to_string(),
            (Some(from), Some(to)) => {
                format!("{}-{}", format_minutes(from), format_minutes(to))
            }
            (Some(from), None) => format!("ab {}", format_minutes(from)),
            (None, Some(to)) => format!("00:00-{}", format_minutes(to)),
        },
    }
}

fn format_minutes(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}
