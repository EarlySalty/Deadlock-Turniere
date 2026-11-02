//! Transaktionsgrenze fuer Punkte-Recompute: Scheduler-/Admin-Pfade muessen
//! DELETE+INSERT innerhalb der offenen PG-Transaktion zurueckrollen koennen.

#![cfg(feature = "testing")]

mod common;

use common::{insert_team, insert_team_member, insert_tournament, temp_pool};
use turnier_core::now_utc;
use turnier_engine::{generate_league_plan_in_tx, recalculate_player_points_in_tx};

async fn insert_completed_match(
    pool: &turnier_db::Pool,
    tournament_id: i64,
    round: i64,
    position: i64,
    team1_id: i64,
    team2_id: i64,
    winner_id: i64,
) {
    sqlx::query(
        "INSERT INTO turnier.bracket_matches \
             (tournament_id, round, position, bracket_type, team1_id, team2_id, winner_id, status, on_stream) \
         VALUES ($1, $2, $3, 'winners', $4, $5, $6, 'completed', true)",
    )
    .bind(tournament_id)
    .bind(round)
    .bind(position)
    .bind(team1_id)
    .bind(team2_id)
    .bind(winner_id)
    .execute(pool)
    .await
    .expect("insert completed match");
}

#[tokio::test]
async fn recalculate_player_points_in_tx_rolls_back_delete_insert_recompute() {
    let db = temp_pool().await;
    let pool = db.pool();
    let tournament_id = insert_tournament(
        pool,
        "Points Tx",
        "completed",
        1,
        "single_elimination",
        "bracket_only",
    )
    .await;

    let team1 = insert_team(pool, tournament_id, "Alpha", 6101).await;
    let team2 = insert_team(pool, tournament_id, "Bravo", 6102).await;
    let team3 = insert_team(pool, tournament_id, "Charlie", 6103).await;
    let team4 = insert_team(pool, tournament_id, "Delta", 6104).await;
    insert_team_member(pool, team1, 7101, "Alpha One", "captain", 0).await;
    insert_team_member(pool, team2, 7102, "Bravo One", "captain", 0).await;
    insert_team_member(pool, team3, 7103, "Charlie One", "captain", 0).await;
    insert_team_member(pool, team4, 7104, "Delta One", "captain", 0).await;

    insert_completed_match(pool, tournament_id, 1, 0, team1, team3, team1).await;
    insert_completed_match(pool, tournament_id, 1, 1, team2, team4, team2).await;
    insert_completed_match(pool, tournament_id, 2, 0, team1, team2, team1).await;

    sqlx::query(
        "INSERT INTO turnier.player_points \
             (discord_id, total_points, tournaments_played, matches_played, matches_won, best_placement, updated_at) \
         VALUES ($1, 42, 1, 1, 1, 9, $2)",
    )
    .bind(7999_i64)
    .bind(now_utc())
    .execute(pool)
    .await
    .expect("insert sentinel");

    let mut tx = pool.begin().await.expect("begin");
    recalculate_player_points_in_tx(&mut tx, tournament_id)
        .await
        .expect("recompute");

    let sentinel_inside: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnier.player_points WHERE discord_id = $1")
            .bind(7999_i64)
            .fetch_one(&mut *tx)
            .await
            .expect("sentinel inside");
    assert_eq!(sentinel_inside, 0);

    let winner_points: (i64, i64, i64, Option<i64>) = sqlx::query_as(
        "SELECT total_points, matches_played, matches_won, best_placement \
         FROM turnier.player_points WHERE discord_id = $1",
    )
    .bind(7101_i64)
    .fetch_one(&mut *tx)
    .await
    .expect("winner points");
    assert_eq!(winner_points, (12, 3, 2, Some(1)));

    tx.rollback().await.expect("rollback");

    let sentinel_after: (i64, i64, i64, Option<i64>) = sqlx::query_as(
        "SELECT total_points, matches_played, matches_won, best_placement \
         FROM turnier.player_points WHERE discord_id = $1",
    )
    .bind(7999_i64)
    .fetch_one(pool)
    .await
    .expect("sentinel after rollback");
    assert_eq!(sentinel_after, (42, 1, 1, Some(9)));

    let recomputed_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnier.player_points WHERE discord_id = $1")
            .bind(7101_i64)
            .fetch_one(pool)
            .await
            .expect("winner after rollback");
    assert_eq!(recomputed_after, 0);
}

async fn player_totals(pool: &turnier_db::Pool, discord_id: i64) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT tournaments_played, matches_played, matches_won \
         FROM turnier.player_points WHERE discord_id = $1",
    )
    .bind(discord_id)
    .fetch_one(pool)
    .await
    .expect("player totals")
}

async fn seeded_league(pool: &turnier_db::Pool) -> (i64, Vec<i64>) {
    let id = insert_tournament(
        pool,
        "Liga",
        "group_phase",
        1,
        "single_elimination",
        "league",
    )
    .await;
    let mut teams = Vec::new();
    for index in 0..4 {
        let team = insert_team(pool, id, &format!("Team {index}"), 8100 + index).await;
        insert_team_member(pool, team, 8200 + index, "Spieler", "captain", 0).await;
        teams.push(team);
    }
    let mut tx = pool.begin().await.expect("begin");
    generate_league_plan_in_tx(&mut tx, id, 50)
        .await
        .expect("plan");
    tx.commit().await.expect("commit");
    (id, teams)
}

async fn complete_league_except_one_game(pool: &turnier_db::Pool, id: i64, teams: &[i64]) {
    sqlx::query(
        "UPDATE turnier.group_matches SET status = 'completed', winner_id = team1_id \
         WHERE group_id IN (SELECT id FROM turnier.groups WHERE tournament_id = $1)",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("complete games");
    sqlx::query(
        "UPDATE turnier.group_matches SET status = 'cancelled', winner_id = NULL \
         WHERE LEAST(team1_id, team2_id) = $1 AND GREATEST(team1_id, team2_id) = $2",
    )
    .bind(teams[0].min(teams[1]))
    .bind(teams[0].max(teams[1]))
    .execute(pool)
    .await
    .expect("cancel one game");
    sqlx::query("UPDATE turnier.tournaments SET status = 'completed' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .expect("finish");
}

async fn seeded_bracket_tournament(
    pool: &turnier_db::Pool,
    mode: &str,
    first_id: i64,
    games: i64,
) -> i64 {
    let id = insert_tournament(pool, mode, "completed", 1, "single_elimination", mode).await;
    let first = insert_team(pool, id, "Alpha", first_id).await;
    let second = insert_team(pool, id, "Bravo", first_id + 1).await;
    insert_team_member(pool, first, first_id, "Alpha", "captain", 0).await;
    insert_team_member(pool, second, first_id + 1, "Bravo", "captain", 0).await;
    for round in 1..=games {
        insert_completed_match(pool, id, round, 0, first, second, first).await;
    }
    id
}

#[tokio::test]
async fn league_counts_games_per_team_and_other_modes_stay_tournament_wide() {
    let db = temp_pool().await;
    let pool = db.pool();
    let (league, teams) = seeded_league(pool).await;
    complete_league_except_one_game(pool, league, &teams).await;
    let bracket = seeded_bracket_tournament(pool, "bracket_only", 8300, 3).await;
    seeded_bracket_tournament(pool, "group_stage", 8400, 2).await;

    for _ in 0..2 {
        let mut tx = pool.begin().await.expect("begin");
        recalculate_player_points_in_tx(&mut tx, bracket)
            .await
            .expect("recompute");
        tx.commit().await.expect("commit");

        assert_eq!(player_totals(pool, 8200).await.1, 2);
        assert_eq!(player_totals(pool, 8201).await.1, 2);
        assert_eq!(player_totals(pool, 8202).await.1, 3);
        assert_eq!(player_totals(pool, 8203).await.1, 3);
        assert_eq!(player_totals(pool, 8300).await, (1, 3, 3));
        assert_eq!(player_totals(pool, 8301).await, (1, 3, 0));
        assert_eq!(player_totals(pool, 8400).await, (1, 2, 2));
        assert_eq!(player_totals(pool, 8401).await, (1, 2, 0));
    }
}
