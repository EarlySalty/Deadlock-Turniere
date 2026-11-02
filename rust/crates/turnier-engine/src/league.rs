pub const LEAGUE_WIN_POINTS: i64 = 2;
pub const LEAGUE_LOSS_POINTS: i64 = -1;
pub const DEFAULT_SLOT_MINUTES: i64 = 50;
pub const MIN_SLOT_MINUTES: i64 = 1;
pub const MAX_SLOT_MINUTES: i64 = 1440;
pub const LEAGUE_GROUP_NAME: &str = "Tabelle";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaguePairing {
    pub round: i64,
    pub team1_id: i64,
    pub team2_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeagueGame {
    pub team1_id: i64,
    pub team2_id: i64,
    pub winner_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeagueRow {
    pub team_id: i64,
    pub wins: i64,
    pub losses: i64,
    pub points: i64,
}

pub fn league_rounds(team_ids: &[i64]) -> Vec<LeaguePairing> {
    if team_ids.len() < 2 {
        return Vec::new();
    }
    let mut slots: Vec<Option<i64>> = team_ids.iter().copied().map(Some).collect();
    if slots.len() % 2 == 1 {
        slots.push(None);
    }
    let round_count = slots.len() - 1;
    let mut pairings = Vec::new();
    for round_index in 0..round_count {
        pairings.extend(round_pairings(&slots, round_index));
        slots[1..].rotate_right(1);
    }
    pairings
}

fn round_pairings(slots: &[Option<i64>], round_index: usize) -> Vec<LeaguePairing> {
    let last = slots.len() - 1;
    (0..slots.len() / 2)
        .filter_map(|position| {
            let first = slots[position]?;
            let second = slots[last - position]?;
            let (team1_id, team2_id) = if position == 0 && round_index % 2 == 1 {
                (second, first)
            } else {
                (first, second)
            };
            Some(LeaguePairing {
                round: round_index as i64 + 1,
                team1_id,
                team2_id,
            })
        })
        .collect()
}

pub fn round_offset_minutes(round: i64, slot_minutes: i64) -> i64 {
    (round - 1) * slot_minutes
}

pub fn is_valid_slot_minutes(slot_minutes: i64) -> bool {
    (MIN_SLOT_MINUTES..=MAX_SLOT_MINUTES).contains(&slot_minutes)
}

pub fn league_table(team_ids: &[i64], games: &[LeagueGame]) -> Vec<LeagueRow> {
    let mut rows: Vec<LeagueRow> = team_ids
        .iter()
        .map(|team_id| team_row(*team_id, games))
        .collect();
    rows.sort_by_key(|row| (std::cmp::Reverse(row.points), std::cmp::Reverse(row.wins)));
    apply_direct_comparison(&mut rows, games);
    rows
}

fn team_row(team_id: i64, games: &[LeagueGame]) -> LeagueRow {
    let mut wins = 0;
    let mut losses = 0;
    for game in games {
        let Some(winner_id) = game.winner_id else {
            continue;
        };
        if team_id != game.team1_id && team_id != game.team2_id {
            continue;
        }
        if winner_id == team_id {
            wins += 1;
        } else if winner_id == game.team1_id || winner_id == game.team2_id {
            losses += 1;
        }
    }
    LeagueRow {
        team_id,
        wins,
        losses,
        points: wins * LEAGUE_WIN_POINTS + losses * LEAGUE_LOSS_POINTS,
    }
}

fn apply_direct_comparison(rows: &mut [LeagueRow], games: &[LeagueGame]) {
    let mut start = 0;
    while start < rows.len() {
        let end = tied_run_end(rows, start);
        if end - start == 2 && second_beat_first(&rows[start], &rows[start + 1], games) {
            rows.swap(start, start + 1);
        }
        start = end;
    }
}

fn tied_run_end(rows: &[LeagueRow], start: usize) -> usize {
    let same_rank =
        |row: &LeagueRow| row.points == rows[start].points && row.wins == rows[start].wins;
    start
        + rows[start..]
            .iter()
            .take_while(|row| same_rank(row))
            .count()
}

fn second_beat_first(first: &LeagueRow, second: &LeagueRow, games: &[LeagueGame]) -> bool {
    games.iter().any(|game| {
        let same_pair = (game.team1_id == first.team_id && game.team2_id == second.team_id)
            || (game.team1_id == second.team_id && game.team2_id == first.team_id);
        same_pair && game.winner_id == Some(second.team_id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn ids(count: i64) -> Vec<i64> {
        (1..=count).collect()
    }

    fn game(team1_id: i64, team2_id: i64, winner_id: i64) -> LeagueGame {
        LeagueGame {
            team1_id,
            team2_id,
            winner_id: Some(winner_id),
        }
    }

    fn order(rows: &[LeagueRow]) -> Vec<i64> {
        rows.iter().map(|row| row.team_id).collect()
    }

    fn rounds_of(pairings: &[LeaguePairing]) -> HashMap<i64, Vec<LeaguePairing>> {
        let mut rounds: HashMap<i64, Vec<LeaguePairing>> = HashMap::new();
        for pairing in pairings {
            rounds.entry(pairing.round).or_default().push(*pairing);
        }
        rounds
    }

    fn assert_each_pair_once(pairings: &[LeaguePairing], team_count: usize) {
        let pairs: HashSet<(i64, i64)> = pairings
            .iter()
            .map(|p| (p.team1_id.min(p.team2_id), p.team1_id.max(p.team2_id)))
            .collect();
        assert_eq!(pairs.len(), pairings.len());
        assert_eq!(pairs.len(), team_count * (team_count - 1) / 2);
    }

    fn assert_no_team_twice_per_round(pairings: &[LeaguePairing]) {
        for (round, games) in rounds_of(pairings) {
            let mut seen = HashSet::new();
            for pairing in games {
                assert!(seen.insert(pairing.team1_id), "Runde {round}");
                assert!(seen.insert(pairing.team2_id), "Runde {round}");
            }
        }
    }

    #[test]
    fn ten_teams_give_45_matches_in_9_rounds_of_5() {
        let pairings = league_rounds(&ids(10));
        assert_eq!(pairings.len(), 45);
        let rounds = rounds_of(&pairings);
        assert_eq!(rounds.len(), 9);
        assert!(rounds.values().all(|games| games.len() == 5));
        assert_each_pair_once(&pairings, 10);
        assert_no_team_twice_per_round(&pairings);
    }

    #[test]
    fn odd_team_count_gives_one_bye_per_round() {
        let pairings = league_rounds(&ids(7));
        assert_eq!(pairings.len(), 21);
        let rounds = rounds_of(&pairings);
        assert_eq!(rounds.len(), 7);
        assert!(rounds.values().all(|games| games.len() == 3));
        assert_each_pair_once(&pairings, 7);
        assert_no_team_twice_per_round(&pairings);
        let mut byes: HashMap<i64, i64> = HashMap::new();
        for games in rounds.values() {
            let playing: HashSet<i64> = games
                .iter()
                .flat_map(|p| [p.team1_id, p.team2_id])
                .collect();
            for team_id in ids(7) {
                if !playing.contains(&team_id) {
                    *byes.entry(team_id).or_default() += 1;
                }
            }
        }
        assert_eq!(byes.len(), 7);
        assert!(byes.values().all(|count| *count == 1));
    }

    #[test]
    fn two_and_three_teams_are_supported_and_one_is_empty() {
        assert_eq!(league_rounds(&ids(2)).len(), 1);
        let three = league_rounds(&ids(3));
        assert_eq!(three.len(), 3);
        assert_eq!(rounds_of(&three).len(), 3);
        assert!(league_rounds(&ids(1)).is_empty());
    }

    #[test]
    fn points_are_plus_two_and_minus_one() {
        let rows = league_table(&ids(3), &[game(1, 2, 1), game(1, 3, 3), game(2, 3, 3)]);
        let by_id: HashMap<i64, LeagueRow> = rows.iter().map(|row| (row.team_id, *row)).collect();
        assert_eq!(by_id[&3].points, 4);
        assert_eq!(by_id[&1].points, 1);
        assert_eq!(by_id[&2].points, -2);
        assert_eq!(order(&rows), vec![3, 1, 2]);
    }

    #[test]
    fn two_way_tie_uses_direct_comparison() {
        let rows = league_table(
            &ids(4),
            &[
                game(2, 1, 2),
                game(1, 3, 1),
                game(2, 3, 3),
                game(1, 4, 1),
                game(2, 4, 2),
                game(3, 4, 4),
            ],
        );
        assert_eq!(rows[0].wins, 2);
        assert_eq!(rows[1].wins, 2);
        assert_eq!(order(&rows)[..2], [2, 1]);
    }

    #[test]
    fn three_way_tie_keeps_stable_order() {
        let rows = league_table(&[7, 5, 9], &[game(7, 5, 7), game(5, 9, 5), game(9, 7, 9)]);
        assert_eq!(order(&rows), vec![7, 5, 9]);
    }

    #[test]
    fn open_games_do_not_count_and_all_zero_keeps_input_order() {
        let open = LeagueGame {
            team1_id: 2,
            team2_id: 1,
            winner_id: None,
        };
        let rows = league_table(&[2, 1], &[open]);
        assert_eq!(order(&rows), vec![2, 1]);
        assert!(rows.iter().all(|row| row.points == 0 && row.wins == 0));
    }
}
