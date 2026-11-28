//! Öffentliche Rang-Auflösung: der [`RankResolver`]-Trait und seine
//! Default-Implementierung [`SteamRankResolver`].
//!
//! Orchestriert die dreistufige Kaskade aus `get_player_rank_profile`
//! (`rank_reader.py`): Cache → Steam-Bridge → Discord-Rollen. Treffer aus Bridge
//! oder Discord werden in den Cache geschrieben. Zusätzlich ein Batch-Lookup über
//! mehrere Discord-IDs (gegen N+1 in den Konsumenten turnier-api/turnier-engine).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use turnier_core::RankProfile;
use turnier_db::Pool;

use crate::bridge::BridgeReader;
use crate::cache::RankCache;
use crate::discord::DiscordMemberClient;
use crate::error::SteamResult;
use crate::role_rank::{self};

/// Source-Wert für aus der Steam-Bridge aufgelöste Profile.
pub const SOURCE_STEAM_BRIDGE: &str = "steam_bridge";

/// Öffentliche Schnittstelle zur Rang-Auflösung. Konsumenten (turnier-api,
/// turnier-engine) injizieren eine Implementierung.
#[async_trait]
pub trait RankResolver: Send + Sync {
    /// Liefert das Rang-Profil zu einer Discord-ID (Cache → Bridge → Discord),
    /// `None`, wenn keine Stufe einen Rang findet.
    async fn rank_profile(&self, discord_id: &str) -> SteamResult<Option<RankProfile>>;

    /// Batch-Variante über mehrere Discord-IDs. Reihenfolge/Duplikate der Eingabe
    /// sind egal; das Ergebnis ist eine Map nur mit gefundenen Profilen.
    async fn rank_profiles(
        &self,
        discord_ids: &[String],
    ) -> SteamResult<HashMap<String, RankProfile>>;

    async fn cached_rank_profiles(
        &self,
        discord_ids: &[String],
    ) -> SteamResult<HashMap<String, RankProfile>>;
}

/// Default-Implementierung des Resolvers.
///
/// Hält den App-DB-Cache, optional den read-only Bridge-Reader (fehlt er, wird
/// die Bridge-Stufe übersprungen) und optional den Discord-Client (fehlt er, kein
/// Token/Guild, entfällt der Discord-Fallback, wie im Original).
pub struct SteamRankResolver {
    cache: RankCache,
    bridge: Option<BridgeReader>,
    discord: Option<Arc<dyn DiscordMemberClient>>,
    main_rank_role_ids: Vec<i64>,
    discord_misses: Mutex<HashMap<String, Instant>>,
    discord_miss_ttl: Duration,
}

impl SteamRankResolver {
    /// Baut den Resolver aus seinen Bausteinen.
    pub fn new(
        cache: RankCache,
        bridge: Option<BridgeReader>,
        discord: Option<Arc<dyn DiscordMemberClient>>,
    ) -> Self {
        Self {
            cache,
            bridge,
            discord,
            main_rank_role_ids: turnier_config::SteamConfig::default().main_rank_role_ids,
            discord_misses: Mutex::new(HashMap::new()),
            discord_miss_ttl: Duration::from_secs(
                turnier_config::SteamConfig::default().rank_miss_cache_seconds,
            ),
        }
    }

    pub fn with_discord_miss_ttl(mut self, seconds: u64) -> Self {
        self.discord_miss_ttl = Duration::from_secs(seconds);
        self
    }

    fn discord_miss_is_fresh(&self, discord_id: &str) -> bool {
        let Ok(mut guard) = self.discord_misses.lock() else {
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

    fn remember_discord_miss(&self, discord_id: &str) {
        if let Ok(mut guard) = self.discord_misses.lock() {
            guard.insert(
                discord_id.to_string(),
                Instant::now() + self.discord_miss_ttl,
            );
        }
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

    async fn resolve_uncached(
        &self,
        discord_id: &str,
        with_discord: bool,
    ) -> SteamResult<Option<RankProfile>> {
        if let Some(profile) = self.resolve_via_bridge(discord_id).await? {
            self.cache.store(discord_id, &profile).await?;
            return Ok(Some(profile));
        }
        if !with_discord || self.discord_miss_is_fresh(discord_id) {
            return Ok(None);
        }
        if let Some(profile) = self.resolve_via_discord(discord_id).await? {
            self.cache.store(discord_id, &profile).await?;
            return Ok(Some(profile));
        }
        self.remember_discord_miss(discord_id);
        Ok(None)
    }

    async fn resolve_many(
        &self,
        discord_ids: &[String],
        with_discord: bool,
    ) -> SteamResult<HashMap<String, RankProfile>> {
        let ids = Self::lookup_ids(discord_ids);
        let mut out = self.cache.get_many(&ids).await?;
        for discord_id in &ids {
            if out.contains_key(discord_id) {
                continue;
            }
            if let Some(profile) = self.resolve_uncached(discord_id, with_discord).await? {
                out.insert(discord_id.clone(), profile);
            }
        }
        Ok(out)
    }

    pub fn with_main_rank_roles(mut self, roles: Vec<i64>) -> Self {
        self.main_rank_role_ids = roles;
        self
    }

    /// Bequemer Konstruktor über einen App-DB-Pool: legt den Cache an und
    /// übernimmt Bridge + Discord-Client.
    pub fn from_pool(
        pool: Pool,
        bridge: Option<BridgeReader>,
        discord: Option<Arc<dyn DiscordMemberClient>>,
    ) -> Self {
        Self::new(RankCache::new(pool), bridge, discord)
    }

    /// Stufe 2: Steam-Bridge. Gibt ein Profil mit `source = steam_bridge` zurück,
    /// wenn ein Link existiert.
    ///
    /// VERHALTEN BEWUSST ERHALTEN: Ein verlinkter Account ohne Rangdaten
    /// (`rank`/`rank_tier` NULL) wird dennoch als Profil zurückgegeben und gecacht,
    /// exakt wie im Original (`rank_reader.py:258-268`). Das maskiert den
    /// Discord-Fallback für 24 h. Siehe known-issues.
    async fn resolve_via_bridge(&self, discord_id: &str) -> SteamResult<Option<RankProfile>> {
        let Some(bridge) = &self.bridge else {
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

    /// Stufe 3: Discord-Rollen-Fallback (HTTP + Subrank-Rollen-Mapping aus der
    /// Bridge-DB). Gibt `None`, wenn kein Discord-Client konfiguriert ist oder der
    /// Member keinen passenden Rang-Rollen hat.
    async fn resolve_via_discord(&self, discord_id: &str) -> SteamResult<Option<RankProfile>> {
        let Some(discord) = &self.discord else {
            return Ok(None);
        };
        let Some(role_ids) = discord.member_role_ids(discord_id).await? else {
            return Ok(None);
        };

        // Subrank-Rollen kommen aus der Bridge-DB (5-min-Cache). Fehlt die Bridge,
        // bleibt das Mapping leer und es greift der Haupt-Tier-Fallback.
        let subrank_roles = match &self.bridge {
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

#[async_trait]
impl RankResolver for SteamRankResolver {
    async fn rank_profile(&self, discord_id: &str) -> SteamResult<Option<RankProfile>> {
        if turnier_core::is_guest_participant_id(discord_id) {
            return Ok(None);
        }
        if let Some(cached) = self.cache.get(discord_id).await? {
            return Ok(Some(cached));
        }
        self.resolve_uncached(discord_id, true).await
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
