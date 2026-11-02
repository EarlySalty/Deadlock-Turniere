use chrono::{DateTime, Duration, Utc};
use sqlx::{Postgres, Transaction};
use turnier_core::now_utc;

use crate::error::{TournamentError, TournamentResult};
use crate::league::{
    is_valid_slot_minutes, league_rounds, round_offset_minutes, LEAGUE_GROUP_NAME,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaguePlanSummary {
    pub group_id: i64,
    pub matches: i64,
}

pub async fn generate_league_plan_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    slot_minutes: i64,
) -> TournamentResult<LeaguePlanSummary> {
    ensure_valid_slot(slot_minutes)?;
    let team_ids = load_team_ids(tx, tournament_id).await?;
    if team_ids.len() < 2 {
        return Err(TournamentError::validation("Mindestens 2 Teams benötigt"));
    }
    let start = plan_start(tx, tournament_id).await?;
    delete_existing_plan(tx, tournament_id).await?;
    let group_id = insert_table_group(tx, tournament_id, &team_ids).await?;

    let pairings = league_rounds(&team_ids);
    for pairing in &pairings {
        sqlx::query(
            "INSERT INTO turnier.group_matches \
                 (group_id, team1_id, team2_id, status, scheduled_at) \
             VALUES ($1, $2, $3, 'pending', $4)",
        )
        .bind(group_id)
        .bind(pairing.team1_id)
        .bind(pairing.team2_id)
        .bind(round_time(start, pairing.round, slot_minutes))
        .execute(&mut **tx)
        .await?;
    }
    Ok(LeaguePlanSummary {
        group_id,
        matches: pairings.len() as i64,
    })
}

pub async fn reschedule_league_plan_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    start: DateTime<Utc>,
    slot_minutes: i64,
) -> TournamentResult<i64> {
    ensure_valid_slot(slot_minutes)?;
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT gm.id, DENSE_RANK() OVER (ORDER BY gm.scheduled_at) \
         FROM turnier.group_matches gm \
         JOIN turnier.groups g ON g.id = gm.group_id \
         WHERE g.tournament_id = $1",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;
    for (match_id, round) in &rows {
        sqlx::query("UPDATE turnier.group_matches SET scheduled_at = $1 WHERE id = $2")
            .bind(round_time(start, *round, slot_minutes))
            .bind(match_id)
            .execute(&mut **tx)
            .await?;
    }
    write_group_phase_start(tx, tournament_id, start).await?;
    Ok(rows.len() as i64)
}

pub async fn league_slot_minutes_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
) -> TournamentResult<Option<i64>> {
    let row: (Option<f64>,) = sqlx::query_as(
        "WITH times AS ( \
             SELECT DISTINCT gm.scheduled_at AS at FROM turnier.group_matches gm \
             JOIN turnier.groups g ON g.id = gm.group_id \
             WHERE g.tournament_id = $1 AND gm.scheduled_at IS NOT NULL \
         ), gaps AS ( \
             SELECT at - LAG(at) OVER (ORDER BY at) AS gap FROM times \
         ) \
         SELECT (EXTRACT(EPOCH FROM MIN(gap)) / 60)::float8 FROM gaps",
    )
    .bind(tournament_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.0.map(|minutes| minutes.round() as i64))
}

fn ensure_valid_slot(slot_minutes: i64) -> TournamentResult<()> {
    if is_valid_slot_minutes(slot_minutes) {
        Ok(())
    } else {
        Err(TournamentError::validation(
            "Die Rundenlänge muss zwischen 1 und 1440 Minuten liegen",
        ))
    }
}

fn round_time(start: DateTime<Utc>, round: i64, slot_minutes: i64) -> DateTime<Utc> {
    start + Duration::minutes(round_offset_minutes(round, slot_minutes))
}

async fn load_team_ids(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
) -> TournamentResult<Vec<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM turnier.teams WHERE tournament_id = $1 ORDER BY created_at, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

async fn plan_start(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
) -> TournamentResult<DateTime<Utc>> {
    let row: Option<(Option<DateTime<Utc>>,)> =
        sqlx::query_as("SELECT group_phase_start FROM turnier.tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some((stored_start,)) = row else {
        return Err(TournamentError::validation("Turnier nicht gefunden"));
    };
    if let Some(start) = stored_start {
        return Ok(start);
    }
    let start = now_utc();
    write_group_phase_start(tx, tournament_id, start).await?;
    Ok(start)
}

async fn write_group_phase_start(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    start: DateTime<Utc>,
) -> TournamentResult<()> {
    sqlx::query(
        "UPDATE turnier.tournaments SET group_phase_start = $1, updated_at = $2 WHERE id = $3",
    )
    .bind(start)
    .bind(now_utc())
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn delete_existing_plan(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
) -> TournamentResult<()> {
    for table in ["group_matches", "group_teams"] {
        let sql = format!(
            "DELETE FROM turnier.{table} WHERE group_id IN \
             (SELECT id FROM turnier.groups WHERE tournament_id = $1)"
        );
        sqlx::query(&sql)
            .bind(tournament_id)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("DELETE FROM turnier.groups WHERE tournament_id = $1")
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_table_group(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    team_ids: &[i64],
) -> TournamentResult<i64> {
    let (group_id,): (i64,) = sqlx::query_as(
        "INSERT INTO turnier.groups (tournament_id, name, seeding_order) \
         VALUES ($1, $2, 0) RETURNING id",
    )
    .bind(tournament_id)
    .bind(LEAGUE_GROUP_NAME)
    .fetch_one(&mut **tx)
    .await?;
    for team_id in team_ids {
        sqlx::query(
            "INSERT INTO turnier.group_teams (group_id, team_id, wins, losses, points) \
             VALUES ($1, $2, 0, 0, 0)",
        )
        .bind(group_id)
        .bind(team_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(group_id)
}
