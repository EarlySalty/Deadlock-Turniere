//! Integrationstest für den Reminder-Dedupe mit Temp-SQLite.
//!
//! Discord ist ein No-op-Fake (unkonfigurierter Broker → `notify_users` liefert
//! `Ok`, jede ID landet in `failed`). Damit wird der Dedupe-Pfad ausgelöst, ohne
//! echten Versand: nach dem ersten Lauf steht genau EIN Dedupe-Eintrag, ein
//! zweiter Lauf im selben Fenster fügt keinen weiteren hinzu.
//!
//! Die Zeit ist injiziert (`now` als Parameter) — keine echte Zeitabhängigkeit.

mod common;

use chrono::NaiveDateTime;

use common::{fake_notifier, insert_tournament, reminder_count, test_config, temp_pool};
use tb_db::Pool;
use tb_scheduler::check_and_send_registration_reminders;

fn naive(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
}

/// Legt ein Opt-in-Profil an, damit die Registrierungs-Reminder Empfänger haben.
async fn insert_optin_profile(pool: &Pool, discord_id: &str) {
    sqlx::query(
        "INSERT INTO user_profiles (discord_id, updated_at, notify_discord_dm, \
                                    notify_registration_reminder) \
         VALUES (?, datetime('now'), 1, 1)",
    )
    .bind(discord_id)
    .execute(pool)
    .await
    .expect("insert profile");
}

#[tokio::test]
async fn registration_reminder_wird_genau_einmal_dedupliziert() {
    let pool = temp_pool().await;
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    // Turnier in 'registration' mit registration_end um 12:00 und nur einem
    // Offset (60 min) — der Reminder ist um 11:00 fällig.
    let id = insert_tournament(&pool, "T", "registration", false).await;
    sqlx::query(
        "UPDATE tournaments SET registration_end = '2026-06-14T12:00:00', \
                                reminder_offsets = '[60]' WHERE id = ?",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    insert_optin_profile(&pool, "111").await;

    // now liegt im 5-min-Fenster (11:00..=11:05).
    let now = naive("2026-06-14T11:02:00");
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("erster lauf");
    assert_eq!(reminder_count(&pool, "sent_tournament_reminders", id).await, 1);

    // Zweiter Lauf im selben Fenster → Dedupe greift, kein weiterer Eintrag.
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("zweiter lauf");
    assert_eq!(reminder_count(&pool, "sent_tournament_reminders", id).await, 1);
}

#[tokio::test]
async fn kein_reminder_ausserhalb_des_fensters() {
    let pool = temp_pool().await;
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    let id = insert_tournament(&pool, "T", "registration", false).await;
    sqlx::query(
        "UPDATE tournaments SET registration_end = '2026-06-14T12:00:00', \
                                reminder_offsets = '[60]' WHERE id = ?",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    insert_optin_profile(&pool, "111").await;

    // now ist 6 min nach dem fälligen Zeitpunkt (11:00) → außerhalb des Fensters.
    let now = naive("2026-06-14T11:06:00");
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("lauf");
    assert_eq!(reminder_count(&pool, "sent_tournament_reminders", id).await, 0);
}

#[tokio::test]
async fn test_turnier_loest_keinen_reminder_aus() {
    let pool = temp_pool().await;
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    // is_test = true → übersprungen.
    let id = insert_tournament(&pool, "T", "registration", true).await;
    sqlx::query(
        "UPDATE tournaments SET registration_end = '2026-06-14T12:00:00', \
                                reminder_offsets = '[60]' WHERE id = ?",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    insert_optin_profile(&pool, "111").await;

    let now = naive("2026-06-14T11:02:00");
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("lauf");
    assert_eq!(reminder_count(&pool, "sent_tournament_reminders", id).await, 0);
}

#[tokio::test]
async fn kein_reminder_ohne_opt_in_empfaenger() {
    let pool = temp_pool().await;
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    let id = insert_tournament(&pool, "T", "registration", false).await;
    sqlx::query(
        "UPDATE tournaments SET registration_end = '2026-06-14T12:00:00', \
                                reminder_offsets = '[60]' WHERE id = ?",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    // Kein Opt-in-Profil → keine Empfänger → kein Dedupe-Eintrag.

    let now = naive("2026-06-14T11:02:00");
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("lauf");
    assert_eq!(reminder_count(&pool, "sent_tournament_reminders", id).await, 0);
}
