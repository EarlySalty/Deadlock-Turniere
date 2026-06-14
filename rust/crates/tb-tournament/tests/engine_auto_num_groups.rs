//! Paritätstest: Auto-Gruppen-Anzahl. Portiert aus
//! `backend/tests/test_engine_auto_num_groups.py` (5 parametrisierte Fälle).

use tb_tournament::auto_num_groups;

#[test]
fn auto_num_groups_returns_expected_best_practice_values() {
    let cases = [(8, 2), (16, 4), (20, 5), (24, 6), (32, 8)];
    for (team_count, expected) in cases {
        assert_eq!(auto_num_groups(team_count), expected, "team_count={team_count}");
    }
}
