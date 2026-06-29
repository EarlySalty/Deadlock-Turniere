//! Rang-Modell (Form). Die konkrete Tier↔Name-Tabelle und die `rank_score`-
//! Formel werden in `turnier-steam` aus dem Original (`rank_reader.py`) zeilengenau
//! portiert und können bei Bedarf hierher (geteilte Domäne) gehoben werden.

use serde::{Deserialize, Serialize};

/// Aufgelöstes Rang-Profil eines Spielers.
///
/// `source` bleibt vorerst ein String, weil das exakte Vokabular der Spalte
/// `rank_cache.source` in `turnier-steam` verifiziert wird, bevor daraus ein Enum
/// wird (Parität vor Typsicherheit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankProfile {
    pub steam_id: Option<String>,
    pub rank: Option<String>,
    pub rank_tier: Option<i64>,
    pub subrank: Option<i64>,
    pub rank_score: i64,
    pub source: String,
}
