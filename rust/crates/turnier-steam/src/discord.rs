//! Discord-REST-Anbindung für den Rollen-Fallback — als injizierbarer Trait,
//! damit Tests ihn faken können (kein echter Netzaufruf).
//!
//! Portiert den HTTP-Teil von `get_discord_role_rank` (`rank_reader.py`):
//! `GET /guilds/{guild}/members/{discord_id}` mit `Authorization: Bot <token>`,
//! 10 s Timeout. Liefert die Rollen-IDs des Members.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::{SteamError, SteamResult};

/// Basis-URL der Discord-REST-API (v10).
const DISCORD_API: &str = "https://discord.com/api/v10";

/// Holt die Rollen-IDs eines Guild-Members. Implementierungen kapseln den
/// konkreten Transport (HTTP in Produktion, Fake im Test).
#[async_trait]
pub trait DiscordMemberClient: Send + Sync {
    /// Liefert die Rollen-IDs (als Strings, wie von Discord geliefert) des Members
    /// `discord_id` in der konfigurierten Guild. `None` bedeutet „Member nicht
    /// auflösbar" (z. B. 404, fehlendes Token) — exakt wie das Python-Original,
    /// das bei Nicht-200 / Fehler `None` zurückgibt.
    async fn member_role_ids(&self, discord_id: &str) -> SteamResult<Option<Vec<String>>>;
}

/// Antwort-Teil des Discord-Member-Objekts (nur die Rollen interessieren uns).
#[derive(Debug, Deserialize)]
struct MemberResponse {
    #[serde(default)]
    roles: Vec<String>,
}

/// Produktiv-Client: reqwest gegen die Discord-REST-API.
pub struct ReqwestDiscordClient {
    client: reqwest::Client,
    guild_id: String,
    bot_token: String,
    base_url: String,
}

impl ReqwestDiscordClient {
    /// Erstellt den Client mit 10-s-Timeout. Gibt `None`, wenn Token oder Guild-ID
    /// fehlen — dann ist der Discord-Fallback wie im Original deaktiviert
    /// (`if not DISCORD_BOT_TOKEN or not DISCORD_GUILD_ID: return None`).
    pub fn new(bot_token: &str, guild_id: &str) -> Option<Self> {
        Self::with_timeout(
            bot_token,
            guild_id,
            turnier_config::NetworkConfig::default().steam_discord_request_seconds,
        )
    }

    pub fn with_timeout(bot_token: &str, guild_id: &str, request_seconds: u64) -> Option<Self> {
        Self::with_endpoint(bot_token, guild_id, request_seconds, DISCORD_API)
    }

    pub fn from_config(config: &turnier_config::Config) -> Option<Self> {
        Self::with_endpoint(
            &config.discord_bot_token,
            &config.discord_guild_id,
            config.network.steam_discord_request_seconds,
            &config.steam.discord_api_base_url,
        )
    }

    fn with_endpoint(
        bot_token: &str,
        guild_id: &str,
        request_seconds: u64,
        base_url: &str,
    ) -> Option<Self> {
        if bot_token.trim().is_empty() || guild_id.trim().is_empty() {
            return None;
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(request_seconds))
            .build()
            .ok()?;
        Some(Self {
            client,
            guild_id: guild_id.to_string(),
            bot_token: bot_token.to_string(),
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }
}

#[async_trait]
impl DiscordMemberClient for ReqwestDiscordClient {
    async fn member_role_ids(&self, discord_id: &str) -> SteamResult<Option<Vec<String>>> {
        let url = format!(
            "{}/guilds/{}/members/{}",
            self.base_url, self.guild_id, discord_id
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bot {}", self.bot_token))
            .send()
            .await
            .map_err(|e| SteamError::DiscordHttp(e.to_string()))?;

        // Nicht-200 ⇒ kein auflösbarer Rang (Original: `if status != 200: return None`).
        if response.status() != reqwest::StatusCode::OK {
            return Ok(None);
        }

        let member: MemberResponse = response
            .json()
            .await
            .map_err(|e| SteamError::DiscordHttp(e.to_string()))?;

        Ok(Some(member.roles))
    }
}
