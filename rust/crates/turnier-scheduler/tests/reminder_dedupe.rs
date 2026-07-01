//! Integrationstests für Reminder-Dedupe mit zentraler Wegwerf-PG-DB.
//!
//! Discord ist ein No-op-Fake (unkonfigurierter Broker → `notify_users` liefert
//! `Ok`, jede ID landet in `failed`). Damit wird der Dedupe-Pfad ausgelöst, ohne
//! echten Versand: nach dem ersten Lauf steht genau EIN Dedupe-Eintrag, ein
//! zweiter Lauf im selben Fenster fügt keinen weiteren hinzu.
//!
//! Die Zeit ist injiziert (`now` als Parameter) — keine echte Zeitabhängigkeit.

mod common;

use chrono::{DateTime, Utc};

use common::{
    fake_notifier, insert_pending_bracket_match, insert_signup, insert_team, insert_team_member,
    insert_tournament, match_reminder_count, reminder_count, temp_db, test_config,
};
use serde_json::json;
use turnier_core::now_utc;
use turnier_db::Pool;
use turnier_scheduler::{
    check_and_send_match_reminders, check_and_send_registration_reminders,
    check_and_send_start_reminders,
};

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("{s}Z"))
        .unwrap()
        .with_timezone(&Utc)
}

/// Legt ein Opt-in-Profil an, damit die Registrierungs-Reminder Empfänger haben.
async fn insert_optin_profile(pool: &Pool, discord_id: i64) {
    sqlx::query(
        "INSERT INTO turnier.user_profiles \
             (discord_id, updated_at, invite_auto_accept, notify_discord_dm, notify_browser, \
              notify_match_start, notify_checkin, notify_team_invite, notify_tournament_news, \
              notify_registration_reminder) \
         VALUES ($1, $2, false, true, false, true, true, true, true, true)",
    )
    .bind(discord_id)
    .bind(now_utc())
    .execute(pool)
    .await
    .expect("insert profile");
}

async fn configure_registration_reminder(pool: &Pool, tournament_id: i64) {
    sqlx::query(
        "UPDATE turnier.tournaments \
         SET registration_end = $1, reminder_offsets = $2::jsonb WHERE id = $3",
    )
    .bind(utc("2026-06-14T12:00:00"))
    .bind(json!([60]))
    .bind(tournament_id)
    .execute(pool)
    .await
    .expect("configure registration reminder");
}

async fn configure_start_reminder(pool: &Pool, tournament_id: i64) {
    sqlx::query(
        "UPDATE turnier.tournaments \
         SET tournament_mode = 'group_stage', group_phase_start = $1, \
             start_reminder_offsets = $2::jsonb \
         WHERE id = $3",
    )
    .bind(utc("2026-06-14T12:00:00"))
    .bind(json!([60]))
    .bind(tournament_id)
    .execute(pool)
    .await
    .expect("configure start reminder");
}

#[tokio::test]
async fn registration_reminder_wird_genau_einmal_dedupliziert() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    // Turnier in 'registration' mit registration_end um 12:00 und nur einem
    // Offset (60 min) — der Reminder ist um 11:00 fällig.
    let id = insert_tournament(&pool, "T", "registration", false).await;
    configure_registration_reminder(&pool, id).await;
    insert_optin_profile(&pool, 111).await;

    // now liegt im 5-min-Fenster (11:00..=11:05).
    let now = utc("2026-06-14T11:02:00");
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("erster lauf");
    assert_eq!(
        reminder_count(&pool, "sent_tournament_reminders", id).await,
        1
    );

    // Zweiter Lauf im selben Fenster → Dedupe greift, kein weiterer Eintrag.
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("zweiter lauf");
    assert_eq!(
        reminder_count(&pool, "sent_tournament_reminders", id).await,
        1
    );
}

#[tokio::test]
async fn kein_reminder_ausserhalb_des_fensters() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    let id = insert_tournament(&pool, "T", "registration", false).await;
    configure_registration_reminder(&pool, id).await;
    insert_optin_profile(&pool, 111).await;

    // now ist 6 min nach dem fälligen Zeitpunkt (11:00) → außerhalb des Fensters.
    let now = utc("2026-06-14T11:06:00");
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("lauf");
    assert_eq!(
        reminder_count(&pool, "sent_tournament_reminders", id).await,
        0
    );
}

#[tokio::test]
async fn test_turnier_loest_keinen_reminder_aus() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    // is_test = true → übersprungen.
    let id = insert_tournament(&pool, "T", "registration", true).await;
    configure_registration_reminder(&pool, id).await;
    insert_optin_profile(&pool, 111).await;

    let now = utc("2026-06-14T11:02:00");
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("lauf");
    assert_eq!(
        reminder_count(&pool, "sent_tournament_reminders", id).await,
        0
    );
}

#[tokio::test]
async fn kein_reminder_ohne_opt_in_empfaenger() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    let id = insert_tournament(&pool, "T", "registration", false).await;
    configure_registration_reminder(&pool, id).await;
    // Kein Opt-in-Profil → keine Empfänger → kein Dedupe-Eintrag.

    let now = utc("2026-06-14T11:02:00");
    check_and_send_registration_reminders(&pool, &notifier, now)
        .await
        .expect("lauf");
    assert_eq!(
        reminder_count(&pool, "sent_tournament_reminders", id).await,
        0
    );
}

#[tokio::test]
async fn start_reminder_wird_an_teilnehmer_gesendet_und_dedupliziert() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    let id = insert_tournament(&pool, "T", "registration", false).await;
    configure_start_reminder(&pool, id).await;
    insert_signup(&pool, id, 222).await;

    let now = utc("2026-06-14T11:02:00");
    check_and_send_start_reminders(&pool, &notifier, now)
        .await
        .expect("erster start-lauf");
    assert_eq!(reminder_count(&pool, "sent_start_reminders", id).await, 1);

    check_and_send_start_reminders(&pool, &notifier, now)
        .await
        .expect("zweiter start-lauf");
    assert_eq!(reminder_count(&pool, "sent_start_reminders", id).await, 1);
}

#[tokio::test]
async fn match_reminder_wird_an_teammitglieder_gesendet_und_dedupliziert() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);

    let tournament_id = insert_tournament(&pool, "T", "bracket", false).await;
    let team1_id = insert_team(&pool, tournament_id, "Team 1", 301).await;
    let team2_id = insert_team(&pool, tournament_id, "Team 2", 302).await;
    insert_team_member(&pool, team1_id, 401).await;
    insert_team_member(&pool, team2_id, 402).await;
    let match_id = insert_pending_bracket_match(&pool, tournament_id, team1_id, team2_id).await;

    check_and_send_match_reminders(&pool, &notifier)
        .await
        .expect("erster match-lauf");
    assert_eq!(match_reminder_count(&pool, match_id).await, 1);

    check_and_send_match_reminders(&pool, &notifier)
        .await
        .expect("zweiter match-lauf");
    assert_eq!(match_reminder_count(&pool, match_id).await, 1);
}
