//! Reine Gruppen-Helfer: Auto-Gruppen-Anzahl und Round-Robin-Paarung.

/// Best-Practice-Gruppen-Anzahl aus der Team-Zahl.
///
/// Portiert `_auto_num_groups(team_count)`: Ziel 4 Teams/Gruppe, geklemmt auf
/// 2..=8. Unter 8 Teams hart 2 Gruppen. `round(team_count / 4)` nutzt Pythons
/// Banker's-Rounding NICHT relevant, da `team_count/4` für die geprüften Werte
/// (16,20,24,32) ganzzahlig bzw. eindeutig ist; wir runden kaufmännisch
/// (`.round()` auf f64), was für alle nicht-.5-Fälle identisch ist.
///
/// HINWEIS (bug-preserved): Das zweite Clamping auf `team_count/2` passiert
/// bewusst weiterhin in `generate_groups` (`actual_groups`), nicht hier — exakt
/// wie im Original, wo `_auto_num_groups` und `_generate_groups_in_db` getrennt
/// klemmen.
pub fn auto_num_groups(team_count: usize) -> usize {
    if team_count < 8 {
        return 2;
    }
    let num_groups = python_round(team_count as f64 / 4.0);
    num_groups.clamp(2, 8)
}

/// Pythons `round()` für positive Werte: kaufmännisch (round-half-to-even).
/// Für die hier auftretenden Quotienten ist das deckungsgleich mit
/// round-half-up; round-half-to-even ist nur bei exaktem `.5` sichtbar.
fn python_round(value: f64) -> usize {
    let floor = value.floor();
    let frac = value - floor;
    let rounded = if (frac - 0.5).abs() < f64::EPSILON {
        // Banker's Rounding: zur geraden Zahl.
        let f = floor as i64;
        if f % 2 == 0 {
            f
        } else {
            f + 1
        }
    } else {
        value.round() as i64
    };
    rounded.max(0) as usize
}

/// Erzeugt die Round-Robin-Paare (jedes Paar genau einmal, `i < j`) über die
/// Indizes `0..n`. Reine Reihenfolge wie im Original
/// (`for i.. for j in i+1..`).
pub fn round_robin_pairs(n: usize) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            pairs.push((i, j));
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_num_groups_best_practice() {
        assert_eq!(auto_num_groups(8), 2);
        assert_eq!(auto_num_groups(16), 4);
        assert_eq!(auto_num_groups(20), 5);
        assert_eq!(auto_num_groups(24), 6);
        assert_eq!(auto_num_groups(32), 8);
    }

    #[test]
    fn auto_num_groups_clamps() {
        assert_eq!(auto_num_groups(2), 2);
        assert_eq!(auto_num_groups(7), 2);
        // 40 Teams -> round(10) -> clamp 8.
        assert_eq!(auto_num_groups(40), 8);
    }

    #[test]
    fn round_robin_pairs_count() {
        assert_eq!(round_robin_pairs(2), vec![(0, 1)]);
        assert_eq!(round_robin_pairs(3), vec![(0, 1), (0, 2), (1, 2)]);
        assert_eq!(round_robin_pairs(4).len(), 6);
    }
}
