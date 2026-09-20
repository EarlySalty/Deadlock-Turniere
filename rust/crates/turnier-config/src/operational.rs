use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::ConfigError;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationConfig {
    pub proposal_approver_role_ids: Vec<String>,
}

impl Default for AuthorizationConfig {
    fn default() -> Self {
        Self {
            proposal_approver_role_ids: vec![
                "1337518124647579661".to_owned(),
                "1401891955931222110".to_owned(),
            ],
        }
    }
}

impl AuthorizationConfig {
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if self.proposal_approver_role_ids.is_empty()
            || self.proposal_approver_role_ids.iter().any(|id| {
                crate::file::validate_id(id, "authorization.proposal_approver_role_ids").is_err()
            })
            || self
                .proposal_approver_role_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.proposal_approver_role_ids.len()
        {
            return Err(ConfigError::Invalid(
                "authorization.proposal_approver_role_ids: eindeutige positive IDs erforderlich",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SteamConfig {
    pub discord_api_base_url: String,
    pub main_rank_role_ids: Vec<i64>,
    pub rank_cache_seconds: u64,
    pub subrank_role_cache_seconds: u64,
}

impl Default for SteamConfig {
    fn default() -> Self {
        Self {
            discord_api_base_url: "https://discord.com/api/v10".to_owned(),
            main_rank_role_ids: vec![
                1331457571118387210,
                1331457652877955072,
                1331457699992436829,
                1331457724848017539,
                1331457879345070110,
                1331457898781474836,
                1331457949654319114,
                1316966867033653338,
                1331458016356208680,
                1331458049637875785,
                1331458087349129296,
            ],
            rank_cache_seconds: 86400,
            subrank_role_cache_seconds: 300,
        }
    }
}

impl SteamConfig {
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        crate::file::validate_url(&self.discord_api_base_url, "steam.discord_api_base_url")?;
        if self.main_rank_role_ids.len() != 11
            || self.main_rank_role_ids.iter().any(|id| *id <= 0)
            || self
                .main_rank_role_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != 11
        {
            return Err(ConfigError::Invalid("steam.main_rank_role_ids: elf eindeutige positive IDs in Tier-Reihenfolge erforderlich"));
        }
        if !(1..=604800).contains(&self.rank_cache_seconds)
            || !(1..=86400).contains(&self.subrank_role_cache_seconds)
        {
            return Err(ConfigError::Invalid(
                "steam: Cache-Fristen außerhalb der Grenzen",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeConfig {
    pub read_connections: u32,
    pub task_connections: u32,
    pub invite_timeout_seconds: u64,
    pub result_timeout_seconds: u64,
    pub create_timeout_seconds: u64,
    pub start_timeout_seconds: u64,
    pub control_timeout_seconds: u64,
    pub convars_timeout_seconds: u64,

    pub busy_timeout_seconds: u64,
    pub poll_milliseconds: u64,
    pub stale_task_milliseconds: i64,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            read_connections: 2,
            task_connections: 4,
            invite_timeout_seconds: 30,
            result_timeout_seconds: 45,
            create_timeout_seconds: 45,
            start_timeout_seconds: 45,
            control_timeout_seconds: 20,
            convars_timeout_seconds: 30,
            busy_timeout_seconds: 5,
            poll_milliseconds: 500,
            stale_task_milliseconds: 120000,
        }
    }
}

impl BridgeConfig {
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        let timeouts = [
            self.invite_timeout_seconds,
            self.result_timeout_seconds,
            self.create_timeout_seconds,
            self.start_timeout_seconds,
            self.control_timeout_seconds,
            self.convars_timeout_seconds,
        ];
        if !(1..=32).contains(&self.read_connections)
            || !(1..=32).contains(&self.task_connections)
            || timeouts.iter().any(|seconds| {
                !(1..=3600).contains(seconds)
                    || (*seconds * 1000) as i64 > self.stale_task_milliseconds
                    || self.poll_milliseconds > *seconds * 1000
            })
        {
            return Err(ConfigError::Invalid(
                "bridge: unvereinbare Verbindungs- oder Aufgabenbudgets",
            ));
        }

        if !(1..=3600).contains(&self.busy_timeout_seconds)
            || !(10..=60000).contains(&self.poll_milliseconds)
            || !(1000..=3600000).contains(&self.stale_task_milliseconds)
            || self.poll_milliseconds > self.stale_task_milliseconds as u64
        {
            return Err(ConfigError::Invalid("bridge: ungültige Zeitgrenzen"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    pub max_connections: u32,
    pub acquire_timeout_seconds: u64,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            max_connections: 8,
            acquire_timeout_seconds: 10,
        }
    }
}

impl DatabaseConfig {
    pub(crate) fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=64).contains(&self.max_connections)
            || !(1..=300).contains(&self.acquire_timeout_seconds)
        {
            return Err(ConfigError::Invalid("database: ungültige Pool-Grenzen"));
        }
        Ok(())
    }
}
