//! Opt-out-Guards fuer alle DM-Sendepfade im Discord-Notifier.

use chrono::{DateTime, Utc};
use turnier_config::Config;
use turnier_db::{test_pool, Pool, TestDb};
use turnier_discord::{BrokerClient, DiscordNotifier, NotificationEvent};

async fn fresh_db() -> TestDb {
    test_pool().await.expect("central test pool")
}

fn notifier(pool: Pool) -> DiscordNotifier {
    let mut config = Config::default();
    config.discord_master_broker_base_url = String::new();
    config.discord_master_broker_token = String::new();
    DiscordNotifier::new(BrokerClient::from_config(&config), pool, &config)
}

async fn insert_optout(pool: &Pool, discord_id: &str) {
    sqlx::query(
        "INSERT INTO turnier.user_profiles \
             (discord_id, invite_auto_accept, notify_discord_dm, notify_browser, updated_at, \
              notify_match_start, notify_checkin, notify_team_invite, notify_tournament_news, \
              notify_registration_reminder) \
         VALUES ($1, true, true, true, $2, true, true, true, false, true)",
    )
    .bind(discord_id.parse::<i64>().expect("numeric discord id"))
    .bind(parse_utc("2026-06-30T00:00:00Z"))
    .execute(pool)
    .await
    .expect("insert profile");

    sqlx::query(
        "INSERT INTO turnier.tournament_dm_optout (discord_id, scope, created_at) \
         VALUES ($1, 'all', $2)",
    )
    .bind(discord_id.parse::<i64>().expect("numeric discord id"))
    .bind(parse_utc("2026-06-30T00:00:00Z"))
    .execute(pool)
    .await
    .expect("insert optout");
}

#[tokio::test]
async fn notify_users_skips_tournament_dm_optout_before_send() {
    let db = fresh_db().await;
    let pool = db.pool();
    let discord_id = "123456789012345678".to_string();
    insert_optout(pool, &discord_id).await;

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
}

#[tokio::test]
async fn notify_casters_match_created_skips_tournament_dm_optout_before_send() {
    let db = fresh_db().await;
    let pool = db.pool();
    let discord_id = "123456789012345679".to_string();
    insert_optout(pool, &discord_id).await;

    let result = notifier(pool.clone())
        .notify_casters_match_created(7, "123456789012345680", std::slice::from_ref(&discord_id))
        .await
        .expect("opt-out skips before broker send");

    assert!(result.sent.is_empty());
    assert!(result.failed.is_empty());
}

fn parse_utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}
