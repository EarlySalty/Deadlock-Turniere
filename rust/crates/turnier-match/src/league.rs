use chrono::{DateTime, Utc};
use sqlx::{Executor, PgConnection, Postgres, Transaction};
use turnier_engine::league::{league_table, LeagueGame};

const CURRENT_ROUND_CTE: &str = "WITH rounds AS ( \
         SELECT gm.id, gm.status, gm.steam_party_id, gm.scheduled_at, \
                DENSE_RANK() OVER (ORDER BY gm.scheduled_at) AS round \
         FROM turnier.group_matches gm \
         JOIN turnier.groups g ON g.id = gm.group_id \
         WHERE g.tournament_id = $1 \
     ), current_round AS ( \
         SELECT MIN(round) AS round FROM rounds \
         WHERE status NOT IN ('completed', 'cancelled', 'forfeit') \
     ) ";

pub async fn tournament_is_league<'e, E>(executor: E, tournament_id: i64) -> sqlx::Result<bool>
where
    E: Executor<'e, Database = Postgres>,
{
    let mode: Option<String> =
        sqlx::query_scalar("SELECT tournament_mode FROM turnier.tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(executor)
            .await?;
    Ok(mode.as_deref() == Some("league"))
}

pub async fn group_is_league<'e, E>(executor: E, group_id: i64) -> sqlx::Result<bool>
where
    E: Executor<'e, Database = Postgres>,
{
    let mode: Option<String> = sqlx::query_scalar(
        "SELECT t.tournament_mode FROM turnier.groups g \
         JOIN turnier.tournaments t ON t.id = g.tournament_id WHERE g.id = $1",
    )
    .bind(group_id)
    .fetch_optional(executor)
    .await?;
    Ok(mode.as_deref() == Some("league"))
}

pub async fn league_is_running(conn: &mut PgConnection, tournament_id: i64) -> sqlx::Result<bool> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM turnier.tournaments WHERE id = $1 FOR SHARE")
            .bind(tournament_id)
            .fetch_optional(conn)
            .await?;
    Ok(status.as_deref() == Some("group_phase"))
}

pub async fn record_group_standing_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    group_id: i64,
    winner_id: i64,
    loser_id: Option<i64>,
) -> sqlx::Result<()> {
    if group_is_league(&mut **tx, group_id).await? {
        return recompute_league_standings_in_tx(tx, group_id).await;
    }
    sqlx::query(
        "UPDATE turnier.group_teams SET wins = wins + 1, points = points + 3 \
         WHERE group_id = $1 AND team_id = $2",
    )
    .bind(group_id)
    .bind(winner_id)
    .execute(&mut **tx)
    .await?;
    if let Some(loser_id) = loser_id {
        sqlx::query(
            "UPDATE turnier.group_teams SET losses = losses + 1 \
             WHERE group_id = $1 AND team_id = $2",
        )
        .bind(group_id)
        .bind(loser_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn recompute_league_standings_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    group_id: i64,
) -> sqlx::Result<()> {
    sqlx::query("SELECT id FROM turnier.groups WHERE id = $1 FOR UPDATE")
        .bind(group_id)
        .execute(&mut **tx)
        .await?;
    let team_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT team_id FROM turnier.group_teams WHERE group_id = $1 ORDER BY id",
    )
    .bind(group_id)
    .fetch_all(&mut **tx)
    .await?;
    let games: Vec<(i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT team1_id, team2_id, winner_id FROM turnier.group_matches \
         WHERE group_id = $1 AND status = 'completed'",
    )
    .bind(group_id)
    .fetch_all(&mut **tx)
    .await?;
    let games: Vec<LeagueGame> = games
        .into_iter()
        .map(|(team1_id, team2_id, winner_id)| LeagueGame {
            team1_id,
            team2_id,
            winner_id,
        })
        .collect();
    for row in league_table(&team_ids, &games) {
        sqlx::query(
            "UPDATE turnier.group_teams SET wins = $1, losses = $2, points = $3 \
             WHERE group_id = $4 AND team_id = $5",
        )
        .bind(row.wins)
        .bind(row.losses)
        .bind(row.points)
        .bind(group_id)
        .bind(row.team_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn write_league_manual_result_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    match_id: i64,
    group_id: i64,
    winner_id: i64,
    winning_slot: i64,
) -> sqlx::Result<bool> {
    let written = sqlx::query(
        "UPDATE turnier.group_matches \
         SET winner_id = $1, status = 'completed', played_at = now() \
         WHERE id = $2 AND group_id = $3 AND status NOT IN ('cancelled', 'forfeit')",
    )
    .bind(winner_id)
    .bind(match_id)
    .bind(group_id)
    .execute(&mut **tx)
    .await?;
    if written.rows_affected() == 0 {
        return Ok(false);
    }
    delete_match_results(tx, match_id).await?;
    sqlx::query(
        "INSERT INTO turnier.match_results (group_match_id, winning_team, source, created_at) \
         VALUES ($1, $2, 'manual', now())",
    )
    .bind(match_id)
    .bind(winning_slot)
    .execute(&mut **tx)
    .await?;
    recompute_league_standings_in_tx(tx, group_id).await?;
    Ok(true)
}

pub async fn reset_league_match_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    match_id: i64,
    group_id: i64,
) -> sqlx::Result<bool> {
    let written = sqlx::query(
        "UPDATE turnier.group_matches \
         SET status = 'pending', winner_id = NULL, played_at = NULL, steam_party_id = NULL, \
             party_code = NULL, deadlock_match_id = NULL, discord_channel_id = NULL, \
             match_duration_s = NULL, match_stats = NULL \
         WHERE id = $1 AND group_id = $2 AND status NOT IN ('cancelled', 'forfeit')",
    )
    .bind(match_id)
    .bind(group_id)
    .execute(&mut **tx)
    .await?;
    if written.rows_affected() == 0 {
        return Ok(false);
    }
    delete_match_results(tx, match_id).await?;
    recompute_league_standings_in_tx(tx, group_id).await?;
    Ok(true)
}

async fn delete_match_results(
    tx: &mut Transaction<'_, Postgres>,
    match_id: i64,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM turnier.match_results WHERE group_match_id = $1")
        .bind(match_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn open_league_match_count<'e, E>(executor: E, tournament_id: i64) -> sqlx::Result<i64>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.group_matches gm \
         JOIN turnier.groups g ON g.id = gm.group_id \
         WHERE g.tournament_id = $1 \
           AND gm.status NOT IN ('completed', 'cancelled', 'forfeit')",
    )
    .bind(tournament_id)
    .fetch_one(executor)
    .await
}

pub async fn current_round_match_ids<'e, E>(
    executor: E,
    tournament_id: i64,
) -> sqlx::Result<Vec<i64>>
where
    E: Executor<'e, Database = Postgres>,
{
    let sql = format!(
        "{CURRENT_ROUND_CTE} \
         SELECT rounds.id FROM rounds JOIN current_round ON rounds.round = current_round.round \
         WHERE rounds.status NOT IN ('completed', 'cancelled', 'forfeit') \
         ORDER BY rounds.id"
    );
    sqlx::query_scalar(&sql)
        .bind(tournament_id)
        .fetch_all(executor)
        .await
}

pub async fn due_lobby_match_ids<'e, E>(
    executor: E,
    tournament_id: i64,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<i64>>
where
    E: Executor<'e, Database = Postgres>,
{
    let sql = format!(
        "{CURRENT_ROUND_CTE} \
         SELECT rounds.id FROM rounds JOIN current_round ON rounds.round = current_round.round \
         WHERE rounds.scheduled_at <= $2 \
           AND rounds.status IN ('pending', 'checkin') \
           AND rounds.steam_party_id IS NULL \
         ORDER BY rounds.id"
    );
    sqlx::query_scalar(&sql)
        .bind(tournament_id)
        .bind(now)
        .fetch_all(executor)
        .await
}

pub async fn running_league_ids_with_auto_lobby<'e, E>(executor: E) -> sqlx::Result<Vec<i64>>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar(
        "SELECT id FROM turnier.tournaments \
         WHERE tournament_mode = 'league' AND status = 'group_phase' \
           AND auto_lobby_enabled = true AND is_test = false \
         ORDER BY id",
    )
    .fetch_all(executor)
    .await
}
