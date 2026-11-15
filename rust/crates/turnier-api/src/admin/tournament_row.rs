//! Mapping einer `tournaments`-Zeile auf das [`Tournament`]-DTO.
//!
//! Die Reminder-Offset-Spalten liegen in Postgres als JSONB und werden hier zu
//! `Vec<i64>` geparst. Die Wire-Form bleibt bei Zeitstempeln und Discord-IDs
//! String-kompatibel.

use std::collections::HashMap;

use turnier_core::{AdminTournamentSummary, Tournament};
use turnier_db::Pool;

use crate::error::{WebError, WebResult};

/// Lädt ein Turnier als DTO (oder 404).
pub async fn load_tournament_dto(pool: &Pool, tournament_id: i64) -> WebResult<Tournament> {
    let row = sqlx::query(&format!(
        "{} WHERE id = $1",
        crate::public::helpers::tournament_select()
    ))
    .bind(tournament_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))?;
    crate::public::helpers::tournament_from_row(&row)
}

/// Lädt alle Turniere als DTO-Liste (`created_at DESC`).
pub async fn load_all_tournaments_dto(pool: &Pool) -> WebResult<Vec<Tournament>> {
    crate::public::helpers::list_tournament_dtos(pool, "ORDER BY created_at DESC").await
}

#[derive(sqlx::FromRow)]
struct TournamentCountRow {
    id: i64,
    team_count: i64,
    player_count: i64,
    signup_count: i64,
    open_signup_count: i64,
}

const TOURNAMENT_COUNTS_SELECT: &str = r#"SELECT t.id,
       (SELECT COUNT(*) FROM turnier."teams" tm WHERE tm.tournament_id = t.id) AS team_count,
       (SELECT COUNT(*) FROM turnier."team_members" m JOIN turnier."teams" tm ON tm.id = m.team_id
         WHERE tm.tournament_id = t.id) AS player_count,
       (SELECT COUNT(*) FROM turnier."tournament_signups" s WHERE s.tournament_id = t.id) AS signup_count,
       (SELECT COUNT(*) FROM turnier."tournament_signups" s
         WHERE s.tournament_id = t.id AND s.team_id IS NULL) AS open_signup_count
  FROM turnier."tournaments" t"#;

pub async fn load_admin_tournament_summaries(
    pool: &Pool,
) -> WebResult<Vec<AdminTournamentSummary>> {
    let tournaments = load_all_tournaments_dto(pool).await?;
    let counts: Vec<TournamentCountRow> = sqlx::query_as(TOURNAMENT_COUNTS_SELECT)
        .fetch_all(pool)
        .await?;
    let by_id: HashMap<i64, TournamentCountRow> =
        counts.into_iter().map(|row| (row.id, row)).collect();
    Ok(tournaments
        .into_iter()
        .map(|tournament| {
            let counts = by_id.get(&tournament.id);
            summarize(tournament, counts)
        })
        .collect())
}

fn summarize(
    tournament: Tournament,
    counts: Option<&TournamentCountRow>,
) -> AdminTournamentSummary {
    AdminTournamentSummary {
        team_count: counts.map_or(0, |c| c.team_count),
        player_count: counts.map_or(0, |c| c.player_count),
        signup_count: counts.map_or(0, |c| c.signup_count),
        open_signup_count: counts.map_or(0, |c| c.open_signup_count),
        tournament,
    }
}
