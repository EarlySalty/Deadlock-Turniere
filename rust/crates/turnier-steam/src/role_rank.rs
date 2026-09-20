//! Auflösung eines Rang-Profils aus Discord-Rollen-IDs.
//!
//! Portiert die reine Logik von `get_discord_role_rank` (`rank_reader.py`) OHNE
//! den HTTP-Teil (der liegt in [`crate::discord`]): aus den Rollen-IDs eines
//! Members und dem Subrank-Rollen-Mapping der Bridge-DB wird der höchste Rang
//! bestimmt; fehlt eine Subrank-Rolle, greift der Haupt-Tier-Fallback.

use turnier_core::RankProfile;

use crate::rank::{self, DEFAULT_SUBRANK};

/// Source-Wert für aus Discord-Rollen abgeleitete Profile.
pub const SOURCE_DISCORD_ROLE: &str = "discord_role";

/// Bestimmt aus den Rollen-IDs eines Members das Rang-Profil.
///
/// Reihenfolge wie im Original:
/// 1. Subrank-Rollen (aus der Bridge-DB, `subrank_roles`): Kandidaten sind
///    `(tier, subrank)`-Paare; gewählt wird das Maximum nach `(tier, subrank)`.
///    Hat das gewählte Tier einen bekannten Namen, ist das Profil fertig.
/// 2. Sonst Haupt-Tier-Rollen: Kandidaten sind `(name, tier)`; gewählt wird das
///    Maximum nach `tier`, mit fixem Default-Subrang 3 (`DEFAULT_SUBRANK`).
/// 3. Sonst `None`.
///
/// `role_ids` sind die rohen Discord-Rollen-ID-Strings; nicht-numerische werden
/// (wie im Original via `.isdigit()`) ignoriert. `subrank_roles` ist das Mapping
/// `(role_id, tier, subrank)` aus der Bridge-DB.
#[cfg(test)]
pub fn resolve_from_roles(
    role_ids: &[String],
    subrank_roles: &[(i64, i64, i64)],
) -> Option<RankProfile> {
    resolve_from_roles_with_mapping(
        role_ids,
        subrank_roles,
        &turnier_config::SteamConfig::default().main_rank_role_ids,
    )
}

pub fn resolve_from_roles_with_mapping(
    role_ids: &[String],
    subrank_roles: &[(i64, i64, i64)],
    main_rank_role_ids: &[i64],
) -> Option<RankProfile> {
    let parsed: Vec<i64> = role_ids
        .iter()
        .filter_map(|id| id.trim().parse::<i64>().ok())
        .collect();

    // Stufe 1: Subrank-Rollen.
    let mut subrank_candidates: Vec<(i64, i64)> = parsed
        .iter()
        .filter_map(|role_id| {
            subrank_roles
                .iter()
                .find(|(rid, _, _)| rid == role_id)
                .map(|(_, tier, subrank)| (*tier, *subrank))
        })
        .collect();

    if let Some(&(rank_tier, subrank)) = subrank_candidates
        .iter()
        .max_by_key(|(tier, subrank)| (*tier, *subrank))
    {
        if let Some(rank_name) = rank::rank_name_for_tier(rank_tier) {
            return Some(RankProfile {
                steam_id: None,
                rank: Some(rank_name.to_string()),
                rank_tier: Some(rank_tier),
                subrank: Some(subrank),
                rank_score: rank::rank_score(Some(rank_tier), Some(subrank)),
                source: SOURCE_DISCORD_ROLE.to_string(),
            });
        }
    }
    subrank_candidates.clear();

    // Stufe 2: Haupt-Tier-Rollen mit Default-Subrang.
    let main_candidate = parsed
        .iter()
        .filter_map(|role_id| {
            main_rank_role_ids
                .iter()
                .position(|rid| rid == role_id)
                .and_then(|index| {
                    let tier = index as i64 + 1;
                    rank::rank_name_for_tier(tier).map(|name| (name, tier))
                })
        })
        .max_by_key(|(_, tier)| *tier);

    let (rank_name, rank_tier) = main_candidate?;
    Some(RankProfile {
        steam_id: None,
        rank: Some(rank_name.to_string()),
        rank_tier: Some(rank_tier),
        subrank: Some(DEFAULT_SUBRANK),
        rank_score: rank::rank_score(Some(rank_tier), Some(DEFAULT_SUBRANK)),
        source: SOURCE_DISCORD_ROLE.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subrank_role_wins_and_picks_highest() {
        // Zwei Subrank-Rollen: Tier 7/sub 2 und Tier 9/sub 4 -> Tier 9 gewinnt.
        let roles = vec!["100".to_string(), "200".to_string(), "garbage".to_string()];
        let subrank_roles = vec![(100, 7, 2), (200, 9, 4)];
        let profile = resolve_from_roles(&roles, &subrank_roles).expect("Profil");
        assert_eq!(profile.rank_tier, Some(9));
        assert_eq!(profile.subrank, Some(4));
        assert_eq!(profile.rank.as_deref(), Some("Phantom"));
        assert_eq!(profile.rank_score, 9 * 6 + 4);
        assert_eq!(profile.source, SOURCE_DISCORD_ROLE);
    }

    #[test]
    fn subrank_tie_breaks_on_subrank() {
        // Gleiches Tier, höherer Subrank gewinnt.
        let roles = vec!["1".to_string(), "2".to_string()];
        let subrank_roles = vec![(1, 5, 2), (2, 5, 6)];
        let profile = resolve_from_roles(&roles, &subrank_roles).expect("Profil");
        assert_eq!(profile.subrank, Some(6));
    }

    #[test]
    fn falls_back_to_main_tier_role_with_default_subrank() {
        // Keine Subrank-Rolle -> Haupt-Tier-Rolle (Archon, Tier 7) mit Subrank 3.
        let roles = vec!["1331457949654319114".to_string()];
        let profile = resolve_from_roles(&roles, &[]).expect("Profil");
        assert_eq!(profile.rank.as_deref(), Some("Archon"));
        assert_eq!(profile.rank_tier, Some(7));
        assert_eq!(profile.subrank, Some(DEFAULT_SUBRANK));
        assert_eq!(profile.rank_score, 7 * 6 + 3);
    }

    #[test]
    fn main_tier_picks_highest_tier() {
        // Oracle (8) und Initiate (1) -> Oracle gewinnt.
        let roles = vec![
            "1316966867033653338".to_string(),
            "1331457571118387210".to_string(),
        ];
        let profile = resolve_from_roles(&roles, &[]).expect("Profil");
        assert_eq!(profile.rank_tier, Some(8));
        assert_eq!(profile.rank.as_deref(), Some("Oracle"));
    }

    #[test]
    fn no_matching_role_is_none() {
        let roles = vec!["999".to_string(), "abc".to_string()];
        assert!(resolve_from_roles(&roles, &[]).is_none());
        assert!(resolve_from_roles(&[], &[]).is_none());
    }

    #[test]
    fn unknown_subrank_tier_falls_through_to_main() {
        // Subrank-Rolle mit Tier 99 (kein Name) -> fällt auf Haupt-Tier-Rolle durch.
        let roles = vec![
            "500".to_string(),
            "1331457571118387210".to_string(), // Initiate
        ];
        let subrank_roles = vec![(500, 99, 4)];
        let profile = resolve_from_roles(&roles, &subrank_roles).expect("Profil");
        assert_eq!(profile.rank.as_deref(), Some("Initiate"));
        assert_eq!(profile.rank_tier, Some(1));
    }
}
