pub const BENCH_SLOTS_MAX: i64 = 5;

pub fn check_bench_slots(v: i64) -> Result<(), String> {
    if (0..=BENCH_SLOTS_MAX).contains(&v) {
        Ok(())
    } else {
        Err(format!(
            "Ersatzplätze pro Team müssen zwischen 0 und {BENCH_SLOTS_MAX} liegen"
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Lineup {
    pub starters: i64,
    pub bench: i64,
}

impl Lineup {
    pub fn is_complete(self, team_size: i64) -> bool {
        self.starters >= team_size
    }

    pub fn starter_free(self, team_size: i64) -> bool {
        self.starters < team_size
    }

    pub fn bench_free(self, bench_slots: i64) -> bool {
        self.bench < bench_slots
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatRequest {
    Starter,
    Bench,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatError {
    TeamFull,
    StartersFullBenchFree,
    StartersFull,
    BenchOff,
    BenchFull,
    CaptainOnBench,
}

impl SeatError {
    pub fn message(self) -> &'static str {
        match self {
            Self::TeamFull => "Team ist bereits voll",
            Self::StartersFullBenchFree => {
                "Alle Stammplätze sind belegt. Tritt als Ersatzspieler bei."
            }
            Self::StartersFull => "Alle Stammplätze sind belegt",
            Self::BenchOff => "In diesem Turnier gibt es keine Ersatzplätze",
            Self::BenchFull => "Alle Ersatzplätze sind belegt",
            Self::CaptainOnBench => "Der Captain bleibt Stammspieler",
        }
    }
}

pub fn place(
    lineup: Lineup,
    team_size: i64,
    bench_slots: i64,
    request: SeatRequest,
) -> Result<bool, SeatError> {
    match request {
        SeatRequest::Bench => bench_seat(lineup, bench_slots).map(|()| true),
        SeatRequest::Starter if lineup.starter_free(team_size) => Ok(false),
        SeatRequest::Starter if lineup.bench_free(bench_slots) => {
            Err(SeatError::StartersFullBenchFree)
        }
        SeatRequest::Starter => Err(SeatError::TeamFull),
        SeatRequest::Auto if lineup.starter_free(team_size) => Ok(false),
        SeatRequest::Auto if lineup.bench_free(bench_slots) => Ok(true),
        SeatRequest::Auto => Err(SeatError::TeamFull),
    }
}

pub fn move_seat(
    lineup: Lineup,
    team_size: i64,
    bench_slots: i64,
    is_captain: bool,
    to_bench: bool,
) -> Result<(), SeatError> {
    if to_bench {
        if is_captain {
            return Err(SeatError::CaptainOnBench);
        }
        return bench_seat(lineup, bench_slots);
    }
    if lineup.starter_free(team_size) {
        Ok(())
    } else {
        Err(SeatError::StartersFull)
    }
}

fn bench_seat(lineup: Lineup, bench_slots: i64) -> Result<(), SeatError> {
    if bench_slots <= 0 {
        return Err(SeatError::BenchOff);
    }
    if !lineup.bench_free(bench_slots) {
        return Err(SeatError::BenchFull);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lineup(starters: i64, bench: i64) -> Lineup {
        Lineup { starters, bench }
    }

    #[test]
    fn bench_never_counts_towards_completeness() {
        for team_size in [6, 8] {
            assert!(!lineup(team_size - 1, 5).is_complete(team_size));
            assert!(lineup(team_size, 0).is_complete(team_size));
            assert!(lineup(team_size, 2).is_complete(team_size));
        }
    }

    #[test]
    fn starters_are_counted_against_the_stored_team_size() {
        assert!(lineup(6, 0).is_complete(6));
        assert!(!lineup(6, 0).is_complete(8));
        assert!(lineup(6, 0).starter_free(8));
        assert!(!lineup(8, 0).starter_free(8));
    }

    #[test]
    fn bench_off_behaves_like_before() {
        assert_eq!(place(lineup(5, 0), 6, 0, SeatRequest::Auto), Ok(false));
        assert_eq!(
            place(lineup(6, 0), 6, 0, SeatRequest::Auto),
            Err(SeatError::TeamFull)
        );
        assert_eq!(
            place(lineup(6, 0), 6, 0, SeatRequest::Starter),
            Err(SeatError::TeamFull)
        );
        assert_eq!(
            place(lineup(0, 0), 6, 0, SeatRequest::Bench),
            Err(SeatError::BenchOff)
        );
    }

    #[test]
    fn full_starters_still_accept_bench_players() {
        assert_eq!(place(lineup(8, 0), 8, 1, SeatRequest::Bench), Ok(true));
        assert_eq!(place(lineup(8, 0), 8, 1, SeatRequest::Auto), Ok(true));
        assert_eq!(
            place(lineup(8, 0), 8, 1, SeatRequest::Starter),
            Err(SeatError::StartersFullBenchFree)
        );
    }

    #[test]
    fn bench_limit_reached_rejects_bench_requests() {
        assert_eq!(
            place(lineup(2, 2), 6, 2, SeatRequest::Bench),
            Err(SeatError::BenchFull)
        );
        assert_eq!(
            place(lineup(6, 2), 6, 2, SeatRequest::Auto),
            Err(SeatError::TeamFull)
        );
        assert_eq!(place(lineup(2, 2), 6, 2, SeatRequest::Auto), Ok(false));
    }

    #[test]
    fn bench_wish_wins_over_free_starter_seats() {
        assert_eq!(place(lineup(1, 0), 6, 1, SeatRequest::Bench), Ok(true));
    }

    #[test]
    fn captain_can_move_the_last_other_starter_but_not_himself() {
        assert_eq!(move_seat(lineup(2, 0), 6, 1, false, true), Ok(()));
        assert_eq!(
            move_seat(lineup(1, 1), 6, 1, true, true),
            Err(SeatError::CaptainOnBench)
        );
    }

    #[test]
    fn moving_to_starters_needs_a_free_starter_seat() {
        assert_eq!(move_seat(lineup(5, 1), 6, 1, false, false), Ok(()));
        assert_eq!(
            move_seat(lineup(6, 1), 6, 1, false, false),
            Err(SeatError::StartersFull)
        );
        assert_eq!(
            move_seat(lineup(6, 0), 6, 0, false, true),
            Err(SeatError::BenchOff)
        );
    }

    #[test]
    fn bench_slots_range() {
        assert!(check_bench_slots(0).is_ok());
        assert!(check_bench_slots(BENCH_SLOTS_MAX).is_ok());
        assert!(check_bench_slots(-1).is_err());
        assert!(check_bench_slots(BENCH_SLOTS_MAX + 1).is_err());
    }
}
