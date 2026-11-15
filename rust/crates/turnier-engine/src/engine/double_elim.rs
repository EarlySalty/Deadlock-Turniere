//! Reine Double-Elimination-Verdrahtungs-Mathematik (DB-frei).
//!
//! Die Persistenz (`build_double_elimination_bracket`) ruft diese Funktionen, um
//! Rundengrößen und Loser-Drop-Ziele zu bestimmen; sie selbst berührt keine DB
//! und ist vollständig per Tabellen-Tests gegen 4/8/16/32-Team-Brackets
//! abgesichert. 1:1 portiert aus `engine.py`
//! (`_double_elimination_losers_round_size` und die zwei Loser-Drop-Schleifen).

use turnier_core::{BracketFormat, TournamentMode};

pub const DOUBLE_ELIMINATION_RULE: &str =
    "mindestens 4 Teams, und die Zahl muss eine Zweierpotenz sein (4, 8, 16, 32 …)";

pub const MIN_BRACKET_ENTRANTS: usize = 2;

pub fn bracket_startable(entrants: Option<usize>) -> bool {
    entrants.is_none_or(|n| n >= MIN_BRACKET_ENTRANTS)
}

pub fn supports_double_elimination(entrants: usize) -> bool {
    entrants >= 4 && crate::engine::seeding::is_power_of_two(entrants)
}

pub fn expected_bracket_entrants(mode: TournamentMode, team_count: usize) -> Option<usize> {
    match mode {
        TournamentMode::League => None,
        TournamentMode::GroupStage => {
            let groups = crate::engine::groups::group_count(None, team_count);
            Some((groups * crate::engine::groups::QUALIFIERS_PER_GROUP).min(team_count))
        }
        TournamentMode::BracketOnly => Some(team_count),
    }
}

pub fn effective_bracket_format(stored: BracketFormat, entrants: Option<usize>) -> BracketFormat {
    match (stored, entrants) {
        (BracketFormat::DoubleElimination, Some(n))
            if n >= MIN_BRACKET_ENTRANTS && !supports_double_elimination(n) =>
        {
            BracketFormat::SingleElimination
        }
        (format, _) => format,
    }
}

/// Anzahl der Matches einer Losers-Bracket-Runde.
///
/// Portiert `_double_elimination_losers_round_size(num_teams, round_num)`:
/// - ungerade Runde: `num_teams / 2^((round+3)/2)` (Integer-Division)
/// - gerade Runde:   `num_teams / 2^((round/2)+1)`
///
/// Die Magic-Exponenten ergeben für die üblichen Bracket-Größen:
/// 8 Teams → [2, 2, 1, 1], 16 → [4, 4, 2, 2, 1, 1], 32 → [8, 8, 4, 4, 2, 2, 1, 1].
pub fn losers_round_size(num_teams: usize, round_num: usize) -> usize {
    if round_num % 2 == 1 {
        num_teams / (1usize << ((round_num + 3) / 2))
    } else {
        num_teams / (1usize << ((round_num / 2) + 1))
    }
}

/// Anzahl der Winners-Bracket-Runden = `log2(num_teams)`.
pub fn total_winners_rounds(num_teams: usize) -> usize {
    // num_teams ist eine Zweierpotenz >= 4 (vom Aufrufer garantiert).
    num_teams.trailing_zeros() as usize
}

/// Anzahl der Losers-Bracket-Runden = `2 * (winners_rounds - 1)`.
pub fn total_losers_rounds(num_teams: usize) -> usize {
    2 * (total_winners_rounds(num_teams) - 1)
}

/// Ziel-Match-Index und Slot, in den der Verlierer von Winners-Runde 1 fällt.
///
/// Portiert die erste Drop-Schleife: WR1-Match `index` → `losers_round[0][index/2]`,
/// Slot 1 bei geradem Index, sonst Slot 2.
pub fn wr1_loser_drop(index: usize) -> (usize, i64) {
    let dest = index / 2;
    let slot = if index % 2 == 0 { 1 } else { 2 };
    (dest, slot)
}

/// Ziel-Match-Index für den Verlierer einer Winners-Runde `>= 2` (Slot ist
/// immer 2).
///
/// Portiert `destination_matches[(position - 1) % destination_count]`. Das
/// Python-Modulo `-1 % n` liefert `n - 1`; in Rust MUSS dafür `rem_euclid`
/// stehen, sonst entstünde ein negativer Index / Out-of-Bounds. Das ist genau
/// der im Port-Audit markierte Stolperstein.
pub fn higher_winners_loser_drop(position: usize, destination_count: usize) -> usize {
    debug_assert!(destination_count > 0);
    let pos = position as i64;
    let count = destination_count as i64;
    (pos - 1).rem_euclid(count) as usize
}

#[cfg(test)]
mod tests {
    #[test]
    fn double_elimination_fallback_contract() {
        assert!(!supports_double_elimination(5));
        assert!(!supports_double_elimination(2));
        assert!(supports_double_elimination(8));
        let double = BracketFormat::DoubleElimination;
        let single = BracketFormat::SingleElimination;
        assert_eq!(effective_bracket_format(double, Some(5)), single);
        assert_eq!(effective_bracket_format(double, Some(4)), double);
        assert_eq!(effective_bracket_format(double, Some(1)), double);
        assert_eq!(effective_bracket_format(double, Some(0)), double);
        assert_eq!(effective_bracket_format(single, Some(8)), single);
        assert_eq!(
            expected_bracket_entrants(TournamentMode::BracketOnly, 5),
            Some(5)
        );
        assert_eq!(
            expected_bracket_entrants(TournamentMode::GroupStage, 4),
            Some(4)
        );
        assert_eq!(
            expected_bracket_entrants(TournamentMode::GroupStage, 12),
            Some(6)
        );
        assert_eq!(
            expected_bracket_entrants(TournamentMode::GroupStage, 5),
            Some(4)
        );
        assert_eq!(expected_bracket_entrants(TournamentMode::League, 6), None);
    }

    use super::*;

    #[test]
    fn round_sizes_for_known_brackets() {
        let sizes = |n: usize| -> Vec<usize> {
            (1..=total_losers_rounds(n))
                .map(|r| losers_round_size(n, r))
                .collect()
        };
        assert_eq!(sizes(4), vec![1, 1]);
        assert_eq!(sizes(8), vec![2, 2, 1, 1]);
        assert_eq!(sizes(16), vec![4, 4, 2, 2, 1, 1]);
        assert_eq!(sizes(32), vec![8, 8, 4, 4, 2, 2, 1, 1]);
    }

    #[test]
    fn winners_and_losers_round_counts() {
        assert_eq!(total_winners_rounds(4), 2);
        assert_eq!(total_losers_rounds(4), 2);
        assert_eq!(total_winners_rounds(8), 3);
        assert_eq!(total_losers_rounds(8), 4);
        assert_eq!(total_winners_rounds(16), 4);
        assert_eq!(total_losers_rounds(16), 6);
    }

    #[test]
    fn wr1_drops_pair_into_one_losers_match() {
        assert_eq!(wr1_loser_drop(0), (0, 1));
        assert_eq!(wr1_loser_drop(1), (0, 2));
        assert_eq!(wr1_loser_drop(2), (1, 1));
        assert_eq!(wr1_loser_drop(3), (1, 2));
    }

    #[test]
    fn higher_winners_drop_uses_rem_euclid() {
        // destination_count = 2: position 0 -> (0-1) rem_euclid 2 = 1; pos 1 -> 0.
        assert_eq!(higher_winners_loser_drop(0, 2), 1);
        assert_eq!(higher_winners_loser_drop(1, 2), 0);
        // destination_count = 1: alles auf 0 (das LB-Finale-Ziel).
        assert_eq!(higher_winners_loser_drop(0, 1), 0);
        // destination_count = 4 (16-Team-WR2): 0->3, 1->0, 2->1, 3->2.
        assert_eq!(higher_winners_loser_drop(0, 4), 3);
        assert_eq!(higher_winners_loser_drop(1, 4), 0);
        assert_eq!(higher_winners_loser_drop(2, 4), 1);
        assert_eq!(higher_winners_loser_drop(3, 4), 2);
    }
}
