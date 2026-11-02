#![cfg(feature = "testing")]

mod common;

use std::sync::Arc;

use chrono::{Duration, Utc};
use common::{
    fake_match_manager, fake_notifier, insert_team, insert_team_member, insert_tournament,
    test_config, tournament_status,
};
use sqlx::postgres::PgConnection;
use sqlx::sqlite::SqlitePoolOptions;
use turnier_engine::recalculate_player_points_in_tx;
use turnier_match::league::write_league_manual_result_in_tx;
use turnier_match::{MatchManager, SteamBridge};
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

async fn standings_and_points(pool: &turnier_db::Pool, group_id: i64) -> (String, String) {
    let mut conn = pool.acquire().await.expect("connection");
    table_and_points(&mut conn, group_id).await
}

async fn table_and_points(conn: &mut PgConnection, group_id: i64) -> (String, String) {
    let standings: String = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(to_jsonb(gt) ORDER BY gt.team_id), '[]')::text \
         FROM turnier.group_teams gt WHERE gt.group_id = $1",
    )
    .bind(group_id)
    .fetch_one(&mut *conn)
    .await
    .expect("standings");
    let points: String = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(to_jsonb(p) ORDER BY p.discord_id), '[]')::text \
         FROM turnier.player_points p",
    )
    .fetch_one(&mut *conn)
    .await
    .expect("player points");
    (standings, points)
}

#[tokio::test]
async fn result_queued_behind_the_finish_changes_nothing() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "Liga", "checkin", false).await;
    sqlx::query("UPDATE turnier.tournaments SET tournament_mode = 'league' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("mode");
    for index in 0..2 {
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
    let (match_id, group_id, team1, team2): (i64, i64, i64, i64) = sqlx::query_as(
        "UPDATE turnier.group_matches gm \
         SET status = 'in_progress', steam_party_id = 'party' \
         FROM turnier.groups g \
         WHERE g.id = gm.group_id AND g.tournament_id = $1 \
         RETURNING gm.id, gm.group_id, gm.team1_id, gm.team2_id",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .expect("running match");

    let mut finish = pool.begin().await.expect("begin");
    write_league_manual_result_in_tx(&mut finish, match_id, group_id, team2, 2)
        .await
        .expect("correction");
    sqlx::query("UPDATE turnier.tournaments SET status = 'completed' WHERE id = $1")
        .bind(id)
        .execute(&mut *finish)
        .await
        .expect("finish");
    recalculate_player_points_in_tx(&mut finish, id)
        .await
        .expect("points");
    let finished = table_and_points(&mut finish, group_id).await;
    assert_ne!(finished.1, "[]");

    let manager = matchmgr.clone();
    let apply = tokio::spawn(async move {
        manager
            .apply_group_match_result(
                id,
                match_id,
                turnier_match::ApplyGroupParams {
                    winner_id: Some(team1),
                    source: "automatic".to_string(),
                    ..Default::default()
                },
            )
            .await
    });
    let mut waiting = 0_i64;
    for _ in 0..1000 {
        waiting = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_stat_activity \
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(&pool)
        .await
        .expect("lock waiters");
        if waiting > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(waiting > 0, "Das Ergebnis muss auf den Abschluss warten");
    finish.commit().await.expect("finish commit");

    let applied = apply.await.expect("apply task");
    assert!(applied.is_err(), "{applied:?}");
    assert_eq!(standings_and_points(&pool, group_id).await, finished);
    let winner: Option<i64> =
        sqlx::query_scalar("SELECT winner_id FROM turnier.group_matches WHERE id = $1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .expect("winner");
    assert_eq!(winner, Some(team2));
}

#[tokio::test]
async fn result_reply_after_the_league_finished_changes_nothing() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let id = insert_tournament(&pool, "Liga", "checkin", false).await;
    sqlx::query("UPDATE turnier.tournaments SET tournament_mode = 'league' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("mode");
    for index in 0..2 {
        let team = insert_team(&pool, id, &format!("Team {index}"), CAPTAIN + index).await;
        insert_team_member(&pool, team, CAPTAIN + index).await;
    }
    let steam = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect("sqlite::memory:")
        .await
        .expect("steam bridge");
    sqlx::query(
        "CREATE TABLE steam_tasks (\
             id INTEGER PRIMARY KEY AUTOINCREMENT, type TEXT, payload TEXT, status TEXT, \
             result TEXT, error TEXT, created_at INTEGER, updated_at INTEGER, \
             started_at INTEGER, finished_at INTEGER, attempts INTEGER DEFAULT 0)",
    )
    .execute(&steam)
    .await
    .expect("steam task schema");
    let matchmgr = Arc::new(MatchManager::new(
        pool.clone(),
        None,
        Some(SteamBridge::from_pool(steam.clone())),
        &config,
    ));
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
    let (match_id, group_id, team1, team2): (i64, i64, i64, i64) = sqlx::query_as(
        "UPDATE turnier.group_matches gm \
         SET status = 'in_progress', steam_party_id = 'party' \
         FROM turnier.groups g \
         WHERE g.id = gm.group_id AND g.tournament_id = $1 \
         RETURNING gm.id, gm.group_id, gm.team1_id, gm.team2_id",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .expect("running match");

    let correct_and_finish = async {
        let task_id = loop {
            let task: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM steam_tasks WHERE type = 'GC_GET_MATCH_RESULT' \
                 AND status = 'PENDING'",
            )
            .fetch_optional(&steam)
            .await
            .expect("task lookup");
            if let Some(task) = task {
                break task;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        let mut tx = pool.begin().await.expect("begin");
        write_league_manual_result_in_tx(&mut tx, match_id, group_id, team2, 2)
            .await
            .expect("correction");
        tx.commit().await.expect("commit");
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
        let after_finish = standings_and_points(&pool, group_id).await;
        sqlx::query("UPDATE steam_tasks SET status = 'DONE', result = ? WHERE id = ?")
            .bind(serde_json::json!({ "winner_id": team1 }).to_string())
            .bind(task_id)
            .execute(&steam)
            .await
            .expect("task reply");
        after_finish
    };

    let (reply, after_finish) = tokio::join!(
        matchmgr.fetch_group_match_result(id, match_id),
        correct_and_finish,
    );

    assert!(reply.is_err(), "{reply:?}");
    assert_eq!(tournament_status(&pool, id).await, "completed");
    assert_ne!(after_finish.1, "[]");
    assert_eq!(standings_and_points(&pool, group_id).await, after_finish);
    let winner: Option<i64> =
        sqlx::query_scalar("SELECT winner_id FROM turnier.group_matches WHERE id = $1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .expect("winner");
    assert_eq!(winner, Some(team2));
}
