//! Reine Zustandsmaschine der Pick/Ban-Sequenz — DB-frei, voll unit-testbar.
//!
//! Hier liegt das Wissen über die feste Reihenfolge (6 Bans + 12 Picks, je 6 pro
//! Team) und über das Fortschreiten anhand eines `current_action_index`. Die
//! Persistenz-Schicht ([`crate::repo`]) konsultiert diese Logik, hält aber selbst
//! keine Sequenz-Konstanten.
//!
//! Portiert `DEFAULT_SEQUENCE` aus `backend/draft/engine.py` 1:1.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Aktionstyp einer Draft-Position. TEXT-Spalte `draft_actions.action_type`
/// (`'ban'` | `'pick'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
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

impl Serialize for TeamSlot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for TeamSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match i64::deserialize(deserializer)? {
            1 => Ok(TeamSlot::One),
            2 => Ok(TeamSlot::Two),
            value => Err(serde::de::Error::custom(format!(
                "ungueltiger Team-Slot: {value}"
            ))),
        }
    }
}

/// Eine Position der Draft-Sequenz: was (Ban/Pick) macht welches Team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceStep {
    #[serde(rename = "action")]
    pub action_type: ActionType,
    #[serde(rename = "team")]
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

/// Wettbewerbs-Preset mit je zwei Bans pro Team und dem Standard-Pickmuster.
pub const COMPETITIVE_2BAN: [SequenceStep; 16] = [
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

pub const COMPETITIVE_2BAN_MID: [SequenceStep; 16] = [
    step(Ban, One),
    step(Ban, Two),
    step(Pick, One),
    step(Pick, Two),
    step(Pick, Two),
    step(Pick, One),
    step(Pick, One),
    step(Pick, Two),
    step(Ban, Two),
    step(Ban, One),
    step(Pick, Two),
    step(Pick, One),
    step(Pick, One),
    step(Pick, Two),
    step(Pick, Two),
    step(Pick, One),
];

/// Wettbewerbs-Preset mit je einem Ban pro Team und dem Standard-Pickmuster.
pub const COMPETITIVE_1BAN: [SequenceStep; 14] = [
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

/// Schnelles Preset ohne Bans und mit dem Standard-Pickmuster.
pub const QUICK_NO_BAN: [SequenceStep; 12] = [
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

const SNAKE_PICKS: [TeamSlot; 12] = [One, Two, Two, One, One, Two, Two, One, One, Two, Two, One];

pub fn sequence_for_bans(bans_per_team: i32) -> Vec<SequenceStep> {
    let bans = bans_per_team.clamp(0, 6) as usize;
    let mut sequence = Vec::with_capacity(bans * 2 + SNAKE_PICKS.len());
    for index in 0..(bans * 2) {
        let team_slot = if index % 2 == 0 { One } else { Two };
        sequence.push(step(Ban, team_slot));
    }
    for team_slot in SNAKE_PICKS {
        sequence.push(step(Pick, team_slot));
    }
    sequence
}

pub fn preset(name: &str) -> Option<&'static [SequenceStep]> {
    match name {
        "competitive_2ban" => Some(&COMPETITIVE_2BAN),
        "competitive_2ban_mid" => Some(&COMPETITIVE_2BAN_MID),
        "competitive_1ban" => Some(&COMPETITIVE_1BAN),
        "quick_no_ban" => Some(&QUICK_NO_BAN),
        _ => None,
    }
}

/// Liefert die Aktion an `index` oder `None`, wenn der Index außerhalb der
/// Sequenz dieser Session liegt.
pub fn step_at(sequence: &[SequenceStep], index: usize) -> Option<SequenceStep> {
    sequence.get(index).copied()
}

/// `true`, wenn ein `current_action_index` das Sequenz-Ende erreicht/überschritten
/// hat — die Session ist dann abgeschlossen. Spiegelt `next_idx >= len(...)`.
pub fn is_complete(sequence: &[SequenceStep], index: usize) -> bool {
    index >= sequence.len()
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
        assert_eq!(step_at(&DEFAULT_SEQUENCE, 0), Some(step(Ban, One)));
        assert_eq!(step_at(&DEFAULT_SEQUENCE, 17), Some(step(Pick, One)));
        assert_eq!(step_at(&DEFAULT_SEQUENCE, 18), None);
        assert_eq!(step_at(&DEFAULT_SEQUENCE, 999), None);
    }

    #[test]
    fn is_complete_greift_genau_am_ende() {
        assert!(!is_complete(&DEFAULT_SEQUENCE, 0));
        assert!(!is_complete(&DEFAULT_SEQUENCE, 17));
        assert!(is_complete(&DEFAULT_SEQUENCE, 18));
        assert!(is_complete(&DEFAULT_SEQUENCE, 19));
    }

    #[test]
    fn enums_serialisieren_als_python_strings() {
        assert_eq!(ActionType::Ban.as_str(), "ban");
        assert_eq!(ActionType::Pick.as_str(), "pick");
        assert_eq!(TeamSlot::One.as_i64(), 1);
        assert_eq!(TeamSlot::Two.as_i64(), 2);
        assert_eq!(
            serde_json::to_string(&ActionType::Pick).unwrap(),
            "\"pick\""
        );
    }

    #[test]
    fn presets_haben_die_erwartete_verteilung() {
        for (name, bans, picks) in [
            ("competitive_2ban", 4, 12),
            ("competitive_2ban_mid", 4, 12),
            ("competitive_1ban", 2, 12),
            ("quick_no_ban", 0, 12),
        ] {
            let sequence = preset(name).expect("bekanntes Preset");
            assert_eq!(
                sequence
                    .iter()
                    .filter(|step| step.action_type == ActionType::Ban)
                    .count(),
                bans
            );
            assert_eq!(
                sequence
                    .iter()
                    .filter(|step| step.action_type == ActionType::Pick)
                    .count(),
                picks
            );
        }
        assert!(preset("unbekannt").is_none());
    }

    #[test]
    fn competitive_2ban_hat_zwei_bans_je_team() {
        let sequence = preset("competitive_2ban").expect("bekanntes Preset");
        for team in [One, Two] {
            assert_eq!(
                sequence
                    .iter()
                    .filter(|step| step.action_type == Ban && step.team_slot == team)
                    .count(),
                2
            );
        }
    }

    #[test]
    fn competitive_2ban_mid_hat_die_erwartete_reihenfolge() {
        let sequence = preset("competitive_2ban_mid").expect("bekanntes Preset");
        let erwartet = [
            (Ban, One),
            (Ban, Two),
            (Pick, One),
            (Pick, Two),
            (Pick, Two),
            (Pick, One),
            (Pick, One),
            (Pick, Two),
            (Ban, Two),
            (Ban, One),
            (Pick, Two),
            (Pick, One),
            (Pick, One),
            (Pick, Two),
            (Pick, Two),
            (Pick, One),
        ];
        assert_eq!(sequence.len(), erwartet.len());
        for (index, (at, ts)) in erwartet.into_iter().enumerate() {
            assert_eq!(sequence[index], step(at, ts), "Schritt {index}");
        }

        for index in [0usize, 1, 8, 9] {
            assert_eq!(sequence[index].action_type, Ban, "Ban an {index}");
        }
        for index in (2..8).chain(10..16) {
            assert_eq!(sequence[index].action_type, Pick, "Pick an {index}");
        }

        for offset in 0..6 {
            let erste = sequence[2 + offset].team_slot;
            let zweite = sequence[10 + offset].team_slot;
            let gespiegelt = match erste {
                One => Two,
                Two => One,
            };
            assert_eq!(zweite, gespiegelt, "Spiegel an Offset {offset}");
        }
    }

    #[test]
    fn sequence_step_hat_den_jsonb_wire_roundtrip() {
        let value = serde_json::to_value(step(Ban, Two)).expect("serialisierbar");
        assert_eq!(value, serde_json::json!({"action": "ban", "team": 2}));
        let decoded: SequenceStep = serde_json::from_value(value).expect("deserialisierbar");
        assert_eq!(decoded, step(Ban, Two));
    }

    #[test]
    fn abschluss_richtet_sich_nach_der_session_sequenz() {
        let sequence = preset("quick_no_ban").expect("bekanntes Preset");
        assert!(!is_complete(sequence, 11));
        assert!(is_complete(sequence, 12));
        assert_eq!(step_at(sequence, 11), Some(step(Pick, One)));
        assert_eq!(step_at(sequence, 12), None);
    }

    #[test]
    fn generator_liefert_fuer_n3_sechs_bans_und_zwoelf_picks() {
        let sequence = sequence_for_bans(3);
        assert_eq!(sequence.len(), 18);
        assert_eq!(sequence.iter().filter(|s| s.action_type == Ban).count(), 6);
        assert_eq!(
            sequence.iter().filter(|s| s.action_type == Pick).count(),
            12
        );
        for team in [One, Two] {
            assert_eq!(
                sequence
                    .iter()
                    .filter(|s| s.action_type == Ban && s.team_slot == team)
                    .count(),
                3
            );
            assert_eq!(
                sequence
                    .iter()
                    .filter(|s| s.action_type == Pick && s.team_slot == team)
                    .count(),
                6
            );
        }
    }

    #[test]
    fn generator_bans_alternieren_vorne_und_picks_folgen_dem_snake() {
        for bans_per_team in 0..=6_i32 {
            let sequence = sequence_for_bans(bans_per_team);
            assert_eq!(sequence.len(), (bans_per_team as usize) * 2 + 12);
            for (index, s) in sequence
                .iter()
                .take((bans_per_team as usize) * 2)
                .enumerate()
            {
                assert_eq!(s.action_type, Ban, "Ban an {index} bei n={bans_per_team}");
                let erwartet = if index % 2 == 0 { One } else { Two };
                assert_eq!(
                    s.team_slot, erwartet,
                    "Ban-Team an {index} bei n={bans_per_team}"
                );
            }
            for (offset, s) in sequence
                .iter()
                .skip((bans_per_team as usize) * 2)
                .enumerate()
            {
                assert_eq!(
                    s.action_type, Pick,
                    "Pick an Offset {offset} bei n={bans_per_team}"
                );
                assert_eq!(s.team_slot, SNAKE_PICKS[offset]);
            }
        }
    }

    #[test]
    fn generator_ohne_bans_entspricht_dem_quick_preset() {
        assert_eq!(sequence_for_bans(0), QUICK_NO_BAN.to_vec());
    }

    #[test]
    fn generator_mit_drei_bans_entspricht_der_default_sequenz() {
        assert_eq!(sequence_for_bans(3), DEFAULT_SEQUENCE.to_vec());
    }
}
