//! Paritätstest: Double-Elimination-Bracket inkl. Loser-Mapping, Grand-Final-
//! Reset, Slot-Cross. Portiert aus `backend/tests/test_engine_double_elim.py`
//! (2 Testfälle), erwartete Werte WÖRTLICH übernommen.

mod common;

use common::temp_pool;
use sqlx::Row;
use turnier_engine::{advance_bracket_winner, generate_bracket};

/// Lädt Matches eines Bracket-Typs/Runde, sortiert `ORDER BY position, id`.
async fn load_matches(
    pool: &sqlx::SqlitePool,
    tournament_id: i64,
    bracket_type: &str,
    round_num: i64,
) -> Vec<(i64, Option<i64>, Option<i64>, Option<i64>, Option<i64>, String)> {
    sqlx::query(
        "SELECT id, team1_id, team2_id, loser_to_match_id, loser_to_slot, status \
         FROM bracket_matches WHERE tournament_id = ? AND bracket_type = ? AND round = ? \
         ORDER BY position, id",
    )
    .bind(tournament_id)
    .bind(bracket_type)
    .bind(round_num)
    .fetch_all(pool)
    .await
    .expect("load")
    .into_iter()
    .map(|r| {
        (
            r.get::<i64, _>("id"),
            r.get::<Option<i64>, _>("team1_id"),
            r.get::<Option<i64>, _>("team2_id"),
            r.get::<Option<i64>, _>("loser_to_match_id"),
            r.get::<Option<i64>, _>("loser_to_slot"),
            r.get::<String, _>("status"),
        )
    })
    .collect()
}

async fn seed_double_elim_tournament(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO tournaments (name, status, created_by, updated_at, bracket_format) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("Double Elim")
    .bind("bracket")
    .bind("admin")
    .bind("now")
    .bind("double_elimination")
    .execute(pool)
    .await
    .expect("insert tournament");

    for team_number in 0..8 {
        sqlx::query(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(1)
        .bind(format!("Team {}", team_number + 1))
        .bind(format!("team-{}", team_number + 1))
        .bind(format!("{:03}", team_number + 1))
        .execute(pool)
        .await
        .expect("insert team");
    }
}

#[tokio::test]
async fn builds_double_elimination_and_grand_final_reset() {
    let pool = temp_pool().await;
    seed_double_elim_tournament(&pool).await;

    let match_count = generate_bracket(&pool, 1).await.expect("generate");
    assert_eq!(match_count, 15);

    // Typ-Zählung.
    let type_rows = sqlx::query(
        "SELECT bracket_type, COUNT(*) AS cnt FROM bracket_matches \
         WHERE tournament_id = ? GROUP BY bracket_type",
    )
    .bind(1)
    .fetch_all(&pool)
    .await
    .expect("counts");
    let mut counts = std::collections::HashMap::new();
    for r in &type_rows {
        counts.insert(r.get::<String, _>("bracket_type"), r.get::<i64, _>("cnt"));
    }
    assert_eq!(counts.get("winners"), Some(&7));
    assert_eq!(counts.get("losers"), Some(&6));
    assert_eq!(counts.get("grand_final"), Some(&2));

    let wr1 = load_matches(&pool, 1, "winners", 1).await;
    let wr2 = load_matches(&pool, 1, "winners", 2).await;
    let wf = load_matches(&pool, 1, "winners", 3).await;
    let lr1 = load_matches(&pool, 1, "losers", 1).await;
    let lr2 = load_matches(&pool, 1, "losers", 2).await;
    let lr3 = load_matches(&pool, 1, "losers", 3).await;
    let lf = load_matches(&pool, 1, "losers", 4).await;
    let gf = load_matches(&pool, 1, "grand_final", 4).await;
    let gf_reset = load_matches(&pool, 1, "grand_final", 5).await;

    assert_eq!(wr1.len(), 4);
    assert_eq!(wr2.len(), 2);
    assert_eq!(wf.len(), 1);
    assert_eq!(lr1.len(), 2);
    assert_eq!(lr2.len(), 2);
    assert_eq!(lr3.len(), 1);
    assert_eq!(lf.len(), 1);
    assert_eq!(gf.len(), 1);
    assert_eq!(gf_reset.len(), 1);

    // Loser-Mapping Winners-Runde 1.
    assert_eq!(wr1[0].3, Some(lr1[0].0));
    assert_eq!(wr1[0].4, Some(1));
    assert_eq!(wr1[1].3, Some(lr1[0].0));
    assert_eq!(wr1[1].4, Some(2));
    assert_eq!(wr1[2].3, Some(lr1[1].0));
    assert_eq!(wr1[2].4, Some(1));
    assert_eq!(wr1[3].3, Some(lr1[1].0));
    assert_eq!(wr1[3].4, Some(2));

    // Loser-Mapping Winners-Runde 2 (Slot-Cross via rem_euclid) + WF -> LB-Finale.
    assert_eq!(wr2[0].3, Some(lr2[1].0));
    assert_eq!(wr2[0].4, Some(2));
    assert_eq!(wr2[1].3, Some(lr2[0].0));
    assert_eq!(wr2[1].4, Some(2));
    assert_eq!(wf[0].3, Some(lf[0].0));
    assert_eq!(wf[0].4, Some(2));

    // WR1 ausspielen: jeweils team1 gewinnt.
    for m in &wr1 {
        advance_bracket_winner(&pool, 1, m.0, m.1.unwrap())
            .await
            .expect("advance");
    }

    let lr1 = load_matches(&pool, 1, "losers", 1).await;
    let wr2 = load_matches(&pool, 1, "winners", 2).await;
    let lr1_pairs: Vec<(Option<i64>, Option<i64>)> = lr1.iter().map(|m| (m.1, m.2)).collect();
    let wr2_pairs: Vec<(Option<i64>, Option<i64>)> = wr2.iter().map(|m| (m.1, m.2)).collect();
    assert_eq!(lr1_pairs, vec![(Some(8), Some(5)), (Some(7), Some(6))]);
    assert_eq!(wr2_pairs, vec![(Some(1), Some(4)), (Some(2), Some(3))]);

    // LR1: jeweils team2 gewinnt.
    advance_bracket_winner(&pool, 1, lr1[0].0, lr1[0].2.unwrap())
        .await
        .expect("advance");
    advance_bracket_winner(&pool, 1, lr1[1].0, lr1[1].2.unwrap())
        .await
        .expect("advance");

    // WR2: team1 gewinnt.
    for m in &wr2 {
        advance_bracket_winner(&pool, 1, m.0, m.1.unwrap())
            .await
            .expect("advance");
    }

    let lr2 = load_matches(&pool, 1, "losers", 2).await;
    let wf = load_matches(&pool, 1, "winners", 3).await;
    let lr2_pairs: Vec<(Option<i64>, Option<i64>)> = lr2.iter().map(|m| (m.1, m.2)).collect();
    let wf_pairs: Vec<(Option<i64>, Option<i64>)> = wf.iter().map(|m| (m.1, m.2)).collect();
    assert_eq!(lr2_pairs, vec![(Some(5), Some(3)), (Some(6), Some(4))]);
    assert_eq!(wf_pairs, vec![(Some(1), Some(2))]);

    // LR2: jeweils team2 gewinnt.
    advance_bracket_winner(&pool, 1, lr2[0].0, lr2[0].2.unwrap())
        .await
        .expect("advance");
    advance_bracket_winner(&pool, 1, lr2[1].0, lr2[1].2.unwrap())
        .await
        .expect("advance");

    let lr3 = load_matches(&pool, 1, "losers", 3).await;
    let lr3_pairs: Vec<(Option<i64>, Option<i64>)> = lr3.iter().map(|m| (m.1, m.2)).collect();
    assert_eq!(lr3_pairs, vec![(Some(3), Some(4))]);
    advance_bracket_winner(&pool, 1, lr3[0].0, lr3[0].1.unwrap())
        .await
        .expect("advance");

    // WF: team1 gewinnt.
    advance_bracket_winner(&pool, 1, wf[0].0, wf[0].1.unwrap())
        .await
        .expect("advance");

    let lf = load_matches(&pool, 1, "losers", 4).await;
    let lf_pairs: Vec<(Option<i64>, Option<i64>)> = lf.iter().map(|m| (m.1, m.2)).collect();
    assert_eq!(lf_pairs, vec![(Some(3), Some(2))]);
    advance_bracket_winner(&pool, 1, lf[0].0, lf[0].1.unwrap())
        .await
        .expect("advance");

    let gf = load_matches(&pool, 1, "grand_final", 4).await;
    let gf_pairs: Vec<(Option<i64>, Option<i64>)> = gf.iter().map(|m| (m.1, m.2)).collect();
    assert_eq!(gf_pairs, vec![(Some(1), Some(3))]);
    // GF: team2 (Losers-Finalist) gewinnt -> Reset eager pending.
    advance_bracket_winner(&pool, 1, gf[0].0, gf[0].2.unwrap())
        .await
        .expect("advance");

    let gf_reset = load_matches(&pool, 1, "grand_final", 5).await;
    assert_eq!(gf_reset[0].5, "pending");
    assert_eq!((gf_reset[0].1, gf_reset[0].2), (Some(1), Some(3)));
}

#[tokio::test]
async fn sets_stream_heuristic() {
    let pool = temp_pool().await;
    seed_double_elim_tournament(&pool).await;

    generate_bracket(&pool, 1).await.expect("generate");

    let rows = sqlx::query(
        "SELECT bracket_type, round, on_stream FROM bracket_matches WHERE tournament_id = 1",
    )
    .fetch_all(&pool)
    .await
    .expect("rows");
    let rows: Vec<(String, i64, i64)> = rows
        .iter()
        .map(|r| {
            (
                r.get::<String, _>("bracket_type"),
                r.get::<i64, _>("round"),
                r.get::<i64, _>("on_stream"),
            )
        })
        .collect();

    // Winners + Grand Final immer on_stream.
    assert!(rows
        .iter()
        .filter(|(t, _, _)| t == "winners")
        .all(|(_, _, s)| *s == 1));
    assert!(rows
        .iter()
        .filter(|(t, _, _)| t == "grand_final")
        .all(|(_, _, s)| *s == 1));

    let losers: Vec<&(String, i64, i64)> =
        rows.iter().filter(|(t, _, _)| t == "losers").collect();
    let last_round = losers.iter().map(|(_, r, _)| *r).max().unwrap();
    // LB-Finale on_stream, frühere Loser-Runden off-stream.
    assert!(losers
        .iter()
        .filter(|(_, r, _)| *r == last_round)
        .all(|(_, _, s)| *s == 1));
    assert!(losers
        .iter()
        .filter(|(_, r, _)| *r != last_round)
        .all(|(_, _, s)| *s == 0));
}
