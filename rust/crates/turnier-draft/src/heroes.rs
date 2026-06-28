//! Statische Deadlock-Heldenliste + O(1)-Validierung.
//!
//! Portiert `backend/draft/heroes.py` 1:1: dieselben 26 Helden in derselben
//! Reihenfolge. [`DEADLOCK_HEROES`] ist die geordnete Liste (z. B. für die
//! öffentliche `/heroes`-Route), [`is_valid_hero`] prüft exakte Namensgleichheit
//! über ein einmalig gebautes [`HashSet`].

use std::collections::HashSet;

use once_cell::sync::Lazy;

/// Die 26 Deadlock-Helden in der Reihenfolge des Python-Originals.
/// Reihenfolge ist Teil des Vertrags (die `/heroes`-Route gibt sie so aus).
pub static DEADLOCK_HEROES: [&str; 26] = [
    "Abrams",
    "Bebop",
    "Calico",
    "Dynamo",
    "Grey Talon",
    "Haze",
    "Holliday",
    "Infernus",
    "Ivy",
    "Kelvin",
    "Lady Geist",
    "Lash",
    "McGinnis",
    "Mirage",
    "Mo & Krill",
    "Paradox",
    "Pocket",
    "Seven",
    "Shiv",
    "Sinclair",
    "Vindicta",
    "Viscous",
    "Vyper",
    "Warden",
    "Wraith",
    "Yamato",
];

/// Einmalig gebautes Set für O(1)-Lookups (entspricht `HERO_SET` im Original).
static HERO_SET: Lazy<HashSet<&'static str>> = Lazy::new(|| DEADLOCK_HEROES.iter().copied().collect());

/// Prüft, ob `name` exakt einem bekannten Helden entspricht. Wie im Original
/// gibt es keine Normalisierung (Groß-/Kleinschreibung zählt).
pub fn is_valid_hero(name: &str) -> bool {
    HERO_SET.contains(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liste_hat_26_helden() {
        assert_eq!(DEADLOCK_HEROES.len(), 26);
    }

    #[test]
    fn bekannte_helden_sind_gueltig() {
        assert!(is_valid_hero("Abrams"));
        assert!(is_valid_hero("Mo & Krill"));
        assert!(is_valid_hero("Grey Talon"));
        assert!(is_valid_hero("Yamato"));
    }

    #[test]
    fn unbekannte_und_falsch_geschriebene_sind_ungueltig() {
        assert!(!is_valid_hero("abrams")); // case-sensitiv wie im Original
        assert!(!is_valid_hero("Unbekannt"));
        assert!(!is_valid_hero(""));
        assert!(!is_valid_hero("Mo and Krill"));
    }
}
