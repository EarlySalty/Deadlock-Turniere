#![cfg(feature = "testing")]

use serde_json::json;
use turnier_scrim::dto::{SignupRequest, WeeklyAvailability};
use turnier_scrim::model::{AvailabilitySlot, AvailabilityStatus};
use turnier_scrim::repository::PgScrimReadRepository;
use turnier_scrim::service::ScrimService;

fn weekly(saturday: AvailabilitySlot) -> WeeklyAvailability {
    let unknown = AvailabilitySlot {
        status: AvailabilityStatus::Unknown,
        from: None,
        to: None,
    };
    WeeklyAvailability {
        mon: unknown.clone(),
        tue: unknown.clone(),
        wed: unknown.clone(),
        thu: unknown.clone(),
        fri: unknown.clone(),
        sat: saturday,
        sun: unknown,
    }
}

fn available(from: u16, to: u16) -> AvailabilitySlot {
    AvailabilitySlot {
        status: AvailabilityStatus::Available,
        from: Some(from),
        to: Some(to),
    }
}

#[tokio::test]
async fn signup_creates_and_then_updates_one_participant_per_discord_id() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));

    let created = service
        .signup(
            "910001",
            "First Name",
            SignupRequest {
                rank: Some("Oracle".to_string()),
                roles: Some("Flex".to_string()),
                availability: None,
                availability_slots: Some(weekly(available(1_200, 1_320))),
            },
            None,
            Some(9_001),
        )
        .await
        .expect("create participant");
    assert_eq!(created.participant.display_name, "First Name");
    assert_eq!(created.participant.status, "new");
    assert_eq!(created.participant.source, "web_form");
    assert!(created.participant.availability_confirmed);
    assert_eq!(
        created.participant.availability_slots.sat,
        available(1_200, 1_320)
    );
    assert!(
        !created.role_ids.contains(&9_001),
        "non-reserve signup must not add the reserve role"
    );

    let updated = service
        .signup(
            "910001",
            "Updated Name",
            SignupRequest {
                rank: Some("Phantom".to_string()),
                roles: Some("Duo".to_string()),
                availability: Some("legacy text".to_string()),
                availability_slots: None,
            },
            None,
            Some(9_001),
        )
        .await
        .expect("update participant");
    assert_eq!(updated.participant.id, created.participant.id);
    assert_eq!(updated.participant.display_name, "Updated Name");
    assert_eq!(updated.participant.rank.as_deref(), Some("Phantom"));
    assert_eq!(updated.participant.roles.as_deref(), Some("Duo"));
    assert_eq!(
        updated.participant.availability.as_deref(),
        Some("legacy text")
    );
    assert_eq!(
        updated.participant.availability_slots.sat,
        available(1_200, 1_320),
        "existing structured slots take precedence when signup only sends free text"
    );

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM scrim.participants WHERE discord_id=910001")
            .fetch_one(db.pool())
            .await
            .expect("participant count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn availability_update_changes_only_availability_fields() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    sqlx::query(
        "INSERT INTO scrim.participants(\
             id, discord_id, display_name, rank, rank_source, rank_verified, roles, \
             availability, availability_slots, notes, status, source, created_at, updated_at\
         ) VALUES (920001, 920002, 'Player', 'Oracle', 'self', true, 'Flex', \
             'old', '{\"sat\":{\"status\":\"available\",\"from\":1200,\"to\":1320}}'::jsonb, \
             'note', 'assigned', 'discord_reaction', now(), now())",
    )
    .execute(db.pool())
    .await
    .expect("seed participant");
    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));

    let participant = service
        .update_availability("920002", weekly(available(1_140, 1_260)))
        .await
        .expect("update availability");
    assert_eq!(participant.id, 920001);
    assert_eq!(participant.rank.as_deref(), Some("Oracle"));
    assert_eq!(participant.roles.as_deref(), Some("Flex"));
    assert_eq!(participant.status, "assigned");
    assert_eq!(participant.source, "discord_reaction");
    assert_eq!(participant.availability_slots.sat, available(1_140, 1_260));

    let unchanged: bool = sqlx::query_scalar(
        "SELECT display_name='Player' AND rank='Oracle' AND rank_source='self' \
                AND rank_verified AND roles='Flex' AND notes='note' \
                AND status='assigned' AND source='discord_reaction' \
           FROM scrim.participants WHERE id=920001",
    )
    .fetch_one(db.pool())
    .await
    .expect("unchanged participant fields");
    assert!(unchanged);
    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT availability_slots FROM scrim.participants WHERE id=920001")
            .fetch_one(db.pool())
            .await
            .expect("stored slots");
    assert_eq!(
        stored["sat"],
        json!({"status":"available","from":1140,"to":1260})
    );
}

#[tokio::test]
async fn availability_validation_matches_legacy_canonicalization() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    sqlx::query(
        "INSERT INTO scrim.participants(\
             id, discord_id, display_name, rank_source, rank_verified, status, source, created_at, updated_at\
         ) VALUES (930001, 930002, 'Player', 'self', false, 'new', 'web_form', now(), now())",
    )
    .execute(db.pool())
    .await
    .expect("seed participant");
    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));

    let invalid = weekly(available(1_320, 1_200));
    assert!(service
        .update_availability("930002", invalid)
        .await
        .is_err());

    let non_available_with_times = AvailabilitySlot {
        status: AvailabilityStatus::Unavailable,
        from: Some(1_200),
        to: Some(1_320),
    };
    let canonical = service
        .update_availability("930002", weekly(non_available_with_times))
        .await
        .expect("canonical unavailable slot");
    assert_eq!(
        canonical.availability_slots.sat,
        AvailabilitySlot {
            status: AvailabilityStatus::Unavailable,
            from: None,
            to: None,
        }
    );
}

#[tokio::test]
async fn signup_waits_for_the_live_discord_reaction_advisory_lock() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let mut tx = db.pool().begin().await.expect("lock transaction");
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(0x4451_0008_0004_0001_i64)
        .execute(&mut *tx)
        .await
        .expect("hold live reaction lock");

    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));
    let pending = tokio::spawn(async move {
        service
            .signup(
                "940001",
                "Locked Player",
                SignupRequest {
                    rank: None,
                    roles: None,
                    availability: None,
                    availability_slots: None,
                },
                None,
                None,
            )
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        !pending.is_finished(),
        "signup did not wait for the Discord reaction lock"
    );

    tx.commit().await.expect("release lock");
    pending
        .await
        .expect("signup task")
        .expect("signup after lock release");
}

/// Ein per Self-Service gemeldeter Rang darf nie als verifiziert gelten.
///
/// Sonst kann ein Spieler einen von der Orga bestaetigten Rang ueberschreiben und der
/// neue, selbst gemeldete Wert traegt weiter das Verifiziert-Kennzeichen. Solange der
/// Rang unveraendert bleibt, bleibt eine bestehende Verifizierung erhalten.
#[tokio::test]
async fn signup_never_leaves_a_self_reported_rank_marked_as_verified() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));

    let created = service
        .signup(
            "910042",
            "Verified Player",
            SignupRequest {
                rank: Some("Oracle".to_string()),
                roles: None,
                availability: None,
                availability_slots: None,
            },
            None,
            None,
        )
        .await
        .expect("create participant");

    // Die Orga bestaetigt den Rang.
    sqlx::query(
        "UPDATE scrim.participants SET rank_source='admin', rank_verified=true WHERE id=$1",
    )
    .bind(created.participant.id)
    .execute(db.pool())
    .await
    .expect("mark rank verified");

    // Gleicher Rang erneut gemeldet: die Bestaetigung bleibt bestehen.
    service
        .signup(
            "910042",
            "Verified Player",
            SignupRequest {
                rank: Some("Oracle".to_string()),
                roles: Some("Flex".to_string()),
                availability: None,
                availability_slots: None,
            },
            None,
            None,
        )
        .await
        .expect("resignup with same rank");

    let (source, verified): (String, bool) =
        sqlx::query_as("SELECT rank_source, rank_verified FROM scrim.participants WHERE id=$1")
            .bind(created.participant.id)
            .fetch_one(db.pool())
            .await
            .expect("read rank flags");
    assert_eq!(
        source, "admin",
        "unveraenderter Rang behaelt seine Herkunft"
    );
    assert!(verified, "unveraenderter Rang bleibt verifiziert");

    // Anderer Rang per Self-Service: Verifizierung faellt weg.
    service
        .signup(
            "910042",
            "Verified Player",
            SignupRequest {
                rank: Some("Phantom".to_string()),
                roles: None,
                availability: None,
                availability_slots: None,
            },
            None,
            None,
        )
        .await
        .expect("resignup with changed rank");

    let (source, verified): (String, bool) =
        sqlx::query_as("SELECT rank_source, rank_verified FROM scrim.participants WHERE id=$1")
            .bind(created.participant.id)
            .fetch_one(db.pool())
            .await
            .expect("read rank flags");
    assert_eq!(source, "self", "geaenderter Rang ist selbst gemeldet");
    assert!(!verified, "geaenderter Rang darf nicht verifiziert bleiben");
}
