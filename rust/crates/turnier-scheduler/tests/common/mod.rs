//! Geteilte Test-Helfer: zentrale Wegwerf-PG-DB + Manager/Notifier ohne externe
//! Dienste.
//!
//! Discord ist hier ein No-op-Fake: der [`DiscordNotifier`] hängt an einem
//! unkonfigurierten Broker (leere Basis-URL). `notify_users` liefert dann
//! deterministisch `Ok` (jede ID landet in `failed`/`Unconfigured`) — es geht KEIN
//! echter Netzwerk-Call raus, und der Dedupe-Pfad wird trotzdem ausgelöst.
//! Steam-Bridge ist `None`.

#![allow(dead_code)]

use std::sync::Arc;

use turnier_config::Config;
use turnier_core::now_utc;
use turnier_db::{dynamic_sql::ReminderDedupeTable, test_pool, Pool, TestDb};
use turnier_discord::{BrokerClient, DiscordNotifier};
use turnier_match::MatchManager;

pub const TEST_ADMIN_ID: i64 = 123_456_789_012_345_700;

/// Frische, isolierte PG-Testdatenbank mit zentralen Migrationen.
pub async fn temp_db() -> TestDb {
    test_pool().await.expect("central test pool")
}

/// Default-Config (keine echten Tokens nötig für die DB-Pfade).
pub fn test_config() -> Config {
    Config::default()
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
    let now = now_utc();
    sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
             (name, status, created_by, is_test, auto_lobby_enabled, team_size, \
              series_format, match_objective, created_at, updated_at, bracket_format, \
              invite_mode, tournament_mode, exclude_from_leaderboard, tournament_game_mode, \
              no_show_grace_minutes, source) \
         VALUES ($1, $2, $3, $4, false, 6, 1, 'auto', $5, $6, 'single_elimination', \
                 'always', 'bracket_only', false, 'standard', 10, 'test') \
         RETURNING id",
    )
    .bind(name)
    .bind(status)
    .bind(TEST_ADMIN_ID)
    .bind(is_test)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
    .expect("insert tournament")
}

/// Liest den aktuellen Status eines Turniers.
pub async fn tournament_status(pool: &Pool, tournament_id: i64) -> String {
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM turnier.tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_one(pool)
            .await
            .expect("status");
    status
}

/// Zählt die Audit-Einträge einer bestimmten Action.
pub async fn audit_count(pool: &Pool, action: &str) -> i64 {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM turnier.audit_log WHERE action = $1")
            .bind(action)
            .fetch_one(pool)
            .await
            .expect("audit count");
    count
}

/// Zählt die Dedupe-Einträge einer Reminder-Tabelle für ein Turnier.
pub async fn reminder_count(pool: &Pool, table: &str, tournament_id: i64) -> i64 {
    let table = ReminderDedupeTable::from_unqualified_name(table).expect("allowed reminder table");
    let sql = format!(
        "SELECT COUNT(*) FROM {} WHERE tournament_id = $1",
        table.qualified_name()
    );
    let (count,): (i64,) = sqlx::query_as(&sql)
        .bind(tournament_id)
        .fetch_one(pool)
        .await
        .expect("reminder count");
    count
}

/// Zählt Match-Reminder-Dedupe-Einträge für ein Bracket-Match.
pub async fn match_reminder_count(pool: &Pool, match_id: i64) -> i64 {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM turnier.sent_match_reminders \
         WHERE match_type = 'bracket' AND match_id = $1 AND kind = 'next_up'",
    )
    .bind(match_id)
    .fetch_one(pool)
    .await
    .expect("match reminder count");
    count
}

pub async fn insert_team(pool: &Pool, tournament_id: i64, name: &str, captain_id: i64) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.teams \
             (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status) \
         VALUES ($1, $2, $3, $4, $5, 'open') RETURNING id",
    )
    .bind(tournament_id)
    .bind(name)
    .bind(name.to_lowercase())
    .bind(captain_id)
    .bind(now_utc())
    .fetch_one(pool)
    .await
    .expect("insert team")
}

pub async fn insert_team_member(pool: &Pool, team_id: i64, discord_id: i64) {
    sqlx::query(
        "INSERT INTO turnier.team_members (team_id, discord_id, role, joined_at) \
         VALUES ($1, $2, 'member', $3)",
    )
    .bind(team_id)
    .bind(discord_id)
    .bind(now_utc())
    .execute(pool)
    .await
    .expect("insert team member");
}

pub async fn insert_signup(pool: &Pool, tournament_id: i64, discord_id: i64) {
    sqlx::query(
        "INSERT INTO turnier.tournament_signups (tournament_id, discord_id, signed_up_at) \
         VALUES ($1, $2, $3)",
    )
    .bind(tournament_id)
    .bind(discord_id)
    .bind(now_utc())
    .execute(pool)
    .await
    .expect("insert signup");
}

pub async fn insert_pending_bracket_match(
    pool: &Pool,
    tournament_id: i64,
    team1_id: i64,
    team2_id: i64,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.bracket_matches \
             (tournament_id, round, position, bracket_type, team1_id, team2_id, status, on_stream) \
         VALUES ($1, 1, 0, 'winners', $2, $3, 'pending', false) RETURNING id",
    )
    .bind(tournament_id)
    .bind(team1_id)
    .bind(team2_id)
    .fetch_one(pool)
    .await
    .expect("insert bracket match")
}
