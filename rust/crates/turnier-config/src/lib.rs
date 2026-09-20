//! Zentrale TOML-Konfiguration. Secrets werden nach der Dateiprüfung separat geladen.
//! Betriebswerte werden als unveränderliche Momentaufnahme weitergegeben.

pub mod secrets;

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
mod file;
mod operational;
pub use file::*;
pub use operational::*;

/// Vollständige, aufgelöste Laufzeit-Konfiguration.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub turnier_enable_test_mode: bool,
    pub cors_extra_origins: Vec<String>,
    pub logging: LoggingConfig,
    pub network: NetworkConfig,
    pub assets: AssetsConfig,
    pub limits: LimitsConfig,
    pub scheduler: SchedulerConfig,
    pub authorization: AuthorizationConfig,
    pub steam: SteamConfig,
    pub bridge: BridgeConfig,
    pub database: DatabaseConfig,
    pub observer_agent: Option<ObserverAgentConfig>,
    // --- Discord-OAuth-Delegation via Deadlock-Bots / Master-Broker ---
    pub discord_oauth_internal_api_base_url: String,
    #[serde(skip)]
    pub discord_oauth_internal_api_token: String,
    pub discord_master_broker_base_url: String,
    #[serde(skip)]
    pub discord_master_broker_token: String,

    // --- Discord-Kanäle / Rollen ---
    pub discord_match_channel_category_id: i64,
    pub discord_match_channel_delete_delay_seconds: i64,
    pub discord_sammelpunkt_channel_id: i64,
    pub discord_team1_voice_channel_id: i64,
    pub discord_team2_voice_channel_id: i64,
    pub discord_tournament_lobby_channel_id: i64,
    pub discord_caster_role_id: i64,
    pub discord_caster_voice_channel_id: i64,
    #[serde(skip)]
    pub discord_bot_token: String,
    pub turnier_public_url: String,

    pub discord_guild_id: String,
    pub discord_admin_role_ids: String,
    pub discord_tournament_admin_role_ids: String,
    pub discord_mod_role_ids: String,
    pub scrim_guild_id: i64,
    pub scrim_signup_role_id: Option<i64>,
    pub scrim_reserve_role_id: Option<i64>,
    pub scrim_announce_channel_id: i64,
    pub scrim_substitute_sweep_interval_seconds: u64,

    // --- JWT (im Port effektiv ungenutzt: Sessions sind opake Tokens) ---
    #[serde(skip)]
    pub jwt_secret: String,

    // --- Pfade ---
    pub avatar_dir: String,
    pub steam_bridge_db_path: String,

    // --- Server ---
    pub backend_host: String,
    pub backend_port: i64,
    pub backend_allowed_hosts: String,
    pub expose_api_docs: bool,
    pub frontend_url: String,

    // --- Benachrichtigungen ---
    #[serde(skip)]
    pub discord_webhook_url: String,

    // --- Scrim cutover ---
    #[serde(skip)]
    pub turnier_internal_api_token: String,

    // --- Steam-Bot (interne HTTP-API, localhost) ---
    pub steam_bot_base_url: String,
    #[serde(skip)]
    pub steam_bot_internal_token: String,

    // --- Auto-Observer / Steam Bot 2 ---
    pub observer_enabled: bool,
    /// Separater Kill-Switch fuer jede automatisierte Interaktion mit dem
    /// lokalen Deadlock-Client. Default bleibt aus; Shadow/Assist funktionieren
    /// ohne diesen Schalter.
    pub observer_game_control_enabled: bool,
    #[serde(skip)]
    pub observer_agent_token: String,
    pub observer_steam_bot2_base_url: String,
    pub observer_deadlock_api_base_url: String,
    pub observer_controller_query: String,
    pub observer_pawn_query: String,

    // --- Routine-Turniere (sicherer Default: aus) ---
    pub routine_tournaments_enabled: bool,
    pub routine_proposal_channel_id: i64,
    pub routine_tournament_preset_id: i64,
    pub routine_tournament_weekday: String,
    pub routine_tournament_time_utc: String,
    pub routine_tournament_lead_days: i64,
    pub routine_tournament_checkin_lead_minutes: i64,
    pub routine_tournament_bracket_delay_minutes: i64,
}

impl Config {
    /// Admin-Rollen-IDs = allgemeine Admin-Rollen ∪ Turnier-Admin-Rollen.
    pub fn admin_role_ids(&self) -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        for raw in [
            &self.discord_admin_role_ids,
            &self.discord_tournament_admin_role_ids,
        ] {
            ids.extend(split_csv(raw));
        }
        ids
    }

    /// Mod-Rollen-IDs.
    pub fn mod_role_ids(&self) -> BTreeSet<String> {
        split_csv(&self.discord_mod_role_ids).collect()
    }

    /// Erlaubte CORS-Origins: lokaler Vite-Dev-Server + Frontend-URL.
    pub fn cors_allowed_origins(&self) -> Vec<String> {
        let mut origins = BTreeSet::new();
        origins.extend(self.cors_extra_origins.iter().cloned());
        let frontend = self.frontend_url.trim_end_matches('/');
        if !frontend.is_empty() {
            origins.insert(frontend.to_string());
        }
        origins.into_iter().collect()
    }

    /// Erlaubte Hosts (TrustedHost-Äquivalent): Loopback + Hostnamen aus
    /// Frontend-/Public-URL/Backend-Host + zusätzlich konfigurierte.
    pub fn allowed_hosts(&self) -> Vec<String> {
        let mut hosts = BTreeSet::new();
        for h in ["127.0.0.1", "localhost", "::1"] {
            hosts.insert(h.to_string());
        }
        for candidate in [
            &self.frontend_url,
            &self.turnier_public_url,
            &self.backend_host,
        ] {
            if let Some(host) = hostname_of(candidate) {
                hosts.insert(host);
            }
        }
        hosts.extend(split_csv(&self.backend_allowed_hosts).map(|s| s.to_lowercase()));
        hosts.into_iter().collect()
    }

    /// `/docs` nur, wenn die API-Doku bewusst freigeschaltet ist.
    pub fn docs_enabled(&self) -> bool {
        self.expose_api_docs
    }
}

fn split_csv(raw: &str) -> impl Iterator<Item = String> + '_ {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extrahiert den normalisierten Hostnamen aus einer URL oder einem Host:Port-String.
fn hostname_of(value: &str) -> Option<String> {
    let candidate = value.trim();
    if candidate.is_empty() {
        return None;
    }
    let host = if let Some(idx) = candidate.find("://") {
        let after = &candidate[idx + 3..];
        after.split(['/', '?', '#']).next().unwrap_or(after)
    } else {
        candidate
    };
    // Port abschneiden (nur bei genau einem ':' — IPv6 mit mehreren ':' ignorieren).
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let host = if host.matches(':').count() == 1 {
        host.split(':').next().unwrap_or(host)
    } else {
        host
    };
    let host = host.trim().to_lowercase();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_extraction() {
        assert_eq!(
            hostname_of("https://example.com/path"),
            Some("example.com".to_string())
        );
        assert_eq!(hostname_of("127.0.0.1:8900"), Some("127.0.0.1".to_string()));
        assert_eq!(hostname_of("localhost"), Some("localhost".to_string()));
        assert_eq!(hostname_of(""), None);
    }

    #[test]
    fn csv_splitting_trims_and_drops_empty() {
        let out: Vec<String> = split_csv(" a, b ,, c ").collect();
        assert_eq!(out, vec!["a", "b", "c"]);
    }
}

/// Deterministische Bestandsdefaults für Tests und explizite Konstruktion.
/// Der Produktionsstart verwendet load_file, nicht Default.
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            turnier_enable_test_mode: true,
            cors_extra_origins: vec!["http://localhost:5173".to_string()],
            logging: LoggingConfig::default(),
            network: NetworkConfig::default(),
            assets: AssetsConfig::default(),
            limits: LimitsConfig::default(),
            scheduler: SchedulerConfig::default(),
            authorization: AuthorizationConfig::default(),
            steam: SteamConfig::default(),
            bridge: BridgeConfig::default(),
            database: DatabaseConfig::default(),
            observer_agent: None,
            discord_oauth_internal_api_base_url: "http://127.0.0.1:8766".to_string(),
            discord_oauth_internal_api_token: "".to_string(),
            discord_master_broker_base_url: "http://127.0.0.1:8766".to_string(),
            discord_master_broker_token: "".to_string(),
            discord_match_channel_category_id: 1412800850580996256,
            discord_match_channel_delete_delay_seconds: 300,
            discord_sammelpunkt_channel_id: 1426160735469174875,
            discord_team1_voice_channel_id: 1462434609563173019,
            discord_team2_voice_channel_id: 1462434639858897017,
            discord_tournament_lobby_channel_id: 1412411665713987635,
            discord_caster_role_id: 1495154811799077067,
            discord_caster_voice_channel_id: 1495155113772450042,
            discord_bot_token: "".to_string(),
            turnier_public_url: "https://deutsche-deadlock-community.de/turnier".to_string(),
            discord_guild_id: "1289721245281292288".to_string(),
            discord_admin_role_ids:
                "1304169657124782100,1337518124647579661,1411000883155832852,1401891955931222110"
                    .to_string(),
            discord_tournament_admin_role_ids: "1494120177577754747".to_string(),
            discord_mod_role_ids: "1474210107255554331".to_string(),
            scrim_guild_id: 1289721245281292288,
            scrim_signup_role_id: Some(1520849762851618817),
            scrim_reserve_role_id: Some(1523803562306703430),
            scrim_announce_channel_id: 1521522998199324853,
            scrim_substitute_sweep_interval_seconds: 600,
            jwt_secret: "".to_string(),
            avatar_dir: "data/avatars".to_string(),
            steam_bridge_db_path:
                "C:\\Users\\Nani-Admin\\Documents\\Deadlock\\service\\deadlock.sqlite3".to_string(),
            backend_host: "127.0.0.1".to_string(),
            backend_port: 8900,
            backend_allowed_hosts: "".to_string(),
            expose_api_docs: false,
            frontend_url: "https://deutsche-deadlock-community.de/turnier".to_string(),
            discord_webhook_url: "".to_string(),
            turnier_internal_api_token: "".to_string(),
            steam_bot_base_url: "http://127.0.0.1:8782".to_string(),
            steam_bot_internal_token: "".to_string(),
            observer_enabled: true,
            observer_game_control_enabled: false,
            observer_agent_token: "".to_string(),
            observer_steam_bot2_base_url: "http://127.0.0.1:8784".to_string(),
            observer_deadlock_api_base_url: "https://api.deadlock-api.com".to_string(),
            observer_controller_query: "SELECT * FROM CCitadelPlayerController".to_string(),
            observer_pawn_query: "SELECT * FROM CCitadelPlayerPawn".to_string(),
            routine_tournaments_enabled: false,
            routine_proposal_channel_id: 1474543558793887937,
            routine_tournament_preset_id: 0,
            routine_tournament_weekday: "saturday".to_string(),
            routine_tournament_time_utc: "18:00".to_string(),
            routine_tournament_lead_days: 7,
            routine_tournament_checkin_lead_minutes: 30,
            routine_tournament_bracket_delay_minutes: 180,
        }
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("schema_version", &self.schema_version)
            .finish_non_exhaustive()
    }
}
