use turnier_match::core_adapter::{resolve_bracket_result, resolve_group_result};
use turnier_match_core::TeamSlot;

#[test]
fn bracket_adapter_preserves_zero_based_slots() {
    let team0 = resolve_bracket_result(10, 20, Some(0), None).expect("team1");
    assert_eq!((team0.winner_id, team0.winner_slot), (10, TeamSlot::Team0));

    let team1 = resolve_bracket_result(10, 20, Some(1), None).expect("team2");
    assert_eq!((team1.winner_id, team1.winner_slot), (20, TeamSlot::Team1));
    assert!(resolve_bracket_result(10, 20, Some(2), None).is_err());
}

#[test]
fn bracket_adapter_checks_id_and_slot_consistency() {
    assert!(resolve_bracket_result(10, 20, Some(0), Some(10)).is_ok());
    assert!(resolve_bracket_result(10, 20, Some(1), Some(10)).is_err());
    assert!(resolve_bracket_result(10, 20, None, Some(30)).is_err());
}

#[test]
fn group_adapter_preserves_one_based_slots_and_winner_id_precedence() {
    let team0 = resolve_group_result(10, 20, Some(1), None).expect("team1");
    assert_eq!((team0.winner_id, team0.winner_slot), (10, TeamSlot::Team0));

    let team1 = resolve_group_result(10, 20, Some(2), None).expect("team2");
    assert_eq!((team1.winner_id, team1.winner_slot), (20, TeamSlot::Team1));

    let legacy_precedence =
        resolve_group_result(10, 20, Some(1), Some(20)).expect("winner_id wins");
    assert_eq!(legacy_precedence.winner_slot, TeamSlot::Team1);
    assert!(resolve_group_result(10, 20, Some(0), None).is_err());
}

#[test]
fn adapters_preserve_legacy_same_team_results() {
    let bracket = resolve_bracket_result(10, 10, Some(0), Some(10)).expect("legacy bracket");
    assert_eq!(bracket.winner_slot, TeamSlot::Team0);
    assert!(resolve_bracket_result(10, 10, Some(1), Some(10)).is_err());

    let group = resolve_group_result(10, 10, Some(2), None).expect("legacy group");
    assert_eq!(group.winner_slot, TeamSlot::Team0);
}
