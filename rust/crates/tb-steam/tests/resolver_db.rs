//! Integrationstests gegen eine Temp-SQLite (App-DB via tb-db) + Fake-Discord.
//!
//! Kein echter Discord-/Steam-Dienst: der Discord-Client ist gefaket, die
//! Bridge-Stufe ist hier nicht bestückt (eigene Bridge-DB wäre ein fremdes
//! Schema). Geprüft werden Cache-Round-Trip, TTL-Verfall und der
//! Discord-Rollen-Fallback inkl. Persistenz im `rank_cache`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tb_core::RankProfile;
use tb_db::{connect_str, run_migrations, Pool};
use tb_steam::{
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

/// Erzeugt einen frischen In-Memory-Pool mit angewandter Migration.
async fn temp_pool() -> Pool {
    // Eigene benannte In-Memory-DB pro Test (shared cache), damit der Pool über
    // mehrere Connections dieselbe DB sieht.
    let url = format!(
        "sqlite:file:tb_steam_test_{}?mode=memory&cache=shared",
        std::process::id() as u64 * 1000 + rand_suffix()
    );
    let pool = connect_str(&url, 1).await.expect("pool");
    run_migrations(&pool).await.expect("migrate");
    pool
}

/// Kleiner, abhängigkeitsfreier Zufalls-Suffix für eindeutige DB-Namen.
fn rand_suffix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64
}

#[tokio::test]
async fn cache_store_and_get_roundtrip() {
    let pool = temp_pool().await;
    let cache = RankCache::new(pool);

    let profile = RankProfile {
        steam_id: Some("STEAM_1".into()),
        rank: Some("Archon".into()),
        rank_tier: Some(7),
        subrank: Some(4),
        rank_score: 7 * 6 + 4,
        source: "steam_bridge".into(),
    };

    cache.store("user_a", &profile).await.expect("store");

    // Aus dem (frischen) Cache lesbar.
    let got = cache.get("user_a").await.expect("get").expect("hit");
    assert_eq!(got.rank.as_deref(), Some("Archon"));
    assert_eq!(got.rank_score, 7 * 6 + 4);
    assert_eq!(got.source, "steam_bridge");
}

#[tokio::test]
async fn ttl_filter_hides_expired_rows() {
    let pool = temp_pool().await;
    // Manuell einen abgelaufenen Eintrag setzen (cached_at weit in der Vergangenheit).
    sqlx::query(
        "INSERT INTO rank_cache \
         (discord_id, source, steam_id, rank, rank_tier, subrank, rank_score, cached_at) \
         VALUES ('old_user', 'steam_bridge', 's', 'Oracle', 8, 3, 51, 0)",
    )
    .execute(&pool)
    .await
    .expect("insert expired");

    let cache = RankCache::new(pool);
    // Kein L1 vorgewärmt -> L2-SELECT mit TTL-Filter muss den alten Eintrag verwerfen.
    let got = cache.get("old_user").await.expect("get");
    assert!(got.is_none(), "abgelaufener Eintrag darf nicht geliefert werden");
}

#[tokio::test]
async fn discord_fallback_resolves_and_caches() {
    let pool = temp_pool().await;

    let mut members = HashMap::new();
    // Archon-Haupt-Tier-Rolle (Tier 7), kein Subrank-Mapping -> Default-Subrank 3.
    members.insert("u_disc".to_string(), vec!["1331457949654319114".to_string()]);
    let discord: Arc<dyn DiscordMemberClient> = Arc::new(FakeDiscord { members });

    let resolver = SteamRankResolver::from_pool(pool.clone(), None, Some(discord));

    let profile = resolver
        .rank_profile("u_disc")
        .await
        .expect("resolve")
        .expect("profile");
    assert_eq!(profile.rank.as_deref(), Some("Archon"));
    assert_eq!(profile.rank_tier, Some(7));
    assert_eq!(profile.subrank, Some(3));
    assert_eq!(profile.rank_score, 7 * 6 + 3);
    assert_eq!(profile.source, SOURCE_DISCORD_ROLE);

    // Persistiert: ein direkter Cache-Read liefert dasselbe Profil.
    let cache = RankCache::new(pool);
    let cached = cache.get("u_disc").await.expect("get").expect("hit");
    assert_eq!(cached.rank.as_deref(), Some("Archon"));
    assert_eq!(cached.source, SOURCE_DISCORD_ROLE);
}

#[tokio::test]
async fn unknown_user_resolves_to_none() {
    let pool = temp_pool().await;
    let discord: Arc<dyn DiscordMemberClient> = Arc::new(FakeDiscord {
        members: HashMap::new(),
    });
    let resolver = SteamRankResolver::from_pool(pool, None, Some(discord));

    assert!(resolver.rank_profile("nobody").await.expect("resolve").is_none());
}

#[tokio::test]
async fn batch_lookup_dedups_and_collects() {
    let pool = temp_pool().await;
    let mut members = HashMap::new();
    members.insert("a".to_string(), vec!["1316966867033653338".to_string()]); // Oracle (8)
    let discord: Arc<dyn DiscordMemberClient> = Arc::new(FakeDiscord { members });
    let resolver = SteamRankResolver::from_pool(pool, None, Some(discord));

    let ids = vec!["a".to_string(), "a".to_string(), "b".to_string()];
    let map = resolver.rank_profiles(&ids).await.expect("batch");
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("a").unwrap().rank.as_deref(), Some("Oracle"));
    assert!(!map.contains_key("b"));
}
