//! Integrationstest gegen eine Temp-SQLite: die `discord_tasks`-Queries und der
//! Notify-Flag-SELECT müssen exakt zum Migrationsschema passen. KEINE externen
//! Dienste (kein Broker, kein Discord) — nur Persistenz.

use std::path::PathBuf;

use serde_json::json;
use sqlx::Row;
use tb_db::{connect, run_migrations, Pool};
use tb_discord::event::NotificationEvent;
use tb_discord::tasks::{self, TaskType};

fn temp_db(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("tb_discord_{}_{}.db", tag, std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

async fn fresh_pool(tag: &str) -> (Pool, PathBuf) {
    let path = temp_db(tag);
    let pool = connect(&path, 2).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    (pool, path)
}

#[tokio::test]
async fn discord_task_lebenszyklus_running_done() {
    let (pool, path) = fresh_pool("done").await;

    let id = tasks::create_running(&pool, TaskType::CreateChannel, &json!({ "match_id": 7 }))
        .await
        .expect("create");

    let (status, payload): (String, String) =
        sqlx::query("SELECT status, payload FROM discord_tasks WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .map(|r| (r.get("status"), r.get("payload")))
            .expect("select");
    assert_eq!(status, "RUNNING");
    assert!(payload.contains("\"match_id\":7"));

    tasks::mark_done(&pool, id, Some(&json!({ "channel_id": "999" }))).await.expect("done");

    let (status, result, error): (String, Option<String>, Option<String>) =
        sqlx::query("SELECT status, result_payload, error FROM discord_tasks WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .map(|r| (r.get("status"), r.get("result_payload"), r.get("error")))
            .expect("select2");
    assert_eq!(status, "DONE");
    assert!(result.unwrap().contains("999"));
    assert!(error.is_none());

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn discord_task_failed_speichert_fehlertext() {
    let (pool, path) = fresh_pool("failed").await;

    let id = tasks::create_running(&pool, TaskType::SendDm, &json!({ "x": 1 })).await.expect("create");
    tasks::mark_failed(&pool, id, "Discord-Broker Fehler").await.expect("failed");

    let (status, error): (String, Option<String>) =
        sqlx::query("SELECT status, error FROM discord_tasks WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .map(|r| (r.get("status"), r.get("error")))
            .expect("select");
    assert_eq!(status, "FAILED");
    assert_eq!(error.as_deref(), Some("Discord-Broker Fehler"));

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn notify_flag_select_passt_zum_schema() {
    let (pool, path) = fresh_pool("flags").await;

    // Ein Profil mit DM aus + match_start an einfügen.
    sqlx::query(
        "INSERT INTO user_profiles (discord_id, notify_discord_dm, notify_match_start, updated_at) \
         VALUES (?, 0, 1, ?)",
    )
    .bind("123")
    .bind("2026-06-14T00:00:00Z")
    .execute(&pool)
    .await
    .expect("insert profile");

    // Genau die im Notifier verwendete Spalten-Whitelist prüfen.
    let column = NotificationEvent::MatchStart.column();
    let sql = format!(
        "SELECT notify_discord_dm AS dm, {column} AS ev FROM user_profiles WHERE discord_id = ?"
    );
    let row = sqlx::query(&sql).bind("123").fetch_one(&pool).await.expect("select flags");
    let dm: i64 = row.get("dm");
    let ev: i64 = row.get("ev");
    assert_eq!(dm, 0, "DM-Master-Schalter aus");
    assert_eq!(ev, 1, "match_start an");

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}
