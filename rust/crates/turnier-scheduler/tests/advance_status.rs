//! Integrationstests für [`advance_tournament_status`] mit Temp-SQLite.
//!
//! Getestet wird der Übergang OHNE Generierungs-Seiteneffekte (registration →
//! checkin) — die Generierung selbst (group_phase/bracket) ist in turnier-engine
//! abgedeckt. Discord/Match sind No-op-Fakes.

mod common;

use common::{
    audit_count, fake_match_manager, fake_notifier, insert_tournament, test_config, temp_pool,
    tournament_status,
};
use turnier_scheduler::{advance_tournament_status, SchedulerError};

#[tokio::test]
async fn scheduler_quelle_advanciert_und_auditet_auto_advance() {
    let pool = temp_pool().await;
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "registration", false).await;

    let meta = advance_tournament_status(
        &pool, &matchmgr, &notifier, id, "registration", "checkin", "scheduler", None,
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
    let pool = temp_pool().await;
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
        Some("user-42"),
    )
    .await
    .expect("advance ok");

    assert_eq!(audit_count(&pool, "tournament_advance").await, 1);
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 0);
    let (user_id,): (Option<String>,) =
        sqlx::query_as("SELECT user_id FROM audit_log WHERE action = 'tournament_advance'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user_id.as_deref(), Some("user-42"));
}

#[tokio::test]
async fn ungueltiger_uebergang_wirft_und_aendert_nichts() {
    let pool = temp_pool().await;
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "registration", false).await;

    // registration → bracket ist nicht erlaubt (nur registration → checkin).
    let err = advance_tournament_status(
        &pool, &matchmgr, &notifier, id, "registration", "bracket", "scheduler", None,
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
    let pool = temp_pool().await;
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    // Turnier ist tatsächlich in 'checkin'.
    let id = insert_tournament(&pool, "T", "checkin", false).await;

    // Aufrufer glaubt, es sei noch 'registration' → der Übergang
    // registration→checkin ist gültig, aber das WHERE status='registration'
    // trifft keine Zeile → StatusConflict.
    let err = advance_tournament_status(
        &pool, &matchmgr, &notifier, id, "registration", "checkin", "scheduler", None,
    )
    .await
    .expect_err("muss konfligieren");

    assert!(matches!(err, SchedulerError::StatusConflict));
    // Status unverändert.
    assert_eq!(tournament_status(&pool, id).await, "checkin");
}
