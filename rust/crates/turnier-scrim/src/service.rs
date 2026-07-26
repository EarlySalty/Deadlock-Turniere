use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::decision::{is_scrim_history_entry, validate_match_request_batch};
use crate::dto::{SelfServiceParticipant, SignupRequest, WeeklyAvailability};
use crate::model::{
    AvailabilitySlot, AvailabilityStatus, MatchRequestBatchInput, ScrimMatch, ScrimReadModel,
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
        return Err(ScrimError::InvalidProposal("Platzhalter".to_string()));
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
    .map_err(|_| ScrimError::InvalidProposal("Platzhalter".to_string()))
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
