//! Zweistufiger Rang-Cache: prozesslokales L1 (HashMap) + persistentes L2
//! (Tabelle `rank_cache`).
//!
//! Portiert `_get_memory_cache`/`_set_memory_cache`/`_get_cached_rank_profile`/
//! `_store_cached_rank_profile` aus `rank_reader.py`. TTL ist 24 h.
//!
//! Gegenüber dem Original:
//! - Der TTL-Filter liegt direkt im SELECT (`cached_at + 86400 > unixepoch()`),
//!   das spart das separate DELETE-on-expire (zweiter Roundtrip im Python). Eine
//!   abgelaufene Zeile bleibt liegen und wird beim nächsten UPSERT überschrieben —
//!   funktional identisch zum Original (abgelaufen ⇒ Cache-Miss).
//! - Pro Aufruf wird genau eine Connection aus dem geteilten Pool verwendet statt
//!   bis zu drei frischer Verbindungen.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sqlx::FromRow;
use turnier_core::RankProfile;
use turnier_db::Pool;

use crate::error::SteamResult;

/// Cache-TTL: 24 Stunden (entspricht `RANK_CACHE_TTL_SECONDS`).
pub const RANK_CACHE_TTL_SECONDS: i64 = 60 * 60 * 24;

/// L1-TTL als `Duration` für die In-Memory-Ebene.
const L1_TTL: Duration = Duration::from_secs(RANK_CACHE_TTL_SECONDS as u64);

/// Eine Zeile aus `rank_cache` (Spalten 1:1 zum Schema).
#[derive(Debug, FromRow)]
struct RankCacheRow {
    source: String,
    steam_id: Option<String>,
    rank: Option<String>,
    rank_tier: Option<i64>,
    subrank: Option<i64>,
    rank_score: Option<i64>,
}

impl RankCacheRow {
    fn into_profile(self) -> RankProfile {
        RankProfile {
            steam_id: self.steam_id,
            rank: self.rank,
            rank_tier: self.rank_tier,
            subrank: self.subrank,
            // Defensiv: `rank_score` ist nullable in der Tabelle; eine fehlende
            // Spalte wird als 0 gelesen (Python liefert hier `None`/0 ebenso roh).
            rank_score: self.rank_score.unwrap_or(0),
            source: self.source,
        }
    }
}

/// L1-Eintrag: Ablaufzeitpunkt + gecachtes Profil.
struct MemoryEntry {
    expires_at: Instant,
    profile: RankProfile,
}

/// Zweistufiger Rang-Cache über dem geteilten App-DB-Pool.
pub struct RankCache {
    pool: Pool,
    memory: Mutex<HashMap<String, MemoryEntry>>,
}

impl RankCache {
    /// Erstellt den Cache über dem App-DB-Pool (`rank_cache`-Tabelle).
    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            memory: Mutex::new(HashMap::new()),
        }
    }

    /// Liest ein noch gültiges Profil: erst L1, dann L2 (mit TTL-Filter im SELECT).
    /// Ein L2-Treffer füllt L1 nach. Gibt `None` bei Miss oder abgelaufenem Eintrag.
    pub async fn get(&self, discord_id: &str) -> SteamResult<Option<RankProfile>> {
        if let Some(profile) = self.get_memory(discord_id) {
            return Ok(Some(profile));
        }

        let row: Option<RankCacheRow> = sqlx::query_as(
            "SELECT source, steam_id, rank, rank_tier, subrank, rank_score \
             FROM rank_cache \
             WHERE discord_id = ? AND cached_at + ? > unixepoch()",
        )
        .bind(discord_id)
        .bind(RANK_CACHE_TTL_SECONDS)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let profile = row.into_profile();
        self.set_memory(discord_id, &profile);
        Ok(Some(profile))
    }

    /// Persistiert ein Profil per UPSERT in `rank_cache` und füllt L1.
    ///
    /// `cached_at` wird wie im Original auf `strftime('%s','now')` (= aktuelle
    /// Unix-Zeit) gesetzt. `source` fällt auf `"unknown"` zurück, falls leer.
    pub async fn store(&self, discord_id: &str, profile: &RankProfile) -> SteamResult<()> {
        let source = if profile.source.is_empty() {
            "unknown"
        } else {
            profile.source.as_str()
        };

        sqlx::query(
            "INSERT INTO rank_cache \
             (discord_id, source, steam_id, rank, rank_tier, subrank, rank_score, cached_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%s','now')) \
             ON CONFLICT(discord_id) DO UPDATE SET \
             source = excluded.source, \
             steam_id = excluded.steam_id, \
             rank = excluded.rank, \
             rank_tier = excluded.rank_tier, \
             subrank = excluded.subrank, \
             rank_score = excluded.rank_score, \
             cached_at = excluded.cached_at",
        )
        .bind(discord_id)
        .bind(source)
        .bind(&profile.steam_id)
        .bind(&profile.rank)
        .bind(profile.rank_tier)
        .bind(profile.subrank)
        .bind(profile.rank_score)
        .execute(&self.pool)
        .await?;

        self.set_memory(discord_id, profile);
        Ok(())
    }

    /// L1-Lookup mit Lazy-Expiry (abgelaufene Einträge werden entfernt).
    fn get_memory(&self, discord_id: &str) -> Option<RankProfile> {
        let mut guard = self.memory.lock().ok()?;
        let entry = guard.get(discord_id)?;
        if entry.expires_at <= Instant::now() {
            guard.remove(discord_id);
            return None;
        }
        Some(entry.profile.clone())
    }

    /// Setzt einen L1-Eintrag mit 24-h-TTL.
    fn set_memory(&self, discord_id: &str, profile: &RankProfile) {
        if let Ok(mut guard) = self.memory.lock() {
            guard.insert(
                discord_id.to_string(),
                MemoryEntry {
                    expires_at: Instant::now() + L1_TTL,
                    profile: profile.clone(),
                },
            );
        }
    }
}
