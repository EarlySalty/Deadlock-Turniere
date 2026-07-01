//! Integrationstests der Ergebnisverarbeitung (Bracket + Group) gegen eine
//! zentrale Wegwerf-PG-DB. Belegt insbesondere die bewusst erhaltenen Befunde:
//! Bracket schreibt winner_id in `match_results.winning_team`, Group den Slot.

mod common;

use common::*;
use serde_json::{json, Value};
use sqlx::Row;
use turnier_core::now_utc;
use turnier_match::{ApplyBracketParams, ApplyGroupParams};

#[tokio::test]
async fn bracket_ergebnis_setzt_winner_und_status() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams {
        winner_id: Some(team2),
        ..ApplyBracketParams::automatic()
    };
    let outcome = mgr
        .apply_bracket_match_result(tid, mid, params)
        .await
        .expect("apply");
    assert_eq!(outcome.winner_id, team2);
    // Bracket-Konvention 0-basiert: team2 → 1.
    assert_eq!(outcome.winning_team, 1);

    // Match auf completed + winner_id gesetzt.
    let row = sqlx::query("SELECT winner_id, status FROM turnier.bracket_matches WHERE id = $1")
        .bind(mid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("winner_id"), team2);
    assert_eq!(row.get::<String, _>("status"), "completed");

    // BEFUND (needs-decision): match_results.winning_team enthält die winner_id,
    // NICHT den Slot.
    let mr = sqlx::query(
        "SELECT winning_team, source FROM turnier.match_results WHERE bracket_match_id = $1",
    )
    .bind(mid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mr.get::<i64, _>("winning_team"), team2);
    assert_eq!(mr.get::<String, _>("source"), "automatic");
}

#[tokio::test]
async fn bracket_winning_team_0_waehlt_team1() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams {
        winning_team: Some(0),
        ..ApplyBracketParams::automatic()
    };
    let outcome = mgr
        .apply_bracket_match_result(tid, mid, params)
        .await
        .expect("apply");
    assert_eq!(outcome.winner_id, team1);
    assert_eq!(outcome.winning_team, 0);
}

#[tokio::test]
async fn bracket_propagiert_in_folge_match() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let semi =
        insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    // Folge-Match nimmt den Sieger über source_match1_id auf Slot team1.
    let finale = insert_bracket_match(&pool, tid, 2, 0, None, None, "pending").await;
    sqlx::query("UPDATE turnier.bracket_matches SET source_match1_id = $1 WHERE id = $2")
        .bind(semi)
        .bind(finale)
        .execute(&pool)
        .await
        .unwrap();
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams {
        winner_id: Some(team1),
        ..ApplyBracketParams::automatic()
    };
    mgr.apply_bracket_match_result(tid, semi, params)
        .await
        .expect("apply");

    let team1_in_final = sqlx::query("SELECT team1_id FROM turnier.bracket_matches WHERE id = $1")
        .bind(finale)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<Option<i64>, _>("team1_id");
    assert_eq!(team1_in_final, Some(team1));
}

#[tokio::test]
async fn mini_group_tiebreak_nutzt_head_to_head_vor_seed_und_point_diff() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let team3 = insert_team(&pool, tid, "Gamma").await;
    let team4 = insert_team(&pool, tid, "Delta").await;
    let target = insert_bracket_match(&pool, tid, 2, 0, None, None, "pending").await;
    let mini_group_id: i64 = sqlx::query_scalar(
        "INSERT INTO turnier.bracket_mini_groups \
             (tournament_id, round, position, advances_to_match_id, advances_to_slot, created_at) \
         VALUES ($1, 1, 0, $2, 1, $3) RETURNING id",
    )
    .bind(tid)
    .bind(target)
    .bind(now_utc())
    .fetch_one(&pool)
    .await
    .unwrap();

    for (seed_order, team_id) in [team1, team2, team3, team4].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO turnier.bracket_mini_group_teams \
                 (mini_group_id, team_id, seed_order) VALUES ($1, $2, $3)",
        )
        .bind(mini_group_id)
        .bind(team_id)
        .bind(seed_order as i64)
        .execute(&pool)
        .await
        .unwrap();
    }

    // team1 und team2 enden beide bei 2 Wins. team1 hat Seed 0 und hohe
    // Punktdifferenz, verliert aber das direkte Duell gegen team2.
    insert_mini_group_match(
        &pool,
        tid,
        mini_group_id,
        0,
        team1,
        team2,
        Some(team2),
        "completed",
        Some(json!({"team1_score": 5, "team2_score": 6})),
    )
    .await;
    insert_mini_group_match(
        &pool,
        tid,
        mini_group_id,
        1,
        team1,
        team3,
        Some(team1),
        "completed",
        Some(json!({"team1_score": 20, "team2_score": 0})),
    )
    .await;
    insert_mini_group_match(
        &pool,
        tid,
        mini_group_id,
        2,
        team1,
        team4,
        Some(team1),
        "completed",
        Some(json!({"team1_score": 20, "team2_score": 0})),
    )
    .await;
    insert_mini_group_match(
        &pool,
        tid,
        mini_group_id,
        3,
        team2,
        team3,
        Some(team2),
        "completed",
        Some(json!({"team1_score": 1, "team2_score": 0})),
    )
    .await;
    insert_mini_group_match(
        &pool,
        tid,
        mini_group_id,
        4,
        team4,
        team2,
        Some(team4),
        "completed",
        Some(json!({"team1_score": 20, "team2_score": 0})),
    )
    .await;
    let final_round_robin = insert_mini_group_match(
        &pool,
        tid,
        mini_group_id,
        5,
        team3,
        team4,
        None,
        "in_progress",
        None,
    )
    .await;
    let mgr = manager_without_services(pool.clone());

    mgr.apply_bracket_match_result(
        tid,
        final_round_robin,
        ApplyBracketParams {
            winner_id: Some(team3),
            ..ApplyBracketParams::automatic()
        },
    )
    .await
    .expect("apply final mini-group match");

    let propagated = sqlx::query("SELECT team1_id FROM turnier.bracket_matches WHERE id = $1")
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<Option<i64>, _>("team1_id");
    assert_eq!(propagated, Some(team2));
}

#[tokio::test]
async fn bracket_terminal_ohne_force_fehler() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "completed").await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams {
        winner_id: Some(team1),
        ..ApplyBracketParams::automatic()
    };
    let err = mgr
        .apply_bracket_match_result(tid, mid, params)
        .await
        .unwrap_err();
    assert!(matches!(err, turnier_match::MatchError::State(_)));
}

#[tokio::test]
async fn group_ergebnis_setzt_slot_und_tabelle() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let gid = insert_group(&pool, tid, "Gruppe A").await;
    insert_group_team(&pool, gid, team1).await;
    insert_group_team(&pool, gid, team2).await;
    let mid = insert_group_match(&pool, gid, team1, team2, "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    // winning_team=1 (1-basiert!) → team1 gewinnt.
    let params = ApplyGroupParams {
        winning_team: Some(1),
        ..ApplyGroupParams::manual()
    };
    let outcome = mgr
        .apply_group_match_result(tid, mid, params)
        .await
        .expect("apply");
    assert_eq!(outcome.winner_id, team1);
    assert_eq!(outcome.winning_team, 1);

    // group_teams: Sieger +1 win +3 points, Verlierer +1 loss.
    let winner = sqlx::query(
        "SELECT wins, points, losses FROM turnier.group_teams WHERE group_id = $1 AND team_id = $2",
    )
    .bind(gid)
    .bind(team1)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(winner.get::<i64, _>("wins"), 1);
    assert_eq!(winner.get::<i64, _>("points"), 3);
    let loser =
        sqlx::query("SELECT losses FROM turnier.group_teams WHERE group_id = $1 AND team_id = $2")
            .bind(gid)
            .bind(team2)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(loser.get::<i64, _>("losses"), 1);

    // BEFUND (needs-decision): Group schreibt den SLOT (1) in winning_team.
    let mr =
        sqlx::query("SELECT winning_team FROM turnier.match_results WHERE group_match_id = $1")
            .bind(mid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(mr.get::<i64, _>("winning_team"), 1);
}

#[tokio::test]
async fn group_winner_id_fremd_fehler() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let gid = insert_group(&pool, tid, "Gruppe A").await;
    insert_group_team(&pool, gid, team1).await;
    insert_group_team(&pool, gid, team2).await;
    let mid = insert_group_match(&pool, gid, team1, team2, "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyGroupParams {
        winner_id: Some(99999),
        ..ApplyGroupParams::manual()
    };
    let err = mgr
        .apply_group_match_result(tid, mid, params)
        .await
        .unwrap_err();
    assert!(matches!(err, turnier_match::MatchError::Invalid(_)));
}

#[allow(clippy::too_many_arguments)]
async fn insert_mini_group_match(
    pool: &turnier_db::Pool,
    tournament_id: i64,
    mini_group_id: i64,
    position: i64,
    team1_id: i64,
    team2_id: i64,
    winner_id: Option<i64>,
    status: &str,
    match_stats: Option<Value>,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.bracket_matches \
             (tournament_id, round, position, bracket_type, mini_group_id, \
              team1_id, team2_id, winner_id, status, match_stats, on_stream) \
         VALUES ($1, 1, $2, 'winners', $3, $4, $5, $6, $7, $8::jsonb, false) \
         RETURNING id",
    )
    .bind(tournament_id)
    .bind(position)
    .bind(mini_group_id)
    .bind(team1_id)
    .bind(team2_id)
    .bind(winner_id)
    .bind(status)
    .bind(match_stats)
    .fetch_one(pool)
    .await
    .expect("insert mini-group match")
}

#[tokio::test]
async fn bracket_player_stats_werden_persistiert() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let players = vec![json!({ "hero": "Abrams", "kills": 5, "deaths": 1, "assists": 3 })];
    let params = ApplyBracketParams {
        winner_id: Some(team1),
        duration_s: Some(615),
        players: Some(players.clone()),
        ..ApplyBracketParams::automatic()
    };
    let outcome = mgr
        .apply_bracket_match_result(tid, mid, params)
        .await
        .expect("apply");
    assert_eq!(outcome.players.len(), 1);
    assert_eq!(outcome.duration_s, Some(615));

    let stats: Option<serde_json::Value> =
        sqlx::query("SELECT match_stats FROM turnier.bracket_matches WHERE id = $1")
            .bind(mid)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("match_stats");
    assert_eq!(stats.unwrap()[0]["hero"], "Abrams");
}
