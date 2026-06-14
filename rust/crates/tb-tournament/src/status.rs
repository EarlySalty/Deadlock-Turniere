//! Turnier-Modus-Bestimmung und Status-Übergangstabelle (reine Logik).

use tb_core::{TournamentMode, TournamentStatus};

/// Schwelle für den automatischen Group-Stage-Modus (`>= 16` Teams).
pub const AUTO_GROUP_STAGE_THRESHOLD: usize = 16;

/// Bestimmt den Turnier-Modus aus der Team-Anzahl (Override hat Vorrang).
///
/// Portiert `determine_tournament_mode`: `>= 16` Teams → Group-Stage, sonst
/// Bracket-Only; `force_mode` überschreibt die Heuristik.
pub fn determine_tournament_mode(
    team_count: usize,
    force_mode: Option<TournamentMode>,
) -> TournamentMode {
    if let Some(mode) = force_mode {
        return mode;
    }
    if team_count >= AUTO_GROUP_STAGE_THRESHOLD {
        TournamentMode::GroupStage
    } else {
        TournamentMode::BracketOnly
    }
}

/// Die erlaubten Folge-Status zu einem gegebenen Status.
///
/// Portiert `VALID_STATUS_TRANSITIONS`. `Draft` ist hier nicht als Schlüssel
/// gelistet zu lassen wäre falsch — das Original kennt jeden Schlüssel; wir
/// liefern für `Archived` eine leere Liste (kein Folgestatus).
pub fn valid_next_statuses(status: TournamentStatus) -> &'static [TournamentStatus] {
    use TournamentStatus::*;
    match status {
        Draft => &[Registration],
        Registration => &[Checkin],
        Checkin => &[GroupPhase],
        GroupPhase => &[Bracket],
        Bracket => &[Completed],
        Completed => &[Archived],
        Archived => &[],
    }
}

/// Ist der Übergang `from -> to` erlaubt?
pub fn is_valid_transition(from: TournamentStatus, to: TournamentStatus) -> bool {
    valid_next_statuses(from).contains(&to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_from_count_and_override() {
        assert_eq!(determine_tournament_mode(15, None), TournamentMode::BracketOnly);
        assert_eq!(determine_tournament_mode(16, None), TournamentMode::GroupStage);
        assert_eq!(
            determine_tournament_mode(4, Some(TournamentMode::GroupStage)),
            TournamentMode::GroupStage
        );
        assert_eq!(
            determine_tournament_mode(64, Some(TournamentMode::BracketOnly)),
            TournamentMode::BracketOnly
        );
    }

    #[test]
    fn transition_chain_matches_python() {
        use TournamentStatus::*;
        assert!(is_valid_transition(Draft, Registration));
        assert!(is_valid_transition(Registration, Checkin));
        assert!(is_valid_transition(Checkin, GroupPhase));
        assert!(is_valid_transition(GroupPhase, Bracket));
        assert!(is_valid_transition(Bracket, Completed));
        assert!(is_valid_transition(Completed, Archived));
        assert!(!is_valid_transition(Draft, Checkin));
        assert!(valid_next_statuses(Archived).is_empty());
    }
}
