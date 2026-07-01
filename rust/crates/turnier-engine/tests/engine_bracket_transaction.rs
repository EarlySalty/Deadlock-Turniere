//! Transaktionsgrenze für Bracket-Rebuilds: der Admin-Pfad muss denselben
//! Bracket-Aufbau innerhalb seiner offenen Update-Transaktion nutzen können.

#![cfg(feature = "testing")]

mod common;

use common::{insert_team, insert_tournament, temp_pool};
use sqlx::Row;
use turnier_engine::generate_bracket_in_tx;

#[tokio::test]
async fn generate_bracket_in_tx_rolls_back_with_outer_transaction() {
    let db = temp_pool().await;
    let pool = db.pool();

    let tournament_id = insert_tournament(
        pool,
        "Tx Bracket",
        "group_phase",
        6,
        "single_elimination",
        "bracket_only",
    )
    .await;

    for team_number in 0..4 {
        insert_team(
            pool,
            tournament_id,
            &format!("Team {}", team_number + 1),
            2000 + team_number,
        )
        .await;
    }

    let mut tx = pool.begin().await.expect("begin");
    let match_count = generate_bracket_in_tx(&mut tx, tournament_id)
        .await
        .expect("generate");
    assert_eq!(match_count, 3);

    let inside_count: i64 =
        sqlx::query("SELECT COUNT(*) AS cnt FROM turnier.bracket_matches WHERE tournament_id = $1")
            .bind(tournament_id)
            .fetch_one(&mut *tx)
            .await
            .expect("count inside tx")
            .get("cnt");
    assert_eq!(inside_count, 3);

    tx.rollback().await.expect("rollback");

    let outside_count: i64 =
        sqlx::query("SELECT COUNT(*) AS cnt FROM turnier.bracket_matches WHERE tournament_id = $1")
            .bind(tournament_id)
            .fetch_one(pool)
            .await
            .expect("count outside tx")
            .get("cnt");
    assert_eq!(outside_count, 0);
}
