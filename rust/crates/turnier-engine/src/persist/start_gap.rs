use chrono::{DateTime, Duration, Utc};
use sqlx::PgConnection;
use turnier_core::{TournamentMode, TournamentStatus};

use crate::error::{TournamentError, TournamentResult};

pub const MIN_START_GAP_HOURS: i64 = 4;

pub const TOURNAMENT_SCHEDULE_ADVISORY_LOCK_KEY: i64 = i64::from_be_bytes(*b"trnactv1");

#[derive(Debug, Clone, sqlx::FromRow)]
struct StartRow {
    id: i64,
    name: String,
    status: TournamentStatus,
    is_test: bool,
    tournament_mode: TournamentMode,
    tournament_mode_auto: bool,
    group_phase_start: Option<DateTime<Utc>>,
    bracket_start: Option<DateTime<Utc>>,
    registration_end: Option<DateTime<Utc>>,
}

impl StartRow {
    fn starts(&self) -> Vec<DateTime<Utc>> {
        candidate_starts(
            self.tournament_mode,
            mode_undecided(self.tournament_mode, self.tournament_mode_auto, self.status),
            self.group_phase_start,
            self.bracket_start,
            self.registration_end,
        )
    }

    fn takes_part(&self) -> bool {
        !self.is_test && TournamentStatus::PLANNED.contains(&self.status)
    }
}

pub fn effective_start(
    tournament_mode: TournamentMode,
    group_phase_start: Option<DateTime<Utc>>,
    bracket_start: Option<DateTime<Utc>>,
    registration_end: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    let planned = if tournament_mode == TournamentMode::BracketOnly {
        bracket_start
    } else {
        group_phase_start.or(bracket_start)
    };
    planned.or(registration_end)
}

pub fn mode_undecided(
    tournament_mode: TournamentMode,
    tournament_mode_auto: bool,
    status: TournamentStatus,
) -> bool {
    tournament_mode_auto
        && tournament_mode != TournamentMode::League
        && matches!(
            status,
            TournamentStatus::Draft | TournamentStatus::Registration | TournamentStatus::Checkin
        )
}

pub fn candidate_starts(
    tournament_mode: TournamentMode,
    undecided: bool,
    group_phase_start: Option<DateTime<Utc>>,
    bracket_start: Option<DateTime<Utc>>,
    registration_end: Option<DateTime<Utc>>,
) -> Vec<DateTime<Utc>> {
    let modes = if undecided {
        vec![TournamentMode::GroupStage, TournamentMode::BracketOnly]
    } else {
        vec![tournament_mode]
    };
    let mut starts: Vec<DateTime<Utc>> = modes
        .into_iter()
        .filter_map(|mode| {
            effective_start(mode, group_phase_start, bracket_start, registration_end)
        })
        .collect();
    starts.sort();
    starts.dedup();
    starts
}

pub fn starts_too_close(left: DateTime<Utc>, right: DateTime<Utc>) -> bool {
    (left - right).abs() < Duration::hours(MIN_START_GAP_HOURS)
}

pub fn start_gap_message(other_id: i64, other_name: &str) -> String {
    format!(
        "Zu nah an Turnier #{other_id} {other_name}: mindestens {MIN_START_GAP_HOURS} Stunden Abstand zwischen Turnierstarts."
    )
}

pub async fn acquire_tournament_schedule_lock(conn: &mut PgConnection) -> TournamentResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(TOURNAMENT_SCHEDULE_ADVISORY_LOCK_KEY)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn ensure_start_gap(conn: &mut PgConnection, tournament_id: i64) -> TournamentResult<()> {
    acquire_tournament_schedule_lock(&mut *conn).await?;
    let Some(target) = load_start_row(&mut *conn, tournament_id).await? else {
        return Err(TournamentError::StatusConflict);
    };
    if !target.takes_part() {
        return Ok(());
    }
    let starts = target.starts();
    if starts.is_empty() {
        return Ok(());
    }
    let others = load_other_planned_rows(&mut *conn, tournament_id).await?;
    match first_conflict(&starts, &others) {
        Some(other) => Err(TournamentError::StartGapConflict(start_gap_message(
            other.id,
            &other.name,
        ))),
        None => Ok(()),
    }
}

fn first_conflict<'a>(starts: &[DateTime<Utc>], others: &'a [StartRow]) -> Option<&'a StartRow> {
    others
        .iter()
        .filter(|other| other.takes_part())
        .find(|other| {
            other
                .starts()
                .iter()
                .any(|theirs| starts.iter().any(|ours| starts_too_close(*ours, *theirs)))
        })
}

async fn load_start_row(
    conn: &mut PgConnection,
    tournament_id: i64,
) -> TournamentResult<Option<StartRow>> {
    let row = sqlx::query_as(
        "SELECT id, name, status, is_test, tournament_mode, tournament_mode_auto, \
         group_phase_start, bracket_start, registration_end \
         FROM turnier.tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

async fn load_other_planned_rows(
    conn: &mut PgConnection,
    tournament_id: i64,
) -> TournamentResult<Vec<StartRow>> {
    let rows = sqlx::query_as(
        "SELECT id, name, status, is_test, tournament_mode, tournament_mode_auto, \
         group_phase_start, bracket_start, registration_end \
         FROM turnier.tournaments \
         WHERE id <> $1 AND is_test = false AND status = ANY($2) \
         ORDER BY id",
    )
    .bind(tournament_id)
    .bind(TournamentStatus::PLANNED.as_slice())
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32, minute: u32) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("2026-11-14T{hour:02}:{minute:02}:00Z"))
            .map(|value| value.with_timezone(&Utc))
            .unwrap_or_default()
    }

    fn row(id: i64, status: TournamentStatus, is_test: bool, start: DateTime<Utc>) -> StartRow {
        StartRow {
            id,
            name: format!("Turnier {id}"),
            status,
            is_test,
            tournament_mode: TournamentMode::GroupStage,
            tournament_mode_auto: false,
            group_phase_start: Some(start),
            bracket_start: None,
            registration_end: None,
        }
    }

    #[test]
    fn vier_stunden_abstand_ist_erlaubt() {
        assert!(!starts_too_close(at(12, 0), at(16, 0)));
        assert!(!starts_too_close(at(16, 0), at(12, 0)));
        assert!(starts_too_close(at(12, 0), at(15, 59)));
        assert!(starts_too_close(at(15, 59), at(12, 0)));
    }

    #[test]
    fn konflikt_nur_mit_geplanten_echten_turnieren() {
        let others = vec![
            row(1, TournamentStatus::Registration, true, at(13, 0)),
            row(2, TournamentStatus::Completed, false, at(13, 0)),
            row(3, TournamentStatus::Draft, false, at(20, 0)),
            row(4, TournamentStatus::Checkin, false, at(14, 0)),
        ];
        assert_eq!(first_conflict(&[at(12, 0)], &others).map(|r| r.id), Some(4));
        assert_eq!(first_conflict(&[at(8, 0)], &others).map(|r| r.id), None);
    }

    #[test]
    fn meldung_nennt_das_andere_turnier() {
        assert_eq!(
            start_gap_message(16, "Deadlock PUG Turnier #1"),
            "Zu nah an Turnier #16 Deadlock PUG Turnier #1: mindestens 4 Stunden Abstand zwischen Turnierstarts."
        );
    }

    #[test]
    fn bracket_only_startet_mit_dem_bracket() {
        assert_eq!(
            effective_start(
                TournamentMode::BracketOnly,
                Some(at(10, 0)),
                Some(at(12, 0)),
                Some(at(9, 0))
            ),
            Some(at(12, 0))
        );
        assert_eq!(
            effective_start(
                TournamentMode::GroupStage,
                None,
                Some(at(12, 0)),
                Some(at(9, 0))
            ),
            Some(at(12, 0))
        );
    }

    #[test]
    fn anmeldeschluss_gilt_als_start_ohne_phasenzeiten() {
        assert_eq!(
            effective_start(TournamentMode::BracketOnly, None, None, Some(at(18, 0))),
            Some(at(18, 0))
        );
        assert_eq!(
            effective_start(TournamentMode::GroupStage, None, None, Some(at(18, 0))),
            Some(at(18, 0))
        );
    }

    #[test]
    fn undecided_auto_mode_counts_group_and_bracket_start() {
        let starts = candidate_starts(
            TournamentMode::BracketOnly,
            true,
            Some(at(12, 0)),
            Some(at(18, 0)),
            None,
        );
        assert_eq!(starts, vec![at(12, 0), at(18, 0)]);
        let decided = candidate_starts(
            TournamentMode::BracketOnly,
            false,
            Some(at(12, 0)),
            Some(at(18, 0)),
            None,
        );
        assert_eq!(decided, vec![at(18, 0)]);
    }

    #[test]
    fn auto_mode_is_undecided_only_before_finalize() {
        let bracket = TournamentMode::BracketOnly;
        assert!(mode_undecided(
            bracket,
            true,
            TournamentStatus::Registration
        ));
        assert!(mode_undecided(bracket, true, TournamentStatus::Checkin));
        assert!(!mode_undecided(bracket, true, TournamentStatus::Bracket));
        assert!(!mode_undecided(bracket, false, TournamentStatus::Checkin));
        assert!(!mode_undecided(
            TournamentMode::League,
            true,
            TournamentStatus::Draft
        ));
    }

    #[test]
    fn undecided_neighbour_conflicts_with_its_group_start() {
        let mut other = row(7, TournamentStatus::Registration, false, at(12, 0));
        other.tournament_mode = TournamentMode::BracketOnly;
        other.tournament_mode_auto = true;
        other.bracket_start = Some(at(20, 0));
        let others = vec![other];
        assert_eq!(first_conflict(&[at(13, 0)], &others).map(|r| r.id), Some(7));
        assert_eq!(first_conflict(&[at(16, 0)], &others).map(|r| r.id), None);
    }
}
