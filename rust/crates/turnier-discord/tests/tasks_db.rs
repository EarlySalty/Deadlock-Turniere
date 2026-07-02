//! Integrationstest gegen eine Wegwerf-PG-DB: die `discord_tasks`-Queries und der
//! Notify-Flag-SELECT müssen exakt zum Migrationsschema passen. KEINE externen
//! Dienste (kein Broker, kein Discord) — nur Persistenz.

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::Row;
use turnier_db::{test_pool, TestDb};
use turnier_discord::event::NotificationEvent;
use turnier_discord::tasks::{self, TaskType};

async fn fresh_db() -> TestDb {
    test_pool().await.expect("central test pool")
}

#[tokio::test]
async fn discord_task_lebenszyklus_running_done() {
    let db = fresh_db().await;
    let pool = db.pool();

    let id = tasks::create_running(pool, TaskType::CreateChannel, &json!({ "match_id": 7 }))
        .await
        .expect("create");

    let (status, payload): (String, serde_json::Value) =
        sqlx::query("SELECT status, payload FROM turnier.discord_tasks WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .map(|r| (r.get("status"), r.get("payload")))
            .expect("select");
    assert_eq!(status, "RUNNING");
    assert_eq!(payload["match_id"], 7);

    tasks::mark_done(pool, id, Some(&json!({ "channel_id": "999" })))
        .await
        .expect("done");

    let (status, result, error): (String, Option<serde_json::Value>, Option<String>) = sqlx::query(
        "SELECT status, result_payload, error FROM turnier.discord_tasks WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map(|r| (r.get("status"), r.get("result_payload"), r.get("error")))
    .expect("select2");
    assert_eq!(status, "DONE");
    assert_eq!(result.unwrap()["channel_id"], "999");
    assert!(error.is_none());
}

#[tokio::test]
async fn discord_task_failed_speichert_fehlertext() {
    let db = fresh_db().await;
    let pool = db.pool();

    let id = tasks::create_running(pool, TaskType::SendDm, &json!({ "x": 1 }))
        .await
        .expect("create");
    tasks::mark_failed(pool, id, "Discord-Broker Fehler")
        .await
        .expect("failed");

    let (status, error): (String, Option<String>) =
        sqlx::query("SELECT status, error FROM turnier.discord_tasks WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .map(|r| (r.get("status"), r.get("error")))
            .expect("select");
    assert_eq!(status, "FAILED");
    assert_eq!(error.as_deref(), Some("Discord-Broker Fehler"));
}

#[tokio::test]
async fn notify_flag_select_passt_zum_schema() {
    let db = fresh_db().await;
    let pool = db.pool();

    // Ein Profil mit DM aus + match_start an einfügen.
    sqlx::query(
        "INSERT INTO turnier.user_profiles \
             (discord_id, invite_auto_accept, notify_discord_dm, notify_browser, updated_at, \
              notify_match_start, notify_checkin, notify_team_invite, notify_tournament_news, \
              notify_registration_reminder) \
         VALUES ($1, true, false, true, $2, true, true, true, false, true)",
    )
    .bind(123_i64)
    .bind(parse_utc("2026-06-14T00:00:00Z"))
    .execute(pool)
    .await
    .expect("insert profile");

    // Genau die im Notifier verwendete Spalten-Whitelist prüfen.
    let column = NotificationEvent::MatchStart.column();
    let sql = format!(
        "SELECT notify_discord_dm AS dm, {column} AS ev FROM turnier.user_profiles WHERE discord_id = $1"
    );
    let row = sqlx::query(&sql)
        .bind(123_i64)
        .fetch_one(pool)
        .await
        .expect("select flags");
    let dm: bool = row.get("dm");
    let ev: bool = row.get("ev");
    assert!(!dm, "DM-Master-Schalter aus");
    assert!(ev, "match_start an");
}

fn parse_utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
