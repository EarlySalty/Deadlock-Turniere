#![cfg(feature = "testing")]

//! Integrationstests gegen eine zentrale Wegwerf-Postgres-DB + Fake-Discord.
//!
//! Kein echter Discord-/Steam-Dienst: der Discord-Client ist gefaket, die
//! Bridge-Stufe ist hier nicht bestückt (eigene Bridge-DB wäre ein fremdes
//! Schema). Geprüft werden Cache-Round-Trip, TTL-Verfall und der
//! Discord-Rollen-Fallback inkl. Persistenz im `rank_cache`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use turnier_core::RankProfile;
use turnier_db::{test_pool, Pool, TestDb};
use turnier_steam::{
    DiscordMemberClient, RankCache, RankResolver, SteamRankResolver, SteamResult,
    SOURCE_DISCORD_ROLE,
};

/// Fake-Discord-Client: liefert vorgegebene Rollen-IDs pro Discord-ID.
struct FakeDiscord {
    members: HashMap<String, Vec<String>>,
}

#[async_trait]
impl DiscordMemberClient for FakeDiscord {
    async fn member_role_ids(&self, discord_id: &str) -> SteamResult<Option<Vec<String>>> {
        Ok(self.members.get(discord_id).cloned())
    }
}

struct DbCtx {
    _db: TestDb,
    pool: Pool,
}

async fn temp_pool() -> DbCtx {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool().clone();
    DbCtx { _db: db, pool }
}

fn id(value: &str) -> i64 {
    value.parse::<i64>().expect("numeric discord id")
}

#[tokio::test]
async fn cache_store_and_get_roundtrip() {
    const USER_A: &str = "910000020000000001";

    let db = temp_pool().await;
    let cache = RankCache::new(db.pool);

    let profile = RankProfile {
        steam_id: Some("STEAM_1".into()),
        rank: Some("Archon".into()),
        rank_tier: Some(7),
        subrank: Some(4),
        rank_score: 7 * 6 + 4,
        source: "steam_bridge".into(),
    };

    cache.store(USER_A, &profile).await.expect("store");

    // Aus dem (frischen) Cache lesbar.
    let got = cache.get(USER_A).await.expect("get").expect("hit");
    assert_eq!(got.rank.as_deref(), Some("Archon"));
    assert_eq!(got.rank_score, 7 * 6 + 4);
    assert_eq!(got.source, "steam_bridge");
}

#[tokio::test]
async fn ttl_filter_hides_expired_rows() {
    const OLD_USER: &str = "910000020000000002";

    let db = temp_pool().await;
    // Manuell einen abgelaufenen Eintrag setzen (cached_at weit in der Vergangenheit).
    sqlx::query(
        "INSERT INTO turnier.\"rank_cache\" \
         (discord_id, source, steam_id, rank, rank_tier, subrank, rank_score, cached_at) \
         VALUES ($1, 'steam_bridge', 's', 'Oracle', 8, 3, 51, now() - interval '25 hours')",
    )
    .bind(id(OLD_USER))
    .execute(&db.pool)
    .await
    .expect("insert expired");

    let cache = RankCache::new(db.pool);
    // Kein L1 vorgewärmt -> L2-SELECT mit TTL-Filter muss den alten Eintrag verwerfen.
    let got = cache.get(OLD_USER).await.expect("get");
    assert!(
        got.is_none(),
        "abgelaufener Eintrag darf nicht geliefert werden"
    );
}

#[tokio::test]
async fn discord_fallback_resolves_and_caches() {
    const DISCORD_USER: &str = "910000020000000003";

    let db = temp_pool().await;

    let mut members = HashMap::new();
    // Archon-Haupt-Tier-Rolle (Tier 7), kein Subrank-Mapping -> Default-Subrank 3.
    members.insert(
        DISCORD_USER.to_string(),
        vec!["1331457949654319114".to_string()],
    );
    let discord: Arc<dyn DiscordMemberClient> = Arc::new(FakeDiscord { members });

    let resolver = SteamRankResolver::from_pool(db.pool.clone(), None, Some(discord));

    let profile = resolver
        .rank_profile(DISCORD_USER)
        .await
        .expect("resolve")
        .expect("profile");
    assert_eq!(profile.rank.as_deref(), Some("Archon"));
    assert_eq!(profile.rank_tier, Some(7));
    assert_eq!(profile.subrank, Some(3));
    assert_eq!(profile.rank_score, 7 * 6 + 3);
    assert_eq!(profile.source, SOURCE_DISCORD_ROLE);

    // Persistiert: ein direkter Cache-Read liefert dasselbe Profil.
    let cache = RankCache::new(db.pool);
    let cached = cache.get(DISCORD_USER).await.expect("get").expect("hit");
    assert_eq!(cached.rank.as_deref(), Some("Archon"));
    assert_eq!(cached.source, SOURCE_DISCORD_ROLE);
}

#[tokio::test]
async fn unknown_user_resolves_to_none() {
    const UNKNOWN_USER: &str = "910000020000000004";

    let db = temp_pool().await;
    let discord: Arc<dyn DiscordMemberClient> = Arc::new(FakeDiscord {
        members: HashMap::new(),
    });
    let resolver = SteamRankResolver::from_pool(db.pool, None, Some(discord));

    assert!(resolver
        .rank_profile(UNKNOWN_USER)
        .await
        .expect("resolve")
        .is_none());
}

#[tokio::test]
async fn batch_lookup_dedups_and_collects() {
    const USER_A: &str = "910000020000000005";
    const USER_B: &str = "910000020000000006";

    let db = temp_pool().await;
    let mut members = HashMap::new();
    members.insert(USER_A.to_string(), vec!["1316966867033653338".to_string()]); // Oracle (8)
    let discord: Arc<dyn DiscordMemberClient> = Arc::new(FakeDiscord { members });
    let resolver = SteamRankResolver::from_pool(db.pool, None, Some(discord));

    let ids = vec![USER_A.to_string(), USER_A.to_string(), USER_B.to_string()];
    let map = resolver.rank_profiles(&ids).await.expect("batch");
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(USER_A).unwrap().rank.as_deref(), Some("Oracle"));
    assert!(!map.contains_key(USER_B));
}
