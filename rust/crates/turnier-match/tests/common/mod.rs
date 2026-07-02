//! Geteilte Test-Helfer: zentrale Wegwerf-PG-DB + Manager-Bau ohne externe
//! Dienste (kein Discord, keine Steam-Bridge).

#![allow(dead_code)]

use turnier_config::Config;
use turnier_core::now_utc;
use turnier_db::{test_pool, Pool, TestDb};
use turnier_match::MatchManager;

pub const TEST_ADMIN_ID: i64 = 123456789012345700;
pub const TEST_CAPTAIN_ID: i64 = 123456789012345701;
pub const TEST_CASTER_ID: i64 = 123456789012345702;
pub const TEST_FALLBACK_CASTER_ID: i64 = 123456789012345703;

/// Frische, isolierte PG-Testdatenbank mit zentralen Migrationen.
pub async fn temp_db() -> TestDb {
    test_pool().await.expect("central test pool")
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
    insert_tournament_with_series(pool, name, is_test, auto_lobby, 1).await
}

/// Legt ein Turnier mit frei wählbarem Serienformat an.
pub async fn insert_tournament_with_series(
    pool: &Pool,
    name: &str,
    is_test: bool,
    auto_lobby: bool,
    series_format: i64,
) -> i64 {
    let now = now_utc();
    sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
             (name, status, team_size, bracket_format, created_by, created_at, updated_at, \
              invite_mode, tournament_mode, series_format, exclude_from_leaderboard, \
              tournament_game_mode, auto_lobby_enabled, is_test, match_objective, \
              no_show_grace_minutes, source, lobby_settings) \
         VALUES ($1, 'bracket', 6, 'single_elimination', $2, $3, $4, \
                 'always', 'bracket_only', $5, false, 'standard', $6, $7, \
                 'auto', 10, 'manual', '{}'::jsonb) \
         RETURNING id",
    )
    .bind(name)
    .bind(TEST_ADMIN_ID)
    .bind(now)
    .bind(now)
    .bind(series_format)
    .bind(auto_lobby)
    .bind(is_test)
    .fetch_one(pool)
    .await
    .expect("insert tournament")
}

/// Legt ein Team an und gibt seine ID zurück.
pub async fn insert_team(pool: &Pool, tournament_id: i64, name: &str) -> i64 {
    let now = now_utc();
    sqlx::query_scalar(
        "INSERT INTO turnier.teams \
             (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status) \
         VALUES ($1, $2, $3, $4, $5, 'open') RETURNING id",
    )
    .bind(tournament_id)
    .bind(name)
    .bind(name.to_lowercase())
    .bind(TEST_CAPTAIN_ID)
    .bind(now)
    .fetch_one(pool)
    .await
    .expect("insert team")
}

/// Legt ein Team-Mitglied an.
pub async fn insert_team_member(
    pool: &Pool,
    team_id: i64,
    discord_id: i64,
    discord_name: &str,
    steam_id: Option<&str>,
) -> i64 {
    let now = now_utc();
    sqlx::query_scalar(
        "INSERT INTO turnier.team_members \
             (team_id, discord_id, discord_name, steam_id, rank_score, role, joined_at) \
         VALUES ($1, $2, $3, $4, 0, 'member', $5) RETURNING id",
    )
    .bind(team_id)
    .bind(discord_id)
    .bind(discord_name)
    .bind(steam_id)
    .bind(now)
    .fetch_one(pool)
    .await
    .expect("insert team member")
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
    sqlx::query_scalar(
        "INSERT INTO turnier.bracket_matches \
             (tournament_id, round, position, bracket_type, team1_id, team2_id, status, on_stream) \
         VALUES ($1, $2, $3, 'winners', $4, $5, $6, false) RETURNING id",
    )
    .bind(tournament_id)
    .bind(round)
    .bind(position)
    .bind(team1_id)
    .bind(team2_id)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("insert bracket match")
}

/// Legt eine Gruppe an und gibt ihre ID zurück.
pub async fn insert_group(pool: &Pool, tournament_id: i64, name: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.groups (tournament_id, name, seeding_order) \
         VALUES ($1, $2, 0) RETURNING id",
    )
    .bind(tournament_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert group")
}

/// Legt eine `group_teams`-Zeile an.
pub async fn insert_group_team(pool: &Pool, group_id: i64, team_id: i64) {
    sqlx::query(
        "INSERT INTO turnier.group_teams (group_id, team_id, wins, losses, points) \
         VALUES ($1, $2, 0, 0, 0)",
    )
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
    sqlx::query_scalar(
        "INSERT INTO turnier.group_matches (group_id, team1_id, team2_id, status) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(group_id)
    .bind(team1_id)
    .bind(team2_id)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("insert group match")
}

pub async fn insert_tournament_caster(pool: &Pool, tournament_id: i64, discord_id: i64) {
    sqlx::query(
        "INSERT INTO turnier.tournament_casters (tournament_id, discord_id, assigned_at, assigned_by) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(tournament_id)
    .bind(discord_id)
    .bind(now_utc())
    .bind(TEST_ADMIN_ID)
    .execute(pool)
    .await
    .expect("insert tournament caster");
}

pub async fn insert_match_caster(pool: &Pool, match_id: i64, match_type: &str, discord_id: i64) {
    sqlx::query(
        "INSERT INTO turnier.match_casters (match_id, match_type, discord_id, assigned_at, assigned_by) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(match_id)
    .bind(match_type)
    .bind(discord_id)
    .bind(now_utc())
    .bind(TEST_ADMIN_ID)
    .execute(pool)
    .await
    .expect("insert match caster");
}
