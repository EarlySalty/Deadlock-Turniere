//! Transaktionsgrenze für Bracket-Rebuilds: der Admin-Pfad muss denselben
//! Bracket-Aufbau innerhalb seiner offenen Update-Transaktion nutzen können.

mod common;

use common::temp_pool;
use sqlx::Row;
use turnier_engine::generate_bracket_in_tx;

#[tokio::test]
async fn generate_bracket_in_tx_rolls_back_with_outer_transaction() {
    let pool = temp_pool().await;

    sqlx::query("INSERT INTO tournaments (name, status, created_by, updated_at) VALUES (?, ?, ?, ?)")
        .bind("Tx Bracket")
        .bind("group_phase")
        .bind("admin")
        .bind("now")
        .execute(&pool)
        .await
        .expect("insert tournament");

    for team_number in 0..4 {
        sqlx::query(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?)",
        )
        .bind(1)
        .bind(format!("Team {}", team_number + 1))
        .bind(format!("team-{}", team_number + 1))
        .bind(format!("captain-{}", team_number + 1))
        .execute(&pool)
        .await
        .expect("insert team");
    }

    let mut tx = pool.begin().await.expect("begin");
    let match_count = generate_bracket_in_tx(&mut tx, 1).await.expect("generate");
    assert_eq!(match_count, 3);

    let inside_count: i64 = sqlx::query("SELECT COUNT(*) AS cnt FROM bracket_matches WHERE tournament_id = 1")
        .fetch_one(&mut *tx)
        .await
        .expect("count inside tx")
        .get("cnt");
    assert_eq!(inside_count, 3);

    tx.rollback().await.expect("rollback");

    let outside_count: i64 =
        sqlx::query("SELECT COUNT(*) AS cnt FROM bracket_matches WHERE tournament_id = 1")
            .fetch_one(&pool)
            .await
            .expect("count outside tx")
            .get("cnt");
    assert_eq!(outside_count, 0);
}
