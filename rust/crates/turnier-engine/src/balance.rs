use std::cmp::Reverse;
use std::collections::HashSet;

use crate::engine::seeding::snake_draft_group_index;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalancePlayer {
    pub id: i64,
    pub score: Option<i64>,
    pub priority: bool,
    pub signup_order: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalancedPlayer {
    pub id: i64,
    pub score: f64,
    pub priority: bool,
    pub used_median: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalancedTeam {
    pub players: Vec<BalancedPlayer>,
    pub score_sum: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalanceResult {
    pub teams: Vec<BalancedTeam>,
    pub waitlist: Vec<BalancedPlayer>,
    pub median_score: f64,
    pub gap: f64,
}

pub fn balance_players(
    mut players: Vec<BalancePlayer>,
    team_count: usize,
    team_size: usize,
) -> Result<BalanceResult, &'static str> {
    if team_size == 0 {
        return Err("team size must be positive");
    }
    let mut ids = HashSet::new();
    if players.iter().any(|player| !ids.insert(player.id)) {
        return Err("player ids must be unique");
    }
    let median_score = median_rank(&players);
    players.sort_by_key(|player| (Reverse(player.priority), player.signup_order, player.id));
    let team_count = team_count.min(players.len() / team_size);
    let selected_count = team_count * team_size;
    let mut selected = players;
    let waitlist = selected.split_off(selected_count);
    selected.sort_by(|left, right| {
        effective_score(right, median_score)
            .total_cmp(&effective_score(left, median_score))
            .then_with(|| (left.signup_order, left.id).cmp(&(right.signup_order, right.id)))
    });
    let mut teams = vec![
        BalancedTeam {
            players: Vec::new(),
            score_sum: 0.0
        };
        team_count
    ];
    for (index, player) in selected.into_iter().enumerate() {
        let player = effective_player(player, median_score);
        let team = &mut teams[snake_draft_group_index(index, team_count)];
        team.score_sum += player.score;
        team.players.push(player);
    }
    improve_teams(&mut teams);
    Ok(BalanceResult {
        gap: score_gap(teams.iter().map(|team| team.score_sum)),
        teams,
        waitlist: waitlist
            .into_iter()
            .map(|player| effective_player(player, median_score))
            .collect(),
        median_score,
    })
}

fn median_rank(players: &[BalancePlayer]) -> f64 {
    let mut scores: Vec<_> = players
        .iter()
        .filter_map(|player| player.score.filter(|score| *score > 3))
        .collect();
    scores.sort_unstable();
    if scores.is_empty() {
        return 0.0;
    }
    let middle = scores.len() / 2;
    if scores.len() % 2 == 0 {
        (scores[middle - 1] as f64 + scores[middle] as f64) / 2.0
    } else {
        scores[middle] as f64
    }
}

fn effective_score(player: &BalancePlayer, median: f64) -> f64 {
    player
        .score
        .filter(|score| *score > 3)
        .map(|score| score as f64)
        .unwrap_or(median)
}

fn effective_player(player: BalancePlayer, median: f64) -> BalancedPlayer {
    let score = player.score.filter(|score| *score > 3);
    BalancedPlayer {
        id: player.id,
        score: score.map(|score| score as f64).unwrap_or(median),
        priority: player.priority,
        used_median: score.is_none(),
    }
}

fn score_gap(scores: impl Iterator<Item = f64>) -> f64 {
    let (min, max) = scores.fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), score| {
        (min.min(score), max.max(score))
    });
    if min.is_finite() {
        max - min
    } else {
        0.0
    }
}

fn improve_teams(teams: &mut [BalancedTeam]) {
    loop {
        let mut best_gap = score_gap(teams.iter().map(|team| team.score_sum));
        let mut best_swap = None;
        for left in 0..teams.len() {
            for right in left + 1..teams.len() {
                for a in 0..teams[left].players.len() {
                    for b in 0..teams[right].players.len() {
                        let delta = teams[right].players[b].score - teams[left].players[a].score;
                        let gap = score_gap(teams.iter().enumerate().map(|(index, team)| {
                            team.score_sum
                                + if index == left {
                                    delta
                                } else if index == right {
                                    -delta
                                } else {
                                    0.0
                                }
                        }));
                        if gap < best_gap {
                            best_gap = gap;
                            best_swap = Some((left, right, a, b, delta));
                        }
                    }
                }
            }
        }
        let Some((left, right, a, b, delta)) = best_swap else {
            break;
        };
        let (left_teams, right_teams) = teams.split_at_mut(right);
        std::mem::swap(
            &mut left_teams[left].players[a],
            &mut right_teams[0].players[b],
        );
        left_teams[left].score_sum += delta;
        right_teams[0].score_sum -= delta;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn players(scores: &[Option<i64>]) -> Vec<BalancePlayer> {
        scores
            .iter()
            .enumerate()
            .map(|(index, score)| BalancePlayer {
                id: index as i64,
                score: *score,
                priority: false,
                signup_order: index,
            })
            .collect()
    }

    #[test]
    fn thirteen_signups_make_two_full_teams_and_one_waitlisted() {
        let input = players(&(7..20).map(Some).collect::<Vec<_>>());
        let result = balance_players(input.clone(), 2, 6).expect("balance");
        assert_eq!(result.teams.len(), 2);
        assert!(result.teams.iter().all(|team| team.players.len() == 6));
        assert_eq!(result.waitlist[0].id, 12);
        assert_eq!(result.gap, 0.0);
        assert_eq!(balance_players(input, 2, 6).expect("repeat"), result);
    }

    #[test]
    fn capacity_and_priority_preserve_signup_order() {
        let mut input = players(&[Some(7); 13]);
        input[12].priority = true;
        let result = balance_players(input, 1, 6).expect("balance");
        assert!(result.teams[0].players.iter().any(|player| player.id == 12));
        assert_eq!(
            result
                .waitlist
                .iter()
                .map(|player| player.id)
                .collect::<Vec<_>>(),
            (5..12).collect::<Vec<_>>()
        );
    }

    #[test]
    fn median_includes_ranked_waitlist_and_replaces_zero_and_tier_zero() {
        let result =
            balance_players(players(&[Some(0), Some(3), Some(7), Some(8)]), 1, 2).expect("balance");
        assert_eq!(result.median_score, 7.5);
        assert!(result.teams[0]
            .players
            .iter()
            .all(|player| player.used_median && player.score == 7.5));
        let unknown = balance_players(players(&[None, Some(0)]), 1, 2).expect("unranked");
        assert_eq!(unknown.median_score, 0.0);
        assert_eq!(unknown.gap, 0.0);
    }

    #[test]
    fn swaps_reach_a_strict_pairwise_local_minimum() {
        let result = balance_players(
            players(&[Some(7), Some(8), Some(9), Some(10), Some(11), Some(13)]),
            2,
            3,
        )
        .expect("balance");
        assert_eq!(result.gap, 0.0);
        for a in &result.teams[0].players {
            for b in &result.teams[1].players {
                let delta = b.score - a.score;
                assert!(
                    score_gap(
                        [
                            result.teams[0].score_sum + delta,
                            result.teams[1].score_sum - delta
                        ]
                        .into_iter()
                    ) >= result.gap
                );
            }
        }
    }

    #[test]
    fn not_enough_players_and_invalid_inputs_are_safe() {
        let result = balance_players(players(&[Some(7)]), 2, 6).expect("waitlist");
        assert!(result.teams.is_empty());
        assert_eq!(result.waitlist.len(), 1);
        assert!(balance_players(Vec::new(), 2, 0).is_err());
        let mut input = players(&[Some(7), Some(8)]);
        input[1].id = input[0].id;
        assert!(balance_players(input, 1, 2).is_err());
    }
}
