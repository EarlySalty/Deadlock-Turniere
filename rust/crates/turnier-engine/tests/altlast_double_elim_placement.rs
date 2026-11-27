#![cfg(feature = "testing")]

mod common;

use common::{insert_team, insert_team_member, insert_tournament, temp_pool};
use turnier_db::{Pool, TestDb};
use turnier_engine::{advance_bracket_winner, generate_bracket, recalculate_player_points};

const DOUBLE: &str = "double_elimination";
const SINGLE: &str = "single_elimination";

#[derive(Clone, Copy, PartialEq)]
enum GrandFinal {
    WinnersSideWins,
    LosersSideForcesReset,
    LosersSideWinsResetLeftOpen,
}

struct Playable {
    id: i64,
    team1: i64,
    team2: i64,
    bracket_type: String,
    round: i64,
}

struct Outcome {
    champion: i64,
    runner_up: i64,
    losers_final_loser: Option<i64>,
    losers_semifinal_loser: Option<i64>,
    semifinal_losers: Vec<i64>,
}

async fn seed(pool: &Pool, team_count: i64, format: &str) -> (i64, Vec<(i64, i64)>) {
    let tournament_id =
        insert_tournament(pool, "Altlast DE", "bracket", 1, format, "bracket_only").await;
    let mut teams = Vec::new();
    for index in 0..team_count {
        let player = 8800 + index;
        let team_id = insert_team(pool, tournament_id, &format!("DE {index}"), player).await;
        insert_team_member(pool, team_id, player, &format!("P{index}"), "captain", 0).await;
        teams.extend([(team_id, player)]);
    }
    (tournament_id, teams)
}

async fn next_playable(pool: &Pool, tournament_id: i64) -> Option<Playable> {
    let row: Option<(i64, i64, i64, String, i64)> = sqlx::query_as(
        "SELECT id, team1_id, team2_id, bracket_type, round FROM turnier.bracket_matches \
         WHERE tournament_id = $1 AND status NOT IN ('completed', 'cancelled') \
           AND team1_id IS NOT NULL AND team2_id IS NOT NULL AND winner_id IS NULL \
         ORDER BY round, id LIMIT 1",
    )
    .bind(tournament_id)
    .fetch_optional(pool)
    .await
    .expect("next playable");
    row.map(|(id, team1, team2, bracket_type, round)| Playable {
        id,
        team1,
        team2,
        bracket_type,
        round,
    })
}

async fn losers_max_round(pool: &Pool, tournament_id: i64) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT MAX(round) FROM turnier.bracket_matches \
         WHERE tournament_id = $1 AND bracket_type = 'losers'",
    )
    .bind(tournament_id)
    .fetch_one(pool)
    .await
    .expect("losers max round")
}

async fn winners_max_round(pool: &Pool, tournament_id: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT MAX(round) FROM turnier.bracket_matches \
         WHERE tournament_id = $1 AND bracket_type = 'winners'",
    )
    .bind(tournament_id)
    .fetch_one(pool)
    .await
    .expect("winners max round")
}

async fn play_out(pool: &Pool, tournament_id: i64, team_count: i64, gf: GrandFinal) -> Outcome {
    let losers_final_round = losers_max_round(pool, tournament_id).await;
    let winners_final_round = winners_max_round(pool, tournament_id).await;
    let mut outcome = Outcome {
        champion: 0,
        runner_up: 0,
        losers_final_loser: None,
        losers_semifinal_loser: None,
        semifinal_losers: Vec::new(),
    };
    let mut grand_finals_played = 0;
    let mut played = 0;
    while let Some(m) = next_playable(pool, tournament_id).await {
        played += 1;
        assert!(played <= 2 * team_count, "match {} offered again", m.id);
        let is_grand_final = m.bracket_type == "grand_final";
        if is_grand_final {
            grand_finals_played += 1;
        }
        let team2_wins =
            is_grand_final && grand_finals_played == 1 && gf != GrandFinal::WinnersSideWins;
        let (winner, loser) = if team2_wins {
            (m.team2, m.team1)
        } else {
            (m.team1, m.team2)
        };
        sqlx::query(
            "UPDATE turnier.bracket_matches SET winner_id = $1, status = 'completed' WHERE id = $2",
        )
        .bind(winner)
        .bind(m.id)
        .execute(pool)
        .await
        .expect("complete match");
        advance_bracket_winner(pool, tournament_id, m.id, winner)
            .await
            .expect("advance");
        outcome.champion = winner;
        outcome.runner_up = loser;
        if m.bracket_type == "losers" && Some(m.round) == losers_final_round {
            outcome.losers_final_loser = Some(loser);
        }
        if m.bracket_type == "losers" && losers_final_round.map(|r| r - 1) == Some(m.round) {
            outcome.losers_semifinal_loser = Some(loser);
        }
        if m.bracket_type == "winners" && m.round == winners_final_round - 1 {
            outcome.semifinal_losers.push(loser);
        }
        if is_grand_final && gf == GrandFinal::LosersSideWinsResetLeftOpen {
            break;
        }
    }
    let expected_grand_finals = match gf {
        GrandFinal::LosersSideForcesReset => 2,
        _ => 1,
    };
    if losers_final_round.is_some() {
        assert_eq!(grand_finals_played, expected_grand_finals);
    }
    outcome
}

async fn complete_and_recalculate(pool: &Pool, tournament_id: i64) {
    sqlx::query("UPDATE turnier.tournaments SET status = 'completed' WHERE id = $1")
        .bind(tournament_id)
        .execute(pool)
        .await
        .expect("complete");
    recalculate_player_points(pool, tournament_id)
        .await
        .expect("recalculate");
}

async fn placement(pool: &Pool, teams: &[(i64, i64)], team_id: i64) -> Option<i64> {
    let player = teams
        .iter()
        .find(|(id, _)| *id == team_id)
        .map(|(_, player)| *player)
        .expect("team player");
    sqlx::query_scalar("SELECT best_placement FROM turnier.player_points WHERE discord_id = $1")
        .bind(player)
        .fetch_one(pool)
        .await
        .expect("player points")
}

async fn run(team_count: i64, format: &str, gf: GrandFinal) -> (Vec<(i64, i64)>, Outcome, TestDb) {
    let db = temp_pool().await;
    let pool = db.pool();
    let (tournament_id, teams) = seed(pool, team_count, format).await;
    generate_bracket(pool, tournament_id)
        .await
        .expect("generate");
    let outcome = play_out(pool, tournament_id, team_count, gf).await;
    complete_and_recalculate(pool, tournament_id).await;
    (teams, outcome, db)
}

async fn assert_double_elim_places(team_count: i64, gf: GrandFinal) {
    let (teams, outcome, db) = run(team_count, DOUBLE, gf).await;
    let pool = db.pool();
    assert_eq!(placement(pool, &teams, outcome.champion).await, Some(1));
    assert_eq!(placement(pool, &teams, outcome.runner_up).await, Some(2));
    let third = outcome.losers_final_loser.expect("losers final played");
    assert_eq!(placement(pool, &teams, third).await, Some(3));
    let fourth = outcome
        .losers_semifinal_loser
        .expect("losers semifinal played");
    assert_eq!(placement(pool, &teams, fourth).await, Some(4));
    let placed_first = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM turnier.player_points WHERE best_placement = 1",
    )
    .fetch_one(pool)
    .await
    .expect("count first");
    assert_eq!(placed_first, 1);
}

async fn assert_single_elim_places(team_count: i64) {
    let (teams, outcome, db) = run(team_count, SINGLE, GrandFinal::WinnersSideWins).await;
    let pool = db.pool();
    assert_eq!(placement(pool, &teams, outcome.champion).await, Some(1));
    assert_eq!(placement(pool, &teams, outcome.runner_up).await, Some(2));
    assert_eq!(outcome.semifinal_losers.len(), 2);
    for loser in outcome.semifinal_losers {
        assert_eq!(placement(pool, &teams, loser).await, Some(3));
    }
}

#[tokio::test]
async fn double_elim_places_follow_bracket_for_4_8_16_teams() {
    for team_count in [4, 8, 16] {
        assert_double_elim_places(team_count, GrandFinal::WinnersSideWins).await;
    }
}

#[tokio::test]
async fn double_elim_reset_winner_is_placed_first() {
    for team_count in [4, 8, 16] {
        assert_double_elim_places(team_count, GrandFinal::LosersSideForcesReset).await;
    }
}

#[tokio::test]
async fn double_elim_open_reset_awards_no_title() {
    for team_count in [4, 8, 16] {
        let (teams, outcome, db) =
            run(team_count, DOUBLE, GrandFinal::LosersSideWinsResetLeftOpen).await;
        let pool = db.pool();
        assert_eq!(placement(pool, &teams, outcome.champion).await, None);
        assert_eq!(placement(pool, &teams, outcome.runner_up).await, None);
        let third = outcome.losers_final_loser.expect("losers final played");
        assert_eq!(placement(pool, &teams, third).await, Some(3));
    }
}

#[tokio::test]
async fn single_elim_places_stay_unchanged_for_4_8_16_teams() {
    for team_count in [4, 8, 16] {
        assert_single_elim_places(team_count).await;
    }
}
