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
const SUBRANK_ROLE_CACHE_TTL: Duration = Duration::from_secs(300);

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
}

impl BridgeReader {
    /// Öffnet den read-only Pool auf die Bridge-DB unter `db_path`.
    ///
    /// Gibt `Ok(None)`, wenn der Pfad leer ist ODER die Datei nicht existiert —
    /// das entspricht der Python-Semantik (`if not STEAM_BRIDGE_DB_PATH: return`
    /// bzw. der read-only-Connect schlägt fehl → der Resolver fällt zur
    /// Discord-Stufe durch). Statt still zu scheitern wird einmalig gewarnt.
    pub async fn open(db_path: &str, guild_id: &str) -> SteamResult<Option<Self>> {
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
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await?;

        Ok(Some(Self {
            pool,
            guild_id: guild_id.to_string(),
            subrank_role_cache: Mutex::new(None),
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
        if cached.fetched_at.elapsed() < SUBRANK_ROLE_CACHE_TTL {
            Some(cached.roles.clone())
        } else {
            None
        }
    }
}
