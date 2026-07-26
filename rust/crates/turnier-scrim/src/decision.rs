use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};

use crate::model::{
    MatchRequestBatchInput, MatchRequestFacts, MatchRequestResponse, ReplacementNeed,
    ResponseChoice, RosterMember, SlotFacts, ValidatedMatchRequest, ValidatedMatchRequestBatch,
};
use crate::{ScrimError, ScrimResult};

pub fn validate_match_request_batch(
    input: &MatchRequestBatchInput,
    now: DateTime<Utc>,
    active_team_ids: &BTreeSet<i32>,
) -> ScrimResult<ValidatedMatchRequestBatch> {
    let deadline_at = input.deadline_at;
    if deadline_at <= now {
        return Err(ScrimError::InvalidProposal(
            "deadline_at must be in the future".to_string(),
        ));
    }
    if input.matches.is_empty() {
        return Err(ScrimError::InvalidProposal(
            "matches must be a non-empty array".to_string(),
        ));
    }

    let mut seen_team_ids = BTreeSet::new();
    let mut matches = Vec::with_capacity(input.matches.len());
    for pairing in &input.matches {
        if pairing.team_a_id <= 0 || pairing.team_b_id.is_some_and(|id| id <= 0) {
            return Err(ScrimError::InvalidProposal(
                "team ids must be positive".to_string(),
            ));
        }
        if pairing.team_b_id == Some(pairing.team_a_id) {
            return Err(ScrimError::InvalidProposal(
                "team_a_id and team_b_id must differ".to_string(),
            ));
        }
        for team_id in [Some(pairing.team_a_id), pairing.team_b_id]
            .into_iter()
            .flatten()
        {
            if active_team_ids.contains(&team_id) || !seen_team_ids.insert(team_id) {
                return Err(ScrimError::InvalidProposal(
                    "Ein Team darf im gleichen aktiven Abfragezeitraum nur in einem Match stecken."
                        .to_string(),
                ));
            }
        }

        let slots = pairing
            .slots
            .as_ref()
            .or(input.slots.as_ref())
            .ok_or_else(|| {
                ScrimError::InvalidProposal("Each match needs two to five slots".to_string())
            })?
            .clone();
        if !(2..=5).contains(&slots.len()) {
            return Err(ScrimError::InvalidProposal(
                "Each match needs two to five slots".to_string(),
            ));
        }
        if slots
            .iter()
            .any(|slot| slot.from >= slot.to || slot.to > 1_440)
        {
            return Err(ScrimError::InvalidProposal(
                "slot time window is invalid".to_string(),
            ));
        }
        matches.push(ValidatedMatchRequest {
            team_a_id: pairing.team_a_id,
            team_b_id: pairing.team_b_id,
            slots,
        });
    }

    Ok(ValidatedMatchRequestBatch {
        template: input.template,
        deadline_at,
        matches,
    })
}

pub fn derive_match_request_facts(
    request_id: i32,
    slot_count: usize,
    team_ids: &[i32],
    members: &[RosterMember],
    responses: &[MatchRequestResponse],
    deadline_passed: bool,
    released_slot_index: Option<usize>,
) -> ScrimResult<MatchRequestFacts> {
    let teams = team_ids.iter().copied().collect::<BTreeSet<_>>();
    if teams.len() != team_ids.len() || teams.is_empty() || slot_count == 0 {
        return Err(ScrimError::InvalidResponse(
            "invalid request teams or slots".to_string(),
        ));
    }
    let member_map = members
        .iter()
        .map(|member| ((member.team_id, member.participant_id), member))
        .collect::<BTreeMap<_, _>>();
    if member_map.len() != members.len()
        || members
            .iter()
            .any(|member| !teams.contains(&member.team_id))
    {
        return Err(ScrimError::InvalidResponse(
            "invalid request roster".to_string(),
        ));
    }

    let mut slots = (0..slot_count)
        .map(|index| SlotFacts {
            index,
            available_count: 0,
            starter_available_count: 0,
            team_available_count: 0,
        })
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut responded = BTreeSet::new();
    let mut no_slot = BTreeSet::new();
    let mut available_by_slot = vec![BTreeSet::new(); slot_count];

    for response in responses {
        let key = (response.team_id, response.participant_id);
        if response.request_id != request_id {
            return Err(ScrimError::InvalidResponse(
                "response belongs to another request".to_string(),
            ));
        }
        if !member_map.contains_key(&key) {
            continue;
        }
        if !seen.insert((
            response.team_id,
            response.participant_id,
            response.slot_index,
        )) {
            return Err(ScrimError::InvalidResponse(
                "duplicate participant slot response".to_string(),
            ));
        }
        responded.insert(key);
        if response.slot_index == -1 {
            if response.response != ResponseChoice::Unavailable {
                return Err(ScrimError::InvalidResponse(
                    "slot -1 must be unavailable".to_string(),
                ));
            }
            no_slot.insert(key);
            continue;
        }
        let slot_index = usize::try_from(response.slot_index).map_err(|_| {
            ScrimError::InvalidResponse("response slot is out of range".to_string())
        })?;
        if slot_index >= slot_count {
            return Err(ScrimError::InvalidResponse(
                "response slot is out of range".to_string(),
            ));
        }
        if response.response != ResponseChoice::Available {
            continue;
        }
        let member = member_map[&key];
        slots[slot_index].available_count += 1;
        if !member.is_bench {
            slots[slot_index].starter_available_count += 1;
        }
        available_by_slot[slot_index].insert(response.team_id);
    }
    for (slot, available_teams) in slots.iter_mut().zip(&available_by_slot) {
        slot.team_available_count = u32::try_from(available_teams.len()).map_err(|_| {
            ScrimError::InvalidResponse("team availability count overflow".to_string())
        })?;
    }

    let recommended_slot_index = deadline_passed
        .then(|| {
            slots
                .iter()
                .filter(|slot| {
                    slot.available_count > 0 && slot.team_available_count as usize == teams.len()
                })
                .fold(None, |best: Option<&SlotFacts>, candidate| match best {
                    Some(current)
                        if current.available_count > candidate.available_count
                            || (current.available_count == candidate.available_count
                                && current.starter_available_count
                                    >= candidate.starter_available_count) =>
                    {
                        Some(current)
                    }
                    _ => Some(candidate),
                })
                .map(|slot| slot.index)
        })
        .flatten();

    let selected_slot_index = deadline_passed
        .then(|| {
            released_slot_index
                .filter(|index| *index < slot_count)
                .or(recommended_slot_index)
        })
        .flatten();
    let replacement_needs = selected_slot_index
        .map(|slot_index| {
            members
                .iter()
                .filter(|member| !member.is_bench)
                .filter(|member| {
                    !seen.contains(&(member.team_id, member.participant_id, slot_index as i32))
                        || !responses.iter().any(|response| {
                            response.team_id == member.team_id
                                && response.participant_id == member.participant_id
                                && response.slot_index == slot_index as i32
                                && response.response == ResponseChoice::Available
                        })
                })
                .map(|member| {
                    let key = (member.team_id, member.participant_id);
                    let reason = if no_slot.contains(&key) {
                        "Kein Slot passt"
                    } else if !responded.contains(&key) {
                        "Antwort fehlt"
                    } else {
                        "Für ausgewählten Slot nicht zugesagt"
                    };
                    ReplacementNeed {
                        request_id,
                        team_id: member.team_id,
                        participant_id: member.participant_id,
                        display_name: member.display_name.clone(),
                        slot_index,
                        reason: reason.to_string(),
                        is_bench: member.is_bench,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(MatchRequestFacts {
        slots,
        missing_response_count: u32::try_from(member_map.len() - responded.len()).map_err(
            |_| ScrimError::InvalidResponse("missing response count overflow".to_string()),
        )?,
        no_slot_count: u32::try_from(no_slot.len()).map_err(|_| {
            ScrimError::InvalidResponse("no-slot response count overflow".to_string())
        })?,
        recommended_slot_index,
        selected_slot_index,
        replacement_needs,
    })
}

pub fn is_scrim_history_entry(
    status: &str,
    _lobby_state: Option<&str>,
    has_selected_result: bool,
) -> bool {
    !matches!(
        status.to_ascii_lowercase().as_str(),
        "cancelled" | "canceled"
    ) && has_selected_result
}
