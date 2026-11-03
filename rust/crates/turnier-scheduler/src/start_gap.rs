use sqlx::PgConnection;

use turnier_engine::TournamentError;

use crate::error::{SchedulerError, SchedulerResult};

pub use turnier_engine::{
    effective_start, start_gap_message, starts_too_close, MIN_START_GAP_HOURS,
    TOURNAMENT_SCHEDULE_ADVISORY_LOCK_KEY,
};

pub async fn acquire_tournament_schedule_lock(conn: &mut PgConnection) -> SchedulerResult<()> {
    turnier_engine::acquire_tournament_schedule_lock(conn).await?;
    Ok(())
}

pub async fn ensure_start_gap(conn: &mut PgConnection, tournament_id: i64) -> SchedulerResult<()> {
    turnier_engine::ensure_start_gap(conn, tournament_id)
        .await
        .map_err(|err| match err {
            TournamentError::StatusConflict => SchedulerError::StatusConflict,
            other => other.into(),
        })
}
