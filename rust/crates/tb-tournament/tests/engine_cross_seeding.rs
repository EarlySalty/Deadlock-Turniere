//! Paritätstest: Cross-Seeding aus den Gruppenphasen (Sieger Gruppe i vs Zweiter
//! Gruppe i+1). Portiert aus `backend/tests/test_engine_cross_seeding.py`.

mod common;

use common::temp_pool;
use sqlx::Row;
use std::collections::HashMap;
use tb_tournament::generate_bracket;

#[tokio::test]
async fn generate_bracket_uses_cross_seed_pairs_from_groups() {
    let pool = temp_pool().await;

    sqlx::query("INSERT INTO tournaments (name, status, created_by, updated_at) VALUES (?, ?, ?, ?)")
        .bind("Cross Seed")
        .bind("bracket")
        .bind("admin")
        .bind("now")
        .execute(&pool)
        .await
        .expect("insert tournament");

    let mut team_ids: HashMap<&str, i64> = HashMap::new();
    for team_name in ["A1", "A2", "B1", "B2", "C1", "C2", "D1", "D2"] {
        let row = sqlx::query(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) \
             VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(1)
        .bind(team_name)
        .bind(team_name.to_lowercase())
        .bind(format!("captain-{team_name}"))
        .fetch_one(&pool)
        .await
        .expect("insert team");
        team_ids.insert(team_name, row.get::<i64, _>("id"));
    }

    let mut group_ids: HashMap<&str, i64> = HashMap::new();
    for (seeding_order, group_name) in ["A", "B", "C", "D"].iter().enumerate() {
        let row = sqlx::query(
            "INSERT INTO groups (tournament_id, name, seeding_order) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(1)
        .bind(format!("Gruppe {group_name}"))
        .bind(seeding_order as i64)
        .fetch_one(&pool)
        .await
        .expect("insert group");
        group_ids.insert(group_name, row.get::<i64, _>("id"));
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
        sqlx::query(
            "INSERT INTO group_teams (group_id, team_id, points, wins, losses) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(group_ids[group_name])
        .bind(team_ids[team_name])
        .bind(points)
        .bind(wins)
        .bind(losses)
        .execute(&pool)
        .await
        .expect("insert group_team");
    }

    let match_count = generate_bracket(&pool, 1).await.expect("generate");
    assert_eq!(match_count, 7);

    let expected_pairs = vec![
        (team_ids["A1"], team_ids["B2"]),
        (team_ids["B1"], team_ids["C2"]),
        (team_ids["C1"], team_ids["D2"]),
        (team_ids["D1"], team_ids["A2"]),
    ];

    let rows = sqlx::query(
        "SELECT team1_id, team2_id FROM bracket_matches \
         WHERE tournament_id = ? AND round = ? AND bracket_type = 'winners' ORDER BY position",
    )
    .bind(1)
    .bind(1)
    .fetch_all(&pool)
    .await
    .expect("round one");
    let round_one_pairs: Vec<(i64, i64)> = rows
        .iter()
        .map(|r| {
            (
                r.get::<i64, _>("team1_id"),
                r.get::<i64, _>("team2_id"),
            )
        })
        .collect();

    assert_eq!(round_one_pairs, expected_pairs);
}
