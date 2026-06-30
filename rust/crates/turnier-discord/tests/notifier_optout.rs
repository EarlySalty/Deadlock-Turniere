//! Opt-out-Guards fuer alle DM-Sendepfade im Discord-Notifier.

use std::path::PathBuf;

use turnier_config::Config;
use turnier_db::{connect, run_migrations, Pool};
use turnier_discord::{BrokerClient, DiscordNotifier, NotificationEvent};

fn temp_db(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "tb_discord_optout_{}_{}.db",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

async fn fresh_pool(tag: &str) -> (Pool, PathBuf) {
    let path = temp_db(tag);
    let pool = connect(&path, 2).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    (pool, path)
}

fn notifier(pool: Pool) -> DiscordNotifier {
    let mut config = Config::from_env();
    config.discord_master_broker_base_url = String::new();
    config.discord_master_broker_token = String::new();
    DiscordNotifier::new(BrokerClient::from_config(&config), pool, &config)
}

async fn insert_optout(pool: &Pool, discord_id: &str) {
    sqlx::query(
        "INSERT INTO user_profiles \
             (discord_id, notify_discord_dm, notify_match_start, updated_at) \
         VALUES (?, 1, 1, ?)",
    )
    .bind(discord_id)
    .bind("2026-06-30T00:00:00Z")
    .execute(pool)
    .await
    .expect("insert profile");

    sqlx::query("INSERT INTO tournament_dm_optout (discord_id, scope) VALUES (?, 'all')")
        .bind(discord_id)
        .execute(pool)
        .await
        .expect("insert optout");
}

#[tokio::test]
async fn notify_users_skips_tournament_dm_optout_before_send() {
    let (pool, path) = fresh_pool("notify_users").await;
    let discord_id = "123456789012345678".to_string();
    insert_optout(&pool, &discord_id).await;

    let result = notifier(pool.clone())
        .notify_users(
            std::slice::from_ref(&discord_id),
            NotificationEvent::MatchStart,
            "Platzhalter",
        )
        .await
        .expect("opt-out skips before broker send");

    assert!(result.sent.is_empty());
    assert_eq!(result.skipped, vec![discord_id]);
    assert!(result.failed.is_empty());

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn notify_casters_match_created_skips_tournament_dm_optout_before_send() {
    let (pool, path) = fresh_pool("notify_casters").await;
    let discord_id = "123456789012345679".to_string();
    insert_optout(&pool, &discord_id).await;

    let result = notifier(pool.clone())
        .notify_casters_match_created(7, "123456789012345680", std::slice::from_ref(&discord_id))
        .await
        .expect("opt-out skips before broker send");

    assert!(result.sent.is_empty());
    assert!(result.failed.is_empty());

    pool.close().await;
    let _ = std::fs::remove_file(&path);
}
