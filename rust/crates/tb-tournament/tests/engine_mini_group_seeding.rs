//! Paritätstest: Mini-Group-Seeding ohne Byes für Nicht-Zweierpotenzen.
//! Portiert aus `backend/tests/test_engine_mini_group_seeding.py`
//! (6 parametrisierte Fälle).

mod common;

use common::temp_pool;
use sqlx::Row;
use tb_tournament::generate_bracket;

async fn run_case(team_count: i64, expected_match_count: i64, expected_mini_group_count: i64) {
    let pool = temp_pool().await;

    sqlx::query("INSERT INTO tournaments (name, status, created_by, updated_at) VALUES (?, ?, ?, ?)")
        .bind(format!("Mini RR {team_count}"))
        .bind("bracket")
        .bind("admin")
        .bind("now")
        .execute(&pool)
        .await
        .expect("insert tournament");

    for team_number in 0..team_count {
        sqlx::query(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?)",
        )
        .bind(1)
        .bind(format!("Team {}", team_number + 1))
        .bind(format!("team-{}", team_number + 1))
        .bind(format!("{:03}", team_number + 1))
        .execute(&pool)
        .await
        .expect("insert team");
    }

    let match_count = generate_bracket(&pool, 1).await.expect("generate");
    assert_eq!(match_count, expected_match_count, "match_count für {team_count} Teams");

    let total: i64 = sqlx::query("SELECT COUNT(*) AS cnt FROM bracket_matches WHERE tournament_id = 1")
        .fetch_one(&pool)
        .await
        .expect("count")
        .get("cnt");
    assert_eq!(total, expected_match_count);

    let mini_groups = sqlx::query(
        "SELECT id, round, advances_to_match_id, advances_to_slot \
         FROM bracket_mini_groups WHERE tournament_id = 1 ORDER BY round, position, id",
    )
    .fetch_all(&pool)
    .await
    .expect("mini groups");
    assert_eq!(
        mini_groups.len() as i64,
        expected_mini_group_count,
        "mini_group_count für {team_count} Teams"
    );

    // Jedes Match hat in beiden Slots eine Quelle (kein Freilos).
    let matches = sqlx::query(
        "SELECT team1_id, team2_id, source_match1_id, source_match2_id, \
         source_mini_group1_id, source_mini_group2_id FROM bracket_matches \
         WHERE tournament_id = 1 ORDER BY round, position, id",
    )
    .fetch_all(&pool)
    .await
    .expect("matches");
    for m in &matches {
        let t1: Option<i64> = m.get("team1_id");
        let sm1: Option<i64> = m.get("source_match1_id");
        let smg1: Option<i64> = m.get("source_mini_group1_id");
        assert!(t1.is_some() || sm1.is_some() || smg1.is_some());
        let t2: Option<i64> = m.get("team2_id");
        let sm2: Option<i64> = m.get("source_match2_id");
        let smg2: Option<i64> = m.get("source_mini_group2_id");
        assert!(t2.is_some() || sm2.is_some() || smg2.is_some());
    }

    let max_round: i64 = mini_groups.iter().map(|g| g.get::<i64, _>("round")).max().unwrap();
    for mg in &mini_groups {
        let mg_id: i64 = mg.get("id");
        let participant_count: i64 =
            sqlx::query("SELECT COUNT(*) AS cnt FROM bracket_mini_group_teams WHERE mini_group_id = ?")
                .bind(mg_id)
                .fetch_one(&pool)
                .await
                .expect("count")
                .get("cnt");
        let rr_match_count: i64 =
            sqlx::query("SELECT COUNT(*) AS cnt FROM bracket_matches WHERE mini_group_id = ?")
                .bind(mg_id)
                .fetch_one(&pool)
                .await
                .expect("count")
                .get("cnt");
        assert_eq!(rr_match_count, participant_count * (participant_count - 1) / 2);

        let advances_to: Option<i64> = mg.get("advances_to_match_id");
        if advances_to.is_none() {
            let downstream: i64 = sqlx::query(
                "SELECT COUNT(*) AS cnt FROM bracket_matches \
                 WHERE source_mini_group1_id = ? OR source_mini_group2_id = ?",
            )
            .bind(mg_id)
            .bind(mg_id)
            .fetch_one(&pool)
            .await
            .expect("count")
            .get("cnt");
            let round: i64 = mg.get("round");
            if round < max_round {
                assert!(downstream > 0);
            }
        } else {
            let slot: Option<i64> = mg.get("advances_to_slot");
            assert!(matches!(slot, Some(1) | Some(2)));
        }
    }
}

#[tokio::test]
async fn mini_groups_without_byes() {
    // (team_count, expected_match_count, expected_mini_group_count)
    let cases = [
        (5, 5, 1),
        (6, 6, 1),
        (7, 8, 2),
        (9, 9, 1),
        (11, 12, 2),
        (13, 14, 2),
    ];
    for (team_count, expected_match_count, expected_mini_group_count) in cases {
        run_case(team_count, expected_match_count, expected_mini_group_count).await;
    }
}
