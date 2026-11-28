use std::collections::HashMap;

use turnier_core::{BracketFormat, BracketType};

pub const PLACEMENT_POINTS: [(i64, i64); 4] = [(1, 10), (2, 6), (3, 3), (4, 3)];
pub const PARTICIPATION_POINTS: i64 = 1;

#[derive(Debug, Clone, Copy)]
pub struct CompletedMatch {
    pub round: i64,
    pub bracket_type: BracketType,
    pub team1_id: Option<i64>,
    pub team2_id: Option<i64>,
    pub winner_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointsContribution {
    pub points: i64,
    pub matches_played: i64,
    pub matches_won: i64,
    pub placement: Option<i64>,
    pub placement_shared: bool,
}

fn placement_points(placement: i64) -> i64 {
    PLACEMENT_POINTS
        .iter()
        .find(|(p, _)| *p == placement)
        .map(|(_, v)| *v)
        .unwrap_or(0)
}

fn loser_of(m: &CompletedMatch) -> Option<i64> {
    m.winner_id?;
    if m.winner_id == m.team2_id {
        m.team1_id
    } else {
        m.team2_id
    }
}

pub fn team_placements(matches: &[CompletedMatch], format: BracketFormat) -> HashMap<i64, i64> {
    match format {
        BracketFormat::SingleElimination => single_elimination_placements(matches),
        BracketFormat::DoubleElimination => double_elimination_placements(matches),
    }
}

fn single_elimination_placements(matches: &[CompletedMatch]) -> HashMap<i64, i64> {
    let mut placements: HashMap<i64, i64> = HashMap::new();
    let Some(max_round) = matches.iter().map(|m| m.round).max() else {
        return placements;
    };
    let Some(final_match) = matches.iter().find(|m| m.round == max_round) else {
        return placements;
    };
    let Some(winner_id) = final_match.winner_id else {
        return placements;
    };
    placements.insert(winner_id, 1);
    if let Some(loser_id) = loser_of(final_match) {
        placements.insert(loser_id, 2);
    }
    for semifinal in matches.iter().filter(|m| m.round == max_round - 1) {
        if let Some(sf_loser) = loser_of(semifinal) {
            placements.entry(sf_loser).or_insert(3);
        }
    }
    placements
}

fn double_elimination_placements(matches: &[CompletedMatch]) -> HashMap<i64, i64> {
    let mut placements: HashMap<i64, i64> = HashMap::new();
    let decided = |kind: BracketType| {
        matches
            .iter()
            .filter(move |m| m.bracket_type == kind && m.winner_id.is_some())
    };
    if let Some(final_match) = decided(BracketType::GrandFinal).max_by_key(|m| m.round) {
        if let Some(winner_id) = final_match.winner_id {
            placements.insert(winner_id, 1);
        }
        if let Some(loser_id) = loser_of(final_match) {
            placements.insert(loser_id, 2);
        }
    }
    let Some(losers_final_round) = decided(BracketType::Losers).map(|m| m.round).max() else {
        return placements;
    };
    for (round, place) in [(losers_final_round, 3), (losers_final_round - 1, 4)] {
        for m in decided(BracketType::Losers).filter(|m| m.round == round) {
            if let Some(loser_id) = loser_of(m) {
                placements.entry(loser_id).or_insert(place);
            }
        }
    }
    placements
}

pub fn placement_is_shared(format: BracketFormat, placement: Option<i64>) -> bool {
    format == BracketFormat::SingleElimination && placement == Some(3)
}

pub fn team_wins(matches: &[CompletedMatch]) -> HashMap<i64, i64> {
    let mut wins: HashMap<i64, i64> = HashMap::new();
    for m in matches {
        if let Some(winner_id) = m.winner_id {
            *wins.entry(winner_id).or_insert(0) += 1;
        }
    }
    wins
}

pub fn contribution_for_team(
    team_id: i64,
    placements: &HashMap<i64, i64>,
    bracket_format: Option<BracketFormat>,
    wins: &HashMap<i64, i64>,
    matches_played: i64,
) -> PointsContribution {
    let placement = placements.get(&team_id).copied();
    let team_win_count = wins.get(&team_id).copied().unwrap_or(0);

    let mut points = PARTICIPATION_POINTS;
    if let Some(p) = placement {
        points += placement_points(p);
    }
    points += team_win_count / 2;

    PointsContribution {
        points,
        matches_played,
        matches_won: team_win_count,
        placement,
        placement_shared: bracket_format.is_some_and(|f| placement_is_shared(f, placement)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(round: i64, t1: i64, t2: i64, winner: i64) -> CompletedMatch {
        CompletedMatch {
            round,
            bracket_type: BracketType::Winners,
            team1_id: Some(t1),
            team2_id: Some(t2),
            winner_id: Some(winner),
        }
    }

    #[test]
    fn placements_single_elim_4_teams() {
        let matches = vec![m(2, 1, 2, 1), m(1, 1, 3, 1), m(1, 2, 4, 2)];
        let pl = team_placements(&matches, BracketFormat::SingleElimination);
        assert_eq!(pl.get(&1), Some(&1));
        assert_eq!(pl.get(&2), Some(&2));
        assert_eq!(pl.get(&3), Some(&3));
        assert_eq!(pl.get(&4), Some(&3));
    }

    #[test]
    fn contribution_points_match_formula() {
        let matches = vec![m(2, 1, 2, 1), m(1, 1, 3, 1), m(1, 2, 4, 2)];
        let pl = team_placements(&matches, BracketFormat::SingleElimination);
        let w = team_wins(&matches);
        let c1 = contribution_for_team(
            1,
            &pl,
            Some(BracketFormat::SingleElimination),
            &w,
            matches.len() as i64,
        );
        assert_eq!(c1.points, 12);
        assert_eq!(c1.matches_won, 2);
        assert_eq!(c1.placement, Some(1));
        let c4 = contribution_for_team(
            4,
            &pl,
            Some(BracketFormat::SingleElimination),
            &w,
            matches.len() as i64,
        );
        assert_eq!(c4.points, 4);
        assert_eq!(c4.matches_played, 3);
        assert_eq!(c4.placement, Some(3));
        assert!(c4.placement_shared);
        assert!(!c1.placement_shared);
        let c_none = contribution_for_team(
            999,
            &pl,
            Some(BracketFormat::SingleElimination),
            &w,
            matches.len() as i64,
        );
        assert_eq!(c_none.points, 1);
        assert_eq!(c_none.placement, None);
        assert!(!c_none.placement_shared);
    }

    #[test]
    fn double_elimination_third_place_is_exact() {
        assert!(!placement_is_shared(
            BracketFormat::DoubleElimination,
            Some(3)
        ));
        assert!(!placement_is_shared(
            BracketFormat::DoubleElimination,
            Some(4)
        ));
        assert!(!placement_is_shared(
            BracketFormat::SingleElimination,
            Some(2)
        ));
        assert!(!placement_is_shared(BracketFormat::SingleElimination, None));
    }

    #[test]
    fn win_points_floor_for_odd_wins() {
        let mut wins = HashMap::new();
        wins.insert(9i64, 3i64);
        let pl = HashMap::new();
        let c = contribution_for_team(9, &pl, None, &wins, 5);
        assert_eq!(c.points, 2);
    }
}
