//! Integrationstests der Session-Persistenz gegen eine echte Wegwerf-PG-DB.
//!
//! Deckt den Lebenszyklus ab: anlegen → auflösen (inkl. Flag-Berechnung) →
//! löschen → abgelaufene wegräumen. Keine externen Dienste (kein Broker,
//! kein Discord) — nur DB.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use turnier_auth::{cleanup_expired, create_session, delete_session, resolve_session, RoleSets};
use turnier_db::{test_pool, TestDb};

async fn temp_db() -> TestDb {
    test_pool().await.expect("central test pool")
}

fn role_sets() -> RoleSets {
    RoleSets::new(
        ["admin1".to_string()].into_iter().collect::<HashSet<_>>(),
        ["mod1".to_string()].into_iter().collect::<HashSet<_>>(),
    )
}

#[tokio::test]
async fn anlegen_und_aufloesen_mit_admin_rolle() {
    let db = temp_db().await;
    let pool = db.pool();
    let sets = role_sets();

    let token = create_session(
        pool,
        "123456789012345678",
        "Tester",
        "avatarhash",
        &["admin1".to_string(), "sonst".to_string()],
    )
    .await
    .expect("Session anlegen");

    let session = resolve_session(pool, &token, &sets)
        .await
        .expect("Session auflösen");

    assert_eq!(session.discord_id, "123456789012345678");
    assert_eq!(session.discord_name.as_deref(), Some("Tester"));
    assert_eq!(session.discord_avatar.as_deref(), Some("avatarhash"));
    assert_eq!(session.roles, vec!["admin1", "sonst"]);
    assert!(session.is_admin);
    assert!(session.is_mod, "Admin impliziert Mod");
}

#[tokio::test]
async fn aufloesen_ohne_passende_rolle_ist_normaler_user() {
    let db = temp_db().await;
    let pool = db.pool();
    let sets = role_sets();

    let token = create_session(pool, "123456789012345679", "n", "a", &["fremd".to_string()])
        .await
        .unwrap();
    let session = resolve_session(pool, &token, &sets).await.unwrap();

    assert!(!session.is_admin);
    assert!(!session.is_mod);
}

#[tokio::test]
async fn unbekanntes_token_ist_401() {
    let db = temp_db().await;
    let pool = db.pool();
    let sets = role_sets();

    let err = resolve_session(pool, "gibt-es-nicht", &sets)
        .await
        .unwrap_err();
    assert_eq!(err.status_code(), 401);
}

#[tokio::test]
async fn abgelaufene_session_ist_401_und_wird_geloescht() {
    let db = temp_db().await;
    let pool = db.pool();
    let sets = role_sets();

    // Eine bereits abgelaufene Session direkt einfügen.
    sqlx::query(
        "INSERT INTO turnier.sessions \
             (token, discord_id, discord_roles, expires_at, created_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind("alt")
    .bind(123456789012345680_i64)
    .bind("admin1")
    .bind(parse_utc("2000-01-01T00:00:00Z"))
    .bind(parse_utc("2000-01-01T00:00:00Z"))
    .execute(pool)
    .await
    .unwrap();

    let err = resolve_session(pool, "alt", &sets).await.unwrap_err();
    assert_eq!(err.status_code(), 401);

    // Opportunistic-Cleanup: die Zeile ist nach dem Auflösen weg.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turnier.sessions WHERE token = $1")
        .bind("alt")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn logout_loescht_session() {
    let db = temp_db().await;
    let pool = db.pool();
    let sets = role_sets();

    let token = create_session(pool, "123456789012345681", "n", "a", &[])
        .await
        .unwrap();
    delete_session(pool, &token).await.unwrap();

    let err = resolve_session(pool, &token, &sets).await.unwrap_err();
    assert_eq!(err.status_code(), 401);
}

#[tokio::test]
async fn cleanup_entfernt_nur_abgelaufene() {
    let db = temp_db().await;
    let pool = db.pool();

    // Abgelaufen.
    sqlx::query(
        "INSERT INTO turnier.sessions (token, discord_id, expires_at, created_at) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind("alt")
    .bind(123456789012345682_i64)
    .bind(parse_utc("2000-01-01T00:00:00Z"))
    .bind(parse_utc("2000-01-01T00:00:00Z"))
    .execute(pool)
    .await
    .unwrap();
    // Gültig (frisch angelegt, 7 Tage).
    let gueltig = create_session(pool, "123456789012345683", "n", "a", &[])
        .await
        .unwrap();

    let removed = cleanup_expired(pool).await.unwrap();
    assert_eq!(removed, 1);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turnier.sessions")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(total, 1);

    // Die gültige Session überlebt.
    let sets = role_sets();
    assert!(resolve_session(pool, &gueltig, &sets).await.is_ok());
}

fn parse_utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
