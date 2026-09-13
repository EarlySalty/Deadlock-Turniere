//! `turnier-config` — zentrale Konfiguration des Turnier-Backends.
//!
//! Lädt einmal beim Start aus geschichteten Quellen (siehe [`secrets`]) und wird
//! danach als unveränderliches `Arc<Config>` durch die App gereicht.
//!
//! `DEADLOCK_CENTRAL_DSN` ist Pflicht fuer das Rust-Backend, wird aber bewusst
//! nicht in [`Config`] gespeichert: `turnier_db::connect_central()` liest die
//! Umgebungsvariable direkt und gibt beim Fehlen einen Startfehler zurueck, ohne
//! den Wert zu loggen. `DATABASE_PATH` ist nur noch Python-/SQLite-Legacy und wird
//! vom Rust-Backend ignoriert.

pub mod secrets;

use std::collections::BTreeSet;

use secrets::{get_bool, get_first_string, get_int, get_string};

/// Vollständige, aufgelöste Laufzeit-Konfiguration.
#[derive(Debug, Clone)]
pub struct Config {
    // --- Discord-OAuth-Delegation via Deadlock-Bots / Master-Broker ---
    pub discord_oauth_internal_api_base_url: String,
    pub discord_oauth_internal_api_token: String,
    pub discord_master_broker_base_url: String,
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
    pub discord_webhook_url: String,

    // --- Scrim cutover ---
    pub turnier_internal_api_token: String,

    // --- Steam-Bot (interne HTTP-API, localhost) ---
    pub steam_bot_base_url: String,
    pub steam_bot_internal_token: String,

    // --- Auto-Observer / Steam Bot 2 ---
    pub observer_enabled: bool,
    /// Separater Kill-Switch fuer jede automatisierte Interaktion mit dem
    /// lokalen Deadlock-Client. Default bleibt aus; Shadow/Assist funktionieren
    /// ohne diesen Schalter.
    pub observer_game_control_enabled: bool,
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
    /// Lädt die Konfiguration aus den geschichteten Quellen. Defaults entsprechen
    /// dem Python-Original, soweit die Werte im Rust-Backend noch aktiv sind.
    pub fn from_env() -> Self {
        let cfg = Self {
            discord_oauth_internal_api_base_url: get_string(
                "DISCORD_OAUTH_INTERNAL_API_BASE_URL",
                "http://127.0.0.1:8766",
            ),
            discord_oauth_internal_api_token: get_first_string(
                &[
                    "TURNIER_INTERNAL_API_TOKEN",
                    "MASTER_BROKER_TOKEN",
                    "MAIN_BOT_INTERNAL_TOKEN",
                    "TWITCH_INTERNAL_API_TOKEN",
                ],
                "",
            ),
            discord_master_broker_base_url: get_first_string(
                &[
                    "DISCORD_MASTER_BROKER_BASE_URL",
                    "DISCORD_OAUTH_INTERNAL_API_BASE_URL",
                ],
                "http://127.0.0.1:8766",
            ),
            discord_master_broker_token: get_first_string(
                &[
                    "DISCORD_MASTER_BROKER_TOKEN",
                    "TURNIER_INTERNAL_API_TOKEN",
                    "MASTER_BROKER_TOKEN",
                    "MAIN_BOT_INTERNAL_TOKEN",
                    "TWITCH_INTERNAL_API_TOKEN",
                ],
                "",
            ),
            discord_match_channel_category_id: get_int(
                "DISCORD_MATCH_CHANNEL_CATEGORY_ID",
                1412800850580996256,
            ),
            discord_match_channel_delete_delay_seconds: get_int(
                "DISCORD_MATCH_CHANNEL_DELETE_DELAY_SECONDS",
                300,
            ),
            discord_sammelpunkt_channel_id: get_int(
                "DISCORD_SAMMELPUNKT_CHANNEL_ID",
                1426160735469174875,
            ),
            discord_team1_voice_channel_id: get_int(
                "DISCORD_TEAM1_VOICE_CHANNEL_ID",
                1462434609563173019,
            ),
            discord_team2_voice_channel_id: get_int(
                "DISCORD_TEAM2_VOICE_CHANNEL_ID",
                1462434639858897017,
            ),
            discord_tournament_lobby_channel_id: get_int(
                "DISCORD_TOURNAMENT_LOBBY_CHANNEL_ID",
                1412411665713987635,
            ),
            discord_caster_role_id: get_int("DISCORD_CASTER_ROLE_ID", 1495154811799077067),
            discord_caster_voice_channel_id: get_int(
                "DISCORD_CASTER_VOICE_CHANNEL_ID",
                1495155113772450042,
            ),
            discord_bot_token: get_first_string(
                &["DISCORD_BOT_TOKEN", "DISCORD_TOKEN", "BOT_TOKEN"],
                "",
            ),
            turnier_public_url: get_string(
                "TURNIER_PUBLIC_URL",
                "https://deutsche-deadlock-community.de/turnier",
            ),
            discord_guild_id: get_string("DISCORD_GUILD_ID", "1289721245281292288"),
            discord_admin_role_ids: get_string(
                "DISCORD_ADMIN_ROLE_IDS",
                "1304169657124782100,1337518124647579661,1411000883155832852,1401891955931222110",
            ),
            discord_tournament_admin_role_ids: get_string(
                "DISCORD_TOURNAMENT_ADMIN_ROLE_IDS",
                "1494120177577754747",
            ),
            discord_mod_role_ids: get_string("DISCORD_MOD_ROLE_IDS", "1474210107255554331"),
            scrim_guild_id: get_int("SCRIM_GUILD_ID", 1_289_721_245_281_292_288),
            scrim_signup_role_id: optional_positive_int(
                "SCRIM_SIGNUP_ROLE_ID",
                1_520_849_762_851_618_817,
            ),
            scrim_reserve_role_id: optional_positive_int(
                "SCRIM_RESERVE_ROLE_ID",
                1_523_803_562_306_703_430,
            ),
            scrim_announce_channel_id: get_int(
                "SCRIM_ANNOUNCE_CHANNEL_ID",
                1_521_522_998_199_324_853,
            ),
            scrim_substitute_sweep_interval_seconds: positive_seconds(
                get_int("SCRIM_SUBSTITUTE_SWEEP_INTERVAL_SECONDS", 600),
                600,
            ),
            jwt_secret: get_string("JWT_SECRET", ""),
            avatar_dir: get_string("AVATAR_DIR", "data/avatars"),
            steam_bridge_db_path: get_string(
                "STEAM_BRIDGE_DB_PATH",
                r"C:\Users\Nani-Admin\Documents\Deadlock\service\deadlock.sqlite3",
            ),
            backend_host: get_string("BACKEND_HOST", "127.0.0.1"),
            backend_port: get_int("BACKEND_PORT", 8900),
            backend_allowed_hosts: get_string("BACKEND_ALLOWED_HOSTS", ""),
            expose_api_docs: get_bool("EXPOSE_API_DOCS", false),
            frontend_url: get_string(
                "FRONTEND_URL",
                "https://deutsche-deadlock-community.de/turnier",
            ),
            discord_webhook_url: get_string("DISCORD_WEBHOOK_URL", ""),
            turnier_internal_api_token: get_string("TURNIER_INTERNAL_API_TOKEN", ""),
            steam_bot_base_url: get_string("STEAM_BOT_BASE_URL", "http://127.0.0.1:8782"),
            steam_bot_internal_token: get_first_string(
                &["STEAM_BOT_INTERNAL_TOKEN", "TURNIER_INTERNAL_API_TOKEN"],
                "",
            ),
            observer_enabled: get_bool("SCRIM_OBSERVER_ENABLED", false),
            observer_game_control_enabled: get_bool("SCRIM_OBSERVER_GAME_CONTROL_ENABLED", false),
            observer_agent_token: get_string("SCRIM_OBSERVER_AGENT_TOKEN", ""),
            observer_steam_bot2_base_url: get_string(
                "SCRIM_OBSERVER_STEAM_BOT2_BASE_URL",
                "http://127.0.0.1:8784",
            ),
            observer_deadlock_api_base_url: get_string(
                "SCRIM_OBSERVER_DEADLOCK_API_BASE_URL",
                "https://api.deadlock-api.com",
            ),
            observer_controller_query: get_string(
                "SCRIM_OBSERVER_CONTROLLER_QUERY",
                "SELECT * FROM CCitadelPlayerController",
            ),
            observer_pawn_query: get_string(
                "SCRIM_OBSERVER_PAWN_QUERY",
                "SELECT * FROM CCitadelPlayerPawn",
            ),
            routine_tournaments_enabled: get_bool("ROUTINE_TOURNAMENTS_ENABLED", false),
            routine_proposal_channel_id: get_int(
                "ROUTINE_PROPOSAL_CHANNEL_ID",
                1474543558793887937,
            ),
            routine_tournament_preset_id: get_int("ROUTINE_TOURNAMENT_PRESET_ID", 0),
            routine_tournament_weekday: get_string("ROUTINE_TOURNAMENT_WEEKDAY", "saturday"),
            routine_tournament_time_utc: get_string("ROUTINE_TOURNAMENT_TIME_UTC", "18:00"),
            routine_tournament_lead_days: get_int("ROUTINE_TOURNAMENT_LEAD_DAYS", 7),
            routine_tournament_checkin_lead_minutes: get_int(
                "ROUTINE_TOURNAMENT_CHECKIN_LEAD_MINUTES",
                30,
            ),
            routine_tournament_bracket_delay_minutes: get_int(
                "ROUTINE_TOURNAMENT_BRACKET_DELAY_MINUTES",
                180,
            ),
        };

        if cfg.discord_oauth_internal_api_token.is_empty() {
            tracing::warn!(
                "Discord-OAuth Internal-API-Token nicht konfiguriert \
                 (TURNIER_INTERNAL_API_TOKEN/MASTER_BROKER_TOKEN/MAIN_BOT_INTERNAL_TOKEN/TWITCH_INTERNAL_API_TOKEN)"
            );
        }
        cfg
    }

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
        origins.insert("http://localhost:5173".to_string());
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

fn optional_positive_int(key: &str, default: i64) -> Option<i64> {
    match std::env::var(key) {
        Ok(value) => value.trim().parse().ok().filter(|value| *value > 0),
        Err(_) => Some(default),
    }
}

fn positive_seconds(value: i64, default: u64) -> u64 {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(default)
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

    #[test]
    fn positive_seconds_rejects_non_positive_values() {
        assert_eq!(positive_seconds(10, 600), 10);
        assert_eq!(positive_seconds(0, 600), 600);
        assert_eq!(positive_seconds(-1, 600), 600);
    }
}
