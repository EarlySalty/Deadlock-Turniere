use turnier_core::{TournamentMode, TournamentStatus};

pub const AUTO_GROUP_STAGE_THRESHOLD: usize = 16;

pub const PROVISIONAL_AUTO_MODE: TournamentMode = TournamentMode::BracketOnly;

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

pub fn initial_tournament_mode(force_mode: Option<TournamentMode>) -> (TournamentMode, bool) {
    match force_mode {
        Some(mode) => (mode, false),
        None => (PROVISIONAL_AUTO_MODE, true),
    }
}

pub fn resolve_tournament_mode(
    stored_mode: TournamentMode,
    mode_auto: bool,
    final_team_count: usize,
) -> TournamentMode {
    if !mode_auto || stored_mode == TournamentMode::League {
        return stored_mode;
    }
    determine_tournament_mode(final_team_count, None)
}

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

pub fn is_valid_transition(from: TournamentStatus, to: TournamentStatus) -> bool {
    valid_next_statuses(from).contains(&to)
}

pub fn valid_next_statuses_for_mode(
    status: TournamentStatus,
    mode: TournamentMode,
) -> &'static [TournamentStatus] {
    match (status, mode) {
        (TournamentStatus::Checkin, TournamentMode::BracketOnly) => &[TournamentStatus::Bracket],
        (TournamentStatus::GroupPhase, TournamentMode::League) => &[TournamentStatus::Completed],
        (TournamentStatus::Bracket, TournamentMode::League) => &[],
        _ => valid_next_statuses(status),
    }
}

pub fn is_valid_transition_for_mode(
    from: TournamentStatus,
    to: TournamentStatus,
    mode: TournamentMode,
) -> bool {
    valid_next_statuses_for_mode(from, mode).contains(&to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_from_count_and_override() {
        assert_eq!(
            determine_tournament_mode(15, None),
            TournamentMode::BracketOnly
        );
        assert_eq!(
            determine_tournament_mode(16, None),
            TournamentMode::GroupStage
        );
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
    fn auto_mode_counts_final_teams_not_players() {
        for (teams, expected) in [
            (1, TournamentMode::BracketOnly),
            (5, TournamentMode::BracketOnly),
            (6, TournamentMode::BracketOnly),
            (8, TournamentMode::BracketOnly),
            (15, TournamentMode::BracketOnly),
            (16, TournamentMode::GroupStage),
            (17, TournamentMode::GroupStage),
        ] {
            assert_eq!(
                resolve_tournament_mode(PROVISIONAL_AUTO_MODE, true, teams),
                expected,
                "{teams} teams"
            );
        }
    }

    #[test]
    fn explicit_and_league_modes_are_never_resolved() {
        assert_eq!(
            resolve_tournament_mode(TournamentMode::BracketOnly, false, 16),
            TournamentMode::BracketOnly
        );
        assert_eq!(
            resolve_tournament_mode(TournamentMode::GroupStage, false, 4),
            TournamentMode::GroupStage
        );
        assert_eq!(
            resolve_tournament_mode(TournamentMode::League, true, 32),
            TournamentMode::League
        );
    }

    #[test]
    fn create_without_override_is_provisional_auto() {
        assert_eq!(
            initial_tournament_mode(None),
            (TournamentMode::BracketOnly, true)
        );
        assert_eq!(
            initial_tournament_mode(Some(TournamentMode::GroupStage)),
            (TournamentMode::GroupStage, false)
        );
        assert_eq!(
            initial_tournament_mode(Some(TournamentMode::BracketOnly)),
            (TournamentMode::BracketOnly, false)
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

    #[test]
    fn checkin_abschluss_passt_zum_modus() {
        use TournamentStatus::*;
        assert!(is_valid_transition_for_mode(
            Checkin,
            Bracket,
            TournamentMode::BracketOnly
        ));
        assert!(!is_valid_transition_for_mode(
            Checkin,
            GroupPhase,
            TournamentMode::BracketOnly
        ));
        assert!(is_valid_transition_for_mode(
            Checkin,
            GroupPhase,
            TournamentMode::GroupStage
        ));
        assert!(!is_valid_transition_for_mode(
            Checkin,
            Bracket,
            TournamentMode::GroupStage
        ));
        assert!(!is_valid_transition(Checkin, Bracket));
    }

    #[test]
    fn league_runs_from_checkin_over_group_phase_to_completed() {
        use TournamentStatus::*;
        let league = TournamentMode::League;
        assert!(is_valid_transition_for_mode(Checkin, GroupPhase, league));
        assert!(is_valid_transition_for_mode(GroupPhase, Completed, league));
        assert!(!is_valid_transition_for_mode(Checkin, Bracket, league));
        assert!(!is_valid_transition_for_mode(GroupPhase, Bracket, league));
        assert!(is_valid_transition_for_mode(Completed, Archived, league));
    }

    #[test]
    fn group_stage_still_needs_the_bracket() {
        use TournamentStatus::*;
        let mode = TournamentMode::GroupStage;
        assert!(is_valid_transition_for_mode(GroupPhase, Bracket, mode));
        assert!(!is_valid_transition_for_mode(GroupPhase, Completed, mode));
    }
}
