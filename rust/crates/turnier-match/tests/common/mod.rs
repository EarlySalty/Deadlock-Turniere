//! Geteilte Test-Helfer: Temp-SQLite-Pool (App-DB via turnier-db) + Manager-Bau ohne
//! externe Dienste (kein Discord, keine Steam-Bridge).

#![allow(dead_code)]

use turnier_config::Config;
use turnier_db::{connect_str, run_migrations, Pool};
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
    let url = format!("sqlite:file:tb_match_test_{unique}?mode=memory&cache=shared");
    let pool = connect_str(&url, 1).await.expect("pool");
    run_migrations(&pool).await.expect("migrate");
    pool
}

/// Manager ohne Discord-Notifier und ohne Steam-Bridge. Damit testen wir die
/// reinen DB-Pfade (Result/Series); jeder Steam-Task scheitert deterministisch
/// mit „Bridge nicht verfügbar".
pub fn manager_without_services(pool: Pool) -> MatchManager {
    // Defaults aus Config (keine echten Tokens nötig für die DB-Pfade).
    let config = Config::from_env();
    MatchManager::new(pool, None, None, &config)
}

/// Legt ein Turnier an und gibt seine ID zurück.
pub async fn insert_tournament(pool: &Pool, name: &str, is_test: bool, auto_lobby: bool) -> i64 {
    let result = sqlx::query(
        "INSERT INTO tournaments (name, created_by, is_test, auto_lobby_enabled, team_size, \
                                  series_format, match_objective) \
         VALUES (?, 'tester', ?, ?, 6, 1, 'auto')",
    )
    .bind(name)
    .bind(is_test as i64)
    .bind(auto_lobby as i64)
    .execute(pool)
    .await
    .expect("insert tournament");
    result.last_insert_rowid()
}

/// Legt ein Team an und gibt seine ID zurück.
pub async fn insert_team(pool: &Pool, tournament_id: i64, name: &str) -> i64 {
    let result = sqlx::query(
        "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) \
         VALUES (?, ?, ?, 'cap')",
    )
    .bind(tournament_id)
    .bind(name)
    .bind(name.to_lowercase())
    .execute(pool)
    .await
    .expect("insert team");
    result.last_insert_rowid()
}

/// Legt ein Bracket-Match an und gibt seine ID zurück.
pub async fn insert_bracket_match(
    pool: &Pool,
    tournament_id: i64,
    round: i64,
    position: i64,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
    status: &str,
) -> i64 {
    let result = sqlx::query(
        "INSERT INTO bracket_matches (tournament_id, round, position, bracket_type, \
                                      team1_id, team2_id, status) \
         VALUES (?, ?, ?, 'winners', ?, ?, ?)",
    )
    .bind(tournament_id)
    .bind(round)
    .bind(position)
    .bind(team1_id)
    .bind(team2_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert bracket match");
    result.last_insert_rowid()
}

/// Legt eine Gruppe an und gibt ihre ID zurück.
pub async fn insert_group(pool: &Pool, tournament_id: i64, name: &str) -> i64 {
    let result = sqlx::query("INSERT INTO groups (tournament_id, name) VALUES (?, ?)")
        .bind(tournament_id)
        .bind(name)
        .execute(pool)
        .await
        .expect("insert group");
    result.last_insert_rowid()
}

/// Legt eine `group_teams`-Zeile an.
pub async fn insert_group_team(pool: &Pool, group_id: i64, team_id: i64) {
    sqlx::query("INSERT INTO group_teams (group_id, team_id) VALUES (?, ?)")
        .bind(group_id)
        .bind(team_id)
        .execute(pool)
        .await
        .expect("insert group team");
}

/// Legt ein Group-Match an und gibt seine ID zurück.
pub async fn insert_group_match(
    pool: &Pool,
    group_id: i64,
    team1_id: i64,
    team2_id: i64,
    status: &str,
) -> i64 {
    let result = sqlx::query(
        "INSERT INTO group_matches (group_id, team1_id, team2_id, status) VALUES (?, ?, ?, ?)",
    )
    .bind(group_id)
    .bind(team1_id)
    .bind(team2_id)
    .bind(status)
    .execute(pool)
    .await
    .expect("insert group match");
    result.last_insert_rowid()
}
