//! Reine Team-Namensvergabe (DB-frei): kollisionsfreie Namen über eine
//! In-Memory-Menge bereits vergebener `name_key`s.
//!
//! Portiert `_available_team_names`, `_reserve_team_name`, `_next_team_name` aus
//! `engine.py`. Der `name_key` ist der Vergleichsschlüssel (Python `casefold()`).
//! Für die hier auftretenden Namen (ASCII + übliche Unicode-Namen) ist
//! Rust-`to_lowercase()` deckungsgleich mit Pythons `casefold()`; wir nutzen
//! konsequent `to_lowercase()`.
//!
//! WICHTIG: Das Original prüft die NATO-Namen über das Präfix `"team "` +
//! `name.lower()` (Kleinbuchstaben), reserviert dann aber über `casefold()` des
//! VOLLEN Namens (`"Team Alpha".casefold()` = `"team alpha"`). Beide ergeben
//! denselben Schlüssel — diese Äquivalenz bilden wir hier 1:1 ab.

/// NATO-Alphabet-Namensvorrat (16 Namen), Reihenfolge wie `TEAM_NAMES`.
pub const TEAM_NAMES: [&str; 16] = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India", "Juliet",
    "Kilo", "Lima", "Mike", "November", "Oscar", "Papa",
];

/// Casefold-Äquivalent für den `name_key` (siehe Modul-Doku).
///
/// Pythons `str.casefold()` faltet aggressiver als `lower()` — relevant für
/// deutsche Teamnamen ist vor allem `ß` → `ss` (`to_lowercase()` lässt `ß`
/// stehen). Wir bilden `to_lowercase()` + diese Faltung nach, damit die
/// Duplikat-Erkennung (`UNIQUE(tournament_id, name_key)`) konsistent zu den
/// bereits von Python casefold-erzeugten Schlüsseln in der geteilten DB bleibt.
pub fn name_key(name: &str) -> String {
    name.to_lowercase().replace('ß', "ss")
}

/// Hält die bereits vergebenen `name_key`s und vergibt neue, kollisionsfreie
/// Namen. Spiegelt das mutierende `existing_keys`-Set des Originals.
#[derive(Debug, Clone, Default)]
pub struct TeamNamePool {
    keys: std::collections::HashSet<String>,
}

impl TeamNamePool {
    /// Baut den Pool aus den bereits in der DB vorhandenen `name_key`s.
    pub fn new(existing_keys: impl IntoIterator<Item = String>) -> Self {
        Self {
            keys: existing_keys.into_iter().collect(),
        }
    }

    /// Noch verfügbare NATO-Namen (`"team <name>"` noch nicht vergeben).
    /// Portiert `_available_team_names`.
    fn available_nato_names(&self) -> Vec<&'static str> {
        TEAM_NAMES
            .iter()
            .copied()
            .filter(|name| !self.keys.contains(&format!("team {}", name.to_lowercase())))
            .collect()
    }

    /// Reserviert einen Namen ausgehend von `base_name`; hängt bei Kollision
    /// `(2)`, `(3)`, … an. Leerer `base_name` → nächster generischer Name.
    /// Portiert `_reserve_team_name`.
    pub fn reserve(&mut self, base_name: &str) -> String {
        let candidate = base_name.trim();
        if candidate.is_empty() {
            return self.next_name(1);
        }

        let mut suffix = 1u32;
        let mut team_name = candidate.to_string();
        while self.keys.contains(&name_key(&team_name)) {
            suffix += 1;
            team_name = format!("{candidate} ({suffix})");
        }

        self.keys.insert(name_key(&team_name));
        team_name
    }

    /// Liefert den nächsten generischen Namen: erst freie NATO-Namen
    /// (`Team Alpha`, …), dann numerisch (`Team N`). Portiert `_next_team_name`.
    pub fn next_name(&mut self, ordinal_hint: usize) -> String {
        let available = self.available_nato_names();
        let team_name = if let Some(first) = available.first() {
            format!("Team {first}")
        } else {
            let mut suffix = ordinal_hint.max(self.keys.len() + 1);
            let mut name = format!("Team {suffix}");
            while self.keys.contains(&name_key(&name)) {
                suffix += 1;
                name = format!("Team {suffix}");
            }
            name
        };

        self.keys.insert(name_key(&team_name));
        team_name
    }

    /// Leitet einen Teamnamen vom Captain-Namen ab (oder vergibt einen
    /// generischen). Portiert den NAMENS-Teil von `_team_name_from_captain` —
    /// der DB-Lookup des Captain-Namens passiert im Persistenz-Layer und wird
    /// hier als bereits aufgelöster `captain_name` übergeben.
    ///
    /// Regel: nicht-leerer, nicht-rein-numerischer Name → `"<Name> Team"` mit
    /// Kollisions-Suffix; sonst generischer Name.
    pub fn from_captain(&mut self, captain_name: Option<&str>) -> String {
        let trimmed = captain_name.map(str::trim).unwrap_or("");
        if !trimmed.is_empty() && !is_all_digits(trimmed) {
            self.reserve(&format!("{trimmed} Team"))
        } else {
            self.next_name(1)
        }
    }
}

/// Python `str.isdigit()`-Äquivalent für die hier relevanten Fälle: nicht-leer
/// und ausschließlich ASCII-Ziffern. (Pythons `isdigit()` akzeptiert zusätzlich
/// einige Unicode-Ziffern; für Discord-Namen ist die ASCII-Variante deckungs-
/// gleich mit dem beobachteten Verhalten.)
fn is_all_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_appends_numeric_suffix_on_collision() {
        let mut pool = TeamNamePool::default();
        assert_eq!(pool.reserve("Earlysalty Team"), "Earlysalty Team");
        assert_eq!(pool.reserve("Earlysalty Team"), "Earlysalty Team (2)");
        assert_eq!(pool.reserve("Earlysalty Team"), "Earlysalty Team (3)");
    }

    #[test]
    fn next_name_walks_nato_then_numeric() {
        let mut pool = TeamNamePool::default();
        assert_eq!(pool.next_name(1), "Team Alpha");
        assert_eq!(pool.next_name(1), "Team Bravo");
    }

    #[test]
    fn from_captain_uses_name_or_falls_back() {
        let mut pool = TeamNamePool::default();
        assert_eq!(pool.from_captain(Some("Nova")), "Nova Team");
        // Rein numerischer Captain-Name -> generischer Name.
        assert_eq!(pool.from_captain(Some("12345")), "Team Alpha");
        // Leer -> generisch.
        assert_eq!(pool.from_captain(None), "Team Bravo");
    }

    #[test]
    fn existing_keys_block_nato_names() {
        let mut pool = TeamNamePool::new(["team alpha".to_string()]);
        // Alpha ist vergeben -> nächster freier NATO-Name.
        assert_eq!(pool.next_name(1), "Team Bravo");
    }
}
