#![cfg(feature = "testing")]

use std::collections::HashSet;
use turnier_draft::comp::{self, CompError, Preference};

fn token(i: usize) -> String {
    format!("comp-test-capability-{i:032}")
}
fn allowed() -> HashSet<String> {
    HashSet::from(["A".into(), "B".into()])
}
fn prefs() -> Vec<Preference> {
    vec![Preference {
        hero_name: "A".into(),
        priority: 0,
    }]
}

#[tokio::test]
async fn persisted_private_membership_preferences_cas_and_zero_priority() {
    let db = turnier_db::test_pool().await.unwrap();
    let pool = db.pool();
    let room = comp::create(pool, "Host", &token(1)).await.unwrap();
    assert_eq!(room.members.len(), 1);
    assert_eq!(room.you.as_ref(), Some(&room.host_member_id));
    let code = room.code;
    let guest = comp::join(pool, &code.to_lowercase(), "Guest", &token(2))
        .await
        .unwrap();
    let saved = comp::save_preferences(pool, &code, &token(2), 0, &prefs(), &allowed())
        .await
        .unwrap();
    assert_eq!(saved.members[1].preferences[0].priority, 0);
    assert!(matches!(
        comp::save_preferences(pool, &code, &token(2), 0, &[], &allowed()).await,
        Err(CompError::Stale)
    ));
    assert!(matches!(
        comp::save_preferences(pool, &code, &token(3), 0, &prefs(), &allowed()).await,
        Err(CompError::Unauthorized)
    ));
    let public = comp::get(pool, &code, None).await.unwrap();
    assert!(public.you.is_none());
    assert_eq!(public.members[1].preferences, prefs());
    let serialized = serde_json::to_string(&public).unwrap();
    assert!(!serialized.contains("token"));
    assert!(!serialized.contains(&token(2)));
    assert!(!serialized.contains(&comp::token_hash(&token(2)).unwrap()));
    // Same capability after a request retry recovers the same seat, not a new one.
    let retried = comp::join(pool, &code, "Changed Name", &token(2))
        .await
        .unwrap();
    assert_eq!(retried.you, guest.you);
    assert_eq!(retried.members.len(), 2);
    assert_eq!(retried.members[1].name, "Guest");
    // Rebuilding the pool/view does not depend on process-local room storage.
    assert_eq!(
        comp::get(&pool.clone(), &code, Some(&token(2)))
            .await
            .unwrap()
            .you,
        guest.you
    );
}

#[tokio::test]
async fn concurrent_joins_never_exceed_six_and_retry_succeeds_when_full() {
    let db = turnier_db::test_pool().await.unwrap();
    let pool = db.pool();
    let room = comp::create(pool, "Host", &token(0)).await.unwrap();
    let mut tasks = Vec::new();
    for i in 1..=12 {
        let pool = pool.clone();
        let code = room.code.clone();
        tasks.push(tokio::spawn(async move {
            comp::join(&pool, &code, &format!("P{i}"), &token(i)).await
        }));
    }
    let mut accepted = 0;
    let mut full = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(_) => accepted += 1,
            Err(CompError::Full) => full += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!((accepted, full), (5, 7));
    assert_eq!(
        comp::get(pool, &room.code, None)
            .await
            .unwrap()
            .members
            .len(),
        6
    );
    assert_eq!(
        comp::join(pool, &room.code, "Host", &token(0))
            .await
            .unwrap()
            .members
            .len(),
        6
    );
}

#[tokio::test]
async fn host_removal_transfer_and_last_leave_are_atomic() {
    let db = turnier_db::test_pool().await.unwrap();
    let pool = db.pool();
    let host = comp::create(pool, "Host", &token(1)).await.unwrap();
    let guest = comp::join(pool, &host.code, "Guest", &token(2))
        .await
        .unwrap();
    let other = comp::join(pool, &host.code, "Other", &token(3))
        .await
        .unwrap();
    assert!(matches!(
        comp::remove_member(pool, &host.code, &token(2), other.you.as_ref().unwrap()).await,
        Err(CompError::Forbidden)
    ));
    comp::remove_member(pool, &host.code, &token(1), other.you.as_ref().unwrap())
        .await
        .unwrap();
    assert!(matches!(
        comp::save_preferences(pool, &host.code, &token(3), 0, &prefs(), &allowed()).await,
        Err(CompError::Unauthorized)
    ));
    comp::leave(pool, &host.code, &token(1)).await.unwrap();
    let after = comp::get(pool, &host.code, Some(&token(2))).await.unwrap();
    assert_eq!(after.host_member_id, guest.you.unwrap());
    comp::leave(pool, &host.code, &token(2)).await.unwrap();
    assert!(matches!(
        comp::get(pool, &host.code, None).await,
        Err(CompError::NotFound)
    ));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM turnier.comp_members WHERE lobby_code=$1")
            .bind(&host.code)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn expired_rooms_reject_all_access_and_are_purged_on_creation() {
    let db = turnier_db::test_pool().await.unwrap();
    let pool = db.pool();
    let room = comp::create(pool, "Host", &token(1)).await.unwrap();
    sqlx::query(
        "UPDATE turnier.comp_lobbies SET expires_at=now()-interval '1 second' WHERE code=$1",
    )
    .bind(&room.code)
    .execute(pool)
    .await
    .unwrap();
    assert!(matches!(
        comp::get(pool, &room.code, None).await,
        Err(CompError::NotFound)
    ));
    assert!(matches!(
        comp::join(pool, &room.code, "Late", &token(2)).await,
        Err(CompError::NotFound)
    ));
    assert!(matches!(
        comp::save_preferences(pool, &room.code, &token(1), 0, &prefs(), &allowed()).await,
        Err(CompError::NotFound)
    ));
    comp::create(pool, "New", &token(3)).await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM turnier.comp_members WHERE lobby_code=$1")
            .bind(&room.code)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn concurrent_saves_with_same_revision_have_one_winner() {
    let db = turnier_db::test_pool().await.unwrap();
    let pool = db.pool();
    let room = comp::create(pool, "Host", &token(1)).await.unwrap();
    let p = prefs();
    let a = allowed();
    let t = token(1);
    let (one, two) = tokio::join!(
        comp::save_preferences(pool, &room.code, &t, 0, &p, &a),
        comp::save_preferences(pool, &room.code, &t, 0, &[], &a),
    );
    assert!(matches!(
        (&one, &two),
        (Ok(_), Err(CompError::Stale)) | (Err(CompError::Stale), Ok(_))
    ));
    assert_eq!(
        comp::get(pool, &room.code, None).await.unwrap().members[0].revision,
        1
    );
}
