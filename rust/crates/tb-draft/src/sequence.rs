//! Reine Zustandsmaschine der Pick/Ban-Sequenz — DB-frei, voll unit-testbar.
//!
//! Hier liegt das Wissen über die feste Reihenfolge (6 Bans + 12 Picks, je 6 pro
//! Team) und über das Fortschreiten anhand eines `current_action_index`. Die
//! Persistenz-Schicht ([`crate::repo`]) konsultiert diese Logik, hält aber selbst
//! keine Sequenz-Konstanten.
//!
//! Portiert `DEFAULT_SEQUENCE` aus `backend/draft/engine.py` 1:1.

use serde::{Deserialize, Serialize};

/// Aktionstyp einer Draft-Position. TEXT-Spalte `draft_actions.action_type`
/// (`'ban'` | `'pick'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum ActionType {
    Ban,
    Pick,
}

impl ActionType {
    /// Der DB-/Wire-String dieser Variante (`"ban"` | `"pick"`).
    pub fn as_str(self) -> &'static str {
        match self {
            ActionType::Ban => "ban",
            ActionType::Pick => "pick",
        }
    }
}

/// Team-Slot einer Draft-Position. INTEGER-Spalte `draft_actions.team_slot`
/// (`1` | `2`). Bewusst kein Enum mit String-Mapping — die Spalte ist numerisch;
/// der Typ kapselt nur die beiden gültigen Werte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeamSlot {
    One,
    Two,
}

impl TeamSlot {
    /// Der numerische DB-/Wire-Wert (`1` | `2`).
    pub fn as_i64(self) -> i64 {
        match self {
            TeamSlot::One => 1,
            TeamSlot::Two => 2,
        }
    }
}

/// Eine Position der Draft-Sequenz: was (Ban/Pick) macht welches Team.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceStep {
    pub action_type: ActionType,
    pub team_slot: TeamSlot,
}

use ActionType::{Ban, Pick};
use TeamSlot::{One, Two};

/// Standard-Sequenz: 6 Bans + 12 Picks (6 pro Team). 1:1 zu `DEFAULT_SEQUENCE`.
pub const DEFAULT_SEQUENCE: [SequenceStep; 18] = [
    step(Ban, One),
    step(Ban, Two),
    step(Ban, One),
    step(Ban, Two),
    step(Ban, One),
    step(Ban, Two),
    step(Pick, One),
    step(Pick, Two),
    step(Pick, Two),
    step(Pick, One),
    step(Pick, One),
    step(Pick, Two),
    step(Pick, Two),
    step(Pick, One),
    step(Pick, One),
    step(Pick, Two),
    step(Pick, Two),
    step(Pick, One),
];

/// Kurz-Konstruktor für die Sequenz-Tabelle (`const`-fähig).
const fn step(action_type: ActionType, team_slot: TeamSlot) -> SequenceStep {
    SequenceStep {
        action_type,
        team_slot,
    }
}

/// Anzahl der Aktionen in der Standard-Sequenz (entspricht `len(DEFAULT_SEQUENCE)`).
pub const SEQUENCE_LEN: usize = DEFAULT_SEQUENCE.len();

/// Liefert die Aktion an `index` oder `None`, wenn der Index außerhalb der
/// Sequenz liegt. Zentralisiert den Index-Zugriff (im Original an zwei Stellen
/// uneinheitlich abgesichert) über `slice::get`.
pub fn step_at(index: usize) -> Option<SequenceStep> {
    DEFAULT_SEQUENCE.get(index).copied()
}

/// `true`, wenn ein `current_action_index` das Sequenz-Ende erreicht/überschritten
/// hat — die Session ist dann abgeschlossen. Spiegelt `next_idx >= len(...)`.
pub fn is_complete(index: usize) -> bool {
    index >= SEQUENCE_LEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequenz_hat_6_bans_und_12_picks() {
        assert_eq!(SEQUENCE_LEN, 18);
        let bans = DEFAULT_SEQUENCE
            .iter()
            .filter(|s| s.action_type == ActionType::Ban)
            .count();
        let picks = DEFAULT_SEQUENCE
            .iter()
            .filter(|s| s.action_type == ActionType::Pick)
            .count();
        assert_eq!(bans, 6);
        assert_eq!(picks, 12);
    }

    #[test]
    fn picks_sind_je_team_gleich_verteilt() {
        let team1_picks = DEFAULT_SEQUENCE
            .iter()
            .filter(|s| s.action_type == ActionType::Pick && s.team_slot == TeamSlot::One)
            .count();
        let team2_picks = DEFAULT_SEQUENCE
            .iter()
            .filter(|s| s.action_type == ActionType::Pick && s.team_slot == TeamSlot::Two)
            .count();
        assert_eq!(team1_picks, 6);
        assert_eq!(team2_picks, 6);
    }

    #[test]
    fn bans_alternieren_team1_team2() {
        // Erste 6 Positionen sind Bans, beginnend mit Team 1, alternierend.
        for (i, slot) in [One, Two, One, Two, One, Two].into_iter().enumerate() {
            let s = DEFAULT_SEQUENCE[i];
            assert_eq!(s.action_type, ActionType::Ban);
            assert_eq!(s.team_slot, slot);
        }
    }

    #[test]
    fn pick_phase_folgt_dem_python_muster() {
        // Positionen 6..18 exakt wie DEFAULT_SEQUENCE im Original.
        let erwartet = [
            (Pick, One),
            (Pick, Two),
            (Pick, Two),
            (Pick, One),
            (Pick, One),
            (Pick, Two),
            (Pick, Two),
            (Pick, One),
            (Pick, One),
            (Pick, Two),
            (Pick, Two),
            (Pick, One),
        ];
        for (offset, (at, ts)) in erwartet.into_iter().enumerate() {
            let s = DEFAULT_SEQUENCE[6 + offset];
            assert_eq!(s.action_type, at);
            assert_eq!(s.team_slot, ts);
        }
    }

    #[test]
    fn step_at_ist_an_den_raendern_sicher() {
        assert_eq!(step_at(0), Some(step(Ban, One)));
        assert_eq!(step_at(17), Some(step(Pick, One)));
        assert_eq!(step_at(18), None);
        assert_eq!(step_at(999), None);
    }

    #[test]
    fn is_complete_greift_genau_am_ende() {
        assert!(!is_complete(0));
        assert!(!is_complete(17));
        assert!(is_complete(18));
        assert!(is_complete(19));
    }

    #[test]
    fn enums_serialisieren_als_python_strings() {
        assert_eq!(ActionType::Ban.as_str(), "ban");
        assert_eq!(ActionType::Pick.as_str(), "pick");
        assert_eq!(TeamSlot::One.as_i64(), 1);
        assert_eq!(TeamSlot::Two.as_i64(), 2);
        assert_eq!(serde_json::to_string(&ActionType::Pick).unwrap(), "\"pick\"");
    }
}
