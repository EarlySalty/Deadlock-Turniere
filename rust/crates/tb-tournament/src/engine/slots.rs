//! Der Bracket-Slot als typsicheres Enum.
//!
//! Im Python-Original war ein Bracket-Slot ein `@dataclass _BracketEntryRef` mit
//! DREI optionalen Feldern (`team_id` / `source_match_id` / `source_mini_group_id`),
//! die nie gleichzeitig gesetzt sein durften — eine Einladung zu ungültigen
//! Kombinationen. Hier kollabiert das zu genau einem Enum, das immer exakt einen
//! Zustand trägt. `Empty` entspricht `_BracketEntryRef()` ohne gesetzte Felder
//! (ein noch unbestimmter Slot, z. B. ein wartendes Losers-Bracket-Match).

/// Ein Slot in einem Bracket-Match: woher kommt das Team dieses Slots?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BracketSlot {
    /// Konkretes Team (bereits gesetzt).
    Team(i64),
    /// Gewinner eines Quell-Matches (über `source_match*_id` verdrahtet).
    FromMatch(i64),
    /// Sieger einer Quell-Mini-Group (über `source_mini_group*_id` verdrahtet).
    FromMiniGroup(i64),
    /// Noch unbestimmt (kein Team, keine Quelle) — wird später per Advancement
    /// befüllt. Entspricht `_BracketEntryRef()` im Original.
    Empty,
}

impl BracketSlot {
    /// `team_id` für die Spalten `team1_id`/`team2_id` (nur bei [`Self::Team`]).
    pub fn team_id(&self) -> Option<i64> {
        match self {
            BracketSlot::Team(id) => Some(*id),
            _ => None,
        }
    }

    /// `source_match*_id` (nur bei [`Self::FromMatch`]).
    pub fn source_match_id(&self) -> Option<i64> {
        match self {
            BracketSlot::FromMatch(id) => Some(*id),
            _ => None,
        }
    }

    /// `source_mini_group*_id` (nur bei [`Self::FromMiniGroup`]).
    pub fn source_mini_group_id(&self) -> Option<i64> {
        match self {
            BracketSlot::FromMiniGroup(id) => Some(*id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_exactly_one_field() {
        let t = BracketSlot::Team(7);
        assert_eq!(t.team_id(), Some(7));
        assert_eq!(t.source_match_id(), None);
        assert_eq!(t.source_mini_group_id(), None);

        let m = BracketSlot::FromMatch(3);
        assert_eq!(m.team_id(), None);
        assert_eq!(m.source_match_id(), Some(3));

        let g = BracketSlot::FromMiniGroup(5);
        assert_eq!(g.source_mini_group_id(), Some(5));

        let e = BracketSlot::Empty;
        assert_eq!(e.team_id(), None);
        assert_eq!(e.source_match_id(), None);
        assert_eq!(e.source_mini_group_id(), None);
    }
}
