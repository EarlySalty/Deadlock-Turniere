//! Paritätstest: Cross-Seeding aus den Gruppenphasen (Sieger Gruppe i vs Zweiter
//! Gruppe i+1). Portiert aus `backend/tests/test_engine_cross_seeding.py`.

#![cfg(feature = "testing")]

mod common;

use common::{insert_group, insert_group_team, insert_team, insert_tournament, temp_pool};
use sqlx::Row;
use std::collections::HashMap;
use turnier_engine::generate_bracket;

#[tokio::test]
async fn generate_bracket_uses_cross_seed_pairs_from_groups() {
    let db = temp_pool().await;
    let pool = db.pool();

    let tournament_id = insert_tournament(
        pool,
        "Cross Seed",
        "bracket",
        6,
        "single_elimination",
        "group_stage",
    )
    .await;

    let mut team_ids: HashMap<&str, i64> = HashMap::new();
    for (idx, team_name) in ["A1", "A2", "B1", "B2", "C1", "C2", "D1", "D2"]
        .iter()
        .enumerate()
    {
        let team_id = insert_team(pool, tournament_id, team_name, 3000 + idx as i64).await;
        team_ids.insert(team_name, team_id);
    }

    let mut group_ids: HashMap<&str, i64> = HashMap::new();
    for (seeding_order, group_name) in ["A", "B", "C", "D"].iter().enumerate() {
        let group_id = insert_group(
            pool,
            tournament_id,
            &format!("Gruppe {group_name}"),
            seeding_order as i64,
        )
        .await;
        group_ids.insert(group_name, group_id);
    }

    let group_rows = [
        ("A", "A1", 9, 3, 0),
        ("A", "A2", 6, 2, 1),
        ("B", "B1", 9, 3, 0),
        ("B", "B2", 6, 2, 1),
        ("C", "C1", 9, 3, 0),
        ("C", "C2", 6, 2, 1),
        ("D", "D1", 9, 3, 0),
        ("D", "D2", 6, 2, 1),
    ];
    for (group_name, team_name, points, wins, losses) in group_rows {
        insert_group_team(
            pool,
            group_ids[group_name],
            team_ids[team_name],
            points,
            wins,
            losses,
        )
        .await;
    }

    let match_count = generate_bracket(pool, tournament_id)
        .await
        .expect("generate");
    assert_eq!(match_count, 7);

    let expected_pairs = vec![
        (team_ids["A1"], team_ids["B2"]),
        (team_ids["B1"], team_ids["C2"]),
        (team_ids["C1"], team_ids["D2"]),
        (team_ids["D1"], team_ids["A2"]),
    ];

    let rows = sqlx::query(
        "SELECT team1_id, team2_id FROM turnier.bracket_matches \
         WHERE tournament_id = $1 AND round = $2 AND bracket_type = 'winners' ORDER BY position",
    )
    .bind(tournament_id)
    .bind(1)
    .fetch_all(pool)
    .await
    .expect("round one");
    let round_one_pairs: Vec<(i64, i64)> = rows
        .iter()
        .map(|r| (r.get::<i64, _>("team1_id"), r.get::<i64, _>("team2_id")))
        .collect();

    assert_eq!(round_one_pairs, expected_pairs);
}

#[tokio::test]
async fn generate_bracket_breaks_group_standing_ties_by_group_team_order() {
    let db = temp_pool().await;
    let pool = db.pool();

    let tournament_id = insert_tournament(
        pool,
        "Group Tie",
        "bracket",
        6,
        "single_elimination",
        "group_stage",
    )
    .await;

    let first_team = insert_team(pool, tournament_id, "First Inserted In Group", 4101).await;
    let second_team = insert_team(pool, tournament_id, "Second Inserted In Group", 4102).await;
    let third_team = insert_team(pool, tournament_id, "Third Inserted In Group", 4103).await;
    let last_team = insert_team(pool, tournament_id, "Last Inserted In Group", 4104).await;

    let group_id = insert_group(pool, tournament_id, "Gruppe A", 0).await;

    insert_group_team(pool, group_id, second_team, 6, 2, 0).await;
    insert_group_team(pool, group_id, first_team, 6, 2, 0).await;
    insert_group_team(pool, group_id, third_team, 6, 2, 0).await;
    insert_group_team(pool, group_id, last_team, 6, 2, 0).await;

    let match_count = generate_bracket(pool, tournament_id)
        .await
        .expect("generate");
    assert_eq!(match_count, 1);

    let row = sqlx::query(
        "SELECT team1_id, team2_id FROM turnier.bracket_matches \
         WHERE tournament_id = $1 AND round = $2 AND bracket_type = 'winners'",
    )
    .bind(tournament_id)
    .bind(1)
    .fetch_one(pool)
    .await
    .expect("round one");

    assert_eq!(row.get::<i64, _>("team1_id"), second_team);
    assert_eq!(row.get::<i64, _>("team2_id"), first_team);
}
