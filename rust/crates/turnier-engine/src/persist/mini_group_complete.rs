//! Mini-Group-Round-Robin abschließen: Sieger ermitteln und propagieren.
//! Portiert `complete_mini_group_round_robin`.

use serde_json::Value;
use sqlx::{Pool, Postgres};

use crate::error::TournamentResult;
use crate::mini_groups::{aggregate, select_mini_group_winner, MiniGroupMatch};

use super::advance::propagate_resolved_entry;

#[derive(sqlx::FromRow)]
struct MgMatchRow {
    team1_id: Option<i64>,
    team2_id: Option<i64>,
    winner_id: Option<i64>,
    status: String,
    match_stats: Option<Value>,
}

/// Wertet eine vollständig gespielte Mini-Group aus, schreibt den Sieger in den
/// Ziel-Slot des Folge-Matches und propagiert ihn. Liefert die Sieger-Team-ID,
/// oder `None`, wenn die Mini-Group (noch) nicht auswertbar ist.
pub async fn complete_mini_group_round_robin(
    pool: &Pool<Postgres>,
    mini_group_id: i64,
) -> TournamentResult<Option<i64>> {
    let mut tx = pool.begin().await?;

    let mini_group: Option<(i64, i64, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT id, tournament_id, advances_to_match_id, advances_to_slot \
         FROM turnier.bracket_mini_groups WHERE id = $1",
    )
    .bind(mini_group_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((_id, _tid, advances_to_match_id, advances_to_slot)) = mini_group else {
        tx.commit().await?;
        return Ok(None);
    };

    // Teilnehmer (mit Team) in Seed-Reihenfolge.
    let team_rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT team_id, seed_order FROM turnier.bracket_mini_group_teams \
         WHERE mini_group_id = $1 AND team_id IS NOT NULL ORDER BY seed_order, id",
    )
    .bind(mini_group_id)
    .fetch_all(&mut *tx)
    .await?;
    if team_rows.len() < 2 {
        tx.commit().await?;
        return Ok(None);
    }
    let team_ids: Vec<i64> = team_rows.iter().map(|(t, _)| *t).collect();
    let seed_order: std::collections::HashMap<i64, i64> =
        team_rows.iter().map(|(t, s)| (*t, *s)).collect();

    // Alle Round-Robin-Matches dieser Mini-Group.
    let match_rows: Vec<MgMatchRow> = sqlx::query_as::<_, MgMatchRow>(
        "SELECT team1_id, team2_id, winner_id, status, match_stats \
         FROM turnier.bracket_matches WHERE mini_group_id = $1 ORDER BY round, position, id",
    )
    .bind(mini_group_id)
    .fetch_all(&mut *tx)
    .await?;

    // Auswertbar nur, wenn ALLE Matches completed sind und einen Sieger haben.
    if match_rows.is_empty()
        || match_rows
            .iter()
            .any(|r| r.status != "completed" || r.winner_id.is_none())
    {
        tx.commit().await?;
        return Ok(None);
    }

    let mini_matches: Vec<MiniGroupMatch> = match_rows
        .iter()
        .map(|r| MiniGroupMatch {
            team1_id: r.team1_id.expect("completed match has team1"),
            team2_id: r.team2_id.expect("completed match has team2"),
            winner_id: r.winner_id.expect("completed match has winner"),
            match_stats: r.match_stats.clone(),
        })
        .collect();

    let (wins, point_diff, h2h) = aggregate(&team_ids, &mini_matches);
    let winner_team_id = select_mini_group_winner(&team_ids, &wins, &point_diff, &seed_order, &h2h);

    // Sieger in den Ziel-Slot des Folge-Matches schreiben.
    if let (Some(target_match_id), Some(slot)) = (advances_to_match_id, advances_to_slot) {
        if slot == 1 {
            sqlx::query("UPDATE turnier.bracket_matches SET team1_id = $1 WHERE id = $2")
                .bind(winner_team_id)
                .bind(target_match_id)
                .execute(&mut *tx)
                .await?;
        } else if slot == 2 {
            sqlx::query("UPDATE turnier.bracket_matches SET team2_id = $1 WHERE id = $2")
                .bind(winner_team_id)
                .bind(target_match_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    propagate_resolved_entry(&mut tx, winner_team_id, None, Some(mini_group_id)).await?;

    tx.commit().await?;
    Ok(Some(winner_team_id))
}
