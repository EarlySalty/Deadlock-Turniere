//! Reine Seeding-Algorithmen (DB-frei): Standard-Seed-Paarung für Single-Elim,
//! Snake-Draft-Gruppenindex und die Slot-Größen/-Verteilung für Mini-Group-
//! Runden bei Nicht-Zweierpotenzen.
//!
//! Diese Funktionen sind 1:1 aus `engine.py` portiert (`_seed_slot_order`,
//! `_slot_sizes_for_round`, `_distribute_entries_across_slots`, der Snake-Draft
//! aus `_generate_groups_in_db`) und gehören zu den am intensivsten getesteten
//! Stellen des Subsystems.

/// Standard-Turnier-Seed-Reihenfolge (1-basiert) für eine Bracket-Größe.
///
/// Portiert `_seed_slot_order(size)`: erzeugt die klassische Paarung
/// 1v(n), 2v(n-1), … so, dass die Top-Seeds erst im Finale aufeinandertreffen.
/// `size` MUSS eine Zweierpotenz sein (Aufrufer garantieren das). Beispiel
/// `size=8`: `[1, 8, 4, 5, 2, 7, 3, 6]` → Paare (1,8)(4,5)(2,7)(3,6).
pub fn seed_slot_order(size: usize) -> Vec<usize> {
    let mut order = vec![1usize];
    let mut current_size = 1usize;
    while current_size < size {
        current_size *= 2;
        let mut next_order = Vec::with_capacity(order.len() * 2);
        for seed in &order {
            next_order.push(*seed);
            next_order.push(current_size + 1 - *seed);
        }
        order = next_order;
    }
    order
}

/// Gruppen-Index eines Teams im Snake-Draft.
///
/// Portiert die Verteilung aus `_generate_groups_in_db`: Teams sind absteigend
/// nach Durchschnitts-Score sortiert (Index `i`, 0-basiert), `num_groups`
/// Gruppen. Gerade „Runden" laufen vorwärts (A,B,C,D), ungerade rückwärts
/// (D,C,B,A) — daher der „Schlangen"-Verlauf, der die Stärke balanciert.
pub fn snake_draft_group_index(i: usize, num_groups: usize) -> usize {
    let round_num = i / num_groups;
    if round_num % 2 == 0 {
        i % num_groups
    } else {
        num_groups - 1 - (i % num_groups)
    }
}

/// Slot-Größen einer Bracket-Runde bei Nicht-Zweierpotenz (Mini-Group-Runde).
///
/// Portiert `_slot_sizes_for_round(num_entries)`: `num_entries // 2` Slots,
/// Basisgröße `num_entries // slot_count`, der Rest wird von HINTEN aufgefüllt
/// (`sizes[-1 - offset] += 1`). So entstehen 2er- und 3er-Slots (3er werden zu
/// Round-Robin-Mini-Groups), ohne dass ein Team ein Freilos bekommt.
pub fn slot_sizes_for_round(num_entries: usize) -> Vec<usize> {
    let slot_count = num_entries / 2;
    let base_size = num_entries / slot_count;
    let remainder = num_entries % slot_count;
    let mut sizes = vec![base_size; slot_count];
    for offset in 0..remainder {
        let idx = sizes.len() - 1 - offset;
        sizes[idx] += 1;
    }
    sizes
}

/// Verteilt Einträge im Schlangen-Muster auf die Slots (alternierend vorwärts/
/// rückwärts über die noch nicht vollen Slots).
///
/// Portiert `_distribute_entries_across_slots`: füllt in jeder „Welle" alle
/// Slots, die noch Platz haben (`len < slot_size`), und kehrt die Reihenfolge in
/// jeder zweiten Welle um (`reverse`). Generisch über den Eintrags-Typ, damit es
/// rein und ohne DB testbar bleibt.
pub fn distribute_entries_across_slots<T: Clone>(
    entries: &[T],
    slot_sizes: &[usize],
) -> Vec<Vec<T>> {
    let mut slots: Vec<Vec<T>> = slot_sizes.iter().map(|_| Vec::new()).collect();
    let mut entry_index = 0usize;
    let mut reverse = false;
    while entry_index < entries.len() {
        let mut active_indices: Vec<usize> = slot_sizes
            .iter()
            .enumerate()
            .filter(|(slot_index, slot_size)| slots[*slot_index].len() < **slot_size)
            .map(|(slot_index, _)| slot_index)
            .collect();
        if reverse {
            active_indices.reverse();
        }
        for slot_index in active_indices {
            if entry_index >= entries.len() {
                break;
            }
            slots[slot_index].push(entries[entry_index].clone());
            entry_index += 1;
        }
        reverse = !reverse;
    }
    slots
}

/// `value` ist eine positive Zweierpotenz. Portiert `_is_power_of_two`.
pub fn is_power_of_two(value: usize) -> bool {
    value > 0 && (value & (value - 1)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_slot_order_classic() {
        assert_eq!(seed_slot_order(1), vec![1]);
        assert_eq!(seed_slot_order(2), vec![1, 2]);
        assert_eq!(seed_slot_order(4), vec![1, 4, 2, 3]);
        assert_eq!(seed_slot_order(8), vec![1, 8, 4, 5, 2, 7, 3, 6]);
        assert_eq!(
            seed_slot_order(16),
            vec![1, 16, 8, 9, 4, 13, 5, 12, 2, 15, 7, 10, 3, 14, 6, 11]
        );
    }

    #[test]
    fn snake_draft_matches_python_pattern() {
        // 4 Gruppen: Runde 1 vorwärts (0,1,2,3), Runde 2 rückwärts (3,2,1,0).
        let g = |i| snake_draft_group_index(i, 4);
        assert_eq!((g(0), g(1), g(2), g(3)), (0, 1, 2, 3));
        assert_eq!((g(4), g(5), g(6), g(7)), (3, 2, 1, 0));
        assert_eq!((g(8), g(9), g(10), g(11)), (0, 1, 2, 3));
    }

    #[test]
    fn slot_sizes_fill_from_back() {
        // 5 Teams -> 2 Slots: base 2, remainder 1 -> letzter Slot +1 -> [2, 3].
        assert_eq!(slot_sizes_for_round(5), vec![2, 3]);
        // 6 -> 3 Slots: base 2, rem 0 -> [2,2,2].
        assert_eq!(slot_sizes_for_round(6), vec![2, 2, 2]);
        // 7 -> 3 Slots: base 2, rem 1 -> [2,2,3].
        assert_eq!(slot_sizes_for_round(7), vec![2, 2, 3]);
        // 11 -> 5 Slots: base 2, rem 1 -> [2,2,2,2,3].
        assert_eq!(slot_sizes_for_round(11), vec![2, 2, 2, 2, 3]);
        // 13 -> 6 Slots: base 2, rem 1 -> [2,2,2,2,2,3].
        assert_eq!(slot_sizes_for_round(13), vec![2, 2, 2, 2, 2, 3]);
    }

    #[test]
    fn distribute_snakes_across_slots() {
        // 5 Einträge auf [2,3]: Welle1 vorwärts füllt Slot0,Slot1 (0,1);
        // Welle2 rückwärts über noch-offene [1,0] -> (2 in Slot1, 3 in Slot0);
        // Welle3 vorwärts über noch-offene [1] -> (4 in Slot1).
        let sizes = slot_sizes_for_round(5);
        let entries: Vec<i64> = vec![0, 1, 2, 3, 4];
        let slots = distribute_entries_across_slots(&entries, &sizes);
        assert_eq!(slots, vec![vec![0, 3], vec![1, 2, 4]]);
    }

    #[test]
    fn power_of_two_check() {
        assert!(is_power_of_two(1));
        assert!(is_power_of_two(8));
        assert!(!is_power_of_two(0));
        assert!(!is_power_of_two(6));
    }
}
