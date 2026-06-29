//! Integrationstests der Session-Persistenz gegen eine echte Temp-SQLite-DB.
//!
//! Deckt den Lebenszyklus ab: anlegen → auflösen (inkl. Flag-Berechnung) →
//! löschen → abgelaufene wegräumen. Keine externen Dienste (kein Broker,
//! kein Discord) — nur DB.

use std::collections::HashSet;

use turnier_auth::{
    cleanup_expired, create_session, delete_session, resolve_session, RoleSets,
};
use turnier_db::{connect_str, run_migrations, Pool};

/// Frische In-Memory-DB mit angewandter Migration.
async fn temp_pool() -> Pool {
    // `:memory:` mit max_connections=1, damit alle Queries dieselbe Instanz
    // teilen (sonst sieht eine zweite Verbindung die Tabellen nicht).
    let pool = connect_str(":memory:", 1).await.expect("Pool öffnen");
    run_migrations(&pool).await.expect("Migration anwenden");
    pool
}

fn role_sets() -> RoleSets {
    RoleSets::new(
        ["admin1".to_string()].into_iter().collect::<HashSet<_>>(),
        ["mod1".to_string()].into_iter().collect::<HashSet<_>>(),
    )
}

#[tokio::test]
async fn anlegen_und_aufloesen_mit_admin_rolle() {
    let pool = temp_pool().await;
    let sets = role_sets();

    let token = create_session(
        &pool,
        "discord-123",
        "Tester",
        "avatarhash",
        &["admin1".to_string(), "sonst".to_string()],
    )
    .await
    .expect("Session anlegen");

    let session = resolve_session(&pool, &token, &sets)
        .await
        .expect("Session auflösen");

    assert_eq!(session.discord_id, "discord-123");
    assert_eq!(session.discord_name.as_deref(), Some("Tester"));
    assert_eq!(session.discord_avatar.as_deref(), Some("avatarhash"));
    assert_eq!(session.roles, vec!["admin1", "sonst"]);
    assert!(session.is_admin);
    assert!(session.is_mod, "Admin impliziert Mod");
}

#[tokio::test]
async fn aufloesen_ohne_passende_rolle_ist_normaler_user() {
    let pool = temp_pool().await;
    let sets = role_sets();

    let token = create_session(&pool, "u", "n", "a", &["fremd".to_string()])
        .await
        .unwrap();
    let session = resolve_session(&pool, &token, &sets).await.unwrap();

    assert!(!session.is_admin);
    assert!(!session.is_mod);
}

#[tokio::test]
async fn unbekanntes_token_ist_401() {
    let pool = temp_pool().await;
    let sets = role_sets();

    let err = resolve_session(&pool, "gibt-es-nicht", &sets)
        .await
        .unwrap_err();
    assert_eq!(err.status_code(), 401);
}

#[tokio::test]
async fn abgelaufene_session_ist_401_und_wird_geloescht() {
    let pool = temp_pool().await;
    let sets = role_sets();

    // Eine bereits abgelaufene Session direkt einfügen.
    sqlx::query(
        "INSERT INTO sessions (token, discord_id, discord_roles, expires_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("alt")
    .bind("u")
    .bind("admin1")
    .bind("2000-01-01T00:00:00+00:00")
    .execute(&pool)
    .await
    .unwrap();

    let err = resolve_session(&pool, "alt", &sets).await.unwrap_err();
    assert_eq!(err.status_code(), 401);

    // Opportunistic-Cleanup: die Zeile ist nach dem Auflösen weg.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE token = ?")
        .bind("alt")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn logout_loescht_session() {
    let pool = temp_pool().await;
    let sets = role_sets();

    let token = create_session(&pool, "u", "n", "a", &[]).await.unwrap();
    delete_session(&pool, &token).await.unwrap();

    let err = resolve_session(&pool, &token, &sets).await.unwrap_err();
    assert_eq!(err.status_code(), 401);
}

#[tokio::test]
async fn cleanup_entfernt_nur_abgelaufene() {
    let pool = temp_pool().await;

    // Abgelaufen.
    sqlx::query("INSERT INTO sessions (token, discord_id, expires_at) VALUES (?, ?, ?)")
        .bind("alt")
        .bind("u")
        .bind("2000-01-01T00:00:00+00:00")
        .execute(&pool)
        .await
        .unwrap();
    // Gültig (frisch angelegt, 7 Tage).
    let gueltig = create_session(&pool, "u2", "n", "a", &[]).await.unwrap();

    let removed = cleanup_expired(&pool).await.unwrap();
    assert_eq!(removed, 1);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 1);

    // Die gültige Session überlebt.
    let sets = role_sets();
    assert!(resolve_session(&pool, &gueltig, &sets).await.is_ok());
}
