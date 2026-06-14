//! Integrationstests für die Serien-Logik (`match_games`) und die Lobby-/
//! Result-Status-Guards.

mod common;

use common::*;
use tb_match::{ApplyBracketParams, GameStats, SteamTaskError};

#[tokio::test]
async fn serie_bo3_entscheidet_nach_zwei_siegen() {
    let pool = temp_pool().await;
    let tid = sqlx::query(
        "INSERT INTO tournaments (name, created_by, series_format) VALUES ('Bo3', 'x', 3)",
    )
    .execute(&pool)
    .await
    .unwrap()
    .last_insert_rowid();
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    // Spiel 1: Team 1 gewinnt.
    let out1 = mgr.record_game_result(mid, 1, 1, &GameStats::default()).await.unwrap();
    assert!(!out1.series_done);
    assert_eq!(out1.wins_team1, 1);
    assert_eq!(out1.next_game_number, Some(2));

    // Spiel 2: Team 1 gewinnt erneut → Serie entschieden (Bo3 → 2 Siege).
    let out2 = mgr.record_game_result(mid, 2, 1, &GameStats::default()).await.unwrap();
    assert!(out2.series_done);
    assert_eq!(out2.series_winner_team, Some(1));
    assert_eq!(out2.wins_team1, 2);
    assert_eq!(out2.next_game_number, None);
}

#[tokio::test]
async fn serie_bo1_entscheidet_sofort() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "Bo1", false, false).await; // series_format=1
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let out = mgr.record_game_result(mid, 1, 2, &GameStats::default()).await.unwrap();
    assert!(out.series_done);
    assert_eq!(out.series_winner_team, Some(2));
}

#[tokio::test]
async fn ensure_game_exists_idempotent() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let id1 = mgr.ensure_game_exists(mid, 1).await.unwrap();
    let id2 = mgr.ensure_game_exists(mid, 1).await.unwrap();
    assert_eq!(id1, id2);
    let games = mgr.get_series_games(mid).await.unwrap();
    assert_eq!(games.len(), 1);
}

#[tokio::test]
async fn record_game_result_ungueltiges_team_fehler() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let err = mgr.record_game_result(mid, 1, 3, &GameStats::default()).await.unwrap_err();
    assert!(matches!(err, tb_match::MatchError::Invalid(_)));
}

#[tokio::test]
async fn create_lobby_lehnt_falschen_status_ab() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    // Status 'completed' → keine Lobby möglich (Guard greift VOR der Bridge).
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "completed").await;
    let mgr = manager_without_services(pool.clone());

    let err = mgr.create_lobby(tid, mid).await.unwrap_err();
    assert!(matches!(err, SteamTaskError::State(_)));
}

#[tokio::test]
async fn create_lobby_lehnt_fehlende_teams_ab() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    // team2 fehlt.
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), None, "pending").await;
    let mgr = manager_without_services(pool.clone());

    let err = mgr.create_lobby(tid, mid).await.unwrap_err();
    assert!(matches!(err, SteamTaskError::State(_)));
}

#[tokio::test]
async fn create_lobby_ohne_bridge_scheitert_an_steam_task() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    // Sauberer Status → Guards passieren, aber ohne Bridge scheitert der Task.
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "pending").await;
    let mgr = manager_without_services(pool.clone());

    let err = mgr.create_lobby(tid, mid).await.unwrap_err();
    assert!(matches!(err, SteamTaskError::Failed(_)));
}

#[tokio::test]
async fn start_match_lehnt_nicht_lobby_created_ab() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "pending").await;
    let mgr = manager_without_services(pool.clone());

    let err = mgr.start_match(tid, mid).await.unwrap_err();
    assert!(matches!(err, SteamTaskError::State(_)));
}

#[tokio::test]
async fn fetch_result_lehnt_nicht_in_progress_ab() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "lobby_created").await;
    let mgr = manager_without_services(pool.clone());

    let err = mgr.fetch_match_result(tid, mid).await.unwrap_err();
    assert!(matches!(err, SteamTaskError::State(_)));
}

#[tokio::test]
async fn auto_lobby_gated_durch_test_turnier() {
    let pool = temp_pool().await;
    // is_test=true → Auto-Lobby tut nichts (kein Fehler).
    let tid = insert_tournament(&pool, "T", true, true).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let _ = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "pending").await;
    let mgr = manager_without_services(pool.clone());

    // Darf NICHT scheitern, obwohl keine Bridge da ist (Gate greift vorher).
    mgr.schedule_auto_lobbies_for_tournament(tid).await.expect("gated no-op");
}

#[tokio::test]
async fn force_reset_setzt_downstream_zurueck() {
    let pool = temp_pool().await;
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let team3 = insert_team(&pool, tid, "Gamma").await;
    let semi = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "completed").await;
    // Sieger team1 ist bereits im Finale (Slot team1) und Finale ist gespielt.
    let finale = insert_bracket_match(&pool, tid, 2, 0, Some(team1), Some(team3), "completed").await;
    sqlx::query(
        "UPDATE bracket_matches SET source_match1_id = ?, winner_id = ? WHERE id = ?",
    )
    .bind(semi)
    .bind(team1)
    .bind(finale)
    .execute(&pool)
    .await
    .unwrap();
    // Semi hat aktuell Sieger team1.
    sqlx::query("UPDATE bracket_matches SET winner_id = ? WHERE id = ?")
        .bind(team1)
        .bind(semi)
        .execute(&pool)
        .await
        .unwrap();
    let mgr = manager_without_services(pool.clone());

    // Force-Reset mit neuem Sieger team2 → Downstream (Finale) wird zurückgesetzt.
    let params =
        ApplyBracketParams { winner_id: Some(team2), force: true, ..ApplyBracketParams::automatic() };
    mgr.apply_bracket_match_result(tid, semi, params).await.expect("force apply");

    use sqlx::Row;
    let finale_row = sqlx::query("SELECT team1_id, status, winner_id FROM bracket_matches WHERE id = ?")
        .bind(finale)
        .fetch_one(&pool)
        .await
        .unwrap();
    // Slot team1 zurückgesetzt (NULL), dann durch advance neu mit team2 befüllt.
    assert_eq!(finale_row.get::<String, _>("status"), "pending");
    assert!(finale_row.get::<Option<i64>, _>("winner_id").is_none());
    // advance_bracket_winner schreibt den neuen Sieger team2 in den Slot.
    assert_eq!(finale_row.get::<Option<i64>, _>("team1_id"), Some(team2));
}
