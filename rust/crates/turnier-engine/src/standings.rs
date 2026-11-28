use std::cmp::{Ordering, Reverse};

use crate::league::LeagueGame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub team_id: i64,
    pub wins: i64,
    pub points: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankedStanding {
    pub team_id: i64,
    pub seed_tiebreak: bool,
}

pub fn rank_standings(seeded: &[Standing], games: &[LeagueGame]) -> Vec<RankedStanding> {
    let mut rows = seeded.to_vec();
    rows.sort_by_key(|row| (Reverse(row.points), Reverse(row.wins)));
    let any_result = games.iter().any(|game| game.winner_id.is_some());
    let mut ranked = Vec::with_capacity(rows.len());
    let mut start = 0;
    while start < rows.len() {
        let end = tied_run_end(&rows, start);
        ranked.extend(rank_tied_run(&rows[start..end], games, any_result));
        start = end;
    }
    ranked
}

fn tied_run_end(rows: &[Standing], start: usize) -> usize {
    let head = rows[start];
    start
        + rows[start..]
            .iter()
            .take_while(|row| row.points == head.points && row.wins == head.wins)
            .count()
}

fn rank_tied_run(run: &[Standing], games: &[LeagueGame], any_result: bool) -> Vec<RankedStanding> {
    if let [first, second] = run {
        match head_to_head(first.team_id, second.team_id, games) {
            Ordering::Less => return decided(&[*second, *first]),
            Ordering::Greater => return decided(&[*first, *second]),
            Ordering::Equal => {}
        }
    }
    let seed_tiebreak = run.len() > 1 && any_result;
    run.iter()
        .map(|row| RankedStanding {
            team_id: row.team_id,
            seed_tiebreak,
        })
        .collect()
}

fn decided(rows: &[Standing]) -> Vec<RankedStanding> {
    rows.iter()
        .map(|row| RankedStanding {
            team_id: row.team_id,
            seed_tiebreak: false,
        })
        .collect()
}

fn head_to_head(first: i64, second: i64, games: &[LeagueGame]) -> Ordering {
    let wins_of = |team_id: i64| {
        games
            .iter()
            .filter(|game| {
                let pair = (game.team1_id, game.team2_id);
                pair == (first, second) || pair == (second, first)
            })
            .filter(|game| game.winner_id == Some(team_id))
            .count()
    };
    wins_of(first).cmp(&wins_of(second))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing(team_id: i64, wins: i64, points: i64) -> Standing {
        Standing {
            team_id,
            wins,
            points,
        }
    }

    fn game(team1_id: i64, team2_id: i64, winner_id: i64) -> LeagueGame {
        LeagueGame {
            team1_id,
            team2_id,
            winner_id: Some(winner_id),
        }
    }

    fn order(ranked: &[RankedStanding]) -> Vec<(i64, bool)> {
        ranked
            .iter()
            .map(|r| (r.team_id, r.seed_tiebreak))
            .collect()
    }

    #[test]
    fn points_then_wins_decide_before_any_tiebreak() {
        let rows = [standing(1, 1, 3), standing(2, 2, 6), standing(3, 2, 3)];
        let ranked = rank_standings(&rows, &[game(2, 1, 2)]);
        assert_eq!(order(&ranked), vec![(2, false), (3, false), (1, false)]);
    }

    #[test]
    fn two_way_tie_is_decided_by_head_to_head() {
        let rows = [standing(1, 1, 3), standing(2, 1, 3), standing(3, 0, 0)];
        let ranked = rank_standings(&rows, &[game(1, 2, 2), game(1, 3, 1), game(2, 3, 3)]);
        assert_eq!(order(&ranked), vec![(2, false), (1, false), (3, false)]);
    }

    #[test]
    fn two_way_tie_without_direct_result_falls_back_to_seed() {
        let rows = [standing(1, 1, 3), standing(2, 1, 3)];
        let ranked = rank_standings(&rows, &[game(1, 3, 1), game(2, 4, 2)]);
        assert_eq!(order(&ranked), vec![(1, true), (2, true)]);
    }

    #[test]
    fn split_direct_results_fall_back_to_seed() {
        let rows = [standing(1, 1, 3), standing(2, 1, 3)];
        let ranked = rank_standings(&rows, &[game(1, 2, 2), game(2, 1, 1)]);
        assert_eq!(order(&ranked), vec![(1, true), (2, true)]);
    }

    #[test]
    fn three_way_tie_keeps_seed_order() {
        let rows = [standing(1, 1, 3), standing(2, 1, 3), standing(3, 1, 3)];
        let ranked = rank_standings(&rows, &[game(1, 2, 1), game(2, 3, 2), game(3, 1, 3)]);
        assert_eq!(order(&ranked), vec![(1, true), (2, true), (3, true)]);
    }

    #[test]
    fn untouched_group_is_not_flagged() {
        let rows = [standing(1, 0, 0), standing(2, 0, 0)];
        let ranked = rank_standings(&rows, &[]);
        assert_eq!(order(&ranked), vec![(1, false), (2, false)]);
    }
}
