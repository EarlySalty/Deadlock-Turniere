use turnier_match_core::{resolve_result, CanonicalTeams, Finality, TeamSlot, Winner};

#[test]
fn canonical_result_maps_slots_and_team_ids_without_owning_a_lifecycle() {
    let teams = CanonicalTeams::new(10_i64, 20_i64);

    let team0 = resolve_result(&teams, Winner::Slot(TeamSlot::Team0)).expect("team0 result");
    assert_eq!(team0.winner_id, 10);
    assert_eq!(team0.winner_slot, TeamSlot::Team0);
    assert_eq!(team0.finality, Finality::Final);

    let team1 = resolve_result(&teams, Winner::TeamId(20)).expect("team1 result");
    assert_eq!(team1.winner_id, 20);
    assert_eq!(team1.winner_slot, TeamSlot::Team1);
}

#[test]
fn canonical_result_preserves_legacy_same_team_behavior_but_rejects_foreign_winners() {
    let same = CanonicalTeams::new(10_i64, 10_i64);
    let resolved = resolve_result(&same, Winner::TeamId(10)).expect("legacy duplicate teams");
    assert_eq!(resolved.winner_slot, TeamSlot::Team0);

    let teams = CanonicalTeams::new(10_i64, 20_i64);
    assert!(resolve_result(&teams, Winner::TeamId(30)).is_err());
}
