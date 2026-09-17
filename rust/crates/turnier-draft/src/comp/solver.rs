//! Exact k-best distinct hero sets, not a greedy hero popularity ranking.
//!
//! Process heroes once; a state is the subset of players already assigned.
//! Descending masks prevent a hero being assigned twice. For each state retain
//! the k best DIFFERENT hero sets and their best player assignment. Discarding
//! other prefixes is safe: future choices depend only on the mask and remaining
//! heroes, and all ranking criteria are additive/deterministic. This avoids
//! enumerating H^6 lineups. At most 64 states, 128 heroes, 6 players and 10 results.

use super::{Preference, MAX_HEROES, MAX_PLAYERS, RESULT_LIMIT};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Assignment {
    pub player_index: usize,
    pub hero_name: String,
    pub priority: u8,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Composition {
    pub score: u8,
    pub top_priority_count: u8,
    pub assignments: Vec<Assignment>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Conflict {
    pub player_indices: Vec<usize>,
    pub available_heroes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Results {
    pub compositions: Vec<Composition>,
    pub waiting_for: Vec<usize>,
    pub conflict: Option<Conflict>,
    pub max_score: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    score: u8,
    top: u8,
    heroes: Vec<usize>,
    picks: [Option<usize>; MAX_PLAYERS],
}

fn compare(a: &Candidate, b: &Candidate) -> Ordering {
    b.score
        .cmp(&a.score)
        .then_with(|| b.top.cmp(&a.top))
        .then_with(|| a.picks.cmp(&b.picks))
}

fn retain_best(bucket: &mut Vec<Candidate>, candidate: Candidate, limit: usize) {
    if let Some(i) = bucket.iter().position(|old| old.heroes == candidate.heroes) {
        if compare(&candidate, &bucket[i]) != Ordering::Less {
            return;
        }
        bucket.remove(i);
    }
    let index = bucket.partition_point(|old| compare(old, &candidate) != Ordering::Greater);
    if index < limit {
        bucket.insert(index, candidate);
        bucket.truncate(limit);
    }
}

pub fn solve(players: &[Vec<Preference>], limit: usize) -> Results {
    let n = players.len();
    let mut result = Results {
        compositions: Vec::new(),
        waiting_for: players
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.is_empty().then_some(i))
            .collect(),
        conflict: None,
        max_score: (n.min(MAX_PLAYERS) * 2) as u8,
    };
    if n == 0 || n > MAX_PLAYERS || !result.waiting_for.is_empty() || limit == 0 {
        return result;
    }
    let limit = limit.min(RESULT_LIMIT);
    let names: Vec<_> = players
        .iter()
        .flatten()
        .map(|p| p.hero_name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if names.len() > MAX_HEROES {
        return result;
    }
    let indices: HashMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();
    let mut weights = vec![vec![None; names.len()]; n];
    for (i, prefs) in players.iter().enumerate() {
        for p in prefs {
            if p.priority <= 2 {
                weights[i][indices[p.hero_name.as_str()]] = Some(p.priority);
            }
        }
    }
    let mut dp = vec![Vec::<Candidate>::new(); 1 << n];
    dp[0].push(Candidate {
        score: 0,
        top: 0,
        heroes: Vec::new(),
        picks: [None; MAX_PLAYERS],
    });
    for h in 0..names.len() {
        for mask in (0..(1 << n)).rev() {
            if dp[mask].is_empty() {
                continue;
            }
            let prefixes = dp[mask].clone();
            for (player, row) in weights.iter().enumerate() {
                if mask & (1 << player) != 0 {
                    continue;
                }
                let Some(priority) = row[h] else {
                    continue;
                };
                for prefix in &prefixes {
                    let mut candidate = prefix.clone();
                    candidate.picks[player] = Some(h);
                    candidate.heroes.push(h);
                    candidate.score += priority;
                    candidate.top += u8::from(priority == 2);
                    retain_best(&mut dp[mask | (1 << player)], candidate, limit);
                }
            }
        }
    }
    result.compositions = dp[(1 << n) - 1]
        .iter()
        .map(|c| Composition {
            score: c.score,
            top_priority_count: c.top,
            assignments: c.picks[..n]
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    let h = h.expect("full player mask has a pick for every player");
                    Assignment {
                        player_index: i,
                        hero_name: names[h].clone(),
                        priority: weights[i][h].expect("only selected heroes are assigned"),
                    }
                })
                .collect(),
        })
        .collect();
    if result.compositions.is_empty() {
        // Hall's condition identifies an actual bottleneck, even when the total
        // union has >= n heroes (e.g. two players can only play the same hero).
        let mut masks: Vec<usize> = (1..(1 << n)).collect();
        masks.sort_by_key(|m| (m.count_ones(), *m));
        for mask in masks {
            let player_indices: Vec<_> = (0..n).filter(|i| mask & (1 << i) != 0).collect();
            let available_heroes: Vec<_> = player_indices
                .iter()
                .flat_map(|i| players[*i].iter().map(|p| p.hero_name.clone()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if available_heroes.len() < player_indices.len() {
                result.conflict = Some(Conflict {
                    player_indices,
                    available_heroes,
                });
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};
    use std::collections::{BTreeMap, HashSet};

    fn prefs(pairs: &[(&str, u8)]) -> Vec<Preference> {
        pairs
            .iter()
            .map(|(hero, priority)| Preference {
                hero_name: (*hero).into(),
                priority: *priority,
            })
            .collect()
    }

    #[test]
    fn zero_is_playable_but_absent_is_not() {
        let r = solve(&[prefs(&[("A", 0)]), prefs(&[("B", 0)])], 10);
        assert_eq!(r.compositions.len(), 1);
        assert_eq!(r.compositions[0].score, 0);
        assert_eq!(r.max_score, 4);
    }

    #[test]
    fn conflicting_favorites_are_assigned_globally_not_greedily() {
        let r = solve(&[prefs(&[("A", 2), ("B", 1)]), prefs(&[("A", 2)])], 10);
        assert_eq!(r.compositions[0].score, 3);
        assert_eq!(r.compositions[0].assignments[0].hero_name, "B");
        assert_eq!(r.compositions[0].assignments[1].hero_name, "A");
    }

    #[test]
    fn hall_bottleneck_and_empty_selection_are_distinct() {
        let r = solve(
            &[
                prefs(&[("A", 2)]),
                prefs(&[("A", 1)]),
                prefs(&[("B", 2), ("C", 2)]),
            ],
            10,
        );
        assert!(r.compositions.is_empty());
        assert_eq!(r.conflict.unwrap().player_indices, vec![0, 1]);
        let r = solve(&[prefs(&[("A", 2)]), vec![]], 10);
        assert_eq!(r.waiting_for, vec![1]);
        assert!(r.conflict.is_none());
        assert!(solve(&[], 10).compositions.is_empty());
    }

    #[test]
    fn different_assignments_of_the_same_hero_set_are_not_duplicate_comps() {
        let p = prefs(&[("A", 2), ("B", 2), ("C", 1)]);
        let r = solve(&[p.clone(), p], 10);
        assert_eq!(r.compositions.len(), 3);
        assert_eq!(r.compositions[0].score, 4);
        assert_eq!(r.compositions[0].assignments[0].hero_name, "A");
    }

    fn brute(players: &[Vec<Preference>]) -> Vec<Composition> {
        fn visit(
            players: &[Vec<Preference>],
            picks: &mut Vec<Assignment>,
            all: &mut Vec<Composition>,
        ) {
            if picks.len() == players.len() {
                all.push(Composition {
                    score: picks.iter().map(|p| p.priority).sum(),
                    top_priority_count: picks.iter().filter(|p| p.priority == 2).count() as u8,
                    assignments: picks.clone(),
                });
                return;
            }
            let i = picks.len();
            for p in &players[i] {
                if picks.iter().any(|a| a.hero_name == p.hero_name) {
                    continue;
                }
                picks.push(Assignment {
                    player_index: i,
                    hero_name: p.hero_name.clone(),
                    priority: p.priority,
                });
                visit(players, picks, all);
                picks.pop();
            }
        }
        let mut all = Vec::new();
        visit(players, &mut Vec::new(), &mut all);
        all.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.top_priority_count.cmp(&a.top_priority_count))
                .then_with(|| {
                    a.assignments
                        .iter()
                        .map(|a| &a.hero_name)
                        .cmp(b.assignments.iter().map(|a| &a.hero_name))
                })
        });
        let mut seen = HashSet::new();
        all.retain(|c| {
            let mut heroes: Vec<_> = c.assignments.iter().map(|a| a.hero_name.clone()).collect();
            heroes.sort();
            seen.insert(heroes)
        });
        all.truncate(10);
        all
    }

    #[test]
    fn exact_top_ten_match_exhaustive_oracle_for_200_generated_pools() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(20260918);
        for case in 0..200 {
            let n = rng.gen_range(1..=5);
            let players: Vec<Vec<Preference>> = (0..n)
                .map(|_| {
                    (0..7)
                        .filter_map(|h| {
                            if rng.gen_bool(0.7) {
                                Some(Preference {
                                    hero_name: format!("Hero{h}"),
                                    priority: rng.gen_range(0..=2),
                                })
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .collect();
            assert_eq!(
                solve(&players, 10).compositions,
                brute(&players),
                "case {case}: {players:?}"
            );
        }
    }

    #[test]
    fn six_players_with_64_heroes_are_bounded_deterministic_and_unique() {
        let players: Vec<Vec<Preference>> = (0..6)
            .map(|i| {
                (0..64)
                    .map(|h| Preference {
                        hero_name: format!("H{h:02}"),
                        priority: ((i + h) % 3) as u8,
                    })
                    .collect()
            })
            .collect();
        let r = solve(&players, 10);
        assert_eq!(r.compositions.len(), 10);
        assert_eq!(r, solve(&players, 10));
        let mut sets = BTreeMap::new();
        for c in &r.compositions {
            assert_eq!(c.score, 12);
            let heroes: BTreeSet<_> = c.assignments.iter().map(|a| &a.hero_name).collect();
            assert_eq!(heroes.len(), 6);
            assert!(sets.insert(heroes, c.score).is_none());
        }
    }
}
