//! Integrationstests für [`advance_tournament_status`] mit zentraler
//! Wegwerf-PG-DB.
//!
//! Getestet wird der Übergang OHNE Generierungs-Seiteneffekte (registration →
//! checkin) — die Generierung selbst (group_phase/bracket) ist in turnier-engine
//! abgedeckt. Discord/Match sind No-op-Fakes.

mod common;

use common::{
    audit_count, fake_match_manager, fake_notifier, insert_tournament, temp_db, test_config,
    tournament_status,
};
use turnier_core::now_utc;
use turnier_scheduler::{advance_tournament_status, SchedulerError};

#[tokio::test]
async fn scheduler_quelle_advanciert_und_auditet_auto_advance() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "registration", false).await;

    let meta = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "registration",
        "checkin",
        "scheduler",
        None,
    )
    .await
    .expect("advance ok");

    assert_eq!(tournament_status(&pool, id).await, "checkin");
    assert_eq!(meta["from"], "registration");
    assert_eq!(meta["to"], "checkin");
    assert_eq!(meta["source"], "scheduler");
    assert_eq!(meta["tournament_id"], id);
    // scheduler-Quelle → tournament_auto_advance.
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 1);
    assert_eq!(audit_count(&pool, "tournament_advance").await, 0);
}

#[tokio::test]
async fn admin_quelle_auditet_tournament_advance_mit_actor() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "registration", false).await;

    advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "registration",
        "checkin",
        "admin",
        Some("123456789012345710"),
    )
    .await
    .expect("advance ok");

    assert_eq!(audit_count(&pool, "tournament_advance").await, 1);
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 0);
    let (user_id,): (Option<i64>,) =
        sqlx::query_as("SELECT user_id FROM turnier.audit_log WHERE action = 'tournament_advance'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user_id, Some(123456789012345710));
}

#[tokio::test]
async fn ungueltiger_uebergang_wirft_und_aendert_nichts() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "registration", false).await;

    // registration → bracket ist nicht erlaubt (nur registration → checkin).
    let err = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "registration",
        "bracket",
        "scheduler",
        None,
    )
    .await
    .expect_err("muss fehlschlagen");

    assert!(matches!(err, SchedulerError::InvalidTransition(_)));
    assert!(err.to_string().contains("Ungültiger Status-Übergang"));
    // Status unverändert, kein Audit.
    assert_eq!(tournament_status(&pool, id).await, "registration");
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 0);
}

#[tokio::test]
async fn optimistic_lock_konflikt_bei_falschem_current_status() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    // Turnier ist tatsächlich in 'checkin'.
    let id = insert_tournament(&pool, "T", "checkin", false).await;

    // Aufrufer glaubt, es sei noch 'registration' → der Übergang
    // registration→checkin ist gültig, aber das WHERE status='registration'
    // trifft keine Zeile → StatusConflict.
    let err = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "registration",
        "checkin",
        "scheduler",
        None,
    )
    .await
    .expect_err("muss konfligieren");

    assert!(matches!(err, SchedulerError::StatusConflict));
    // Status unverändert.
    assert_eq!(tournament_status(&pool, id).await, "checkin");
}

async fn seed_two_team_completed_bracket(pool: &turnier_db::Pool, tournament_id: i64) {
    let mut team_ids = Vec::new();
    for team_number in 1..=2 {
        let team_id: i64 = sqlx::query_scalar(
            "INSERT INTO turnier.teams \
                 (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status) \
             VALUES ($1, $2, $3, $4, $5, 'open') RETURNING id",
        )
            .bind(tournament_id)
            .bind(format!("Team {team_number}"))
            .bind(format!("team-{team_number}"))
            .bind(123456789012345800_i64 + team_number)
            .bind(now_utc())
            .fetch_one(pool)
            .await
            .expect("insert team");
        team_ids.push(team_id);

        sqlx::query(
            "INSERT INTO turnier.team_members (team_id, discord_id, role, joined_at) \
             VALUES ($1, $2, 'member', $3)",
        )
        .bind(team_id)
        .bind(123456789012345900_i64 + team_number)
        .bind(now_utc())
        .execute(pool)
        .await
        .expect("insert member");
    }

    sqlx::query(
        "INSERT INTO turnier.bracket_matches \
         (tournament_id, round, position, bracket_type, team1_id, team2_id, winner_id, status, on_stream) \
         VALUES ($1, 1, 0, 'winners', $2, $3, $2, 'completed', false)",
    )
    .bind(tournament_id)
    .bind(team_ids[0])
    .bind(team_ids[1])
    .execute(pool)
    .await
    .expect("insert completed match");
}

#[tokio::test]
async fn completed_transition_setzt_status_und_rechnet_punkte() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "bracket", false).await;
    seed_two_team_completed_bracket(&pool, id).await;

    advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "bracket",
        "completed",
        "scheduler",
        None,
    )
    .await
    .expect("advance ok");

    assert_eq!(tournament_status(&pool, id).await, "completed");
    let points: Vec<(i64, i64, i64, i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT discord_id, total_points, tournaments_played, matches_played, matches_won, best_placement \
         FROM turnier.player_points ORDER BY discord_id",
    )
    .fetch_all(&pool)
    .await
    .expect("points");
    assert_eq!(
        points,
        vec![
            (123456789012345901, 11, 1, 1, 1, Some(1)),
            (123456789012345902, 7, 1, 1, 0, Some(2)),
        ]
    );
}

#[tokio::test]
async fn completed_transition_rollt_status_und_audit_bei_recompute_fehler_zurueck() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "bracket", false).await;
    seed_two_team_completed_bracket(&pool, id).await;

    sqlx::query(
        "INSERT INTO turnier.player_points \
         (discord_id, total_points, tournaments_played, matches_played, matches_won, updated_at) \
         VALUES ($1, 1, 1, 1, 1, $2)",
    )
    .bind(123456789012345999_i64)
    .bind(now_utc())
    .execute(&pool)
    .await
    .expect("insert sentinel points");
    sqlx::query(
        "CREATE OR REPLACE FUNCTION turnier.fail_player_points_delete() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forced recompute failure'; END; $$",
    )
    .execute(&pool)
    .await
    .expect("create failing function");
    sqlx::query(
        "CREATE TRIGGER fail_player_points_delete BEFORE DELETE ON turnier.player_points \
         FOR EACH STATEMENT EXECUTE FUNCTION turnier.fail_player_points_delete()",
    )
    .execute(&pool)
    .await
    .expect("create failing trigger");

    advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "bracket",
        "completed",
        "scheduler",
        None,
    )
    .await
    .expect_err("recompute must fail");

    assert_eq!(tournament_status(&pool, id).await, "bracket");
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 0);

    let sentinel = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT total_points, tournaments_played, matches_played, matches_won \
         FROM turnier.player_points WHERE discord_id = $1",
    )
    .bind(123456789012345999_i64)
    .fetch_one(&pool)
    .await
    .expect("sentinel points still exist after rollback");
    assert_eq!(sentinel, (1, 1, 1, 1));
}
