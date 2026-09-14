use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Datelike, Utc};

use crate::model::{
    AvailabilityStatus, MatchRequestBatchInput, MatchRequestFacts, MatchRequestResponse,
    ReplacementCandidate, ReplacementNeed, ResponseChoice, RosterMember, ScrimDay, ScrimSlot,
    ScrimSlotSuggestion, ScrimSlotSuggestions, SlotFacts, Team, TeamMember, ValidatedMatchRequest,
    ValidatedMatchRequestBatch, WeeklyAvailability,
};
use crate::{ScrimError, ScrimResult};

pub const STANDARD_SCRIM_DURATION_MINUTES: u16 = 120;
const SLOT_GRID_MINUTES: u16 = 30;
const MATCH_TEAM_SIZE: usize = 6;
const SAFE_TEAM_STARTERS: u32 = 4;
const SAFE_TOTAL_STARTERS: u32 = 10;
const PRIME_TIME_START_MINUTES: i32 = 20 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberSlotState {
    Available,
    Unavailable,
    Unknown,
}

/// Leos eigentlicher Termin-Workflow als deterministische Funktion:
/// Verfügbarkeit wird einmal gepflegt, daraus werden wenige konkrete 2h-Slots.
/// Zuerst zählt, dass beide Teams möglichst vollständig können; Stammzeit und
/// typische Abendzeit sind nur Tie-Breaker. Das LLM ist hier bewusst nicht beteiligt.
pub fn suggest_match_slots(team_a: &Team, team_b: &Team, max_slots: usize) -> ScrimSlotSuggestions {
    let limit = max_slots.clamp(2, 5);
    let starters_a = team_a
        .members
        .iter()
        .filter(|member| !member.is_bench)
        .collect::<Vec<_>>();
    let starters_b = team_b
        .members
        .iter()
        .filter(|member| !member.is_bench)
        .collect::<Vec<_>>();
    let total_starters = starters_a.len() + starters_b.len();
    let match_ready_roster =
        starters_a.len() == MATCH_TEAM_SIZE && starters_b.len() == MATCH_TEAM_SIZE;

    let mut candidates = Vec::new();
    // Reguläre Community-Scrims liefen historisch stabil am Wochenende. Wochentage
    // bleiben für Training/Testmatches manuell wählbar, die Standard-Automatik schlägt
    // aber nur Samstag/Sonntag vor und spart damit unnötige Umfragen.
    for day in REGULAR_SCRIM_DAYS {
        let mut from = 0_u16;
        while from.saturating_add(STANDARD_SCRIM_DURATION_MINUTES) <= 1_440 {
            let to = from + STANDARD_SCRIM_DURATION_MINUTES;
            let (available_a, unknown_a) = team_slot_counts(&starters_a, day, from, to);
            let (available_b, unknown_b) = team_slot_counts(&starters_b, day, from, to);
            let available = available_a + available_b;
            if available > 0 {
                candidates.push(RankedSlot {
                    suggestion: ScrimSlotSuggestion {
                        slot: ScrimSlot {
                            day,
                            date: None,
                            from,
                            to,
                        },
                        team_a_available_starters: u8_count(available_a),
                        team_b_available_starters: u8_count(available_b),
                        available_starters: u8_count(available),
                        total_starters: u8_count(total_starters),
                        missing_starters: u8_count(total_starters.saturating_sub(available)),
                        unknown_starters: u8_count(unknown_a + unknown_b),
                        full_current_roster: available == total_starters && total_starters > 0,
                        match_ready_roster,
                    },
                    min_team_available: available_a.min(available_b),
                    default_affinity: default_time_affinity(team_a, from, to)
                        + default_time_affinity(team_b, from, to),
                    prime_time_distance: (i32::from(from) - PRIME_TIME_START_MINUTES).abs(),
                    day_index: scrim_day_index(day),
                });
            }
            from = from.saturating_add(SLOT_GRID_MINUTES);
        }
    }

    candidates.sort_by(|left, right| {
        right
            .min_team_available
            .cmp(&left.min_team_available)
            .then_with(|| {
                right
                    .suggestion
                    .available_starters
                    .cmp(&left.suggestion.available_starters)
            })
            .then_with(|| {
                left.suggestion
                    .unknown_starters
                    .cmp(&right.suggestion.unknown_starters)
            })
            .then_with(|| right.default_affinity.cmp(&left.default_affinity))
            .then_with(|| left.prime_time_distance.cmp(&right.prime_time_distance))
            .then_with(|| left.day_index.cmp(&right.day_index))
            .then_with(|| left.suggestion.slot.from.cmp(&right.suggestion.slot.from))
    });

    // Erst unterschiedliche Tage anbieten. Das bildet Leos "gib mir konkrete Optionen"
    // besser ab als drei fast identische 30-Minuten-Verschiebungen am selben Abend.
    let mut selected = Vec::new();
    let mut used_days = BTreeSet::new();
    for candidate in &candidates {
        let day = scrim_day_index(candidate.suggestion.slot.day);
        if used_days.insert(day) {
            selected.push(candidate.suggestion.clone());
            if selected.len() == limit {
                break;
            }
        }
    }
    // Gibt es nur einen geeigneten Tag, dürfen weitere nicht überlappende Optionen
    // desselben Tages folgen, damit die bestehende 2..=5-Slot-Abfrage nutzbar bleibt.
    if selected.len() < limit {
        for candidate in &candidates {
            if selected
                .iter()
                .any(|chosen| chosen.slot == candidate.suggestion.slot)
            {
                continue;
            }
            if selected
                .iter()
                .any(|chosen| slots_overlap(&chosen.slot, &candidate.suggestion.slot))
            {
                continue;
            }
            selected.push(candidate.suggestion.clone());
            if selected.len() == limit {
                break;
            }
        }
    }
    // Wenn selbst die nicht überlappende Auswahl zu klein ist, lieber eine echte zweite
    // Option zeigen als auf statische Sa/So-Zeiten zurückzufallen.
    if selected.len() < 2 {
        for candidate in &candidates {
            if selected
                .iter()
                .any(|chosen| chosen.slot == candidate.suggestion.slot)
            {
                continue;
            }
            selected.push(candidate.suggestion.clone());
            if selected.len() == 2 {
                break;
            }
        }
    }

    ScrimSlotSuggestions {
        team_a_id: team_a.id,
        team_b_id: team_b.id,
        duration_minutes: STANDARD_SCRIM_DURATION_MINUTES,
        suggestions: selected,
    }
}

const REGULAR_SCRIM_DAYS: [ScrimDay; 2] = [ScrimDay::Saturday, ScrimDay::Sunday];

#[derive(Debug, Clone)]
struct RankedSlot {
    suggestion: ScrimSlotSuggestion,
    min_team_available: usize,
    default_affinity: i32,
    prime_time_distance: i32,
    day_index: u8,
}

fn team_slot_counts(members: &[&TeamMember], day: ScrimDay, from: u16, to: u16) -> (usize, usize) {
    members.iter().fold(
        (0, 0),
        |(available, unknown), member| match member_slot_state(member, day, from, to) {
            MemberSlotState::Available => (available + 1, unknown),
            MemberSlotState::Unknown => (available, unknown + 1),
            MemberSlotState::Unavailable => (available, unknown),
        },
    )
}

pub fn availability_covers_slot(
    availability: Option<&WeeklyAvailability>,
    slot: &ScrimSlot,
) -> Option<bool> {
    let weekly = availability?;
    let day = match slot.day {
        ScrimDay::Monday => weekly.mon.as_ref(),
        ScrimDay::Tuesday => weekly.tue.as_ref(),
        ScrimDay::Wednesday => weekly.wed.as_ref(),
        ScrimDay::Thursday => weekly.thu.as_ref(),
        ScrimDay::Friday => weekly.fri.as_ref(),
        ScrimDay::Saturday => weekly.sat.as_ref(),
        ScrimDay::Sunday => weekly.sun.as_ref(),
    }?;
    match day.status {
        AvailabilityStatus::Unknown => None,
        AvailabilityStatus::Unavailable => Some(false),
        AvailabilityStatus::Available => {
            let available_from = day.from.unwrap_or(0);
            let available_to = day.to.unwrap_or(1_440);
            Some(available_from <= slot.from && slot.to <= available_to)
        }
    }
}

fn member_slot_state(member: &TeamMember, day: ScrimDay, from: u16, to: u16) -> MemberSlotState {
    let slot = ScrimSlot {
        day,
        date: None,
        from,
        to,
    };
    match availability_covers_slot(member.availability_slots.as_ref(), &slot) {
        Some(true) => MemberSlotState::Available,
        Some(false) => MemberSlotState::Unavailable,
        None => MemberSlotState::Unknown,
    }
}

fn default_time_affinity(team: &Team, from: u16, to: u16) -> i32 {
    let (Some(default_from), Some(default_to)) = (team.default_from, team.default_to) else {
        return 0;
    };
    let default_to = default_to.min(1_440);
    let overlap = i32::from(to).min(default_to) - i32::from(from).max(default_from);
    overlap.max(0)
}

fn slots_overlap(left: &ScrimSlot, right: &ScrimSlot) -> bool {
    left.day == right.day && left.from < right.to && right.from < left.to
}

fn scrim_day_index(day: ScrimDay) -> u8 {
    match day {
        ScrimDay::Monday => 0,
        ScrimDay::Tuesday => 1,
        ScrimDay::Wednesday => 2,
        ScrimDay::Thursday => 3,
        ScrimDay::Friday => 4,
        ScrimDay::Saturday => 5,
        ScrimDay::Sunday => 6,
    }
}

fn u8_count(value: usize) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

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
        if slots.iter().any(|slot| {
            slot.from >= slot.to
                || slot.to > 1_440
                || slot.date.is_some_and(|date| {
                    date < now.date_naive()
                        || date.weekday().num_days_from_monday()
                            != u32::from(scrim_day_index(slot.day))
                })
        }) {
            return Err(ScrimError::InvalidProposal(
                "slot time window or date is invalid".to_string(),
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

    let starter_keys = members
        .iter()
        .filter(|member| !member.is_bench)
        .map(|member| (member.team_id, member.participant_id))
        .collect::<BTreeSet<_>>();
    let roster_complete = teams.len() == 2
        && teams.iter().all(|team_id| {
            members
                .iter()
                .filter(|member| member.team_id == *team_id && !member.is_bench)
                .count()
                == MATCH_TEAM_SIZE
        });

    let mut slots = (0..slot_count)
        .map(|index| SlotFacts {
            index,
            available_count: 0,
            starter_available_count: 0,
            team_available_count: 0,
            min_team_starter_available_count: 0,
        })
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    let mut responded = BTreeSet::new();
    let mut no_slot = BTreeSet::new();
    let mut available_by_slot = vec![BTreeSet::new(); slot_count];
    let mut starter_available_by_slot = vec![BTreeMap::<i32, u32>::new(); slot_count];

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
            *starter_available_by_slot[slot_index]
                .entry(response.team_id)
                .or_insert(0) += 1;
        }
        available_by_slot[slot_index].insert(response.team_id);
    }
    for (index, (slot, available_teams)) in slots.iter_mut().zip(&available_by_slot).enumerate() {
        slot.team_available_count = u32::try_from(available_teams.len()).map_err(|_| {
            ScrimError::InvalidResponse("team availability count overflow".to_string())
        })?;
        slot.min_team_starter_available_count = teams
            .iter()
            .map(|team_id| {
                starter_available_by_slot[index]
                    .get(team_id)
                    .copied()
                    .unwrap_or(0)
            })
            .min()
            .unwrap_or(0);
    }

    let starter_missing_response_count = starter_keys
        .iter()
        .filter(|key| !responded.contains(key))
        .count();
    let best_common_slot = || {
        slots
            .iter()
            // Eine Bench-Zusage allein macht einen Team-Slot nicht spielbar. Mindestens ein
            // Stammspieler je Team muss für einen gemeinsamen Slot zugesagt haben.
            .filter(|slot| slot.min_team_starter_available_count > 0)
            .fold(None, |best: Option<&SlotFacts>, candidate| match best {
                // Leo organisiert um den Stammkader herum. Zuerst wird verhindert, dass ein
                // Team deutlich schlechter dasteht; dann zählt die Gesamtzahl der Starter.
                Some(current)
                    if current.min_team_starter_available_count
                        > candidate.min_team_starter_available_count
                        || (current.min_team_starter_available_count
                            == candidate.min_team_starter_available_count
                            && (current.starter_available_count
                                > candidate.starter_available_count
                                || (current.starter_available_count
                                    == candidate.starter_available_count
                                    && current.available_count >= candidate.available_count))) =>
                {
                    Some(current)
                }
                _ => Some(candidate),
            })
            .map(|slot| slot.index)
    };
    let ready_slot_index = if roster_complete && starter_missing_response_count == 0 {
        let full_slots = slots
            .iter()
            .filter(|slot| slot.starter_available_count as usize == starter_keys.len())
            .filter(|slot| slot.min_team_starter_available_count == MATCH_TEAM_SIZE as u32)
            .map(|slot| slot.index)
            .collect::<Vec<_>>();
        if full_slots.len() == 1 {
            Some(full_slots[0])
        } else {
            None
        }
    } else {
        None
    };
    let recommended_slot_index = deadline_passed.then(best_common_slot).flatten();
    let safe_release_slot_index = recommended_slot_index.filter(|slot_index| {
        let slot = &slots[*slot_index];
        roster_complete
            && slot.min_team_starter_available_count >= SAFE_TEAM_STARTERS
            && slot.starter_available_count >= SAFE_TOTAL_STARTERS
    });

    let selected_slot_index = released_slot_index
        .filter(|index| *index < slot_count)
        .or_else(|| deadline_passed.then_some(safe_release_slot_index).flatten());
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
        starter_missing_response_count: u32::try_from(starter_missing_response_count).map_err(
            |_| ScrimError::InvalidResponse("starter missing response count overflow".to_string()),
        )?,
        no_slot_count: u32::try_from(no_slot.len()).map_err(|_| {
            ScrimError::InvalidResponse("no-slot response count overflow".to_string())
        })?,
        ready_slot_index,
        safe_release_slot_index,
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

pub fn rank_replacement_candidates(candidates: &mut [ReplacementCandidate]) {
    candidates.sort_by(|left, right| {
        candidate_score(right)
            .partial_cmp(&candidate_score(left))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn candidate_score(candidate: &ReplacementCandidate) -> f64 {
    candidate
        .score_data
        .get("score")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
}
