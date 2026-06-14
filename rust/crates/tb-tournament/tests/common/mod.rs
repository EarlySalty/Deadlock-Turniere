//! Geteilte Test-Helfer: Temp-SQLite-Pool (App-DB via tb-db) + Fake-Resolver.
//!
//! Nicht jede Testdatei nutzt jeden Helfer — `dead_code` ist hier erwartbar.
#![allow(dead_code)]

use async_trait::async_trait;
use std::collections::HashMap;
use tb_core::RankProfile;
use tb_db::{connect_str, run_migrations, Pool};
use tb_steam::{RankResolver, SteamResult};

/// Frischer, isolierter In-Memory-Pool mit angewandter Migration (shared cache,
/// damit alle Pool-Connections dieselbe DB sehen).
pub async fn temp_pool() -> Pool {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let unique = nanos
        .wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
        .wrapping_add((std::process::id() as u64) << 40);
    let url = format!("sqlite:file:tb_tournament_test_{unique}?mode=memory&cache=shared");
    let pool = connect_str(&url, 1).await.expect("pool");
    run_migrations(&pool).await.expect("migrate");
    pool
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
