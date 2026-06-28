//! Geteilte Test-Helfer: Temp-SQLite-Pool (App-DB via turnier-db) + Manager/Notifier
//! ohne externe Dienste.
//!
//! Discord ist hier ein No-op-Fake: der [`DiscordNotifier`] hängt an einem
//! unkonfigurierten Broker (leere Basis-URL). `notify_users` liefert dann
//! deterministisch `Ok` (jede ID landet in `failed`/`Unconfigured`) — es geht KEIN
//! echter Netzwerk-Call raus, und der Dedupe-Pfad wird trotzdem ausgelöst.
//! Steam-Bridge ist `None`.

#![allow(dead_code)]

use std::sync::Arc;

use turnier_config::Config;
use turnier_db::{connect_str, run_migrations, Pool};
use turnier_discord::{BrokerClient, DiscordNotifier};
use turnier_match::MatchManager;

/// Frischer, isolierter In-Memory-Pool mit angewandter Migration (shared cache,
/// damit alle Pool-Connections dieselbe DB sehen).
pub async fn temp_pool() -> Pool {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    let unique = nanos
        .wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
        .wrapping_add((std::process::id() as u64) << 40);
    let url = format!("sqlite:file:tb_scheduler_test_{unique}?mode=memory&cache=shared");
    let pool = connect_str(&url, 1).await.expect("pool");
    run_migrations(&pool).await.expect("migrate");
    pool
}

/// Default-Config (keine echten Tokens nötig für die DB-Pfade).
pub fn test_config() -> Config {
    Config::from_env()
}

/// No-op-Notifier: unkonfigurierter Broker → kein echter Versand.
pub fn fake_notifier(pool: Pool, config: &Config) -> DiscordNotifier {
    let broker = BrokerClient::new("", "");
    DiscordNotifier::new(broker, pool, config)
}

/// MatchManager ohne Discord/Bridge — `schedule_auto_lobbies_for_tournament` ist
/// dann ein No-op (gated über `auto_lobby_enabled`, hier i. d. R. aus).
pub fn fake_match_manager(pool: Pool, config: &Config) -> Arc<MatchManager> {
    Arc::new(MatchManager::new(pool, None, None, config))
}

/// Legt ein Turnier mit gegebenem Status an und gibt seine ID zurück.
#[allow(clippy::too_many_arguments)]
pub async fn insert_tournament(pool: &Pool, name: &str, status: &str, is_test: bool) -> i64 {
    let result = sqlx::query(
        "INSERT INTO tournaments (name, status, created_by, is_test, auto_lobby_enabled, \
                                  team_size, series_format, match_objective) \
         VALUES (?, ?, 'tester', ?, 0, 6, 1, 'auto')",
    )
    .bind(name)
    .bind(status)
    .bind(is_test as i64)
    .execute(pool)
    .await
    .expect("insert tournament");
    result.last_insert_rowid()
}

/// Liest den aktuellen Status eines Turniers.
pub async fn tournament_status(pool: &Pool, tournament_id: i64) -> String {
    let (status,): (String,) = sqlx::query_as("SELECT status FROM tournaments WHERE id = ?")
        .bind(tournament_id)
        .fetch_one(pool)
        .await
        .expect("status");
    status
}

/// Zählt die Audit-Einträge einer bestimmten Action.
pub async fn audit_count(pool: &Pool, action: &str) -> i64 {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE action = ?")
            .bind(action)
            .fetch_one(pool)
            .await
            .expect("audit count");
    count
}

/// Zählt die Dedupe-Einträge einer Reminder-Tabelle für ein Turnier.
pub async fn reminder_count(pool: &Pool, table: &str, tournament_id: i64) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE tournament_id = ?");
    let (count,): (i64,) = sqlx::query_as(&sql)
        .bind(tournament_id)
        .fetch_one(pool)
        .await
        .expect("reminder count");
    count
}
