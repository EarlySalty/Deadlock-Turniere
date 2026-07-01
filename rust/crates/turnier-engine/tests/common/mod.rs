//! Geteilte Test-Helfer: zentrale Wegwerf-PG-DB + Fake-Resolver.
//!
//! Nicht jede Testdatei nutzt jeden Helfer — `dead_code` ist hier erwartbar.
#![allow(dead_code)]

use async_trait::async_trait;
use std::collections::HashMap;
use turnier_core::{now_utc, RankProfile};
use turnier_db::{test_pool, Pool, TestDb};
use turnier_steam::{RankResolver, SteamResult};

pub const TEST_ADMIN_ID: i64 = 123_456_789_012_345_700;

/// Frische, isolierte PG-Testdatenbank mit zentralen Migrationen.
pub async fn temp_pool() -> TestDb {
    test_pool().await.expect("central test pool")
}

pub async fn insert_tournament(
    pool: &Pool,
    name: &str,
    status: &str,
    team_size: i64,
    bracket_format: &str,
    tournament_mode: &str,
) -> i64 {
    let now = now_utc();
    sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
             (name, status, team_size, bracket_format, created_by, created_at, updated_at, \
              invite_mode, tournament_mode, series_format, exclude_from_leaderboard, \
              tournament_game_mode, auto_lobby_enabled, is_test, match_objective, \
              no_show_grace_minutes, source) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, \
                 'always', $8, 1, false, 'standard', false, true, 'auto', 10, 'test') \
         RETURNING id",
    )
    .bind(name)
    .bind(status)
    .bind(team_size)
    .bind(bracket_format)
    .bind(TEST_ADMIN_ID)
    .bind(now)
    .bind(now)
    .bind(tournament_mode)
    .fetch_one(pool)
    .await
    .expect("insert tournament")
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

pub async fn insert_team_member(
    pool: &Pool,
    team_id: i64,
    discord_id: i64,
    discord_name: &str,
    role: &str,
    rank_score: i64,
) {
    sqlx::query(
        "INSERT INTO turnier.team_members \
             (team_id, discord_id, discord_name, rank_score, role, joined_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(team_id)
    .bind(discord_id)
    .bind(discord_name)
    .bind(rank_score)
    .bind(role)
    .bind(now_utc())
    .execute(pool)
    .await
    .expect("insert team member");
}

pub async fn insert_signup(
    pool: &Pool,
    tournament_id: i64,
    discord_id: i64,
    discord_name: &str,
    team_id: Option<i64>,
    rank_score: i64,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.tournament_signups \
             (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at) \
         VALUES ($1, $2, $3, NULL, NULL, $4, $5, $6) RETURNING id",
    )
    .bind(tournament_id)
    .bind(discord_id)
    .bind(discord_name)
    .bind(rank_score)
    .bind(team_id)
    .bind(now_utc())
    .fetch_one(pool)
    .await
    .expect("insert signup")
}

pub async fn insert_checkin(pool: &Pool, tournament_id: i64, discord_id: i64) {
    sqlx::query(
        "INSERT INTO turnier.tournament_checkins (tournament_id, discord_id, checked_in_at) \
         VALUES ($1, $2, $3)",
    )
    .bind(tournament_id)
    .bind(discord_id)
    .bind(now_utc())
    .execute(pool)
    .await
    .expect("insert checkin");
}

pub async fn insert_group(pool: &Pool, tournament_id: i64, name: &str, seeding_order: i64) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.groups (tournament_id, name, seeding_order) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tournament_id)
    .bind(name)
    .bind(seeding_order)
    .fetch_one(pool)
    .await
    .expect("insert group")
}

pub async fn insert_group_team(
    pool: &Pool,
    group_id: i64,
    team_id: i64,
    points: i64,
    wins: i64,
    losses: i64,
) {
    sqlx::query(
        "INSERT INTO turnier.group_teams (group_id, team_id, wins, losses, points) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(group_id)
    .bind(team_id)
    .bind(wins)
    .bind(losses)
    .bind(points)
    .execute(pool)
    .await
    .expect("insert group team");
}

/// Resolver, der für jeden Spieler `None` liefert (kein Steam/Discord-Lookup).
/// So nutzt `assign_random_teams` den im Signup gespeicherten rank_score.
pub struct NullResolver;

#[async_trait]
impl RankResolver for NullResolver {
    async fn rank_profile(&self, _discord_id: &str) -> SteamResult<Option<RankProfile>> {
        Ok(None)
    }

    async fn rank_profiles(
        &self,
        _discord_ids: &[String],
    ) -> SteamResult<HashMap<String, RankProfile>> {
        Ok(HashMap::new())
    }
}
