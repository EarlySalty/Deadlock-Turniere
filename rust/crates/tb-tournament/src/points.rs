//! Punkte-Berechnung für die globale Rangliste (reine Logik).
//!
//! Portiert `recalculate_player_points` aus `points.py` — aber mit EINER
//! bewussten strukturellen Änderung gegenüber dem Original: idempotenter
//! Voll-Recompute statt additiver Akkumulation (siehe Modul- und
//! Persistenz-Doku). Die ERGEBNISWERTE pro Turnier (Platzierungs-, Teilnahme-
//! und Win-Punkte, `best_placement`, `matches_played`) sind 1:1 wie im Original;
//! lediglich die Doppelzähl-Anfälligkeit bei Mehrfach-`completed` entfällt.
//!
//! Die Platzierungs-Heuristik (max(round) = Finale, round-1 = Halbfinals) ist
//! eine Single-Elimination-Annahme des Originals und wird hier UNVERÄNDERT
//! übernommen (bug-preserved): bei Double-Elimination liefert sie dieselben
//! (teils unscharfen) Werte wie das Python-Original.

use std::collections::HashMap;

/// Punkte je Platzierung (1./2./3.). `4` ist im Original ein toter Schlüssel
/// (Halbfinal-Verlierer bekommen alle Platz 3); wir behalten ihn 1:1.
pub const PLACEMENT_POINTS: [(i64, i64); 4] = [(1, 10), (2, 6), (3, 3), (4, 3)];
/// Teilnahme-Grundpunkt.
pub const PARTICIPATION_POINTS: i64 = 1;

/// Ein abgeschlossenes Bracket-Match, reduziert auf die für die Punkte nötigen
/// Felder. Reihenfolge der Eingabe MUSS `ORDER BY round DESC` sein (wie die
/// Original-Query), damit `final` deterministisch das erste max-round-Match ist.
#[derive(Debug, Clone, Copy)]
pub struct CompletedMatch {
    pub round: i64,
    pub team1_id: Option<i64>,
    pub team2_id: Option<i64>,
    pub winner_id: Option<i64>,
}

/// Aggregierter Punkte-Beitrag EINES Turniers für EINEN Spieler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointsContribution {
    pub points: i64,
    pub matches_played: i64,
    pub matches_won: i64,
    pub placement: Option<i64>,
}

fn placement_points(placement: i64) -> i64 {
    PLACEMENT_POINTS
        .iter()
        .find(|(p, _)| *p == placement)
        .map(|(_, v)| *v)
        .unwrap_or(0)
}

/// Bestimmt die Team-Platzierungen aus den abgeschlossenen Bracket-Matches.
///
/// Portiert die Platzierungs-Heuristik 1:1: höchste Runde = Finale (Sieger=1,
/// Verlierer=2), `max-1` = Halbfinals (Verlierer=3, sofern noch nicht gesetzt).
/// `matches` MUSS nach `round DESC` sortiert sein.
pub fn team_placements(matches: &[CompletedMatch]) -> HashMap<i64, i64> {
    let mut placements: HashMap<i64, i64> = HashMap::new();
    if matches.is_empty() {
        return placements;
    }
    let max_round = matches.iter().map(|m| m.round).max().unwrap();
    // `next(... round == max_round)` über die round-DESC-Liste -> erstes Element
    // mit max_round.
    let final_match = matches.iter().find(|m| m.round == max_round);
    if let Some(final_match) = final_match {
        if let Some(winner_id) = final_match.winner_id {
            let loser_id = if final_match.winner_id == final_match.team2_id {
                final_match.team1_id
            } else {
                final_match.team2_id
            };
            placements.insert(winner_id, 1);
            if let Some(loser_id) = loser_id {
                placements.insert(loser_id, 2);
            }
            for semifinal in matches.iter().filter(|m| m.round == max_round - 1) {
                if semifinal.winner_id.is_some() {
                    let sf_loser = if semifinal.winner_id == semifinal.team2_id {
                        semifinal.team1_id
                    } else {
                        semifinal.team2_id
                    };
                    if let Some(sf_loser) = sf_loser {
                        placements.entry(sf_loser).or_insert(3);
                    }
                }
            }
        }
    }
    placements
}

/// Zählt Team-Wins über alle abgeschlossenen Bracket-Matches.
pub fn team_wins(matches: &[CompletedMatch]) -> HashMap<i64, i64> {
    let mut wins: HashMap<i64, i64> = HashMap::new();
    for m in matches {
        if let Some(winner_id) = m.winner_id {
            *wins.entry(winner_id).or_insert(0) += 1;
        }
    }
    wins
}

/// Berechnet den Punkte-Beitrag EINES Teilnehmers (über sein `team_id`) in EINEM
/// Turnier.
///
/// `total_completed_matches` ist die Gesamtzahl abgeschlossener Bracket-Matches
/// des Turniers — das Original zählt diese GLOBAL als `matches_played` pro
/// Spieler (bug-preserved: nicht team-spezifisch).
///
/// Punkte = Teilnahme (1) + Platzierungspunkte + `floor(wins * 0.5)` (bewusste
/// Integer-Trunkierung wie im Original via `int(wins * 0.5)`).
pub fn contribution_for_team(
    team_id: i64,
    placements: &HashMap<i64, i64>,
    wins: &HashMap<i64, i64>,
    total_completed_matches: i64,
) -> PointsContribution {
    let placement = placements.get(&team_id).copied();
    let team_win_count = wins.get(&team_id).copied().unwrap_or(0);

    let mut points = PARTICIPATION_POINTS;
    if let Some(p) = placement {
        points += placement_points(p);
    }
    // int(wins * 0.5) -> floor für nicht-negative Werte.
    points += team_win_count / 2;

    PointsContribution {
        points,
        matches_played: total_completed_matches,
        matches_won: team_win_count,
        placement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(round: i64, t1: i64, t2: i64, winner: i64) -> CompletedMatch {
        CompletedMatch {
            round,
            team1_id: Some(t1),
            team2_id: Some(t2),
            winner_id: Some(winner),
        }
    }

    #[test]
    fn placements_single_elim_4_teams() {
        // round DESC: Finale (round 2) zuerst, dann zwei Halbfinals (round 1).
        let matches = vec![
            m(2, 1, 2, 1), // Finale: 1 schlägt 2
            m(1, 1, 3, 1), // HF: 1 schlägt 3
            m(1, 2, 4, 2), // HF: 2 schlägt 4
        ];
        let pl = team_placements(&matches);
        assert_eq!(pl.get(&1), Some(&1));
        assert_eq!(pl.get(&2), Some(&2));
        assert_eq!(pl.get(&3), Some(&3));
        assert_eq!(pl.get(&4), Some(&3));
    }

    #[test]
    fn contribution_points_match_formula() {
        let matches = vec![m(2, 1, 2, 1), m(1, 1, 3, 1), m(1, 2, 4, 2)];
        let pl = team_placements(&matches);
        let w = team_wins(&matches);
        // Team 1: Platz 1 (10) + Teilnahme (1) + 2 Wins -> floor(2*0.5)=1 -> 12.
        let c1 = contribution_for_team(1, &pl, &w, matches.len() as i64);
        assert_eq!(c1.points, 12);
        assert_eq!(c1.matches_won, 2);
        assert_eq!(c1.placement, Some(1));
        // Team 4: Halbfinal-Verlierer -> Platz 3 (3) + Teilnahme (1), kein Win = 4;
        // matches_played zählt GLOBAL alle 3 completed Matches (bug-preserved).
        let c4 = contribution_for_team(4, &pl, &w, matches.len() as i64);
        assert_eq!(c4.points, 4);
        assert_eq!(c4.matches_played, 3);
        assert_eq!(c4.placement, Some(3));
        // Ein Team ganz ohne Platz/Win (nicht im Bracket) -> nur Teilnahme 1.
        let c_none = contribution_for_team(999, &pl, &w, matches.len() as i64);
        assert_eq!(c_none.points, 1);
        assert_eq!(c_none.placement, None);
    }

    #[test]
    fn win_points_floor_for_odd_wins() {
        let mut wins = HashMap::new();
        wins.insert(9i64, 3i64);
        let pl = HashMap::new();
        // 3 Wins -> floor(3*0.5)=1, + Teilnahme 1 = 2.
        let c = contribution_for_team(9, &pl, &wins, 5);
        assert_eq!(c.points, 2);
    }
}
