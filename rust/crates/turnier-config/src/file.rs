//! Dateikonfiguration. Laufzeitänderungen benötigen einen geordneten Neustart.
//! Relative Datenpfade beziehen sich auf das Verzeichnis der TOML, nicht auf cwd.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Config;

pub const SCHEMA_VERSION: u32 = 1;
pub const CONFIG_ANCHOR: &str = "turnier-global-toml-v1";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("Konfiguration fehlt oder ist nicht lesbar; expliziten --config-Pfad prüfen")]
    Read,
    #[error("Konfiguration benötigt einen absoluten --config-Pfad")]
    AbsolutePath,
    #[error("TOML ungültig: Syntax, unbekanntes Feld, fehlender Pflichtwert oder falscher Typ")]
    Parse,
    #[error("Konfiguration ungültig: {0}")]
    Invalid(&'static str),
    #[error("Aufruf ungültig: --config /absoluter/pfad/bot.toml erforderlich; optional --check, --check-config oder --print-config")]
    Arguments,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    pub level: LogLevel,
}
impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkConfig {
    pub broker_request_seconds: u64,
    pub broker_connect_seconds: u64,
    pub oauth_request_seconds: u64,
    pub oauth_connect_seconds: u64,
    pub steam_discord_request_seconds: u64,
    pub lobby_provision_seconds: u64,
    pub lobby_request_seconds: u64,
    pub lobby_connect_seconds: u64,
    pub scrim_lobby_code_seconds: u64,
    pub scrim_role_sync_seconds: u64,
    pub observer_request_seconds: u64,
    pub observer_connect_seconds: u64,
    pub observer_keepalive_seconds: u64,
    pub heroes_request_seconds: u64,
}
impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            broker_request_seconds: 20,
            broker_connect_seconds: 5,
            oauth_request_seconds: 20,
            oauth_connect_seconds: 5,
            steam_discord_request_seconds: 10,
            lobby_provision_seconds: 25,
            lobby_request_seconds: 10,
            lobby_connect_seconds: 5,
            scrim_lobby_code_seconds: 25,
            scrim_role_sync_seconds: 20,
            observer_request_seconds: 5,
            observer_connect_seconds: 5,
            observer_keepalive_seconds: 30,
            heroes_request_seconds: 5,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulerConfig {
    pub tick_seconds: u64,
    pub routine_seconds: u64,
    pub scrim_operational_seconds: u64,
    pub lobby_tick_seconds: u64,
    pub lobby_reconcile_seconds: u64,
    pub lobby_collect_seconds: u64,
    pub observer_tick_milliseconds: u64,
    pub observer_evaluate_milliseconds: u64,
    pub observer_stale_seconds: u64,
    pub observer_startup_grace_seconds: u64,
}
impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            tick_seconds: 60,
            routine_seconds: 3600,
            scrim_operational_seconds: 15,
            lobby_tick_seconds: 5,
            lobby_reconcile_seconds: 15,
            lobby_collect_seconds: 30,
            observer_tick_milliseconds: 2000,
            observer_evaluate_milliseconds: 350,
            observer_stale_seconds: 10,
            observer_startup_grace_seconds: 20,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssetsConfig {
    pub heroes_url: String,
    pub heroes_cache_seconds: u64,
    pub heroes_fallback_cache_seconds: u64,
}
impl Default for AssetsConfig {
    fn default() -> Self {
        Self {
            heroes_url: "https://api.deadlock-api.com/v1/assets/heroes".to_owned(),
            heroes_cache_seconds: 86400,
            heroes_fallback_cache_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    pub session_lifetime_days: i64,
    pub avatar_bytes: usize,
    pub request_body_bytes: usize,
    pub comp_body_bytes: usize,
    pub comp_clients: usize,
    pub comp_reads_per_minute: u32,
    pub comp_writes_per_minute: u32,
    pub comp_creations_per_hour: usize,
    pub draft_creations_per_hour: usize,
    pub draft_viewer_ttl_seconds: u64,
}
impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            session_lifetime_days: 7,
            avatar_bytes: 2 * 1024 * 1024,
            request_body_bytes: 2 * 1024 * 1024,
            comp_body_bytes: 32 * 1024,
            comp_clients: 10000,
            comp_reads_per_minute: 600,
            comp_writes_per_minute: 120,
            comp_creations_per_hour: 10,
            draft_creations_per_hour: 10,
            draft_viewer_ttl_seconds: 20,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObserverAgentConfig {
    pub server_base_url: String,
    pub vconsole_address: String,
    pub bot_account_id: i16,
    pub poll_milliseconds: u64,
    pub game_control_enabled: bool,
    pub request_timeout_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigMode {
    Run,
    Check,
    Validate,
    Print,
}
#[derive(Debug)]
pub struct ConfigArgs {
    pub path: PathBuf,
    pub mode: ConfigMode,
}
impl ConfigArgs {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, ConfigError> {
        let mut args = args.into_iter();
        let mut path = None;
        let mut mode = ConfigMode::Run;
        for_arg(&mut args, &mut path, &mut mode)?;
        let path = path.ok_or(ConfigError::Arguments)?;
        if !path.is_absolute() {
            return Err(ConfigError::AbsolutePath);
        }
        Ok(Self { path, mode })
    }
}
fn for_arg(
    args: &mut impl Iterator<Item = OsString>,
    path: &mut Option<PathBuf>,
    mode: &mut ConfigMode,
) -> Result<(), ConfigError> {
    while let Some(arg) = args.next() {
        if arg == "--config" && path.is_none() {
            *path = Some(PathBuf::from(args.next().ok_or(ConfigError::Arguments)?));
        } else {
            if *mode != ConfigMode::Run {
                return Err(ConfigError::Arguments);
            }
            *mode = match arg.to_str() {
                Some("--check") => ConfigMode::Check,
                Some("--check-config") => ConfigMode::Validate,
                Some("--print-config") => ConfigMode::Print,
                _ => return Err(ConfigError::Arguments),
            };
        }
    }
    Ok(())
}

impl Config {
    /// Liest und prüft ohne Secret-Zugriff, Verzeichniserstellung oder Netzwerk.
    pub fn load_file(path: &Path) -> Result<Self, ConfigError> {
        if !path.is_absolute() {
            return Err(ConfigError::AbsolutePath);
        }
        let canonical = path.canonicalize().map_err(|_| ConfigError::Read)?;
        let text = std::fs::read_to_string(&canonical).map_err(|_| ConfigError::Read)?;
        Self::parse_file(&text, &canonical)
    }

    pub fn parse_file(text: &str, path: &Path) -> Result<Self, ConfigError> {
        if !path.is_absolute() {
            return Err(ConfigError::AbsolutePath);
        }
        // Den Parserfehler bewusst verwerfen: Display und Debug können Eingaben enthalten.
        let mut config: Self = toml::from_str(text).map_err(|_| ConfigError::Parse)?;
        config.validate()?;
        let base = path.parent().ok_or(ConfigError::AbsolutePath)?;
        config.avatar_dir = resolve_path(base, &config.avatar_dir)?;
        if !config.steam_bridge_db_path.is_empty() {
            config.steam_bridge_db_path = resolve_path(base, &config.steam_bridge_db_path)?;
        }
        Ok(config)
    }

    /// Erst nach erfolgreicher Dateiprüfung aufrufen. Keine Betriebswerte aus ENV.
    pub fn with_secrets(mut self) -> Self {
        use crate::secrets::get_first_string;
        self.discord_oauth_internal_api_token = get_first_string(
            &[
                "TURNIER_INTERNAL_API_TOKEN",
                "MASTER_BROKER_TOKEN",
                "MAIN_BOT_INTERNAL_TOKEN",
                "TWITCH_INTERNAL_API_TOKEN",
            ],
            "",
        );
        self.discord_master_broker_token = get_first_string(
            &[
                "DISCORD_MASTER_BROKER_TOKEN",
                "TURNIER_INTERNAL_API_TOKEN",
                "MASTER_BROKER_TOKEN",
                "MAIN_BOT_INTERNAL_TOKEN",
                "TWITCH_INTERNAL_API_TOKEN",
            ],
            "",
        );
        self.discord_bot_token =
            get_first_string(&["DISCORD_BOT_TOKEN", "DISCORD_TOKEN", "BOT_TOKEN"], "");
        self.jwt_secret = get_first_string(&["JWT_SECRET"], "");
        self.discord_webhook_url = get_first_string(&["DISCORD_WEBHOOK_URL"], "");
        self.turnier_internal_api_token = get_first_string(&["TURNIER_INTERNAL_API_TOKEN"], "");
        self.steam_bot_internal_token = get_first_string(
            &["STEAM_BOT_INTERNAL_TOKEN", "TURNIER_INTERNAL_API_TOKEN"],
            "",
        );
        self.observer_agent_token = get_first_string(&["SCRIM_OBSERVER_AGENT_TOKEN"], "");
        self
    }

    /// Secret-Felder sind von der Serialisierung ausgeschlossen.
    pub fn safe_status(&self) -> Result<String, ConfigError> {
        serde_json::to_string_pretty(self).map_err(|_| ConfigError::Invalid("Statusdarstellung"))
    }

    pub fn fingerprint(&self) -> Result<String, ConfigError> {
        Ok(format!(
            "{:x}",
            Sha256::digest(self.safe_status()?.as_bytes())
        ))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.authorization.validate()?;
        self.steam.validate()?;
        self.bridge.validate()?;
        self.database.validate()?;
        if self.schema_version != SCHEMA_VERSION {
            return Err(ConfigError::Invalid("schema_version: unterstützt wird 1"));
        }
        if !(1..=65535).contains(&self.backend_port) {
            return Err(ConfigError::Invalid("backend_port: 1 bis 65535"));
        }
        if self.backend_host.parse::<std::net::IpAddr>().is_err() {
            return Err(ConfigError::Invalid(
                "backend_host: IP-Adresse erforderlich",
            ));
        }
        for (value, field) in [
            (
                &self.discord_oauth_internal_api_base_url,
                "discord_oauth_internal_api_base_url",
            ),
            (
                &self.discord_master_broker_base_url,
                "discord_master_broker_base_url",
            ),
            (&self.turnier_public_url, "turnier_public_url"),
            (&self.frontend_url, "frontend_url"),
            (&self.steam_bot_base_url, "steam_bot_base_url"),
            (
                &self.observer_steam_bot2_base_url,
                "observer_steam_bot2_base_url",
            ),
            (
                &self.observer_deadlock_api_base_url,
                "observer_deadlock_api_base_url",
            ),
        ] {
            validate_url(value, field)?;
        }
        for origin in &self.cors_extra_origins {
            validate_url(origin, "cors_extra_origins")?;
        }
        for (id, field) in [
            (
                self.discord_match_channel_category_id,
                "discord_match_channel_category_id",
            ),
            (
                self.discord_sammelpunkt_channel_id,
                "discord_sammelpunkt_channel_id",
            ),
            (
                self.discord_team1_voice_channel_id,
                "discord_team1_voice_channel_id",
            ),
            (
                self.discord_team2_voice_channel_id,
                "discord_team2_voice_channel_id",
            ),
            (
                self.discord_tournament_lobby_channel_id,
                "discord_tournament_lobby_channel_id",
            ),
            (self.discord_caster_role_id, "discord_caster_role_id"),
            (
                self.discord_caster_voice_channel_id,
                "discord_caster_voice_channel_id",
            ),
            (self.scrim_guild_id, "scrim_guild_id"),
            (self.scrim_announce_channel_id, "scrim_announce_channel_id"),
            (
                self.routine_proposal_channel_id,
                "routine_proposal_channel_id",
            ),
        ] {
            if id <= 0 {
                return Err(ConfigError::Invalid(field));
            }
        }
        for (id, field) in [
            (self.scrim_signup_role_id, "scrim_signup_role_id"),
            (self.scrim_reserve_role_id, "scrim_reserve_role_id"),
        ] {
            if id.is_some_and(|id| id <= 0) {
                return Err(ConfigError::Invalid(field));
            }
        }
        validate_id(&self.discord_guild_id, "discord_guild_id")?;
        for (list, field) in [
            (&self.discord_admin_role_ids, "discord_admin_role_ids"),
            (
                &self.discord_tournament_admin_role_ids,
                "discord_tournament_admin_role_ids",
            ),
            (&self.discord_mod_role_ids, "discord_mod_role_ids"),
        ] {
            if list.trim().is_empty() {
                return Err(ConfigError::Invalid(field));
            }
            for id in list.split(',') {
                validate_id(id.trim(), field)?;
            }
        }
        if self.avatar_dir.trim().is_empty() {
            return Err(ConfigError::Invalid("avatar_dir"));
        }
        if !(0..=86400).contains(&self.discord_match_channel_delete_delay_seconds) {
            return Err(ConfigError::Invalid(
                "discord_match_channel_delete_delay_seconds",
            ));
        }
        bounded(
            self.scrim_substitute_sweep_interval_seconds,
            1,
            86400,
            "scrim_substitute_sweep_interval_seconds",
        )?;
        if self.routine_tournament_preset_id < 0
            || (self.routine_tournaments_enabled && self.routine_tournament_preset_id == 0)
        {
            return Err(ConfigError::Invalid(
                "routine_tournament_preset_id: bei Aktivierung positiv",
            ));
        }
        if ![
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
        ]
        .contains(&self.routine_tournament_weekday.as_str())
        {
            return Err(ConfigError::Invalid("routine_tournament_weekday"));
        }
        let time = self
            .routine_tournament_time_utc
            .split_once(':')
            .ok_or(ConfigError::Invalid(
                "routine_tournament_time_utc: HH:MM UTC",
            ))?;
        if time.0.len() != 2
            || time.1.len() != 2
            || time.0.parse::<u8>().map_or(true, |v| v > 23)
            || time.1.parse::<u8>().map_or(true, |v| v > 59)
        {
            return Err(ConfigError::Invalid(
                "routine_tournament_time_utc: HH:MM UTC",
            ));
        }
        if !(1..=365).contains(&self.routine_tournament_lead_days)
            || !(0..=10080).contains(&self.routine_tournament_checkin_lead_minutes)
            || !(0..=10080).contains(&self.routine_tournament_bracket_delay_minutes)
        {
            return Err(ConfigError::Invalid("Routine-Zeitgrenzen"));
        }
        validate_url(&self.assets.heroes_url, "assets.heroes_url")?;
        bounded(
            self.assets.heroes_cache_seconds,
            1,
            604800,
            "assets.heroes_cache_seconds",
        )?;
        bounded(
            self.assets.heroes_fallback_cache_seconds,
            1,
            86400,
            "assets.heroes_fallback_cache_seconds",
        )?;
        let l = &self.limits;
        if !(1..=30).contains(&l.session_lifetime_days)
            || !(1..=16 * 1024 * 1024).contains(&l.avatar_bytes)
            || !(1024..=32 * 1024 * 1024).contains(&l.request_body_bytes)
            || l.avatar_bytes > l.request_body_bytes
            || l.comp_body_bytes > l.request_body_bytes
            || !(1024..=1024 * 1024).contains(&l.comp_body_bytes)
            || !(1..=100000).contains(&l.comp_clients)
            || !(1..=100000).contains(&l.comp_reads_per_minute)
            || !(1..=100000).contains(&l.comp_writes_per_minute)
            || !(1..=1000).contains(&l.comp_creations_per_hour)
            || !(1..=1000).contains(&l.draft_creations_per_hour)
            || !(1..=3600).contains(&l.draft_viewer_ttl_seconds)
        {
            return Err(ConfigError::Invalid(
                "limits: außerhalb der dokumentierten Sicherheitsgrenzen",
            ));
        }
        let n = &self.network;
        for value in [
            n.broker_request_seconds,
            n.broker_connect_seconds,
            n.oauth_request_seconds,
            n.oauth_connect_seconds,
            n.steam_discord_request_seconds,
            n.lobby_provision_seconds,
            n.lobby_request_seconds,
            n.lobby_connect_seconds,
            n.scrim_lobby_code_seconds,
            n.scrim_role_sync_seconds,
            n.observer_request_seconds,
            n.observer_connect_seconds,
            n.observer_keepalive_seconds,
            n.heroes_request_seconds,
        ] {
            bounded(
                value,
                1,
                3600,
                "network: Sekunden müssen zwischen 1 und 3600 liegen",
            )?;
        }
        if n.broker_connect_seconds > n.broker_request_seconds
            || n.oauth_connect_seconds > n.oauth_request_seconds
            || n.lobby_connect_seconds > n.lobby_request_seconds
        {
            return Err(ConfigError::Invalid(
                "network: Verbindungsfrist über Gesamtfrist",
            ));
        }
        let s = &self.scheduler;
        for value in [
            s.tick_seconds,
            s.routine_seconds,
            s.scrim_operational_seconds,
            s.lobby_tick_seconds,
            s.lobby_reconcile_seconds,
            s.lobby_collect_seconds,
            s.observer_stale_seconds,
            s.observer_startup_grace_seconds,
        ] {
            bounded(
                value,
                1,
                86400,
                "scheduler: Sekunden müssen zwischen 1 und 86400 liegen",
            )?;
        }
        bounded(
            s.observer_tick_milliseconds,
            50,
            60000,
            "scheduler.observer_tick_milliseconds",
        )?;
        bounded(
            s.observer_evaluate_milliseconds,
            50,
            60000,
            "scheduler.observer_evaluate_milliseconds",
        )?;
        if let Some(a) = &self.observer_agent {
            validate_url(&a.server_base_url, "observer_agent.server_base_url")?;
            if a.bot_account_id != 2 {
                return Err(ConfigError::Invalid(
                    "observer_agent.bot_account_id: vorhandene Bot-2-Schranke",
                ));
            }
            bounded(
                a.poll_milliseconds,
                150,
                2000,
                "observer_agent.poll_milliseconds",
            )?;
            bounded(
                a.request_timeout_seconds,
                1,
                3600,
                "observer_agent.request_timeout_seconds",
            )?;
            let addr = a
                .vconsole_address
                .parse::<std::net::SocketAddr>()
                .map_err(|_| ConfigError::Invalid("observer_agent.vconsole_address"))?;
            if !addr.ip().is_loopback() || addr.port() == 0 {
                return Err(ConfigError::Invalid(
                    "observer_agent.vconsole_address: lokaler Listener erforderlich",
                ));
            }
        }
        Ok(())
    }
}

fn validate_id(value: &str, field: &'static str) -> Result<(), ConfigError> {
    if value.parse::<i64>().is_ok_and(|v| v > 0) {
        Ok(())
    } else {
        Err(ConfigError::Invalid(field))
    }
}
fn bounded(value: u64, min: u64, max: u64, field: &'static str) -> Result<(), ConfigError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(ConfigError::Invalid(field))
    }
}
pub(crate) fn validate_url(value: &str, field: &'static str) -> Result<(), ConfigError> {
    let url = url::Url::parse(value).map_err(|_| ConfigError::Invalid(field))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ConfigError::Invalid(field));
    }
    Ok(())
}
fn resolve_path(base: &Path, value: &str) -> Result<String, ConfigError> {
    let input = Path::new(value);
    let joined = if input.is_absolute() {
        input.to_path_buf()
    } else {
        base.join(input)
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
        .into_os_string()
        .into_string()
        .map_err(|_| ConfigError::Invalid("Datenpfad muss UTF-8 sein"))
}
