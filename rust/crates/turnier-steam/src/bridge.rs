//! Read-only Reader auf die externe Steam-Bridge-SQLite des Discord-Bots.
//!
//! Portiert `steam/reader.py` (`get_steam_link`) und den Subrank-Rollen-Loader
//! aus `rank_reader.py` (`_load_discord_subrank_roles`). Beide Tabellen liegen in
//! einer FREMDEN DB (Discord-Steam-Bot); der Zugriff ist strikt read-only über
//! einen eigenen Pool mit `SqliteConnectOptions::read_only(true)`.

use std::path::Path;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::FromRow;

use crate::error::SteamResult;
use crate::rank;

/// TTL des Subrank-Rollen-Caches: 5 Minuten (wie `_discord_subrank_role_cache`).

/// Eine Zeile aus `steam_links` der Bridge-DB.
#[derive(Debug, FromRow)]
struct SteamLinkRow {
    steam_id: Option<String>,
    deadlock_rank: Option<i64>,
    deadlock_rank_name: Option<String>,
    deadlock_subrank: Option<i64>,
}

/// Roh-Ergebnis eines Steam-Link-Lookups (Rohdaten + lokal berechneter Score).
#[derive(Debug, Clone)]
pub struct SteamLink {
    pub steam_id: Option<String>,
    pub rank: Option<String>,
    pub rank_tier: Option<i64>,
    pub subrank: Option<i64>,
    pub rank_score: i64,
}

/// Eine Zeile aus `deadlock_subrank_roles` der Bridge-DB.
#[derive(Debug, FromRow)]
struct SubrankRoleRow {
    role_id: Option<i64>,
    rank_value: Option<i64>,
    subrank: Option<i64>,
}

/// Eintrag des Subrank-Rollen-Caches (Mapping role_id → (Tier, Subrank)).
struct CachedSubrankRoles {
    fetched_at: Instant,
    roles: Vec<(i64, i64, i64)>,
}

/// Read-only Zugriff auf die Steam-Bridge-DB. Hält den Pool, die Guild-ID und den
/// 5-Minuten-Cache der Subrank-Rollen.
pub struct BridgeReader {
    pool: SqlitePool,
    guild_id: String,
    subrank_role_cache: Mutex<Option<CachedSubrankRoles>>,
    subrank_role_cache_ttl: Duration,
}

impl BridgeReader {
    /// Öffnet den read-only Pool auf die Bridge-DB unter `db_path`.
    ///
    /// Gibt `Ok(None)`, wenn der Pfad leer ist, die Datei nicht existiert oder
    /// der read-only-Connect fehlschlägt. Die Bridge ist optional; ein kaputter
    /// Pfad darf den Turnier-Boot nicht abbrechen, sondern deaktiviert nur die
    /// Rang-Anreicherung über die Bridge.
    pub async fn open(db_path: &str, guild_id: &str) -> SteamResult<Option<Self>> {
        Self::with_settings(
            db_path,
            guild_id,
            &turnier_config::BridgeConfig::default(),
            turnier_config::SteamConfig::default().subrank_role_cache_seconds,
        )
        .await
    }

    pub async fn from_config(config: &turnier_config::Config) -> SteamResult<Option<Self>> {
        Self::with_settings(
            &config.steam_bridge_db_path,
            &config.discord_guild_id,
            &config.bridge,
            config.steam.subrank_role_cache_seconds,
        )
        .await
    }

    async fn with_settings(
        db_path: &str,
        guild_id: &str,
        settings: &turnier_config::BridgeConfig,
        cache_seconds: u64,
    ) -> SteamResult<Option<Self>> {
        if db_path.trim().is_empty() {
            tracing::warn!("Steam-Bridge-DB-Pfad nicht konfiguriert — Bridge-Lookup übersprungen");
            return Ok(None);
        }
        if !Path::new(db_path).exists() {
            tracing::warn!(
                path = db_path,
                "Steam-Bridge-DB nicht vorhanden — Bridge-Lookup übersprungen, Fallback auf Discord-Rollen"
            );
            return Ok(None);
        }

        let options = SqliteConnectOptions::from_str(db_path)
            .unwrap_or_else(|_| SqliteConnectOptions::new().filename(db_path))
            .read_only(true)
            .create_if_missing(false)
            .busy_timeout(Duration::from_secs(settings.busy_timeout_seconds));

        let pool = match SqlitePoolOptions::new()
            .max_connections(settings.read_connections)
            .connect_with(options)
            .await
        {
            Ok(pool) => pool,
            Err(err) => {
                tracing::warn!(
                    path = db_path,
                    error = %err,
                    "Steam-Bridge-DB kann nicht geöffnet werden — Bridge-Lookup übersprungen, Fallback auf Discord-Rollen"
                );
                return Ok(None);
            }
        };

        Ok(Some(Self {
            pool,
            guild_id: guild_id.to_string(),
            subrank_role_cache: Mutex::new(None),
            subrank_role_cache_ttl: Duration::from_secs(cache_seconds),
        }))
    }

    /// Liest `steam_id` + Rangdaten für einen Discord-User (`steam_links.user_id`).
    ///
    /// Bevorzugt den als primary markierten Account, sonst den besten Account mit
    /// Rangdaten. `primary_account` wird NULL-sicher als 0 behandelt
    /// (`COALESCE(primary_account, 0)`), damit der Primary-Vorrang auch bei NULL
    /// definiert sortiert. Gibt `None`, wenn kein Link existiert.
    pub async fn get_steam_link(&self, discord_id: &str) -> SteamResult<Option<SteamLink>> {
        let row: Option<SteamLinkRow> = sqlx::query_as(
            "SELECT steam_id, deadlock_rank, deadlock_rank_name, deadlock_subrank \
             FROM steam_links \
             WHERE user_id = ? \
             ORDER BY \
             COALESCE(primary_account, 0) DESC, \
             CASE WHEN deadlock_rank IS NULL THEN 1 ELSE 0 END ASC, \
             deadlock_rank DESC, \
             deadlock_subrank DESC \
             LIMIT 1",
        )
        .bind(discord_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let rank_score = rank::rank_score(row.deadlock_rank, row.deadlock_subrank);
        Ok(Some(SteamLink {
            steam_id: row.steam_id,
            rank: row.deadlock_rank_name,
            rank_tier: row.deadlock_rank,
            subrank: row.deadlock_subrank,
            rank_score,
        }))
    }

    /// Lädt das Mapping role_id → (Tier, Subrank) aus `deadlock_subrank_roles` der
    /// konfigurierten Guild, mit 5-Minuten-In-Memory-Cache.
    ///
    /// `guild_id` wird im Python als `int(...)` gebunden; da `guild_id` in der
    /// Bridge-DB numerisch ist und wir die ID als String halten, binden wir den
    /// rohen ID-String — SQLite vergleicht numerische Spalten gegen numerische
    /// Text-Literale wertgleich.
    pub async fn load_subrank_roles(&self) -> SteamResult<Vec<(i64, i64, i64)>> {
        if let Some(cached) = self.cached_roles() {
            return Ok(cached);
        }

        let rows: Vec<SubrankRoleRow> = sqlx::query_as(
            "SELECT role_id, rank_value, subrank \
             FROM deadlock_subrank_roles \
             WHERE guild_id = ?",
        )
        .bind(&self.guild_id)
        .fetch_all(&self.pool)
        .await?;

        let roles: Vec<(i64, i64, i64)> = rows
            .into_iter()
            .filter_map(|r| match (r.role_id, r.rank_value, r.subrank) {
                (Some(role_id), Some(rank_value), Some(subrank)) => {
                    Some((role_id, rank_value, subrank))
                }
                _ => None,
            })
            .collect();

        if let Ok(mut guard) = self.subrank_role_cache.lock() {
            *guard = Some(CachedSubrankRoles {
                fetched_at: Instant::now(),
                roles: roles.clone(),
            });
        }
        Ok(roles)
    }

    /// Gibt die gecachten Subrank-Rollen zurück, solange die TTL nicht abgelaufen ist.
    fn cached_roles(&self) -> Option<Vec<(i64, i64, i64)>> {
        let guard = self.subrank_role_cache.lock().ok()?;
        let cached = guard.as_ref()?;
        if cached.fetched_at.elapsed() < self.subrank_role_cache_ttl {
            Some(cached.roles.clone())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[tokio::test]
    async fn open_existing_but_unopenable_path_degrades_to_none() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "turnier-steam-bridge-dir-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).expect("temp bridge dir");
        let path = dir.to_str().expect("utf-8 temp path");

        let reader = BridgeReader::open(path, "123456789012345678")
            .await
            .expect("bridge open degrades");

        assert!(reader.is_none());
        std::fs::remove_dir_all(dir).expect("remove temp bridge dir");
    }
}
