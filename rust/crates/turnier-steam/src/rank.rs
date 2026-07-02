//! Rang-Domäne — die einzige Quelle der Wahrheit für Tier↔Name und `rank_score`.
//!
//! Im Python-Original lag dieselbe Rangliste in drei Quellen (`MAIN_RANK_ROLE_IDS`,
//! `RANK_NAMES_BY_TIER`, `seeding.RANK_KEYS`) und die Score-Formel in drei
//! Funktionen (`reader._rank_score`, `seeding.rank_score`, dem Discord-Bot). Hier
//! kollabiert beides zu genau einer Tabelle und genau einer Funktion.

/// Default-Subrang, wenn kein präziser Subrang bekannt ist (z. B. reiner
/// Haupt-Tier-Rollen-Fallback). Entspricht der Python-Heuristik `subrank = 3`.
pub const DEFAULT_SUBRANK: i64 = 3;

/// Tier↔Name-Tabelle (Tier 1..=11), Reihenfolge identisch zu `RANK_NAMES_BY_TIER`
/// und `seeding.RANK_KEYS` im Python-Original.
const RANK_NAMES_BY_TIER: [(i64, &str); 11] = [
    (1, "Initiate"),
    (2, "Seeker"),
    (3, "Alchemist"),
    (4, "Arcanist"),
    (5, "Ritualist"),
    (6, "Emissary"),
    (7, "Archon"),
    (8, "Oracle"),
    (9, "Phantom"),
    (10, "Ascendant"),
    (11, "Eternus"),
];

/// Liefert den Anzeigenamen zu einem Tier (1..=11), sonst `None`.
pub fn rank_name_for_tier(tier: i64) -> Option<&'static str> {
    RANK_NAMES_BY_TIER
        .iter()
        .find(|(t, _)| *t == tier)
        .map(|(_, name)| *name)
}

/// Liefert das Tier zu einem Rang-Namen (Groß-/Kleinschreibung egal), sonst `None`.
///
/// Spiegelt `seeding.RANK_VALUES.get(rank.lower(), 0)` — ein unbekannter Name
/// ergibt `None` (≙ Tier 0 im Python-Default).
pub fn tier_for_rank_name(name: &str) -> Option<i64> {
    let needle = name.trim().to_lowercase();
    RANK_NAMES_BY_TIER
        .iter()
        .find(|(_, n)| n.to_lowercase() == needle)
        .map(|(t, _)| *t)
}

/// Klemmt einen Subrang auf das gültige Intervall 1..=6.
///
/// Entspricht exakt `max(1, min(6, int(subrank or 3)))` aus `seeding.py`. Wichtig:
/// Pythons `subrank or 3` nutzt Falsy-Semantik — sowohl `None` ALS AUCH `0` werden
/// zu 3 (nicht etwa 0→1). Erst danach wird auf 1..=6 geklemmt (negative Werte → 1,
/// Werte > 6 → 6).
fn clamp_subrank(subrank: Option<i64>) -> i64 {
    let base = match subrank {
        None | Some(0) => DEFAULT_SUBRANK,
        Some(value) => value,
    };
    base.clamp(1, 6)
}

/// Balance-Score aus Tier und Subrang — die EINE Score-Funktion des Subsystems.
///
/// Formel exakt wie im Python-Original (`reader._rank_score` / `seeding.rank_score`):
/// - Subrang wird auf 1..=6 geklemmt (Default 3 bei `None`).
/// - Tier 0 (bzw. `None` ≙ unbekannter Rang) ⇒ Sonderfall, Score = 3.
/// - sonst `tier * 6 + subrank`.
pub fn rank_score(tier: Option<i64>, subrank: Option<i64>) -> i64 {
    let tier = tier.unwrap_or(0);
    let sub = clamp_subrank(subrank);
    if tier == 0 {
        return 3;
    }
    tier * 6 + sub
}

/// Score aus einem Rang-NAMEN (Convenience für den Discord-Rollen-Pfad).
///
/// Entspricht `seeding.rank_score(rank_name, subrank)`: unbekannter Name ⇒ Tier 0
/// ⇒ Score 3.
pub fn rank_score_for_name(name: &str, subrank: Option<i64>) -> i64 {
    rank_score(tier_for_rank_name(name), subrank)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_name_roundtrip() {
        for tier in 1..=11 {
            let name = rank_name_for_tier(tier).expect("tier hat einen Namen");
            assert_eq!(tier_for_rank_name(name), Some(tier));
        }
    }

    #[test]
    fn tier_lookup_is_case_insensitive() {
        assert_eq!(tier_for_rank_name("eternus"), Some(11));
        assert_eq!(tier_for_rank_name("ETERNUS"), Some(11));
        assert_eq!(tier_for_rank_name("  Archon  "), Some(7));
    }

    #[test]
    fn unknown_name_has_no_tier() {
        assert_eq!(tier_for_rank_name("Obscurus"), None);
        assert_eq!(rank_name_for_tier(0), None);
        assert_eq!(rank_name_for_tier(12), None);
    }

    #[test]
    fn score_matches_python_formula() {
        // Initiate (Tier 1), Subrank 1 -> 1*6+1 = 7
        assert_eq!(rank_score(Some(1), Some(1)), 7);
        // Eternus (Tier 11), Subrank 6 -> 11*6+6 = 72
        assert_eq!(rank_score(Some(11), Some(6)), 72);
        // Tier 0 / unbekannt -> 3
        assert_eq!(rank_score(Some(0), Some(5)), 3);
        assert_eq!(rank_score(None, Some(5)), 3);
    }

    #[test]
    fn subrank_clamping_and_default() {
        // None -> Default 3
        assert_eq!(rank_score(Some(2), None), 2 * 6 + 3);
        // 0 -> Falsy wie Python (`0 or 3`) -> 3, NICHT 1
        assert_eq!(rank_score(Some(2), Some(0)), 2 * 6 + 3);
        // negativ -> auf 1 hochgezogen (Python: -1 ist truthy -> max(1, min(6,-1)))
        assert_eq!(rank_score(Some(2), Some(-1)), 2 * 6 + 1);
        // 9 -> auf 6 gedeckelt
        assert_eq!(rank_score(Some(2), Some(9)), 2 * 6 + 6);
    }

    #[test]
    fn score_for_name_matches_score_for_tier() {
        assert_eq!(
            rank_score_for_name("Archon", Some(4)),
            rank_score(Some(7), Some(4))
        );
        // Unbekannter Name verhält sich wie Tier 0.
        assert_eq!(rank_score_for_name("Nope", Some(4)), 3);
    }
}
