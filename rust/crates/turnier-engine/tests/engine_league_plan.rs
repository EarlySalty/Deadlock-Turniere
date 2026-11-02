#![cfg(feature = "testing")]

mod common;

use chrono::{DateTime, Duration, TimeZone, Utc};
use common::{insert_team, insert_tournament, temp_pool};
use std::collections::BTreeMap;
use turnier_db::Pool;
use turnier_engine::{
    generate_league_plan_in_tx, league_slot_minutes_in_tx, reschedule_league_plan_in_tx,
};

const CAPTAIN: i64 = 123_456_789_012_345_800;

async fn league_with_teams(pool: &Pool, count: i64, start: Option<DateTime<Utc>>) -> i64 {
    let id = insert_tournament(
        pool,
        "Liga",
        "group_phase",
        6,
        "single_elimination",
        "league",
    )
    .await;
    sqlx::query("UPDATE turnier.tournaments SET group_phase_start = $1 WHERE id = $2")
        .bind(start)
        .bind(id)
        .execute(pool)
        .await
        .expect("set start");
    for index in 0..count {
        insert_team(pool, id, &format!("Team {index}"), CAPTAIN + index).await;
    }
    id
}

async fn plan_rows(pool: &Pool, id: i64) -> Vec<(i64, i64, DateTime<Utc>)> {
    sqlx::query_as(
        "SELECT gm.team1_id, gm.team2_id, gm.scheduled_at FROM turnier.group_matches gm \
         JOIN turnier.groups g ON g.id = gm.group_id WHERE g.tournament_id = $1 ORDER BY gm.id",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .expect("plan rows")
}

fn round_sizes(rows: &[(i64, i64, DateTime<Utc>)]) -> BTreeMap<DateTime<Utc>, i64> {
    let mut sizes = BTreeMap::new();
    for (_, _, at) in rows {
        *sizes.entry(*at).or_default() += 1;
    }
    sizes
}

fn start_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 7, 18, 0, 0)
        .single()
        .expect("valid time")
}

#[tokio::test]
async fn ten_teams_get_45_matches_in_9_timed_rounds_in_one_group() {
    let db = temp_pool().await;
    let pool = db.pool().clone();
    let id = league_with_teams(&pool, 10, Some(start_time())).await;

    let mut tx = pool.begin().await.expect("tx");
    let plan = generate_league_plan_in_tx(&mut tx, id, 50)
        .await
        .expect("plan");
    tx.commit().await.expect("commit");
    assert_eq!(plan.matches, 45);

    let groups: Vec<(String, i64)> = sqlx::query_as(
        "SELECT g.name, (SELECT COUNT(*) FROM turnier.group_teams gt WHERE gt.group_id = g.id) \
         FROM turnier.groups g WHERE g.tournament_id = $1",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .expect("groups");
    assert_eq!(groups, vec![("Tabelle".to_string(), 10)]);

    let sizes = round_sizes(&plan_rows(&pool, id).await);
    assert_eq!(sizes.len(), 9);
    assert!(sizes.values().all(|count| *count == 5));
    let times: Vec<DateTime<Utc>> = sizes.keys().copied().collect();
    assert_eq!(times[0], start_time());
    assert_eq!(times[8], start_time() + Duration::minutes(8 * 50));
}

#[tokio::test]
async fn missing_start_is_written_back_from_generation_time() {
    let db = temp_pool().await;
    let pool = db.pool().clone();
    let id = league_with_teams(&pool, 3, None).await;

    let mut tx = pool.begin().await.expect("tx");
    generate_league_plan_in_tx(&mut tx, id, 50)
        .await
        .expect("plan");
    tx.commit().await.expect("commit");

    let stored: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT group_phase_start FROM turnier.tournaments WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("start");
    let rows = plan_rows(&pool, id).await;
    assert_eq!(rows.len(), 3);
    assert_eq!(round_sizes(&rows).len(), 3);
    assert_eq!(stored, rows.iter().map(|row| row.2).min());
}

#[tokio::test]
async fn reschedule_moves_times_and_keeps_every_pairing() {
    let db = temp_pool().await;
    let pool = db.pool().clone();
    let id = league_with_teams(&pool, 6, Some(start_time())).await;

    let mut tx = pool.begin().await.expect("tx");
    generate_league_plan_in_tx(&mut tx, id, 50)
        .await
        .expect("plan");
    tx.commit().await.expect("commit");
    let before = plan_rows(&pool, id).await;

    let new_start = start_time() + Duration::days(1);
    let mut tx = pool.begin().await.expect("tx");
    let updated = reschedule_league_plan_in_tx(&mut tx, id, new_start, 30)
        .await
        .expect("reschedule");
    assert_eq!(updated, 15);
    assert_eq!(
        league_slot_minutes_in_tx(&mut tx, id).await.expect("slot"),
        Some(30)
    );
    tx.commit().await.expect("commit");

    let after = plan_rows(&pool, id).await;
    for (old, new) in before.iter().zip(after.iter()) {
        assert_eq!((old.0, old.1), (new.0, new.1));
        let old_round = (old.2 - start_time()).num_minutes() / 50;
        assert_eq!(new.2, new_start + Duration::minutes(old_round * 30));
    }
}

#[tokio::test]
async fn invalid_slot_and_single_team_are_rejected() {
    let db = temp_pool().await;
    let pool = db.pool().clone();
    let id = league_with_teams(&pool, 1, Some(start_time())).await;

    let mut tx = pool.begin().await.expect("tx");
    assert!(generate_league_plan_in_tx(&mut tx, id, 50).await.is_err());
    assert!(generate_league_plan_in_tx(&mut tx, id, 0).await.is_err());
}
