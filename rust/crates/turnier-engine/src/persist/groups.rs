//! Gruppenphase: Snake-Draft-Verteilung + Round-Robin-Matches (Persistenz).
//! Portiert `generate_groups`/`_generate_groups_in_db` und
//! `generate_group_matches`/`_generate_group_matches_in_db`.

use sqlx::{Pool, Postgres, Transaction};

use crate::engine::groups::auto_num_groups;
use crate::engine::seeding::snake_draft_group_index;
use crate::error::{TournamentError, TournamentResult};

/// Generiert die Gruppen (Snake-Draft nach Durchschnitts-Rank-Score) und liefert
/// die erzeugten Group-IDs. Eigene Transaktion.
pub async fn generate_groups(
    pool: &Pool<Postgres>,
    tournament_id: i64,
    num_groups: Option<usize>,
) -> TournamentResult<Vec<i64>> {
    let mut tx = pool.begin().await?;
    let ids = generate_groups_in_tx(&mut tx, tournament_id, num_groups).await?;
    tx.commit().await?;
    Ok(ids)
}

/// Snake-Draft-Gruppenbildung innerhalb einer bestehenden Transaktion. Auch von
/// `finalize_checkin` (advance_to_group_phase) genutzt.
pub async fn generate_groups_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    num_groups: Option<usize>,
) -> TournamentResult<Vec<i64>> {
    let exists: Option<(i64,)> = sqlx::query_as("SELECT id FROM turnier.tournaments WHERE id = $1")
        .bind(tournament_id)
        .fetch_optional(&mut **tx)
        .await?;
    if exists.is_none() {
        return Err(TournamentError::validation("Turnier nicht gefunden"));
    }

    // Teams in explizit stabiler Erstellreihenfolge laden.
    let teams: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM turnier.teams WHERE tournament_id = $1 ORDER BY created_at, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;
    if teams.len() < 2 {
        return Err(TournamentError::validation("Mindestens 2 Teams benötigt"));
    }

    let target_groups = num_groups.unwrap_or_else(|| auto_num_groups(teams.len()));

    // Durchschnitts-Rank-Score je Team (ein Query je Team wie im Original; die
    // Reihenfolge bleibt die DB-Reihenfolge, danach STABILE Sortierung).
    let mut team_scores: Vec<(i64, f64)> = Vec::with_capacity(teams.len());
    for (team_id,) in &teams {
        let members: Vec<(i64,)> =
            sqlx::query_as("SELECT rank_score FROM turnier.team_members WHERE team_id = $1")
                .bind(team_id)
                .fetch_all(&mut **tx)
                .await?;
        let sum: i64 = members.iter().map(|(s,)| *s).sum();
        let avg = sum as f64 / (members.len().max(1) as f64);
        team_scores.push((*team_id, avg));
    }

    // STABILE Sortierung nach avg_score absteigend (Python `list.sort` ist
    // stabil; bei Score-Gleichstand bleibt die DB-Reihenfolge erhalten).
    team_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Gruppen-Anzahl anpassen (max = Teams/2, min = 2) — zweites Clamping wie im
    // Original (bug-preserved: getrennt von auto_num_groups).
    let actual_groups = target_groups.min(team_scores.len() / 2).max(2);

    // Bestehende Gruppen + Matches/Teams löschen.
    let old_groups: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM turnier.groups WHERE tournament_id = $1")
            .bind(tournament_id)
            .fetch_all(&mut **tx)
            .await?;
    for (gid,) in &old_groups {
        sqlx::query("DELETE FROM turnier.group_matches WHERE group_id = $1")
            .bind(gid)
            .execute(&mut **tx)
            .await?;
        sqlx::query("DELETE FROM turnier.group_teams WHERE group_id = $1")
            .bind(gid)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("DELETE FROM turnier.groups WHERE tournament_id = $1")
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;

    // Gruppen anlegen (Gruppe A, B, C, …).
    let mut group_ids: Vec<i64> = Vec::with_capacity(actual_groups);
    for idx in 0..actual_groups {
        let letter = (b'A' + idx as u8) as char;
        let row: (i64,) = sqlx::query_as(
            "INSERT INTO turnier.groups (tournament_id, name, seeding_order) \
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(tournament_id)
        .bind(format!("Gruppe {letter}"))
        .bind(idx as i64)
        .fetch_one(&mut **tx)
        .await?;
        group_ids.push(row.0);
    }

    // Snake-Draft-Verteilung.
    for (i, (team_id, _avg)) in team_scores.iter().enumerate() {
        let group_idx = snake_draft_group_index(i, actual_groups);
        sqlx::query(
            "INSERT INTO turnier.group_teams (group_id, team_id, wins, losses, points) \
             VALUES ($1, $2, 0, 0, 0)",
        )
        .bind(group_ids[group_idx])
        .bind(team_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(group_ids)
}

/// Generiert Round-Robin-`group_matches` für alle Gruppen. Eigene Transaktion.
pub async fn generate_group_matches(
    pool: &Pool<Postgres>,
    tournament_id: i64,
) -> TournamentResult<i64> {
    let mut tx = pool.begin().await?;
    let count = generate_group_matches_in_tx(&mut tx, tournament_id).await?;
    tx.commit().await?;
    Ok(count)
}

/// Round-Robin-Matches innerhalb einer bestehenden Transaktion. Liefert die
/// Match-Anzahl.
pub async fn generate_group_matches_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
) -> TournamentResult<i64> {
    let mut match_count = 0i64;
    let groups: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM turnier.groups WHERE tournament_id = $1 ORDER BY seeding_order, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;

    for (group_id,) in &groups {
        sqlx::query("DELETE FROM turnier.group_matches WHERE group_id = $1")
            .bind(group_id)
            .execute(&mut **tx)
            .await?;

        let team_ids: Vec<(i64,)> = sqlx::query_as(
            "SELECT team_id FROM turnier.group_teams WHERE group_id = $1 ORDER BY id",
        )
        .bind(group_id)
        .fetch_all(&mut **tx)
        .await?;
        let ids: Vec<i64> = team_ids.into_iter().map(|(t,)| t).collect();

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                sqlx::query(
                    "INSERT INTO turnier.group_matches (group_id, team1_id, team2_id, status) \
                     VALUES ($1, $2, $3, 'pending')",
                )
                .bind(group_id)
                .bind(ids[i])
                .bind(ids[j])
                .execute(&mut **tx)
                .await?;
                match_count += 1;
            }
        }
    }

    Ok(match_count)
}
