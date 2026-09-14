use std::collections::BTreeSet;

use chrono::{Duration, TimeZone, Utc};
use turnier_scrim::decision::{
    derive_match_request_facts, is_scrim_history_entry, rank_replacement_candidates,
    suggest_match_slots, validate_match_request_batch,
};
use turnier_scrim::model::{
    AvailabilitySlot, AvailabilityStatus, MatchRequestBatchInput, MatchRequestPairingInput,
    MatchRequestResponse, MatchRequestTemplate, ReplacementCandidate, ResponseChoice, RosterMember,
    ScrimDay, ScrimSlot, Team, TeamMember, WeeklyAvailability,
};
use turnier_scrim::ScrimError;

fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 1, hour, 0, 0)
        .single()
        .expect("valid fixture")
}

fn slot(day: ScrimDay, from: u16, to: u16) -> ScrimSlot {
    ScrimSlot {
        day,
        date: None,
        from,
        to,
    }
}

fn batch(
    common_slots: Option<Vec<ScrimSlot>>,
    matches: Vec<MatchRequestPairingInput>,
) -> MatchRequestBatchInput {
    MatchRequestBatchInput {
        template: MatchRequestTemplate::RegularScrim,
        deadline_at: at(2),
        slots: common_slots,
        matches,
    }
}

#[test]
fn batch_accepts_open_opponents_and_common_or_match_specific_slots() {
    let common = vec![
        slot(ScrimDay::Saturday, 1_200, 1_320),
        slot(ScrimDay::Sunday, 1_200, 1_320),
    ];
    let input = batch(
        Some(common.clone()),
        vec![
            MatchRequestPairingInput {
                team_a_id: 10,
                team_b_id: Some(20),
                slots: None,
            },
            MatchRequestPairingInput {
                team_a_id: 30,
                team_b_id: None,
                slots: Some(vec![
                    slot(ScrimDay::Friday, 1_140, 1_260),
                    slot(ScrimDay::Saturday, 1_140, 1_260),
                ]),
            },
        ],
    );

    let validated = validate_match_request_batch(&input, at(0), &BTreeSet::new())
        .expect("valid production-shaped batch");
    assert_eq!(validated.matches[0].slots, common);
    assert!(validated.matches[1].team_b_id.is_none());
    assert_eq!(validated.matches[1].slots[0].day, ScrimDay::Friday);
}

#[test]
fn batch_rejects_invalid_slot_counts_windows_and_duplicate_active_teams() {
    let one_slot = batch(
        None,
        vec![MatchRequestPairingInput {
            team_a_id: 10,
            team_b_id: Some(20),
            slots: Some(vec![slot(ScrimDay::Saturday, 1_200, 1_320)]),
        }],
    );
    assert!(validate_match_request_batch(&one_slot, at(0), &BTreeSet::new()).is_err());

    let invalid_window = batch(
        None,
        vec![MatchRequestPairingInput {
            team_a_id: 10,
            team_b_id: Some(20),
            slots: Some(vec![
                slot(ScrimDay::Saturday, 1_320, 1_200),
                slot(ScrimDay::Sunday, 1_200, 1_320),
            ]),
        }],
    );
    assert!(validate_match_request_batch(&invalid_window, at(0), &BTreeSet::new()).is_err());

    let duplicate = batch(
        Some(vec![
            slot(ScrimDay::Saturday, 1_200, 1_320),
            slot(ScrimDay::Sunday, 1_200, 1_320),
        ]),
        vec![
            MatchRequestPairingInput {
                team_a_id: 10,
                team_b_id: Some(20),
                slots: None,
            },
            MatchRequestPairingInput {
                team_a_id: 10,
                team_b_id: None,
                slots: None,
            },
        ],
    );
    assert!(validate_match_request_batch(&duplicate, at(0), &BTreeSet::new()).is_err());

    let active = BTreeSet::from([20]);
    let otherwise_valid = batch(
        Some(vec![
            slot(ScrimDay::Saturday, 1_200, 1_320),
            slot(ScrimDay::Sunday, 1_200, 1_320),
        ]),
        vec![MatchRequestPairingInput {
            team_a_id: 10,
            team_b_id: Some(20),
            slots: None,
        }],
    );
    assert!(validate_match_request_batch(&otherwise_valid, at(0), &active).is_err());
    assert!(validate_match_request_batch(&otherwise_valid, at(2), &BTreeSet::new()).is_err());
}

#[test]
fn batch_rejects_pairings_without_effective_slots() {
    let input = batch(
        None,
        vec![MatchRequestPairingInput {
            team_a_id: 10,
            team_b_id: Some(20),
            slots: None,
        }],
    );

    let error = validate_match_request_batch(&input, at(0), &BTreeSet::new())
        .expect_err("effective slots are required");
    assert!(matches!(
        error,
        ScrimError::InvalidProposal(message) if message == "Each match needs two to five slots"
    ));
}

fn member(team_id: i32, participant_id: i32, is_bench: bool) -> RosterMember {
    RosterMember {
        team_id,
        participant_id,
        display_name: format!("P{participant_id}"),
        is_bench,
    }
}

fn response(
    team_id: i32,
    participant_id: i32,
    slot_index: i32,
    response: ResponseChoice,
) -> MatchRequestResponse {
    MatchRequestResponse {
        request_id: 7,
        team_id,
        participant_id,
        discord_user_id: participant_id.to_string(),
        slot_index,
        response,
        source: "button".to_string(),
        message_id: None,
        channel_id: None,
        responded_at: at(1),
        updated_at: at(1),
    }
}

fn available(from: u16, to: u16) -> AvailabilitySlot {
    AvailabilitySlot {
        status: AvailabilityStatus::Available,
        from: Some(from),
        to: Some(to),
    }
}

fn unavailable() -> AvailabilitySlot {
    AvailabilitySlot {
        status: AvailabilityStatus::Unavailable,
        from: None,
        to: None,
    }
}

fn weekend(sat: Option<(u16, u16)>, sun: Option<(u16, u16)>) -> WeeklyAvailability {
    WeeklyAvailability {
        sat: Some(sat.map_or_else(unavailable, |(from, to)| available(from, to))),
        sun: Some(sun.map_or_else(unavailable, |(from, to)| available(from, to))),
        ..WeeklyAvailability::default()
    }
}

fn scheduling_team(
    id: i32,
    member_windows: Vec<WeeklyAvailability>,
    default_from: Option<i32>,
    default_to: Option<i32>,
) -> Team {
    Team {
        id,
        name: format!("Team {id}"),
        coach: Some("Coach".to_string()),
        coach_discord_id: None,
        discord_role_id: None,
        discord_channel_id: None,
        default_from,
        default_to,
        created_at: at(0),
        members: member_windows
            .into_iter()
            .enumerate()
            .map(|(index, availability_slots)| TeamMember {
                team_id: id,
                participant_id: id * 100 + i32::try_from(index).expect("fixture index"),
                display_name: format!("P{id}-{index}"),
                rank: None,
                discord_id: None,
                roles: None,
                availability: None,
                availability_slots: Some(availability_slots),
                notes: None,
                role: None,
                is_captain: index == 0,
                is_bench: false,
                substitute_until: None,
            })
            .collect(),
    }
}

#[test]
fn scheduling_suggestions_turn_weekly_availability_into_concrete_slots() {
    let all_evening = || weekend(Some((18 * 60, 23 * 60)), Some((18 * 60, 23 * 60)));
    let team_a = scheduling_team(
        10,
        (0..6).map(|_| all_evening()).collect(),
        Some(20 * 60),
        Some(22 * 60),
    );
    let team_b = scheduling_team(
        20,
        (0..6).map(|_| all_evening()).collect(),
        Some(20 * 60),
        Some(22 * 60),
    );

    let result = suggest_match_slots(&team_a, &team_b, 3);

    assert_eq!(result.suggestions.len(), 3);
    assert_eq!(result.suggestions[0].slot.day, ScrimDay::Saturday);
    assert_eq!(result.suggestions[0].slot.from, 20 * 60);
    assert_eq!(result.suggestions[0].slot.to, 22 * 60);
    assert_eq!(result.suggestions[0].available_starters, 12);
    assert!(result.suggestions[0].match_ready_roster);
    assert!(result.suggestions[0].full_current_roster);
}

#[test]
fn scheduling_prefers_both_complete_teams_over_prime_time_with_a_missing_starter() {
    let team_a = scheduling_team(
        10,
        (0..6)
            .map(|_| weekend(Some((19 * 60, 23 * 60)), Some((18 * 60, 22 * 60))))
            .collect(),
        Some(20 * 60),
        Some(22 * 60),
    );
    let mut team_b_windows = (0..5)
        .map(|_| weekend(Some((19 * 60, 23 * 60)), Some((18 * 60, 22 * 60))))
        .collect::<Vec<_>>();
    team_b_windows.push(weekend(None, Some((18 * 60, 22 * 60))));
    let team_b = scheduling_team(20, team_b_windows, Some(20 * 60), Some(22 * 60));

    let result = suggest_match_slots(&team_a, &team_b, 3);

    assert_eq!(result.suggestions[0].slot.day, ScrimDay::Sunday);
    assert_eq!(result.suggestions[0].available_starters, 12);
    assert_eq!(result.suggestions[0].missing_starters, 0);
}

#[test]
fn response_facts_are_derived_from_roster_and_participant_responses() {
    let members = vec![
        member(10, 1, false),
        member(10, 2, false),
        member(10, 3, true),
        member(20, 4, false),
        member(20, 5, false),
    ];
    let responses = vec![
        response(10, 1, 0, ResponseChoice::Available),
        response(10, 2, 1, ResponseChoice::Available),
        response(10, 3, 0, ResponseChoice::Available),
        response(20, 4, 0, ResponseChoice::Available),
        response(20, 5, -1, ResponseChoice::Unavailable),
    ];

    let before = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, false, None)
        .expect("valid response facts");
    assert_eq!(before.slots[0].available_count, 3);
    assert_eq!(before.slots[0].starter_available_count, 2);
    assert_eq!(before.slots[0].team_available_count, 2);
    assert_eq!(before.no_slot_count, 1);
    assert_eq!(before.starter_missing_response_count, 0);
    assert!(before.ready_slot_index.is_none());
    assert!(before.recommended_slot_index.is_none());
    assert!(before.replacement_needs.is_empty());

    let after = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, true, None)
        .expect("deadline facts");
    assert_eq!(after.recommended_slot_index, Some(0));
    assert!(after.safe_release_slot_index.is_none());
    assert!(after.selected_slot_index.is_none());
    assert!(after.replacement_needs.is_empty());
}

#[test]
fn ready_slot_ignores_bench_and_requires_all_twelve_starters() {
    let mut members = Vec::new();
    for participant_id in 1..=6 {
        members.push(member(10, participant_id, false));
    }
    members.push(member(10, 99, true));
    for participant_id in 11..=16 {
        members.push(member(20, participant_id, false));
    }
    members.push(member(20, 199, true));

    let mut responses = Vec::new();
    for participant_id in 1..=6 {
        responses.push(response(10, participant_id, 0, ResponseChoice::Available));
    }
    for participant_id in 11..=16 {
        responses.push(response(20, participant_id, 0, ResponseChoice::Available));
    }

    let facts = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, false, None)
        .expect("facts");
    assert_eq!(
        facts.missing_response_count, 2,
        "die Team-Bank darf noch offen sein"
    );
    assert_eq!(facts.starter_missing_response_count, 0);
    assert_eq!(facts.ready_slot_index, Some(0));
    assert!(
        facts.recommended_slot_index.is_none(),
        "vor Deadline keine erzwungene Empfehlung"
    );

    responses.pop();
    let incomplete = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, false, None)
        .expect("facts");
    assert_eq!(incomplete.starter_missing_response_count, 1);
    assert!(incomplete.ready_slot_index.is_none());
}

#[test]
fn safe_release_allows_two_replacements_but_not_three_on_one_team() {
    let mut members = Vec::new();
    for participant_id in 1..=6 {
        members.push(member(10, participant_id, false));
    }
    for participant_id in 11..=16 {
        members.push(member(20, participant_id, false));
    }

    let mut ten_ready = Vec::new();
    for participant_id in 1..=6 {
        ten_ready.push(response(10, participant_id, 0, ResponseChoice::Available));
    }
    for participant_id in 11..=14 {
        ten_ready.push(response(20, participant_id, 0, ResponseChoice::Available));
    }
    ten_ready.push(response(20, 15, -1, ResponseChoice::Unavailable));
    ten_ready.push(response(20, 16, -1, ResponseChoice::Unavailable));

    let safe = derive_match_request_facts(7, 2, &[10, 20], &members, &ten_ready, true, None)
        .expect("safe facts");
    assert_eq!(safe.recommended_slot_index, Some(0));
    assert_eq!(safe.safe_release_slot_index, Some(0));
    assert_eq!(safe.selected_slot_index, Some(0));
    assert_eq!(safe.replacement_needs.len(), 2);

    let mut nine_ready = Vec::new();
    for participant_id in 1..=6 {
        nine_ready.push(response(10, participant_id, 0, ResponseChoice::Available));
    }
    for participant_id in 11..=13 {
        nine_ready.push(response(20, participant_id, 0, ResponseChoice::Available));
    }
    for participant_id in 14..=16 {
        nine_ready.push(response(
            20,
            participant_id,
            -1,
            ResponseChoice::Unavailable,
        ));
    }

    let unsafe_facts =
        derive_match_request_facts(7, 2, &[10, 20], &members, &nine_ready, true, None)
            .expect("unsafe facts");
    assert_eq!(unsafe_facts.recommended_slot_index, Some(0));
    assert!(unsafe_facts.safe_release_slot_index.is_none());
    assert!(unsafe_facts.selected_slot_index.is_none());
    assert!(unsafe_facts.replacement_needs.is_empty());
}

#[test]
fn starter_availability_beats_extra_bench_votes() {
    let members = vec![
        member(10, 1, false),
        member(10, 2, false),
        member(10, 3, true),
        member(20, 4, false),
        member(20, 5, false),
        member(20, 6, true),
    ];
    // Slot 0 hat vier Stammspieler. Slot 1 hat nur drei Stammspieler, aber durch
    // beide Bankspieler insgesamt mehr Zusagen. Der Stammkader muss gewinnen.
    let responses = vec![
        response(10, 1, 0, ResponseChoice::Available),
        response(10, 2, 0, ResponseChoice::Available),
        response(20, 4, 0, ResponseChoice::Available),
        response(20, 5, 0, ResponseChoice::Available),
        response(10, 1, 1, ResponseChoice::Available),
        response(10, 3, 1, ResponseChoice::Available),
        response(20, 4, 1, ResponseChoice::Available),
        response(20, 6, 1, ResponseChoice::Available),
    ];

    let facts = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, true, None)
        .expect("facts");
    assert_eq!(facts.recommended_slot_index, Some(0));
}

#[test]
fn tie_break_is_starters_then_total_availability_then_first_slot() {
    let members = vec![
        member(10, 1, false),
        member(10, 2, true),
        member(20, 3, false),
        member(20, 4, true),
    ];
    let responses = vec![
        response(10, 1, 0, ResponseChoice::Available),
        response(10, 2, 0, ResponseChoice::Available),
        response(20, 3, 0, ResponseChoice::Available),
        response(20, 4, 0, ResponseChoice::Available),
        response(10, 1, 1, ResponseChoice::Available),
        response(10, 2, 1, ResponseChoice::Available),
        response(20, 3, 1, ResponseChoice::Available),
        response(20, 4, 1, ResponseChoice::Available),
    ];
    let facts = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, true, None)
        .expect("facts");
    assert_eq!(facts.recommended_slot_index, Some(0));

    let members = vec![
        member(10, 1, false),
        member(10, 2, false),
        member(10, 3, true),
        member(20, 4, false),
        member(20, 5, false),
        member(20, 6, true),
    ];
    let starters_win = vec![
        response(10, 1, 0, ResponseChoice::Available),
        response(10, 3, 0, ResponseChoice::Available),
        response(20, 4, 0, ResponseChoice::Available),
        response(20, 6, 0, ResponseChoice::Available),
        response(10, 1, 1, ResponseChoice::Available),
        response(10, 2, 1, ResponseChoice::Available),
        response(20, 4, 1, ResponseChoice::Available),
        response(20, 5, 1, ResponseChoice::Available),
    ];
    let facts = derive_match_request_facts(7, 2, &[10, 20], &members, &starters_win, true, None)
        .expect("facts");
    assert_eq!(facts.recommended_slot_index, Some(1));
}

#[test]
fn released_slot_wins_over_recommendation_for_replacements() {
    let members = vec![member(10, 1, false), member(20, 2, false)];
    let responses = vec![
        response(10, 1, 0, ResponseChoice::Available),
        response(20, 2, 0, ResponseChoice::Available),
    ];

    let facts = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, true, Some(1))
        .expect("released slot facts");
    assert_eq!(facts.recommended_slot_index, Some(0));
    assert_eq!(facts.selected_slot_index, Some(1));
    assert_eq!(facts.replacement_needs.len(), 2);
}

#[test]
fn historical_responses_from_a_previous_roster_do_not_break_facts() {
    let members = vec![member(10, 1, false), member(20, 2, false)];
    let responses = vec![
        response(10, 99, 0, ResponseChoice::Available),
        response(10, 1, 0, ResponseChoice::Available),
        response(20, 2, 0, ResponseChoice::Available),
    ];

    let facts = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, true, None)
        .expect("stale roster response remains readable");
    assert_eq!(facts.slots[0].available_count, 2);
    assert_eq!(facts.missing_response_count, 0);
}

#[test]
fn history_uses_only_final_selected_result_and_never_cancelled() {
    assert!(!is_scrim_history_entry(
        "scheduled",
        Some("finished"),
        false
    ));
    assert!(is_scrim_history_entry("scheduled", None, true));
    assert!(!is_scrim_history_entry("completed", None, false));
    assert!(!is_scrim_history_entry("cancelled", Some("finished"), true));
    assert!(!is_scrim_history_entry("canceled", Some("finished"), true));
}

#[test]
fn response_deadline_example_is_strict() {
    assert!(at(0) + Duration::hours(1) < at(2));
}

#[test]
fn replacement_candidates_rank_by_score_then_stable_id() {
    let candidate = |id, score| ReplacementCandidate {
        id,
        need_id: 1,
        participant_id: Some(i32::try_from(id).expect("fixture id")),
        discord_user_id: None,
        display_name: None,
        rank: None,
        roles: None,
        availability: None,
        candidate_data: serde_json::json!({}),
        score_data: serde_json::json!({"score": score}),
        status: "candidate".to_string(),
    };
    let mut candidates = vec![candidate(3, 50), candidate(2, 90), candidate(1, 50)];

    rank_replacement_candidates(&mut candidates);

    assert_eq!(
        candidates
            .into_iter()
            .map(|candidate| candidate.id)
            .collect::<Vec<_>>(),
        vec![2, 1, 3]
    );
}
