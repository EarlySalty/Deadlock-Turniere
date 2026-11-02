#![cfg(feature = "testing")]

mod common;

use chrono::{Duration, Utc};
use common::{
    fake_match_manager, fake_notifier, insert_team, insert_team_member, insert_tournament,
    test_config, tournament_status,
};
use turnier_scheduler::{
    advance_tournament_status, check_and_send_match_reminders, get_due_next_status, DueStatusRow,
    SchedulerError,
};

const CAPTAIN: i64 = 123_456_789_012_345_900;

#[test]
fn league_never_advances_to_a_bracket_by_time() {
    let now = Utc::now();
    let earlier = Some(now - Duration::hours(1));
    let league = DueStatusRow {
        status: "group_phase".to_string(),
        tournament_mode: Some("league".to_string()),
        bracket_start: earlier,
        ..Default::default()
    };
    assert_eq!(get_due_next_status(&league, now), None);

    let group_stage = DueStatusRow {
        tournament_mode: Some("group_stage".to_string()),
        ..league.clone()
    };
    assert_eq!(get_due_next_status(&group_stage, now), Some("bracket"));

    let checkin = DueStatusRow {
        status: "checkin".to_string(),
        tournament_mode: Some("league".to_string()),
        group_phase_start: earlier,
        ..Default::default()
    };
    assert_eq!(get_due_next_status(&checkin, now), Some("group_phase"));
}

#[tokio::test]
async fn league_goes_from_checkin_to_plan_to_completed_without_bracket() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "Liga", "checkin", true).await;
    sqlx::query("UPDATE turnier.tournaments SET tournament_mode = 'league' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("mode");
    for index in 0..10 {
        insert_team(&pool, id, &format!("Team {index}"), CAPTAIN + index).await;
    }

    let meta = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "checkin",
        "group_phase",
        "scheduler",
        None,
    )
    .await
    .expect("plan");
    assert_eq!(meta["matches_created"], 45);
    assert_eq!(tournament_status(&pool, id).await, "group_phase");

    let refused_bracket = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "group_phase",
        "bracket",
        "manual",
        None,
    )
    .await;
    assert!(matches!(
        refused_bracket,
        Err(SchedulerError::InvalidTransition(_))
    ));

    let refused_finish = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "group_phase",
        "completed",
        "manual",
        None,
    )
    .await;
    match refused_finish {
        Err(SchedulerError::InvalidTransition(message)) => {
            assert!(message.contains("Offen: 45"), "{message}");
        }
        other => panic!("Abschluss muss abgelehnt werden: {other:?}"),
    }
    assert_eq!(tournament_status(&pool, id).await, "group_phase");

    sqlx::query(
        "UPDATE turnier.group_matches SET status = 'completed', winner_id = team1_id \
         WHERE group_id IN (SELECT id FROM turnier.groups WHERE tournament_id = $1)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .expect("finish all");

    advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "group_phase",
        "completed",
        "manual",
        None,
    )
    .await
    .expect("finish");
    assert_eq!(tournament_status(&pool, id).await, "completed");
    let bracket_matches: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnier.bracket_matches WHERE tournament_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(bracket_matches, 0);
}

#[tokio::test]
async fn first_round_gets_its_reminder_after_lobbies_opened_at_the_transition() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "Liga", "checkin", false).await;
    sqlx::query(
        "UPDATE turnier.tournaments SET tournament_mode = 'league', group_phase_start = $1 \
         WHERE id = $2",
    )
    .bind(Utc::now() - Duration::minutes(1))
    .bind(id)
    .execute(&pool)
    .await
    .expect("mode");
    for index in 0..10 {
        let team = insert_team(&pool, id, &format!("Team {index}"), CAPTAIN + index).await;
        insert_team_member(&pool, team, CAPTAIN + index).await;
    }
    advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "checkin",
        "group_phase",
        "scheduler",
        None,
    )
    .await
    .expect("plan");
    sqlx::query(
        "UPDATE turnier.group_matches SET status = 'lobby_created', steam_party_id = 'party' \
         WHERE scheduled_at = (SELECT group_phase_start FROM turnier.tournaments WHERE id = $1) \
           AND group_id IN (SELECT id FROM turnier.groups WHERE tournament_id = $1)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .expect("lobbies");

    for _ in 0..2 {
        check_and_send_match_reminders(&pool, &notifier)
            .await
            .expect("reminders");
        let sent: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM turnier.sent_match_reminders WHERE match_type = 'group'",
        )
        .fetch_one(&pool)
        .await
        .expect("reminder count");
        assert_eq!(sent, 5);
    }
}
