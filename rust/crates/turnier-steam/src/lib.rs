//! `turnier-steam` — Rang-Resolver des Turnier-Backends.
//!
//! Löst zu einer Discord-ID ein Deadlock-Rang-Profil auf — in einer dreistufigen
//! Kaskade (portiert aus `rank_reader.py`):
//! 1. **Cache** — L1 (prozesslokal) + L2 (Tabelle `rank_cache`), TTL 24 h.
//! 2. **Steam-Bridge** — read-only Lookup in der externen Discord-Bot-SQLite
//!    (`steam_links`), bevorzugt der Primary-Account.
//! 3. **Discord-Rollen** — REST-Fallback: höchste Rang-Rolle des Guild-Members,
//!    aufgelöst über das Subrank-Rollen-Mapping der Bridge-DB.
//!
//! Die Rang-Domäne ([`rank`]) ist die EINE Quelle der Wahrheit für die
//! Tier↔Name-Tabelle und die `rank_score`-Formel (im Original dreifach dupliziert).
//!
//! Öffentlicher Einstieg: der Trait [`RankResolver`] mit der Default-Impl
//! [`SteamRankResolver`]; Konsumenten injizieren ihn.

mod bridge;
mod cache;
mod discord;
mod error;
mod rank;
mod resolver;
mod role_rank;

use std::sync::Arc;

use turnier_config::Config;
use turnier_db::Pool;

pub use bridge::{BridgeReader, SteamLink};
pub use cache::{RankCache, RANK_CACHE_TTL_SECONDS};
pub use discord::{DiscordMemberClient, ReqwestDiscordClient};
pub use error::{SteamError, SteamResult};
pub use rank::{
    rank_name_for_tier, rank_score, rank_score_for_name, tier_for_rank_name, DEFAULT_SUBRANK,
};
pub use resolver::{RankResolver, SteamRankResolver, SOURCE_STEAM_BRIDGE};
pub use role_rank::SOURCE_DISCORD_ROLE;

/// Baut den produktiven Resolver aus dem App-DB-Pool und der Config.
///
/// - Der Bridge-Reader wird geöffnet, falls `steam_bridge_db_path` gesetzt ist,
///   die Datei existiert und read-only geöffnet werden kann; sonst entfällt die
///   Bridge-Stufe (mit Warnung).
/// - Der Discord-Client wird erstellt, falls Bot-Token und Guild-ID gesetzt sind;
///   sonst entfällt der Discord-Fallback (wie im Original).
pub async fn build_resolver(pool: Pool, config: &Config) -> SteamResult<SteamRankResolver> {
    let bridge = BridgeReader::open(&config.steam_bridge_db_path, &config.discord_guild_id).await?;

    let discord: Option<Arc<dyn DiscordMemberClient>> =
        ReqwestDiscordClient::new(&config.discord_bot_token, &config.discord_guild_id)
            .map(|c| Arc::new(c) as Arc<dyn DiscordMemberClient>);

    Ok(SteamRankResolver::from_pool(pool, bridge, discord))
}
