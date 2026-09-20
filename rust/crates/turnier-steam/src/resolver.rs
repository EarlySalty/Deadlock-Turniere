//! Öffentliche Rang-Auflösung: der [`RankResolver`]-Trait und seine
//! Default-Implementierung [`SteamRankResolver`].
//!
//! Orchestriert die dreistufige Kaskade aus `get_player_rank_profile`
//! (`rank_reader.py`): Cache → Steam-Bridge → Discord-Rollen. Treffer aus Bridge
//! oder Discord werden in den Cache geschrieben. Zusätzlich ein Batch-Lookup über
//! mehrere Discord-IDs (gegen N+1 in den Konsumenten turnier-api/turnier-engine).

use std::collections::HashMap;
use std::sync::Arc;

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
}

/// Default-Implementierung des Resolvers.
///
/// Hält den App-DB-Cache, optional den read-only Bridge-Reader (fehlt er, wird
/// die Bridge-Stufe übersprungen) und optional den Discord-Client (fehlt er — kein
/// Token/Guild — entfällt der Discord-Fallback, wie im Original).
pub struct SteamRankResolver {
    cache: RankCache,
    bridge: Option<BridgeReader>,
    discord: Option<Arc<dyn DiscordMemberClient>>,
    main_rank_role_ids: Vec<i64>,
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
        }
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
    /// (`rank`/`rank_tier` NULL) wird dennoch als Profil zurückgegeben und gecacht
    /// — exakt wie im Original (`rank_reader.py:258-268`). Das maskiert den
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
        // Stufe 1: Cache.
        if let Some(cached) = self.cache.get(discord_id).await? {
            return Ok(Some(cached));
        }

        // Stufe 2: Steam-Bridge.
        if let Some(profile) = self.resolve_via_bridge(discord_id).await? {
            self.cache.store(discord_id, &profile).await?;
            return Ok(Some(profile));
        }

        // Stufe 3: Discord-Rollen.
        if let Some(profile) = self.resolve_via_discord(discord_id).await? {
            self.cache.store(discord_id, &profile).await?;
            return Ok(Some(profile));
        }

        Ok(None)
    }

    async fn rank_profiles(
        &self,
        discord_ids: &[String],
    ) -> SteamResult<HashMap<String, RankProfile>> {
        let mut out = HashMap::new();
        for discord_id in discord_ids {
            // Duplikate doppelt aufzulösen ist überflüssig; der Cache fängt sie
            // ohnehin ab, aber so sparen wir auch den Cache-Roundtrip.
            if out.contains_key(discord_id) {
                continue;
            }
            if let Some(profile) = self.rank_profile(discord_id).await? {
                out.insert(discord_id.clone(), profile);
            }
        }
        Ok(out)
    }
}
