//! Mini-Group-Auswertung: Tiebreaker-Kette und Punkt-Differenz-Extraktion (reine
//! Logik). Portiert aus `mini_groups.py`.
//!
//! Die Tiebreaker-Kette ist deterministisch und gut testbar:
//! Wins → (2er-Tie) Head-to-Head → Point-Diff → Seed → min(team_id);
//! bei 3+-Way zusätzlich eine Mini-Head-to-Head-Runde unter den Gleichstand-
//! Teams, danach Point-Diff, danach (Seed, team_id).

use std::collections::HashMap;

use serde_json::Value;

/// Symmetrisch gespeicherte Head-to-Head-Ergebnisse: `(a, b) -> winner` und
/// `(b, a) -> winner` (wie im Original Z.201-202).
pub type HeadToHead = HashMap<(i64, i64), i64>;

/// Liefert den direkten Gewinner zwischen zwei Teams, falls er einer der beiden
/// ist. Portiert `_head_to_head_winner`.
pub fn head_to_head_winner(h2h: &HeadToHead, left: i64, right: i64) -> Option<i64> {
    match h2h.get(&(left, right)).copied() {
        Some(w) if w == left => Some(left),
        Some(w) if w == right => Some(right),
        _ => None,
    }
}

/// Bricht einen 2er-Gleichstand: Head-to-Head → Point-Diff → Seed → min(id).
/// Portiert `_break_two_way_tie` (der ungenutzte `wins`-Parameter entfällt —
/// bug-preserved als „toter Parameter").
pub fn break_two_way_tie(
    left: i64,
    right: i64,
    point_diff: &HashMap<i64, i64>,
    seed_order: &HashMap<i64, i64>,
    h2h: &HeadToHead,
) -> i64 {
    if let Some(direct) = head_to_head_winner(h2h, left, right) {
        return direct;
    }
    let (pl, pr) = (point_diff[&left], point_diff[&right]);
    if pl != pr {
        return if pl > pr { left } else { right };
    }
    let (sl, sr) = (seed_order[&left], seed_order[&right]);
    if sl != sr {
        return if sl < sr { left } else { right };
    }
    left.min(right)
}

/// Wählt den Mini-Group-Sieger über die volle Tiebreaker-Kette.
/// Portiert `_select_mini_group_winner`. `team_ids` ist die Seed-geordnete
/// Teilnehmerliste.
pub fn select_mini_group_winner(
    team_ids: &[i64],
    wins: &HashMap<i64, i64>,
    point_diff: &HashMap<i64, i64>,
    seed_order: &HashMap<i64, i64>,
    h2h: &HeadToHead,
) -> i64 {
    let max_wins = team_ids.iter().map(|t| wins[t]).max().unwrap();
    let tied: Vec<i64> = team_ids
        .iter()
        .copied()
        .filter(|t| wins[t] == max_wins)
        .collect();
    if tied.len() == 1 {
        return tied[0];
    }
    if tied.len() == 2 {
        return break_two_way_tie(tied[0], tied[1], point_diff, seed_order, h2h);
    }

    // 3+-Way: Mini-Head-to-Head nur unter den Gleichstand-Teams.
    let mut mini_wins: HashMap<i64, i64> = tied.iter().map(|t| (*t, 0)).collect();
    for &team in &tied {
        for &opponent in &tied {
            if team == opponent {
                continue;
            }
            if head_to_head_winner(h2h, team, opponent) == Some(team) {
                *mini_wins.get_mut(&team).unwrap() += 1;
            }
        }
    }
    let max_mini = *mini_wins.values().max().unwrap();
    let narrowed: Vec<i64> = tied
        .iter()
        .copied()
        .filter(|t| mini_wins[t] == max_mini)
        .collect();
    if narrowed.len() == 1 {
        return narrowed[0];
    }
    if narrowed.len() == 2 {
        return break_two_way_tie(narrowed[0], narrowed[1], point_diff, seed_order, h2h);
    }

    // Point-Diff, dann (Seed, team_id).
    let best_pd = narrowed.iter().map(|t| point_diff[t]).max().unwrap();
    let pd_winners: Vec<i64> = narrowed
        .iter()
        .copied()
        .filter(|t| point_diff[t] == best_pd)
        .collect();
    if pd_winners.len() == 1 {
        return pd_winners[0];
    }
    *pd_winners
        .iter()
        .min_by_key(|t| (seed_order[t], **t))
        .unwrap()
}

/// Extrahiert die Punkt-Differenz eines Matches aus Sicht von `team_id`.
///
/// Portiert `_extract_point_diff` 1:1 (bug-preserved tolerante Heuristik): vier
/// Score-Key-Paar-Schemata, dann ein verschachteltes `score`-Objekt, sonst der
/// `winner_id`-basierte `±1`-Fallback. `match_stats` ist das bereits geparste
/// JSON (oder `None`).
pub fn extract_point_diff(
    match_stats: Option<&Value>,
    team1_id: i64,
    _team2_id: i64,
    winner_id: Option<i64>,
    team_id: i64,
) -> i64 {
    let winner_fallback = || -> i64 {
        match winner_id {
            None => 0,
            Some(w) => {
                if w == team_id {
                    1
                } else {
                    -1
                }
            }
        }
    };

    let Some(Value::Object(parsed)) = match_stats else {
        return winner_fallback();
    };

    let as_i64 = |v: &Value| -> Option<i64> {
        match v {
            Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            Value::String(s) => s.trim().parse::<i64>().ok(),
            _ => None,
        }
    };

    let score_pairs = [
        ("team1_score", "team2_score"),
        ("score_team1", "score_team2"),
        ("team1_points", "team2_points"),
        ("points_team1", "points_team2"),
    ];
    for (key1, key2) in score_pairs {
        if parsed.contains_key(key1) && parsed.contains_key(key2) {
            // Python: bei nicht-parsbaren Werten `break` -> fällt auf nested/winner.
            match (as_i64(&parsed[key1]), as_i64(&parsed[key2])) {
                (Some(s1), Some(s2)) => {
                    return if team1_id == team_id {
                        s1 - s2
                    } else {
                        s2 - s1
                    };
                }
                _ => break,
            }
        }
    }

    if let Some(Value::Object(nested)) = parsed.get("score") {
        let t1 = nested.get("team1").and_then(as_i64);
        let t2 = nested.get("team2").and_then(as_i64);
        if let (Some(s1), Some(s2)) = (t1, t2) {
            return if team1_id == team_id { s1 - s2 } else { s2 - s1 };
        }
    }

    winner_fallback()
}

/// Aggregiert eine vollständige Mini-Group aus ihren abgeschlossenen Matches und
/// liefert (wins, point_diff, head_to_head). Reine Funktion über bereits
/// geladene Match-Daten.
pub struct MiniGroupMatch {
    pub team1_id: i64,
    pub team2_id: i64,
    pub winner_id: i64,
    pub match_stats: Option<Value>,
}

/// Wertet die Round-Robin-Matches aus (Wins, Punkt-Differenz, Head-to-Head).
pub fn aggregate(
    team_ids: &[i64],
    matches: &[MiniGroupMatch],
) -> (HashMap<i64, i64>, HashMap<i64, i64>, HeadToHead) {
    let mut wins: HashMap<i64, i64> = team_ids.iter().map(|t| (*t, 0)).collect();
    let mut point_diff: HashMap<i64, i64> = team_ids.iter().map(|t| (*t, 0)).collect();
    let mut h2h: HeadToHead = HashMap::new();

    for mr in matches {
        *wins.entry(mr.winner_id).or_insert(0) += 1;
        *point_diff.entry(mr.team1_id).or_insert(0) += extract_point_diff(
            mr.match_stats.as_ref(),
            mr.team1_id,
            mr.team2_id,
            Some(mr.winner_id),
            mr.team1_id,
        );
        *point_diff.entry(mr.team2_id).or_insert(0) += extract_point_diff(
            mr.match_stats.as_ref(),
            mr.team1_id,
            mr.team2_id,
            Some(mr.winner_id),
            mr.team2_id,
        );
        h2h.insert((mr.team1_id, mr.team2_id), mr.winner_id);
        h2h.insert((mr.team2_id, mr.team1_id), mr.winner_id);
    }
    (wins, point_diff, h2h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn two_way_tie_uses_head_to_head_first() {
        let mut h2h = HeadToHead::new();
        h2h.insert((1, 2), 2);
        h2h.insert((2, 1), 2);
        let pd: HashMap<i64, i64> = [(1, 5), (2, -5)].into();
        let seed: HashMap<i64, i64> = [(1, 0), (2, 1)].into();
        // H2H sagt 2 gewinnt — schlägt sogar die bessere Point-Diff von 1.
        assert_eq!(break_two_way_tie(1, 2, &pd, &seed, &h2h), 2);
    }

    #[test]
    fn two_way_tie_falls_to_point_diff_then_seed_then_id() {
        let h2h = HeadToHead::new(); // kein direktes Ergebnis
        let pd: HashMap<i64, i64> = [(1, 3), (2, 1)].into();
        let seed: HashMap<i64, i64> = [(1, 1), (2, 0)].into();
        assert_eq!(break_two_way_tie(1, 2, &pd, &seed, &h2h), 1); // point_diff
        let pd2: HashMap<i64, i64> = [(1, 0), (2, 0)].into();
        assert_eq!(break_two_way_tie(1, 2, &pd2, &seed, &h2h), 2); // seed (2 niedriger)
        let seed2: HashMap<i64, i64> = [(1, 0), (2, 0)].into();
        assert_eq!(break_two_way_tie(2, 1, &pd2, &seed2, &h2h), 1); // min id
    }

    #[test]
    fn three_way_clear_winner_by_wins() {
        let ids = vec![1, 2, 3];
        let wins: HashMap<i64, i64> = [(1, 2), (2, 1), (3, 0)].into();
        let pd: HashMap<i64, i64> = [(1, 0), (2, 0), (3, 0)].into();
        let seed: HashMap<i64, i64> = [(1, 0), (2, 1), (3, 2)].into();
        let h2h = HeadToHead::new();
        assert_eq!(select_mini_group_winner(&ids, &wins, &pd, &seed, &h2h), 1);
    }

    #[test]
    fn point_diff_from_score_keys() {
        let stats = json!({"team1_score": 10, "team2_score": 4});
        // Aus Sicht von team1 (id=1): 10-4 = 6.
        assert_eq!(extract_point_diff(Some(&stats), 1, 2, Some(1), 1), 6);
        // Aus Sicht von team2 (id=2): 4-10 = -6.
        assert_eq!(extract_point_diff(Some(&stats), 1, 2, Some(1), 2), -6);
    }

    #[test]
    fn point_diff_winner_fallback_when_no_stats() {
        assert_eq!(extract_point_diff(None, 1, 2, Some(1), 1), 1);
        assert_eq!(extract_point_diff(None, 1, 2, Some(1), 2), -1);
        assert_eq!(extract_point_diff(None, 1, 2, None, 1), 0);
    }

    #[test]
    fn nested_score_object() {
        let stats = json!({"score": {"team1": 13, "team2": 7}});
        assert_eq!(extract_point_diff(Some(&stats), 1, 2, Some(1), 1), 6);
    }
}
