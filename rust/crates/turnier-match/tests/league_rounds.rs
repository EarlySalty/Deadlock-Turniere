#![cfg(feature = "testing")]

mod common;

use chrono::{DateTime, Duration, Utc};
use common::{
    insert_group, insert_group_team, insert_team, insert_tournament, manager_without_services,
    temp_db,
};
use turnier_core::now_utc;
use turnier_db::Pool;
use turnier_match::league::{current_round_match_ids, due_lobby_match_ids};
use turnier_match::ApplyGroupParams;

struct League {
    id: i64,
    group_id: i64,
    teams: Vec<i64>,
}

async fn league(pool: &Pool, team_count: usize) -> League {
    let id = insert_tournament(pool, "Liga", false, true).await;
    sqlx::query(
        "UPDATE turnier.tournaments SET tournament_mode = 'league', status = 'group_phase' \
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("league mode");
    let group_id = insert_group(pool, id, "Tabelle").await;
    let mut teams = Vec::new();
    for index in 0..team_count {
        let team = insert_team(pool, id, &format!("Team {index}")).await;
        insert_group_team(pool, group_id, team).await;
        teams.push(team);
    }
    League {
        id,
        group_id,
        teams,
    }
}

async fn scheduled_match(
    pool: &Pool,
    group_id: i64,
    team1_id: i64,
    team2_id: i64,
    at: DateTime<Utc>,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.group_matches (group_id, team1_id, team2_id, status, scheduled_at) \
         VALUES ($1, $2, $3, 'pending', $4) RETURNING id",
    )
    .bind(group_id)
    .bind(team1_id)
    .bind(team2_id)
    .bind(at)
    .fetch_one(pool)
    .await
    .expect("insert match")
}

async fn standing(pool: &Pool, group_id: i64, team_id: i64) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT wins, losses, points FROM turnier.group_teams \
         WHERE group_id = $1 AND team_id = $2",
    )
    .bind(group_id)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .expect("standing")
}

fn automatic_win(winner_id: i64) -> ApplyGroupParams {
    ApplyGroupParams {
        winner_id: Some(winner_id),
        source: "automatic".to_string(),
        ..Default::default()
    }
}

#[tokio::test]
async fn lobbies_open_only_for_the_lowest_open_round_once_it_is_due() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let manager = manager_without_services(pool.clone());
    let liga = league(&pool, 4).await;
    let [a, b, c, d] = liga.teams[..] else {
        panic!("vier Teams erwartet");
    };
    let now = now_utc();
    let first_round = now + Duration::minutes(30);
    let second_round = now + Duration::minutes(80);
    let m1 = scheduled_match(&pool, liga.group_id, a, b, first_round).await;
    let m2 = scheduled_match(&pool, liga.group_id, c, d, first_round).await;
    let m3 = scheduled_match(&pool, liga.group_id, a, c, second_round).await;
    let m4 = scheduled_match(&pool, liga.group_id, b, d, second_round).await;

    assert!(due_lobby_match_ids(&pool, liga.id, now)
        .await
        .expect("before start")
        .is_empty());
    assert_eq!(
        due_lobby_match_ids(&pool, liga.id, first_round)
            .await
            .expect("round one due"),
        vec![m1, m2]
    );
    assert_eq!(
        due_lobby_match_ids(&pool, liga.id, second_round + Duration::hours(1))
            .await
            .expect("round two due but round one open"),
        vec![m1, m2]
    );

    manager
        .apply_group_match_result(liga.id, m1, automatic_win(a))
        .await
        .expect("result one");
    assert_eq!(
        due_lobby_match_ids(&pool, liga.id, second_round)
            .await
            .expect("one open match left"),
        vec![m2]
    );

    manager
        .apply_group_match_result(liga.id, m2, automatic_win(c))
        .await
        .expect("result two");
    assert!(due_lobby_match_ids(&pool, liga.id, first_round)
        .await
        .expect("round two not due")
        .is_empty());
    assert_eq!(
        due_lobby_match_ids(&pool, liga.id, second_round)
            .await
            .expect("round two due"),
        vec![m3, m4]
    );
    assert_eq!(
        current_round_match_ids(&pool, liga.id)
            .await
            .expect("current round"),
        vec![m3, m4]
    );
}

#[tokio::test]
async fn running_lobby_blocks_nothing_but_is_not_opened_twice() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let liga = league(&pool, 4).await;
    let [a, b, c, d] = liga.teams[..] else {
        panic!("vier Teams erwartet");
    };
    let now = now_utc();
    let m1 = scheduled_match(&pool, liga.group_id, a, b, now).await;
    let m2 = scheduled_match(&pool, liga.group_id, c, d, now).await;
    sqlx::query("UPDATE turnier.group_matches SET steam_party_id = 'party' WHERE id = $1")
        .bind(m1)
        .execute(&pool)
        .await
        .expect("mark lobby");

    assert_eq!(
        due_lobby_match_ids(&pool, liga.id, now).await.expect("due"),
        vec![m2]
    );
}

#[tokio::test]
async fn automatic_result_writes_league_points_and_stays_terminal() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let manager = manager_without_services(pool.clone());
    let liga = league(&pool, 2).await;
    let [a, b] = liga.teams[..] else {
        panic!("zwei Teams erwartet");
    };
    let m1 = scheduled_match(&pool, liga.group_id, a, b, now_utc()).await;

    manager
        .apply_group_match_result(liga.id, m1, automatic_win(b))
        .await
        .expect("result");

    assert_eq!(standing(&pool, liga.group_id, b).await, (1, 0, 2));
    assert_eq!(standing(&pool, liga.group_id, a).await, (0, 1, -1));
    assert!(manager
        .apply_group_match_result(liga.id, m1, automatic_win(a))
        .await
        .is_err());
    assert_eq!(standing(&pool, liga.group_id, b).await, (1, 0, 2));
}

#[tokio::test]
async fn group_stage_keeps_plus_three_and_zero() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let manager = manager_without_services(pool.clone());
    let liga = league(&pool, 2).await;
    sqlx::query("UPDATE turnier.tournaments SET tournament_mode = 'group_stage' WHERE id = $1")
        .bind(liga.id)
        .execute(&pool)
        .await
        .expect("group mode");
    let [a, b] = liga.teams[..] else {
        panic!("zwei Teams erwartet");
    };
    let m1 = scheduled_match(&pool, liga.group_id, a, b, now_utc()).await;

    manager
        .apply_group_match_result(liga.id, m1, automatic_win(a))
        .await
        .expect("result");

    assert_eq!(standing(&pool, liga.group_id, a).await, (1, 0, 3));
    assert_eq!(standing(&pool, liga.group_id, b).await, (0, 1, 0));
}
