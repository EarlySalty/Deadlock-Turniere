//! Single-Elimination-Bracket-Aufbau + `generate_bracket`-Orchestrierung.
//!
//! Portiert `generate_bracket`, `_build_seeded_bracket`, `_build_bracket_round`,
//! `_build_paired_bracket`, `_insert_mini_group` und `_build_group_cross_seed_pairs`.
//! Die rekursiven Runden-Builder sind als `Box::pin`-Futures umgesetzt
//! (async-Rekursion).

use std::future::Future;
use std::pin::Pin;

use sqlx::{Pool, Postgres, Transaction};
use turnier_core::BracketFormat;

use crate::engine::double_elim::MIN_BRACKET_ENTRANTS;
use crate::engine::groups::QUALIFIERS_PER_GROUP;
use crate::engine::seeding::{
    distribute_entries_across_slots, is_power_of_two, seed_slot_order, slot_sizes_for_round,
};
use crate::engine::slots::BracketSlot;
use crate::error::{TournamentError, TournamentResult};
use crate::league::LeagueGame;
use crate::standings::{rank_standings, Standing};

use super::double_elim::build_double_elimination_bracket;
use super::{clear_bracket_tree, insert_bracket_match};

/// Ein Qualifikant aus der Gruppenphase (oder Fallback: alle Teams).
struct Qualifier {
    team_id: i64,
    points: i64,
    wins: i64,
    seed: i64,
    origin_order: i64,
}

/// Generiert das Bracket aus den Gruppen-Standings (Top 2/Gruppe) oder (ohne
/// Gruppen) aus allen Teams. Liefert die Anzahl erzeugter Matches.
///
/// Portiert `generate_bracket(tournament_id, bracket_format)`: Das übergebene
/// `bracket_format`-Argument wird (wie im Original) ignoriert und stattdessen aus
/// der `tournaments`-Zeile gelesen.
pub async fn generate_bracket(pool: &Pool<Postgres>, tournament_id: i64) -> TournamentResult<i64> {
    let mut tx = pool.begin().await?;
    let match_count = generate_bracket_in_tx(&mut tx, tournament_id).await?;
    tx.commit().await?;
    Ok(match_count)
}

async fn ranked_group_standings(
    tx: &mut Transaction<'_, Postgres>,
    group_id: i64,
) -> TournamentResult<Vec<Standing>> {
    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT team_id, wins, points FROM turnier.group_teams WHERE group_id = $1 ORDER BY id",
    )
    .bind(group_id)
    .fetch_all(&mut **tx)
    .await?;
    let seeded: Vec<Standing> = rows
        .into_iter()
        .map(|(team_id, wins, points)| Standing {
            team_id,
            wins,
            points,
        })
        .collect();
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
    let ranked = rank_standings(&seeded, &games);
    Ok(ranked
        .iter()
        .filter_map(|row| seeded.iter().find(|s| s.team_id == row.team_id).copied())
        .collect())
}

/// Transaktionsfähige Variante von [`generate_bracket`].
///
/// Der Aufrufer besitzt Commit/Rollback. Genutzt für Admin-Updates, bei denen
/// Löschen, Rebuild, Statuswechsel und Audit atomar zusammengehören.
pub async fn generate_bracket_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
) -> TournamentResult<i64> {
    clear_bracket_tree(tx, tournament_id).await?;

    let format_row: Option<(BracketFormat,)> =
        sqlx::query_as("SELECT bracket_format FROM turnier.tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(&mut **tx)
            .await?;
    let bracket_format = format_row
        .map(|r| r.0)
        .unwrap_or(BracketFormat::SingleElimination);

    // Gruppen (nach seeding_order) und je Gruppe die Top-2-Standings.
    let group_ids: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM turnier.groups WHERE tournament_id = $1 ORDER BY seeding_order, id",
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut qualified: Vec<Qualifier> = Vec::new();
    let mut grouped_qualifiers: Vec<Vec<Qualifier>> = Vec::new();
    for (group_id,) in &group_ids {
        let standings = ranked_group_standings(tx, *group_id).await?;
        let mut group_qs: Vec<Qualifier> = Vec::new();
        for (
            rank_pos,
            Standing {
                team_id,
                wins,
                points,
            },
        ) in standings
            .iter()
            .take(crate::engine::groups::QUALIFIERS_PER_GROUP)
            .enumerate()
        {
            qualified.push(Qualifier {
                team_id: *team_id,
                points: *points,
                wins: *wins,
                seed: rank_pos as i64,
                origin_order: qualified.len() as i64,
            });
            group_qs.push(Qualifier {
                team_id: *team_id,
                points: *points,
                wins: *wins,
                seed: rank_pos as i64,
                origin_order: group_qs.len() as i64,
            });
        }
        grouped_qualifiers.push(group_qs);
    }

    // Fallback ohne Gruppenphase: alle Teams in DB-Reihenfolge.
    if qualified.is_empty() {
        let all_teams: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM turnier.teams WHERE tournament_id = $1 ORDER BY created_at, id",
        )
        .bind(tournament_id)
        .fetch_all(&mut **tx)
        .await?;
        qualified = all_teams
            .into_iter()
            .enumerate()
            .map(|(i, (id,))| Qualifier {
                team_id: id,
                points: 0,
                wins: 0,
                seed: i as i64,
                origin_order: i as i64,
            })
            .collect();
    }

    ensure_bracket_entrants(qualified.len())?;

    let cross_seed_pairs = build_group_cross_seed_pairs(&grouped_qualifiers);

    let match_count = if bracket_format == BracketFormat::DoubleElimination {
        let pairs_or_entries = match cross_seed_pairs {
            Some(pairs) => DoubleElimInput::Pairs(pairs),
            None => {
                let entries = sort_and_map_entries(qualified);
                DoubleElimInput::Entries(entries)
            }
        };
        build_double_elimination_bracket(tx, tournament_id, pairs_or_entries).await?
    } else if let Some(pairs) = cross_seed_pairs {
        build_paired_bracket(tx, tournament_id, pairs, 1, "winners").await?
    } else {
        let entries = sort_and_map_entries(qualified);
        build_seeded_bracket(tx, tournament_id, entries).await?
    };

    Ok(match_count)
}

/// Sortiert die Qualifikanten (`-points, -wins, seed`, urspruengliche Reihenfolge)
/// und mappt sie auf Team-Slots. Portiert die zweifach duplizierte
/// Sortier-/Map-Logik einmalig.
fn sort_and_map_entries(mut qualified: Vec<Qualifier>) -> Vec<BracketSlot> {
    qualified.sort_by_key(|q| (-q.points, -q.wins, q.seed, q.origin_order, q.team_id));
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

pub(crate) fn ensure_bracket_entrants(count: usize) -> TournamentResult<()> {
    if count < MIN_BRACKET_ENTRANTS {
        return Err(TournamentError::validation(format!(
            "Mindestens {MIN_BRACKET_ENTRANTS} Teams für Bracket benötigt"
        )));
    }
    Ok(())
}

pub const PLAYED_BRACKET_FORMAT_SQL: &str = r#"(SELECT CASE
        WHEN count(*) = 0 THEN NULL
        WHEN bool_or(bm.bracket_type IN ('losers', 'grand_final')) THEN 'double_elimination'
        ELSE 'single_elimination' END
    FROM turnier.bracket_matches bm WHERE bm.tournament_id = turnier."tournaments".id)"#;

/// Cross-Seed-Paare aus den Gruppen-Qualifikanten: Sieger Gruppe i vs Zweiter
/// Gruppe i+1 (zyklisch). Nur wenn jede Gruppe >= 2 Qualifikanten hat UND die
/// Gesamtzahl eine Zweierpotenz ist. Portiert `_build_group_cross_seed_pairs`.
fn build_group_cross_seed_pairs(
    grouped: &[Vec<Qualifier>],
) -> Option<Vec<(BracketSlot, BracketSlot)>> {
    if grouped.is_empty() {
        return None;
    }
    if grouped.iter().any(|g| g.len() < QUALIFIERS_PER_GROUP) {
        return None;
    }
    let qualifier_count = grouped.len() * QUALIFIERS_PER_GROUP;
    if !is_power_of_two(qualifier_count) {
        return None;
    }
    let group_count = grouped.len();
    let mut pairs = Vec::with_capacity(group_count);
    for (index, group) in grouped.iter().enumerate() {
        let next_group = &grouped[(index + 1) % group_count];
        pairs.push((
            BracketSlot::Team(group[0].team_id),
            BracketSlot::Team(next_group[QUALIFIERS_PER_GROUP - 1].team_id),
        ));
    }
    Some(pairs)
}

/// Baut ein Single-Elimination-Bracket aus einer Seed-Liste. Portiert
/// `_build_seeded_bracket`.
pub(crate) async fn build_seeded_bracket(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    entries: Vec<BracketSlot>,
) -> TournamentResult<i64> {
    ensure_bracket_entrants(entries.len())?;
    build_bracket_round(tx, tournament_id, entries, 1).await
}

/// Baut ein Paar-basiertes Bracket (Cross-Seed-Runde 1), dann rekursiv weiter.
/// Portiert `_build_paired_bracket`.
async fn build_paired_bracket(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    pairs: Vec<(BracketSlot, BracketSlot)>,
    current_round: i64,
    bracket_type: &str,
) -> TournamentResult<i64> {
    ensure_bracket_entrants(pairs.len() * 2)?;
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
    Ok(match_count
        + build_bracket_round(tx, tournament_id, next_entries, current_round + 1).await?)
}

/// Eine Bracket-Runde: bei Zweierpotenz Standard-Seed-Paarung, sonst Mini-Group-
/// Slots. Rekursiv bis zum Finale. Portiert `_build_bracket_round`.
fn build_bracket_round<'a>(
    tx: &'a mut Transaction<'_, Postgres>,
    tournament_id: i64,
    entries: Vec<BracketSlot>,
    current_round: i64,
) -> Pin<Box<dyn Future<Output = TournamentResult<i64>> + Send + 'a>> {
    Box::pin(async move {
        ensure_bracket_entrants(entries.len())?;

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
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    round_num: i64,
    position: i64,
    match_position_start: i64,
    entries: &[BracketSlot],
) -> TournamentResult<(i64, i64)> {
    let mg_row: (i64,) = sqlx::query_as(
        "INSERT INTO turnier.bracket_mini_groups \
         (tournament_id, round, position, advances_to_match_id, advances_to_slot, created_at) \
         VALUES ($1, $2, $3, NULL, NULL, $4) RETURNING id",
    )
    .bind(tournament_id)
    .bind(round_num)
    .bind(position)
    .bind(turnier_core::now_utc())
    .fetch_one(&mut **tx)
    .await?;
    let mini_group_id = mg_row.0;

    for (seed_order, entry) in entries.iter().enumerate() {
        sqlx::query(
            "INSERT INTO turnier.bracket_mini_group_teams \
             (mini_group_id, team_id, seed_order, source_match_id, source_mini_group_id) \
             VALUES ($1, $2, $3, $4, $5)",
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
