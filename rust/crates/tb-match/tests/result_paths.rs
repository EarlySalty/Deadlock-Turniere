//! Integrationstests der Ergebnisverarbeitung (Bracket + Group) gegen eine
//! migrierte Temp-SQLite. Belegt insbesondere die bewusst erhaltenen Befunde:
//! Bracket schreibt winner_id in `match_results.winning_team`, Group den Slot.

mod common;

use common::*;
use serde_json::json;
use sqlx::Row;
use tb_match::{ApplyBracketParams, ApplyGroupParams};

#[tokio::test]
async fn bracket_ergebnis_setzt_winner_und_status() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let params =
        ApplyBracketParams { winner_id: Some(team2), ..ApplyBracketParams::automatic() };
    let outcome = mgr.apply_bracket_match_result(tid, mid, params).await.expect("apply");
    assert_eq!(outcome.winner_id, team2);
    // Bracket-Konvention 0-basiert: team2 → 1.
    assert_eq!(outcome.winning_team, 1);

    // Match auf completed + winner_id gesetzt.
    let row = sqlx::query("SELECT winner_id, status FROM bracket_matches WHERE id = ?")
        .bind(mid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("winner_id"), team2);
    assert_eq!(row.get::<String, _>("status"), "completed");

    // BEFUND (needs-decision): match_results.winning_team enthält die winner_id,
    // NICHT den Slot.
    let mr = sqlx::query("SELECT winning_team, source FROM match_results WHERE bracket_match_id = ?")
        .bind(mid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mr.get::<i64, _>("winning_team"), team2);
    assert_eq!(mr.get::<String, _>("source"), "automatic");
}

#[tokio::test]
async fn bracket_winning_team_0_waehlt_team1() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams { winning_team: Some(0), ..ApplyBracketParams::automatic() };
    let outcome = mgr.apply_bracket_match_result(tid, mid, params).await.expect("apply");
    assert_eq!(outcome.winner_id, team1);
    assert_eq!(outcome.winning_team, 0);
}

#[tokio::test]
async fn bracket_propagiert_in_folge_match() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let semi = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    // Folge-Match nimmt den Sieger über source_match1_id auf Slot team1.
    let finale = insert_bracket_match(&pool, tid, 2, 0, None, None, "pending").await;
    sqlx::query("UPDATE bracket_matches SET source_match1_id = ? WHERE id = ?")
        .bind(semi)
        .bind(finale)
        .execute(&pool)
        .await
        .unwrap();
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams { winner_id: Some(team1), ..ApplyBracketParams::automatic() };
    mgr.apply_bracket_match_result(tid, semi, params).await.expect("apply");

    let team1_in_final =
        sqlx::query("SELECT team1_id FROM bracket_matches WHERE id = ?")
            .bind(finale)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<Option<i64>, _>("team1_id");
    assert_eq!(team1_in_final, Some(team1));
}

#[tokio::test]
async fn bracket_terminal_ohne_force_fehler() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "completed").await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams { winner_id: Some(team1), ..ApplyBracketParams::automatic() };
    let err = mgr.apply_bracket_match_result(tid, mid, params).await.unwrap_err();
    assert!(matches!(err, tb_match::MatchError::State(_)));
}

#[tokio::test]
async fn group_ergebnis_setzt_slot_und_tabelle() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let gid = insert_group(&pool, tid, "Gruppe A").await;
    insert_group_team(&pool, gid, team1).await;
    insert_group_team(&pool, gid, team2).await;
    let mid = insert_group_match(&pool, gid, team1, team2, "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    // winning_team=1 (1-basiert!) → team1 gewinnt.
    let params = ApplyGroupParams { winning_team: Some(1), ..ApplyGroupParams::manual() };
    let outcome = mgr.apply_group_match_result(tid, mid, params).await.expect("apply");
    assert_eq!(outcome.winner_id, team1);
    assert_eq!(outcome.winning_team, 1);

    // group_teams: Sieger +1 win +3 points, Verlierer +1 loss.
    let winner = sqlx::query("SELECT wins, points, losses FROM group_teams WHERE group_id = ? AND team_id = ?")
        .bind(gid)
        .bind(team1)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(winner.get::<i64, _>("wins"), 1);
    assert_eq!(winner.get::<i64, _>("points"), 3);
    let loser = sqlx::query("SELECT losses FROM group_teams WHERE group_id = ? AND team_id = ?")
        .bind(gid)
        .bind(team2)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(loser.get::<i64, _>("losses"), 1);

    // BEFUND (needs-decision): Group schreibt den SLOT (1) in winning_team.
    let mr = sqlx::query("SELECT winning_team FROM match_results WHERE group_match_id = ?")
        .bind(mid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mr.get::<i64, _>("winning_team"), 1);
}

#[tokio::test]
async fn group_winner_id_fremd_fehler() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let gid = insert_group(&pool, tid, "Gruppe A").await;
    insert_group_team(&pool, gid, team1).await;
    insert_group_team(&pool, gid, team2).await;
    let mid = insert_group_match(&pool, gid, team1, team2, "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyGroupParams { winner_id: Some(99999), ..ApplyGroupParams::manual() };
    let err = mgr.apply_group_match_result(tid, mid, params).await.unwrap_err();
    assert!(matches!(err, tb_match::MatchError::Invalid(_)));
}

#[tokio::test]
async fn bracket_player_stats_werden_persistiert() {
    let pool = temp_pool().await;
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
    let outcome = mgr.apply_bracket_match_result(tid, mid, params).await.expect("apply");
    assert_eq!(outcome.players.len(), 1);
    assert_eq!(outcome.duration_s, Some(615));

    let stats: Option<String> =
        sqlx::query("SELECT match_stats FROM bracket_matches WHERE id = ?")
            .bind(mid)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("match_stats");
    assert!(stats.unwrap().contains("Abrams"));
}
