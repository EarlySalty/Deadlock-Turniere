//! Integrationstests für die Serien-Logik (`match_games`) und die Lobby-/
//! Result-Status-Guards.

mod common;

use common::*;
use turnier_match::{repo, ApplyBracketParams, GameStats, MatchKind, SteamTaskError};

#[tokio::test]
async fn serie_bo3_entscheidet_nach_zwei_siegen() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament_with_series(&pool, "Bo3", false, false, 3).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    // Spiel 1: Team 1 gewinnt.
    let out1 = mgr
        .record_game_result(mid, 1, 1, &GameStats::default())
        .await
        .unwrap();
    assert!(!out1.series_done);
    assert_eq!(out1.wins_team1, 1);
    assert_eq!(out1.next_game_number, Some(2));

    // Spiel 2: Team 1 gewinnt erneut → Serie entschieden (Bo3 → 2 Siege).
    let out2 = mgr
        .record_game_result(mid, 2, 1, &GameStats::default())
        .await
        .unwrap();
    assert!(out2.series_done);
    assert_eq!(out2.series_winner_team, Some(1));
    assert_eq!(out2.wins_team1, 2);
    assert_eq!(out2.next_game_number, None);
}

#[tokio::test]
async fn serie_bo1_entscheidet_sofort() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "Bo1", false, false).await; // series_format=1
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let out = mgr
        .record_game_result(mid, 1, 2, &GameStats::default())
        .await
        .unwrap();
    assert!(out.series_done);
    assert_eq!(out.series_winner_team, Some(2));
}

#[tokio::test]
async fn ensure_game_exists_idempotent() {
    let db = temp_db().await;
    let pool = db.pool().clone();
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
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "in_progress").await;
    let mgr = manager_without_services(pool.clone());

    let err = mgr
        .record_game_result(mid, 1, 3, &GameStats::default())
        .await
        .unwrap_err();
    assert!(matches!(err, turnier_match::MatchError::Invalid(_)));
}

#[tokio::test]
async fn create_lobby_lehnt_falschen_status_ab() {
    let db = temp_db().await;
    let pool = db.pool().clone();
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
    let db = temp_db().await;
    let pool = db.pool().clone();
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
    let db = temp_db().await;
    let pool = db.pool().clone();
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
    let db = temp_db().await;
    let pool = db.pool().clone();
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
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid =
        insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "lobby_created").await;
    let mgr = manager_without_services(pool.clone());

    let err = mgr.fetch_match_result(tid, mid).await.unwrap_err();
    assert!(matches!(err, SteamTaskError::State(_)));
}

#[tokio::test]
async fn auto_lobby_gated_durch_test_turnier() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    // is_test=true → Auto-Lobby tut nichts (kein Fehler).
    let tid = insert_tournament(&pool, "T", true, true).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let _ = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "pending").await;
    let mgr = manager_without_services(pool.clone());

    // Darf NICHT scheitern, obwohl keine Bridge da ist (Gate greift vorher).
    mgr.schedule_auto_lobbies_for_tournament(tid)
        .await
        .expect("gated no-op");
}

#[tokio::test]
async fn force_reset_setzt_downstream_zurueck() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let team3 = insert_team(&pool, tid, "Gamma").await;
    let semi = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "completed").await;
    // Sieger team1 ist bereits im Finale (Slot team1) und Finale ist gespielt.
    let finale =
        insert_bracket_match(&pool, tid, 2, 0, Some(team1), Some(team3), "completed").await;
    sqlx::query(
        "UPDATE turnier.bracket_matches SET source_match1_id = $1, winner_id = $2 WHERE id = $3",
    )
    .bind(semi)
    .bind(team1)
    .bind(finale)
    .execute(&pool)
    .await
    .unwrap();
    // Semi hat aktuell Sieger team1.
    sqlx::query("UPDATE turnier.bracket_matches SET winner_id = $1 WHERE id = $2")
        .bind(team1)
        .bind(semi)
        .execute(&pool)
        .await
        .unwrap();
    let mgr = manager_without_services(pool.clone());

    // Force-Reset mit neuem Sieger team2 → Downstream (Finale) wird zurückgesetzt.
    let params = ApplyBracketParams {
        winner_id: Some(team2),
        force: true,
        ..ApplyBracketParams::automatic()
    };
    mgr.apply_bracket_match_result(tid, semi, params)
        .await
        .expect("force apply");

    use sqlx::Row;
    let finale_row = sqlx::query(
        "SELECT team1_id, status, winner_id FROM turnier.bracket_matches WHERE id = $1",
    )
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

async fn exec(pool: &turnier_db::Pool, sql: &str, binds: &[i64]) {
    let mut query = sqlx::query(sql);
    for value in binds {
        query = query.bind(*value);
    }
    query.execute(pool).await.expect("fixture update");
}

async fn match_state(
    pool: &turnier_db::Pool,
    id: i64,
) -> (Option<i64>, Option<i64>, Option<i64>, String) {
    sqlx::query_as(
        "SELECT team1_id, team2_id, winner_id, status FROM turnier.bracket_matches WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("match state")
}

const SET_WINNER: &str = "UPDATE turnier.bracket_matches SET winner_id = $1 WHERE id = $2";

#[tokio::test]
async fn reset_of_completed_semifinal_clears_played_final() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let team3 = insert_team(&pool, tid, "Gamma").await;
    let semi = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "completed").await;
    let finale =
        insert_bracket_match(&pool, tid, 2, 0, Some(team1), Some(team3), "completed").await;
    exec(&pool, SET_WINNER, &[team1, semi]).await;
    exec(&pool, SET_WINNER, &[team1, finale]).await;
    exec(
        &pool,
        "UPDATE turnier.bracket_matches SET source_match1_id = $1 WHERE id = $2",
        &[semi, finale],
    )
    .await;
    turnier_match::series::ensure_game_exists(&pool, semi, 1)
        .await
        .expect("semi game");
    turnier_match::series::ensure_game_exists(&pool, finale, 1)
        .await
        .expect("final game");
    let mgr = manager_without_services(pool.clone());

    mgr.reset_bracket_match_result(tid, semi)
        .await
        .expect("reset completed semi");

    let games: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.match_games WHERE bracket_match_id = ANY($1)",
    )
    .bind(vec![semi, finale])
    .fetch_one(&pool)
    .await
    .expect("count games");
    assert_eq!(games, 0);

    assert_eq!(
        match_state(&pool, semi).await,
        (Some(team1), Some(team2), None, "pending".to_string())
    );
    assert_eq!(
        match_state(&pool, finale).await,
        (None, Some(team3), None, "pending".to_string())
    );
    assert!(mgr.reset_bracket_match_result(tid, semi).await.is_err());
}

#[tokio::test]
async fn force_correction_moves_loser_in_losers_bracket() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let team3 = insert_team(&pool, tid, "Gamma").await;
    let winners =
        insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "completed").await;
    let losers =
        insert_bracket_match(&pool, tid, 1, 0, Some(team3), Some(team2), "completed").await;
    let losers_next = insert_bracket_match(&pool, tid, 2, 0, Some(team3), None, "pending").await;
    let winners_next = insert_bracket_match(&pool, tid, 2, 0, Some(team1), None, "pending").await;
    exec(
        &pool,
        "UPDATE turnier.bracket_matches SET source_match1_id = $1 WHERE id = $2",
        &[winners, winners_next],
    )
    .await;
    exec(&pool, SET_WINNER, &[team1, winners]).await;
    exec(&pool, SET_WINNER, &[team3, losers]).await;
    exec(
        &pool,
        "UPDATE turnier.bracket_matches SET bracket_type = 'losers' WHERE id IN ($1, $2)",
        &[losers, losers_next],
    )
    .await;
    exec(
        &pool,
        "UPDATE turnier.bracket_matches SET loser_to_match_id = $1, loser_to_slot = 2 WHERE id = $2",
        &[losers, winners],
    )
    .await;
    exec(
        &pool,
        "UPDATE turnier.bracket_matches SET source_match1_id = $1 WHERE id = $2",
        &[losers, losers_next],
    )
    .await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams {
        winner_id: Some(team2),
        force: true,
        ..ApplyBracketParams::automatic()
    };
    mgr.apply_bracket_match_result(tid, winners, params)
        .await
        .expect("force correction");

    assert_eq!(
        match_state(&pool, losers).await,
        (Some(team3), Some(team1), None, "pending".to_string())
    );
    assert_eq!(
        match_state(&pool, losers_next).await,
        (None, None, None, "pending".to_string())
    );
    assert_eq!(
        match_state(&pool, winners_next).await,
        (Some(team2), None, None, "pending".to_string())
    );
}

#[tokio::test]
async fn caster_laden_nutzt_match_fallback_wenn_turnier_liste_leer_ist() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let mid = insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "pending").await;
    insert_match_caster(&pool, mid, "bracket", TEST_FALLBACK_CASTER_ID).await;

    let casters = repo::load_match_casters(&pool, MatchKind::Bracket, mid)
        .await
        .expect("load casters");
    assert_eq!(casters, vec![TEST_FALLBACK_CASTER_ID.to_string()]);

    insert_tournament_caster(&pool, tid, TEST_CASTER_ID).await;
    let casters = repo::load_match_casters(&pool, MatchKind::Bracket, mid)
        .await
        .expect("load tournament casters");
    assert_eq!(casters, vec![TEST_CASTER_ID.to_string()]);
}

async fn insert_mini_group(pool: &turnier_db::Pool, tid: i64) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.bracket_mini_groups (tournament_id, round, position, created_at) \
         VALUES ($1, 1, 2, now()) RETURNING id",
    )
    .bind(tid)
    .fetch_one(pool)
    .await
    .expect("insert mini group")
}

async fn insert_mini_match(
    pool: &turnier_db::Pool,
    tid: i64,
    mini_group_id: i64,
    position: i64,
    teams: (i64, i64),
    status: &str,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.bracket_matches \
             (tournament_id, round, position, bracket_type, mini_group_id, team1_id, team2_id, status, on_stream) \
         VALUES ($1, 1, $2, 'winners', $3, $4, $5, $6, false) RETURNING id",
    )
    .bind(tid)
    .bind(position)
    .bind(mini_group_id)
    .bind(teams.0)
    .bind(teams.1)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("insert mini match")
}

#[tokio::test]
async fn mini_group_result_leaves_next_round_slot_alone() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let team3 = insert_team(&pool, tid, "Gamma").await;
    let mini_group_id = insert_mini_group(&pool, tid).await;
    let mini = insert_mini_match(&pool, tid, mini_group_id, 2, (team2, team3), "in_progress").await;
    let next = insert_bracket_match(&pool, tid, 2, 1, Some(team1), None, "pending").await;
    let mgr = manager_without_services(pool.clone());

    let params = ApplyBracketParams {
        winner_id: Some(team2),
        ..ApplyBracketParams::automatic()
    };
    mgr.apply_bracket_match_result(tid, mini, params)
        .await
        .expect("mini result");

    assert_eq!(
        match_state(&pool, next).await,
        (Some(team1), None, None, "pending".to_string())
    );
}

#[tokio::test]
async fn reset_refuses_mini_group_paths_and_live_follow_ups() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let tid = insert_tournament(&pool, "T", false, false).await;
    let team1 = insert_team(&pool, tid, "Alpha").await;
    let team2 = insert_team(&pool, tid, "Beta").await;
    let team3 = insert_team(&pool, tid, "Gamma").await;
    let mini_group_id = insert_mini_group(&pool, tid).await;
    let mini = insert_mini_match(&pool, tid, mini_group_id, 2, (team2, team3), "completed").await;
    exec(&pool, SET_WINNER, &[team2, mini]).await;
    let feeder =
        insert_bracket_match(&pool, tid, 1, 0, Some(team1), Some(team2), "completed").await;
    exec(&pool, SET_WINNER, &[team1, feeder]).await;
    let fed_mini = insert_mini_match(&pool, tid, mini_group_id, 3, (team1, team3), "pending").await;
    exec(
        &pool,
        "UPDATE turnier.bracket_matches SET source_match1_id = $1 WHERE id = $2",
        &[feeder, fed_mini],
    )
    .await;
    let semi = insert_bracket_match(&pool, tid, 1, 1, Some(team1), Some(team3), "completed").await;
    exec(&pool, SET_WINNER, &[team3, semi]).await;
    let finale =
        insert_bracket_match(&pool, tid, 2, 0, Some(team3), Some(team2), "in_progress").await;
    exec(
        &pool,
        "UPDATE turnier.bracket_matches SET source_match1_id = $1 WHERE id = $2",
        &[semi, finale],
    )
    .await;
    let mgr = manager_without_services(pool.clone());

    for id in [mini, feeder, semi] {
        assert!(mgr.reset_bracket_match_result(tid, id).await.is_err());
        assert_eq!(match_state(&pool, id).await.3, "completed");
    }
    let correction = ApplyBracketParams {
        winner_id: Some(team1),
        force: true,
        ..ApplyBracketParams::automatic()
    };
    assert!(mgr
        .apply_bracket_match_result(tid, semi, correction)
        .await
        .is_err());
    assert_eq!(
        match_state(&pool, finale).await,
        (Some(team3), Some(team2), None, "in_progress".to_string())
    );

    exec(
        &pool,
        "UPDATE turnier.tournaments SET status = 'completed' WHERE id = $1",
        &[tid],
    )
    .await;
    exec(
        &pool,
        "UPDATE turnier.bracket_matches SET status = 'completed', winner_id = $1 WHERE id = $2",
        &[team3, finale],
    )
    .await;
    assert!(mgr.reset_bracket_match_result(tid, semi).await.is_err());
    assert_eq!(match_state(&pool, semi).await.3, "completed");
}
