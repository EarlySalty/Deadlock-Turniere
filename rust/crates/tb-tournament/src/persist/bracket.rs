//! Single-Elimination-Bracket-Aufbau + `generate_bracket`-Orchestrierung.
//!
//! Portiert `generate_bracket`, `_build_seeded_bracket`, `_build_bracket_round`,
//! `_build_paired_bracket`, `_insert_mini_group` und `_build_group_cross_seed_pairs`.
//! Die rekursiven Runden-Builder sind als `Box::pin`-Futures umgesetzt
//! (async-Rekursion).

use std::future::Future;
use std::pin::Pin;

use sqlx::{Pool, Sqlite, Transaction};

use crate::engine::seeding::{
    distribute_entries_across_slots, is_power_of_two, seed_slot_order, slot_sizes_for_round,
};
use crate::engine::slots::BracketSlot;
use crate::error::{TournamentError, TournamentResult};

use super::double_elim::build_double_elimination_bracket;
use super::{clear_bracket_tree, insert_bracket_match};

/// Ein Qualifikant aus der Gruppenphase (oder Fallback: alle Teams).
struct Qualifier {
    team_id: i64,
    points: i64,
    wins: i64,
    seed: i64,
}

/// Generiert das Bracket aus den Gruppen-Standings (Top 2/Gruppe) oder — ohne
/// Gruppen — aus allen Teams. Liefert die Anzahl erzeugter Matches.
///
/// Portiert `generate_bracket(tournament_id, bracket_format)`: Das übergebene
/// `bracket_format`-Argument wird (wie im Original) ignoriert und stattdessen aus
/// der `tournaments`-Zeile gelesen.
pub async fn generate_bracket(pool: &Pool<Sqlite>, tournament_id: i64) -> TournamentResult<i64> {
    let mut tx = pool.begin().await?;

    clear_bracket_tree(&mut tx, tournament_id).await?;

    let format_row: Option<(String,)> =
        sqlx::query_as("SELECT bracket_format FROM tournaments WHERE id = ?")
            .bind(tournament_id)
            .fetch_optional(&mut *tx)
            .await?;
    let bracket_format = format_row
        .map(|r| r.0)
        .unwrap_or_else(|| "single_elimination".to_string());

    // Gruppen (nach seeding_order) und je Gruppe die Top-2-Standings.
    let group_ids: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM groups WHERE tournament_id = ? ORDER BY seeding_order")
            .bind(tournament_id)
            .fetch_all(&mut *tx)
            .await?;

    let mut qualified: Vec<Qualifier> = Vec::new();
    let mut grouped_qualifiers: Vec<Vec<Qualifier>> = Vec::new();
    for (group_id,) in &group_ids {
        let standings: Vec<(i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT team_id, wins, losses, points FROM group_teams \
             WHERE group_id = ? ORDER BY points DESC, wins DESC",
        )
        .bind(group_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut group_qs: Vec<Qualifier> = Vec::new();
        for (rank_pos, (team_id, wins, _losses, points)) in standings.iter().take(2).enumerate() {
            qualified.push(Qualifier {
                team_id: *team_id,
                points: *points,
                wins: *wins,
                seed: rank_pos as i64,
            });
            group_qs.push(Qualifier {
                team_id: *team_id,
                points: *points,
                wins: *wins,
                seed: rank_pos as i64,
            });
        }
        grouped_qualifiers.push(group_qs);
    }

    // Fallback ohne Gruppenphase: alle Teams in DB-Reihenfolge.
    if qualified.is_empty() {
        let all_teams: Vec<(i64,)> =
            sqlx::query_as("SELECT id FROM teams WHERE tournament_id = ?")
                .bind(tournament_id)
                .fetch_all(&mut *tx)
                .await?;
        qualified = all_teams
            .into_iter()
            .enumerate()
            .map(|(i, (id,))| Qualifier {
                team_id: id,
                points: 0,
                wins: 0,
                seed: i as i64,
            })
            .collect();
    }

    if qualified.len() < 2 {
        return Err(TournamentError::validation(
            "Mindestens 2 Teams für Bracket benötigt",
        ));
    }

    let cross_seed_pairs = build_group_cross_seed_pairs(&grouped_qualifiers);

    let match_count = if bracket_format == "double_elimination" {
        let pairs_or_entries = match cross_seed_pairs {
            Some(pairs) => DoubleElimInput::Pairs(pairs),
            None => {
                let entries = sort_and_map_entries(qualified);
                DoubleElimInput::Entries(entries)
            }
        };
        build_double_elimination_bracket(&mut tx, tournament_id, pairs_or_entries).await?
    } else if let Some(pairs) = cross_seed_pairs {
        build_paired_bracket(&mut tx, tournament_id, pairs, 1, "winners").await?
    } else {
        let entries = sort_and_map_entries(qualified);
        build_seeded_bracket(&mut tx, tournament_id, entries).await?
    };

    tx.commit().await?;
    Ok(match_count)
}

/// Sortiert die Qualifikanten (`-points, -wins, seed`) und mappt sie auf
/// Team-Slots. Portiert die zweifach duplizierte Sortier-/Map-Logik einmalig.
fn sort_and_map_entries(mut qualified: Vec<Qualifier>) -> Vec<BracketSlot> {
    qualified.sort_by(|a, b| {
        (-a.points, -a.wins, a.seed).cmp(&(-b.points, -b.wins, b.seed))
    });
    qualified
        .into_iter()
        .map(|q| BracketSlot::Team(q.team_id))
        .collect()
}

/// Eingabe für den Double-Elim-Builder: entweder Cross-Seed-Paare oder eine
/// flache Seed-Liste.
pub(crate) enum DoubleElimInput {
    Pairs(Vec<(BracketSlot, BracketSlot)>),
    Entries(Vec<BracketSlot>),
}

/// Cross-Seed-Paare aus den Gruppen-Qualifikanten: Sieger Gruppe i vs Zweiter
/// Gruppe i+1 (zyklisch). Nur wenn jede Gruppe >= 2 Qualifikanten hat UND die
/// Gesamtzahl eine Zweierpotenz ist. Portiert `_build_group_cross_seed_pairs`.
fn build_group_cross_seed_pairs(
    grouped: &[Vec<Qualifier>],
) -> Option<Vec<(BracketSlot, BracketSlot)>> {
    if grouped.is_empty() {
        return None;
    }
    if grouped.iter().any(|g| g.len() < 2) {
        return None;
    }
    let qualifier_count = grouped.len() * 2;
    if !is_power_of_two(qualifier_count) {
        return None;
    }
    let group_count = grouped.len();
    let mut pairs = Vec::with_capacity(group_count);
    for (index, group) in grouped.iter().enumerate() {
        let next_group = &grouped[(index + 1) % group_count];
        pairs.push((
            BracketSlot::Team(group[0].team_id),
            BracketSlot::Team(next_group[1].team_id),
        ));
    }
    Some(pairs)
}

/// Baut ein Single-Elimination-Bracket aus einer Seed-Liste. Portiert
/// `_build_seeded_bracket`.
pub(crate) async fn build_seeded_bracket(
    tx: &mut Transaction<'_, Sqlite>,
    tournament_id: i64,
    entries: Vec<BracketSlot>,
) -> TournamentResult<i64> {
    if entries.len() < 2 {
        return Err(TournamentError::validation(
            "Mindestens 2 Teams für Bracket benötigt",
        ));
    }
    build_bracket_round(tx, tournament_id, entries, 1).await
}

/// Baut ein Paar-basiertes Bracket (Cross-Seed-Runde 1), dann rekursiv weiter.
/// Portiert `_build_paired_bracket`.
async fn build_paired_bracket(
    tx: &mut Transaction<'_, Sqlite>,
    tournament_id: i64,
    pairs: Vec<(BracketSlot, BracketSlot)>,
    current_round: i64,
    bracket_type: &str,
) -> TournamentResult<i64> {
    if pairs.is_empty() {
        return Err(TournamentError::validation(
            "Mindestens 2 Teams für Bracket benötigt",
        ));
    }
    let mut next_entries: Vec<BracketSlot> = Vec::new();
    let mut match_count = 0i64;
    for (position, (left, right)) in pairs.into_iter().enumerate() {
        let match_id = insert_bracket_match(
            tx,
            tournament_id,
            current_round,
            position as i64,
            left,
            right,
            None,
            bracket_type,
            None,
            None,
        )
        .await?;
        next_entries.push(BracketSlot::FromMatch(match_id));
        match_count += 1;
    }

    if next_entries.len() == 1 {
        return Ok(match_count);
    }
    Ok(match_count + build_bracket_round(tx, tournament_id, next_entries, current_round + 1).await?)
}

/// Eine Bracket-Runde: bei Zweierpotenz Standard-Seed-Paarung, sonst Mini-Group-
/// Slots. Rekursiv bis zum Finale. Portiert `_build_bracket_round`.
fn build_bracket_round<'a>(
    tx: &'a mut Transaction<'_, Sqlite>,
    tournament_id: i64,
    entries: Vec<BracketSlot>,
    current_round: i64,
) -> Pin<Box<dyn Future<Output = TournamentResult<i64>> + Send + 'a>> {
    Box::pin(async move {
        if entries.len() < 2 {
            return Err(TournamentError::validation(
                "Mindestens 2 Teams für Bracket benötigt",
            ));
        }

        if is_power_of_two(entries.len()) {
            let order = seed_slot_order(entries.len());
            let ordered: Vec<BracketSlot> = order.iter().map(|seed| entries[seed - 1]).collect();
            let mut next_entries: Vec<BracketSlot> = Vec::new();
            let mut match_count = 0i64;
            let mut position = 0i64;
            let mut slot_idx = 0;
            while slot_idx < ordered.len() {
                let left = ordered[slot_idx];
                let right = ordered[slot_idx + 1];
                let match_id = insert_bracket_match(
                    tx,
                    tournament_id,
                    current_round,
                    position,
                    left,
                    right,
                    None,
                    "winners",
                    None,
                    None,
                )
                .await?;
                next_entries.push(BracketSlot::FromMatch(match_id));
                match_count += 1;
                position += 1;
                slot_idx += 2;
            }

            if next_entries.len() == 1 {
                return Ok(match_count);
            }
            return Ok(match_count
                + build_bracket_round(tx, tournament_id, next_entries, current_round + 1).await?);
        }

        // Nicht-Zweierpotenz: Slot-Größen + Schlangen-Verteilung, 2er-Slots als
        // Match, 3er+-Slots als Mini-Group (Round-Robin).
        let slot_sizes = slot_sizes_for_round(entries.len());
        let slot_entries = distribute_entries_across_slots(&entries, &slot_sizes);

        let mut next_round_entries: Vec<BracketSlot> = Vec::new();
        let mut match_position = 0i64;
        let mut match_count = 0i64;
        for (slot_position, slot) in slot_entries.into_iter().enumerate() {
            if slot.len() == 2 {
                let match_id = insert_bracket_match(
                    tx,
                    tournament_id,
                    current_round,
                    match_position,
                    slot[0],
                    slot[1],
                    None,
                    "winners",
                    None,
                    None,
                )
                .await?;
                next_round_entries.push(BracketSlot::FromMatch(match_id));
                match_position += 1;
                match_count += 1;
                continue;
            }

            let (mini_group_id, mini_match_count) = insert_mini_group(
                tx,
                tournament_id,
                current_round,
                slot_position as i64,
                match_position,
                &slot,
            )
            .await?;
            next_round_entries.push(BracketSlot::FromMiniGroup(mini_group_id));
            match_position += mini_match_count;
            match_count += mini_match_count;
        }

        if next_round_entries.len() == 1 {
            return Ok(match_count);
        }
        Ok(match_count
            + build_bracket_round(tx, tournament_id, next_round_entries, current_round + 1).await?)
    })
}

/// Legt eine Mini-Group an: Header-Zeile, Teilnehmer (mit seed_order und Quell-
/// Referenzen) und die Round-Robin-Matches. Liefert (mini_group_id, match_count).
/// Portiert `_insert_mini_group`.
async fn insert_mini_group(
    tx: &mut Transaction<'_, Sqlite>,
    tournament_id: i64,
    round_num: i64,
    position: i64,
    match_position_start: i64,
    entries: &[BracketSlot],
) -> TournamentResult<(i64, i64)> {
    let mg_row: (i64,) = sqlx::query_as(
        "INSERT INTO bracket_mini_groups \
         (tournament_id, round, position, advances_to_match_id, advances_to_slot) \
         VALUES (?, ?, ?, NULL, NULL) RETURNING id",
    )
    .bind(tournament_id)
    .bind(round_num)
    .bind(position)
    .fetch_one(&mut **tx)
    .await?;
    let mini_group_id = mg_row.0;

    for (seed_order, entry) in entries.iter().enumerate() {
        sqlx::query(
            "INSERT INTO bracket_mini_group_teams \
             (mini_group_id, team_id, seed_order, source_match_id, source_mini_group_id) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(mini_group_id)
        .bind(entry.team_id())
        .bind(seed_order as i64)
        .bind(entry.source_match_id())
        .bind(entry.source_mini_group_id())
        .execute(&mut **tx)
        .await?;
    }

    let mut match_count = 0i64;
    let mut match_position = match_position_start;
    for left_index in 0..entries.len() {
        for right_index in (left_index + 1)..entries.len() {
            insert_bracket_match(
                tx,
                tournament_id,
                round_num,
                match_position,
                entries[left_index],
                entries[right_index],
                Some(mini_group_id),
                "winners",
                None,
                None,
            )
            .await?;
            match_position += 1;
            match_count += 1;
        }
    }

    Ok((mini_group_id, match_count))
}
