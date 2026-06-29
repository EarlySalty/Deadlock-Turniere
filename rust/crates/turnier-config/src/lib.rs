//! `turnier-config` — zentrale Konfiguration des Turnier-Backends.
//!
//! Lädt einmal beim Start aus geschichteten Quellen (siehe [`secrets`]) und wird
//! danach als unveränderliches `Arc<Config>` durch die App gereicht.

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

    // --- JWT (im Port effektiv ungenutzt: Sessions sind opake Tokens) ---
    pub jwt_secret: String,

    // --- Datenbanken / Pfade ---
    pub database_path: String,
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
}

impl Config {
    /// Lädt die Konfiguration aus den geschichteten Quellen. Defaults entsprechen
    /// exakt dem Python-Original.
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
            jwt_secret: get_string("JWT_SECRET", ""),
            database_path: get_string("DATABASE_PATH", "data/tournament.db"),
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
        for raw in [&self.discord_admin_role_ids, &self.discord_tournament_admin_role_ids] {
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
        for candidate in [&self.frontend_url, &self.turnier_public_url, &self.backend_host] {
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
        assert_eq!(hostname_of("https://example.com/path"), Some("example.com".to_string()));
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
