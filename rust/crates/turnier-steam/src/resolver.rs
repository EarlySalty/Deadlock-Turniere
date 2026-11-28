use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::Semaphore;
use turnier_core::RankProfile;
use turnier_db::Pool;

use crate::bridge::BridgeReader;
use crate::cache::{CachedRank, RankCache};
use crate::discord::DiscordMemberClient;
use crate::error::SteamResult;
use crate::role_rank::{self};

pub const SOURCE_STEAM_BRIDGE: &str = "steam_bridge";

const REFRESH_CONCURRENCY: usize = 4;
const REFRESH_QUEUE_LIMIT: usize = 256;

#[async_trait]
pub trait RankResolver: Send + Sync {
    async fn rank_profile(&self, discord_id: &str) -> SteamResult<Option<RankProfile>>;

    async fn rank_profiles(
        &self,
        discord_ids: &[String],
    ) -> SteamResult<HashMap<String, RankProfile>>;

    async fn cached_rank_profiles(
        &self,
        discord_ids: &[String],
    ) -> SteamResult<HashMap<String, RankProfile>>;
}

#[derive(Default)]
struct ExpiringIds {
    entries: Mutex<HashMap<String, Instant>>,
}

impl ExpiringIds {
    fn is_fresh(&self, discord_id: &str) -> bool {
        let Ok(mut guard) = self.entries.lock() else {
            return false;
        };
        match guard.get(discord_id) {
            Some(expires_at) if *expires_at > Instant::now() => true,
            Some(_) => {
                guard.remove(discord_id);
                false
            }
            None => false,
        }
    }

    fn remember(&self, discord_id: &str, ttl: Duration) {
        if let Ok(mut guard) = self.entries.lock() {
            guard.insert(discord_id.to_string(), Instant::now() + ttl);
        }
    }
}

struct Shared {
    cache: RankCache,
    bridge: Option<BridgeReader>,
    discord: Option<Arc<dyn DiscordMemberClient>>,
    discord_misses: ExpiringIds,
    refresh_failures: ExpiringIds,
    refreshing: Mutex<HashSet<String>>,
    refresh_permits: Semaphore,
}

struct RefreshClaim {
    shared: Arc<Shared>,
    discord_id: String,
}

impl Drop for RefreshClaim {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.shared.refreshing.lock() {
            guard.remove(&self.discord_id);
        }
    }
}

#[derive(Clone)]
struct Lookup {
    shared: Arc<Shared>,
    main_rank_role_ids: Arc<[i64]>,
    discord_miss_ttl: Duration,
}

pub struct SteamRankResolver {
    lookup: Lookup,
}

impl SteamRankResolver {
    pub fn new(
        cache: RankCache,
        bridge: Option<BridgeReader>,
        discord: Option<Arc<dyn DiscordMemberClient>>,
    ) -> Self {
        let defaults = turnier_config::SteamConfig::default();
        Self {
            lookup: Lookup {
                shared: Arc::new(Shared {
                    cache,
                    bridge,
                    discord,
                    discord_misses: ExpiringIds::default(),
                    refresh_failures: ExpiringIds::default(),
                    refreshing: Mutex::new(HashSet::new()),
                    refresh_permits: Semaphore::new(REFRESH_CONCURRENCY),
                }),
                main_rank_role_ids: defaults.main_rank_role_ids.into(),
                discord_miss_ttl: Duration::from_secs(defaults.rank_miss_cache_seconds),
            },
        }
    }

    pub fn from_pool(
        pool: Pool,
        bridge: Option<BridgeReader>,
        discord: Option<Arc<dyn DiscordMemberClient>>,
    ) -> Self {
        Self::new(RankCache::new(pool), bridge, discord)
    }

    pub fn with_main_rank_roles(mut self, roles: Vec<i64>) -> Self {
        self.lookup.main_rank_role_ids = roles.into();
        self
    }

    pub fn with_discord_miss_ttl(mut self, seconds: u64) -> Self {
        self.lookup.discord_miss_ttl = Duration::from_secs(seconds);
        self
    }

    fn lookup_ids(discord_ids: &[String]) -> Vec<String> {
        let mut ids: Vec<String> = discord_ids
            .iter()
            .filter(|id| !turnier_core::is_guest_participant_id(id))
            .cloned()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    async fn resolve_many(
        &self,
        discord_ids: &[String],
        with_discord: bool,
    ) -> SteamResult<HashMap<String, RankProfile>> {
        let ids = Self::lookup_ids(discord_ids);
        let mut cached = self.lookup.shared.cache.lookup_many(&ids).await?;
        let mut out = HashMap::new();
        let mut refresh = Vec::new();
        for discord_id in ids {
            let stale = match cached.remove(&discord_id) {
                Some(CachedRank {
                    profile,
                    fresh: true,
                }) => {
                    out.insert(discord_id, profile);
                    continue;
                }
                Some(entry) => Some(entry.profile).filter(has_rank),
                None => None,
            };
            let fallback = match self
                .lookup
                .resolve_uncached(&discord_id, with_discord)
                .await
            {
                Ok(Some(profile)) => {
                    out.insert(discord_id, profile);
                    continue;
                }
                Ok(None) => stale,
                Err(err) => match stale.filter(|_| !with_discord) {
                    Some(profile) => {
                        tracing::warn!(error = %err, discord_id = %discord_id, "Rang-Auflösung fehlgeschlagen, abgelaufener Cache-Eintrag dient als Ersatz");
                        Some(profile)
                    }
                    None => return Err(err),
                },
            };
            if !with_discord {
                refresh.push(discord_id.clone());
            }
            if let Some(profile) = fallback {
                out.insert(discord_id, profile);
            }
        }
        self.queue_refreshes(refresh);
        Ok(out)
    }

    fn queue_refreshes(&self, discord_ids: Vec<String>) {
        if self.lookup.shared.discord.is_none() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        for discord_id in discord_ids {
            if self.lookup.shared.discord_misses.is_fresh(&discord_id)
                || self.lookup.shared.refresh_failures.is_fresh(&discord_id)
            {
                continue;
            }
            let Some(claim) = self.lookup.claim_refresh(discord_id) else {
                continue;
            };
            let lookup = self.lookup.clone();
            runtime.spawn(async move { lookup.refresh(claim).await });
        }
    }
}

impl Lookup {
    fn claim_refresh(&self, discord_id: String) -> Option<RefreshClaim> {
        let mut guard = self.shared.refreshing.lock().ok()?;
        if guard.len() >= REFRESH_QUEUE_LIMIT || !guard.insert(discord_id.clone()) {
            return None;
        }
        Some(RefreshClaim {
            shared: Arc::clone(&self.shared),
            discord_id,
        })
    }

    async fn refresh(self, claim: RefreshClaim) {
        let Ok(_permit) = self.shared.refresh_permits.acquire().await else {
            return;
        };
        if let Err(err) = self.resolve_uncached(&claim.discord_id, true).await {
            tracing::warn!(error = %err, discord_id = %claim.discord_id, "Rang-Aktualisierung im Hintergrund fehlgeschlagen");
            self.shared
                .refresh_failures
                .remember(&claim.discord_id, self.discord_miss_ttl);
        }
    }

    async fn resolve_uncached(
        &self,
        discord_id: &str,
        with_discord: bool,
    ) -> SteamResult<Option<RankProfile>> {
        if let Some(profile) = self.resolve_via_bridge(discord_id).await? {
            self.shared.cache.store(discord_id, &profile).await?;
            return Ok(Some(profile));
        }
        if !with_discord || self.shared.discord_misses.is_fresh(discord_id) {
            return Ok(None);
        }
        if let Some(profile) = self.resolve_via_discord(discord_id).await? {
            self.shared.cache.store(discord_id, &profile).await?;
            return Ok(Some(profile));
        }
        self.shared
            .discord_misses
            .remember(discord_id, self.discord_miss_ttl);
        Ok(None)
    }

    async fn resolve_via_bridge(&self, discord_id: &str) -> SteamResult<Option<RankProfile>> {
        let Some(bridge) = &self.shared.bridge else {
            return Ok(None);
        };
        let Some(link) = bridge.get_steam_link(discord_id).await? else {
            return Ok(None);
        };
        Ok(Some(RankProfile {
            steam_id: link.steam_id,
            rank: link.rank,
            rank_tier: link.rank_tier,
            subrank: link.subrank,
            rank_score: link.rank_score,
            source: SOURCE_STEAM_BRIDGE.to_string(),
        }))
    }

    async fn resolve_via_discord(&self, discord_id: &str) -> SteamResult<Option<RankProfile>> {
        let Some(discord) = &self.shared.discord else {
            return Ok(None);
        };
        let Some(role_ids) = discord.member_role_ids(discord_id).await? else {
            return Ok(None);
        };
        let subrank_roles = match &self.shared.bridge {
            Some(bridge) => bridge.load_subrank_roles().await?,
            None => Vec::new(),
        };
        Ok(role_rank::resolve_from_roles_with_mapping(
            &role_ids,
            &subrank_roles,
            &self.main_rank_role_ids,
        ))
    }
}

fn has_rank(profile: &RankProfile) -> bool {
    profile.rank.is_some() || profile.rank_tier.is_some()
}

#[async_trait]
impl RankResolver for SteamRankResolver {
    async fn rank_profile(&self, discord_id: &str) -> SteamResult<Option<RankProfile>> {
        let mut profiles = self
            .resolve_many(std::slice::from_ref(&discord_id.to_string()), true)
            .await?;
        Ok(profiles.remove(discord_id))
    }

    async fn rank_profiles(
        &self,
        discord_ids: &[String],
    ) -> SteamResult<HashMap<String, RankProfile>> {
        self.resolve_many(discord_ids, true).await
    }

    async fn cached_rank_profiles(
        &self,
        discord_ids: &[String],
    ) -> SteamResult<HashMap<String, RankProfile>> {
        self.resolve_many(discord_ids, false).await
    }
}
