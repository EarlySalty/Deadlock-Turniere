//! Advancement: Gewinner/Verlierer durch den Bracket-Baum propagieren.
//! Portiert `advance_bracket_winner`, `_advance_bracket_winner_in_db`,
//! `_propagate_resolved_entry`.

use sqlx::{Pool, Sqlite, Transaction};

use crate::error::TournamentResult;

/// Snapshot eines Bracket-Matches VOR der Propagation (Stand wie geladen).
#[derive(sqlx::FromRow)]
struct MatchSnapshot {
    id: i64,
    round: i64,
    position: i64,
    bracket_type: String,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
    loser_to_match_id: Option<i64>,
    loser_to_slot: Option<i64>,
}

/// Setzt nach einem abgeschlossenen Match den Gewinner (und ggf. Verlierer) in
/// die Folge-Matches. Eigene Transaktion. Match nicht gefunden → No-op.
pub async fn advance_bracket_winner(
    pool: &Pool<Sqlite>,
    tournament_id: i64,
    match_id: i64,
    winner_id: i64,
) -> TournamentResult<()> {
    let mut tx = pool.begin().await?;
    let snapshot: Option<MatchSnapshot> = sqlx::query_as::<_, MatchSnapshot>(
        "SELECT id, round, position, bracket_type, team1_id, team2_id, \
         loser_to_match_id, loser_to_slot FROM bracket_matches WHERE id = ?",
    )
    .bind(match_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(snapshot) = snapshot else {
        // Match nicht gefunden — wie im Original stilles Return (kein Fehler).
        tx.commit().await?;
        return Ok(());
    };

    advance_in_tx(&mut tx, tournament_id, &snapshot, winner_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Propagiert einen aufgelösten Eintrag (Gewinner eines Matches ODER Sieger
/// einer Mini-Group) in alle Folge-Slots. Portiert `_propagate_resolved_entry`.
/// Genau EINE der beiden Quellen ist gesetzt.
pub(crate) async fn propagate_resolved_entry(
    tx: &mut Transaction<'_, Sqlite>,
    winner_id: i64,
    source_match_id: Option<i64>,
    source_mini_group_id: Option<i64>,
) -> TournamentResult<()> {
    if let Some(smid) = source_match_id {
        sqlx::query("UPDATE bracket_matches SET team1_id = ? WHERE source_match1_id = ?")
            .bind(winner_id)
            .bind(smid)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE bracket_matches SET team2_id = ? WHERE source_match2_id = ?")
            .bind(winner_id)
            .bind(smid)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE bracket_mini_group_teams SET team_id = ? WHERE source_match_id = ?")
            .bind(winner_id)
            .bind(smid)
            .execute(&mut **tx)
            .await?;
        return Ok(());
    }

    if let Some(smgid) = source_mini_group_id {
        sqlx::query("UPDATE bracket_matches SET team1_id = ? WHERE source_mini_group1_id = ?")
            .bind(winner_id)
            .bind(smgid)
            .execute(&mut **tx)
            .await?;
        sqlx::query("UPDATE bracket_matches SET team2_id = ? WHERE source_mini_group2_id = ?")
            .bind(winner_id)
            .bind(smgid)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "UPDATE bracket_mini_group_teams SET team_id = ? WHERE source_mini_group_id = ?",
        )
        .bind(winner_id)
        .bind(smgid)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn advance_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tournament_id: i64,
    m: &MatchSnapshot,
    winner_id: i64,
) -> TournamentResult<()> {
    // 1) Gewinner in Folge-Matches/-Mini-Groups.
    propagate_resolved_entry(tx, winner_id, Some(m.id), None).await?;

    // 2) Verlierer in sein Losers-Bracket-Ziel (falls verdrahtet).
    if let (Some(loser_to_match_id), Some(loser_to_slot)) = (m.loser_to_match_id, m.loser_to_slot) {
        let loser_id = if Some(winner_id) == m.team1_id {
            m.team2_id
        } else if Some(winner_id) == m.team2_id {
            m.team1_id
        } else {
            None
        };
        if let Some(loser_id) = loser_id {
            // Zwei feste Query-Zweige statt dynamischem Spaltennamen.
            if loser_to_slot == 1 {
                sqlx::query("UPDATE bracket_matches SET team1_id = ? WHERE id = ?")
                    .bind(loser_id)
                    .bind(loser_to_match_id)
                    .execute(&mut **tx)
                    .await?;
            } else {
                sqlx::query("UPDATE bracket_matches SET team2_id = ? WHERE id = ?")
                    .bind(loser_id)
                    .bind(loser_to_match_id)
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }

    // 3) Grand-Final-Sonderbehandlung (Bracket-Reset).
    if m.bracket_type == "grand_final" {
        let other_gf: Option<(i64, i64)> = sqlx::query_as(
            "SELECT id, round FROM bracket_matches \
             WHERE tournament_id = ? AND bracket_type = 'grand_final' AND id != ? \
             ORDER BY round ASC LIMIT 1",
        )
        .bind(tournament_id)
        .bind(m.id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((other_id, other_round)) = other_gf else {
            return Ok(());
        };
        if m.round < other_round {
            // Losers-Finalist (Slot 2) hat das erste GF gewonnen → Reset spielen.
            if Some(winner_id) == m.team2_id {
                sqlx::query(
                    "UPDATE bracket_matches SET team1_id = ?, team2_id = ? WHERE id = ?",
                )
                .bind(m.team1_id)
                .bind(m.team2_id)
                .bind(other_id)
                .execute(&mut **tx)
                .await?;
            } else {
                // Winners-Finalist hat gewonnen → Reset entfällt (cancelled).
                sqlx::query("UPDATE bracket_matches SET status = 'cancelled' WHERE id = ?")
                    .bind(other_id)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        return Ok(());
    }

    // 4) Existiert ein Source-gemapptes Folge-Match? Dann sind wir fertig.
    let next_match: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM bracket_matches \
         WHERE tournament_id = ? AND (source_match1_id = ? OR source_match2_id = ?) LIMIT 1",
    )
    .bind(tournament_id)
    .bind(m.id)
    .bind(m.id)
    .fetch_optional(&mut **tx)
    .await?;
    if next_match.is_some() {
        return Ok(());
    }

    // 5) Legacy-Fallback (nur winners): über round/position. Bug-preserved —
    //    für vom Generator erzeugte Brackets ein toter Pfad (immer source-gemappt).
    if m.bracket_type != "winners" {
        return Ok(());
    }
    let next_round = m.round + 1;
    let next_position = m.position / 2;
    let legacy: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM bracket_matches WHERE tournament_id = ? AND round = ? AND position = ?",
    )
    .bind(tournament_id)
    .bind(next_round)
    .bind(next_position)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((legacy_id,)) = legacy else {
        return Ok(());
    };
    if m.position % 2 == 0 {
        sqlx::query("UPDATE bracket_matches SET team1_id = ? WHERE id = ?")
            .bind(winner_id)
            .bind(legacy_id)
            .execute(&mut **tx)
            .await?;
    } else {
        sqlx::query("UPDATE bracket_matches SET team2_id = ? WHERE id = ?")
            .bind(winner_id)
            .bind(legacy_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
