#![cfg(feature = "testing")]

use std::time::Duration;

use tokio::time::timeout;
use turnier_scrim::repository::{PgScrimReadRepository, RoleOperation};

const TEAM_ROLE_ID: i64 = 7_001;
const COACH_ROLE_ID: i64 = 7_002;
const DISCORD_USER_ID: i64 = 8_001;
const SELF_SERVICE_ADVISORY_LOCK: i64 = 0x4451_0008_0004_0001;

async fn seed_substitute(
    pool: &turnier_db::Pool,
    participant_id: i32,
    substitute_until: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO scrim.teams(id, name, discord_role_id, created_at) \
         VALUES (1, 'Team', $1, now())",
    )
    .bind(TEAM_ROLE_ID)
    .execute(pool)
    .await
    .expect("team");
    sqlx::query(
        "INSERT INTO scrim.participants(\
             id, discord_id, display_name, rank_source, status, source, created_at, updated_at\
         ) VALUES ($1, $2, 'Aushilfe', 'manual', 'reserve', 'test', now(), now())",
    )
    .bind(participant_id)
    .bind(DISCORD_USER_ID)
    .execute(pool)
    .await
    .expect("participant");
    sqlx::query(
        "INSERT INTO scrim.team_members(\
             team_id, participant_id, is_bench, substitute_until\
         ) VALUES (1, $1, TRUE, $2::timestamptz)",
    )
    .bind(participant_id)
    .bind(substitute_until)
    .execute(pool)
    .await
    .expect("team member");
}

async fn membership_exists(pool: &turnier_db::Pool, participant_id: i32) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS(\
             SELECT 1 FROM scrim.team_members WHERE team_id=1 AND participant_id=$1\
         )",
    )
    .bind(participant_id)
    .fetch_one(pool)
    .await
    .expect("membership exists")
}

#[tokio::test]
async fn expired_substitute_is_removed_and_team_role_is_removed() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 1, Some("2000-01-01T00:00:00Z")).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let plans = repository.sweep_expired_substitutes().await.expect("sweep");

    assert!(!membership_exists(db.pool(), 1).await);
    assert_eq!(plans.len(), 1);
    let delivery = repository
        .begin_expired_substitute_sync_delivery(&plans[0], None, None)
        .await
        .expect("begin delivery")
        .expect("pending delivery");
    assert_eq!(delivery.plan.discord_user_id, Some(DISCORD_USER_ID as u64));
    assert!(delivery.plan.actions.iter().any(|action| {
        action.operation == RoleOperation::Remove && action.role_id == TEAM_ROLE_ID as u64
    }));
    delivery.finish(false).await.expect("leave retry open");
}

#[tokio::test]
async fn future_substitute_is_left_untouched() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 2, Some("2099-01-01T00:00:00Z")).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let plans = repository.sweep_expired_substitutes().await.expect("sweep");

    assert!(membership_exists(db.pool(), 2).await);
    assert!(plans.is_empty());
}

#[tokio::test]
async fn permanent_team_member_is_never_removed() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 3, None).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let plans = repository.sweep_expired_substitutes().await.expect("sweep");

    assert!(membership_exists(db.pool(), 3).await);
    assert!(plans.is_empty());
}

#[tokio::test]
async fn participant_resync_preserves_team_role_held_as_coach() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 4, None).await;
    sqlx::query(
        "INSERT INTO scrim.teams(id, name, coach_discord_id, discord_role_id, created_at) \
         VALUES (2, 'Coach-Team', $1, $2, now())",
    )
    .bind(DISCORD_USER_ID)
    .bind(COACH_ROLE_ID)
    .execute(db.pool())
    .await
    .expect("coach team");
    sqlx::query("UPDATE scrim.participants SET status='inactive' WHERE id=4")
        .execute(db.pool())
        .await
        .expect("inactive participant");
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let plan = repository
        .participant_resync_plan(4, None, None)
        .await
        .expect("resync plan");

    assert!(plan.actions.iter().any(|action| {
        action.operation == RoleOperation::Add && action.role_id == COACH_ROLE_ID as u64
    }));
    assert!(!plan.actions.iter().any(|action| {
        action.operation == RoleOperation::Remove && action.role_id == COACH_ROLE_ID as u64
    }));
}

#[tokio::test]
async fn participant_resync_preserves_other_membership_for_same_discord_id() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 7, None).await;
    sqlx::query(
        "INSERT INTO scrim.teams(id, name, discord_role_id, created_at) \
         VALUES (2, 'Second-Team', $1, now())",
    )
    .bind(COACH_ROLE_ID)
    .execute(db.pool())
    .await
    .expect("second team");
    sqlx::query(
        "INSERT INTO scrim.participants(\
             id, discord_id, display_name, rank_source, status, source, created_at, updated_at\
         ) VALUES (8, $1, 'Duplikat', 'manual', 'new', 'test', now(), now())",
    )
    .bind(DISCORD_USER_ID)
    .execute(db.pool())
    .await
    .expect("duplicate participant");
    sqlx::query(
        "INSERT INTO scrim.team_members(team_id, participant_id, is_bench) \
         VALUES (2, 8, FALSE)",
    )
    .execute(db.pool())
    .await
    .expect("second membership");
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let plan = repository
        .participant_resync_plan(7, None, None)
        .await
        .expect("resync plan");

    assert!(plan.actions.iter().any(|action| {
        action.operation == RoleOperation::Add && action.role_id == COACH_ROLE_ID as u64
    }));
    assert!(!plan.actions.iter().any(|action| {
        action.operation == RoleOperation::Remove && action.role_id == COACH_ROLE_ID as u64
    }));
}

#[tokio::test]
async fn failed_expiry_sync_remains_pending_after_membership_delete() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 5, Some("2000-01-01T00:00:00Z")).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let first_plans = repository
        .sweep_expired_substitutes()
        .await
        .expect("first sweep");
    let retry_plans = repository
        .sweep_expired_substitutes()
        .await
        .expect("retry sweep");

    assert!(!membership_exists(db.pool(), 5).await);
    assert_eq!(retry_plans, first_plans);

    repository
        .begin_expired_substitute_sync_delivery(&first_plans[0], None, None)
        .await
        .expect("begin delivery")
        .expect("pending delivery")
        .finish(true)
        .await
        .expect("mark delivered");
    assert!(repository
        .sweep_expired_substitutes()
        .await
        .expect("sweep after delivery")
        .is_empty());
}

#[tokio::test]
async fn expiry_retry_uses_current_membership_state() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 6, Some("2000-01-01T00:00:00Z")).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());

    repository
        .sweep_expired_substitutes()
        .await
        .expect("first sweep");
    sqlx::query(
        "INSERT INTO scrim.team_members(team_id, participant_id, is_bench, substitute_until) \
         VALUES (1, 6, FALSE, NULL)",
    )
    .execute(db.pool())
    .await
    .expect("restore permanent membership");
    let retry_plans = repository
        .sweep_expired_substitutes()
        .await
        .expect("retry sweep");

    let delivery = repository
        .begin_expired_substitute_sync_delivery(&retry_plans[0], None, None)
        .await
        .expect("begin retry delivery")
        .expect("retry delivery");
    assert!(delivery.plan.actions.iter().any(|action| {
        action.operation == RoleOperation::Add && action.role_id == TEAM_ROLE_ID as u64
    }));
    assert!(!delivery.plan.actions.iter().any(|action| {
        action.operation == RoleOperation::Remove && action.role_id == TEAM_ROLE_ID as u64
    }));
    delivery.finish(false).await.expect("leave retry open");
}

#[tokio::test]
async fn delivery_refreshes_role_state_before_discord() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 9, Some("2000-01-01T00:00:00Z")).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());
    let plans = repository
        .sweep_expired_substitutes()
        .await
        .expect("first sweep");

    sqlx::query(
        "INSERT INTO scrim.team_members(team_id, participant_id, is_bench, substitute_until) \
         VALUES (1, 9, FALSE, NULL)",
    )
    .execute(db.pool())
    .await
    .expect("restore permanent membership");
    let delivery = repository
        .begin_expired_substitute_sync_delivery(&plans[0], None, None)
        .await
        .expect("begin delivery")
        .expect("pending delivery");
    assert!(delivery.plan.actions.iter().any(|action| {
        action.operation == RoleOperation::Add && action.role_id == TEAM_ROLE_ID as u64
    }));
    delivery.finish(false).await.expect("leave retry open");

    assert!(!repository
        .sweep_expired_substitutes()
        .await
        .expect("retry after stale acknowledgement")
        .is_empty());
}

#[tokio::test]
async fn delivery_ack_does_not_close_newer_receipt() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 10, Some("2000-01-01T00:00:00Z")).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());
    let first_plans = repository
        .sweep_expired_substitutes()
        .await
        .expect("first sweep");

    sqlx::query(
        "INSERT INTO scrim.team_members(team_id, participant_id, is_bench, substitute_until) \
         VALUES (1, 10, TRUE, '2000-01-02T00:00:00Z')",
    )
    .execute(db.pool())
    .await
    .expect("new expired membership");
    repository
        .sweep_expired_substitutes()
        .await
        .expect("second sweep");
    repository
        .begin_expired_substitute_sync_delivery(&first_plans[0], None, None)
        .await
        .expect("begin first delivery")
        .expect("first delivery")
        .finish(true)
        .await
        .expect("acknowledge first receipt");

    assert!(!repository
        .sweep_expired_substitutes()
        .await
        .expect("newer receipt remains")
        .is_empty());
}

#[tokio::test]
async fn delivery_holds_role_mutation_lock_until_discord_result_is_recorded() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 11, Some("2000-01-01T00:00:00Z")).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());
    let plans = repository.sweep_expired_substitutes().await.expect("sweep");
    let delivery = repository
        .begin_expired_substitute_sync_delivery(&plans[0], None, None)
        .await
        .expect("begin delivery")
        .expect("pending delivery");

    let pool = db.pool().clone();
    let mut concurrent_mutation = tokio::spawn(async move {
        let mut tx = pool.begin().await.expect("mutation transaction");
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SELF_SERVICE_ADVISORY_LOCK)
            .execute(&mut *tx)
            .await
            .expect("mutation lock");
        tx.commit().await.expect("mutation commit");
    });

    assert!(
        timeout(Duration::from_millis(100), &mut concurrent_mutation)
            .await
            .is_err(),
        "role mutation must wait while Discord delivery uses its snapshot"
    );
    delivery
        .finish(false)
        .await
        .expect("release delivery without acknowledgement");
    timeout(Duration::from_secs(2), concurrent_mutation)
        .await
        .expect("mutation unblocked")
        .expect("mutation task");
}
