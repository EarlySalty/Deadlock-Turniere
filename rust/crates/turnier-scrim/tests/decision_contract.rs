use std::collections::BTreeSet;

use chrono::{Duration, TimeZone, Utc};
use turnier_scrim::decision::{
    derive_match_request_facts, is_scrim_history_entry, validate_match_request_batch,
};
use turnier_scrim::model::{
    MatchRequestBatchInput, MatchRequestPairingInput, MatchRequestResponse, MatchRequestTemplate,
    ResponseChoice, RosterMember, ScrimDay, ScrimSlot,
};
use turnier_scrim::ScrimError;

fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 1, hour, 0, 0)
        .single()
        .expect("valid fixture")
}

fn slot(day: ScrimDay, from: u16, to: u16) -> ScrimSlot {
    ScrimSlot { day, from, to }
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
    assert!(before.recommended_slot_index.is_none());
    assert!(before.replacement_needs.is_empty());

    let after = derive_match_request_facts(7, 2, &[10, 20], &members, &responses, true, None)
        .expect("deadline facts");
    assert_eq!(after.recommended_slot_index, Some(0));
    assert_eq!(after.selected_slot_index, Some(0));
    assert_eq!(after.replacement_needs.len(), 2);
    assert!(after.replacement_needs.iter().all(|need| !need.is_bench));
}

#[test]
fn tie_break_is_total_availability_then_starters_then_first_slot() {
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
