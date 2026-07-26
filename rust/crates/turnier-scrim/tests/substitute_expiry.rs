#![cfg(feature = "testing")]

use turnier_scrim::repository::{PgScrimReadRepository, RoleOperation};

const TEAM_ROLE_ID: i64 = 7_001;
const DISCORD_USER_ID: i64 = 8_001;

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

    let plans = repository
        .sweep_expired_substitutes(None, None)
        .await
        .expect("sweep");

    assert!(!membership_exists(db.pool(), 1).await);
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].discord_user_id, Some(DISCORD_USER_ID as u64));
    assert!(plans[0].actions.iter().any(|action| {
        action.operation == RoleOperation::Remove && action.role_id == TEAM_ROLE_ID as u64
    }));
}

#[tokio::test]
async fn future_substitute_is_left_untouched() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 2, Some("2099-01-01T00:00:00Z")).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let plans = repository
        .sweep_expired_substitutes(None, None)
        .await
        .expect("sweep");

    assert!(membership_exists(db.pool(), 2).await);
    assert!(plans.is_empty());
}

#[tokio::test]
async fn permanent_team_member_is_never_removed() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    seed_substitute(db.pool(), 3, None).await;
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let plans = repository
        .sweep_expired_substitutes(None, None)
        .await
        .expect("sweep");

    assert!(membership_exists(db.pool(), 3).await);
    assert!(plans.is_empty());
}
