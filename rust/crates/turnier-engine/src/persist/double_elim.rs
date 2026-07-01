//! Double-Elimination-Bracket-Aufbau. Portiert `_build_double_elimination_bracket`.
//!
//! Verdrahtet Winners-Runden, Losers-Runden, die Loser-Drops, das Grand-Final +
//! Grand-Final-Reset und die Stream-Heuristik. Die reine Rundengrößen-/Drop-
//! Mathematik kommt aus [`crate::engine::double_elim`]; hier sitzt nur die
//! Persistenz. Grand-Final-Slot-Konvention (bug-preserved, jetzt explizit):
//! Slot 1 = Winners-Finalist, Slot 2 = Losers-Finalist.

use sqlx::{Postgres, Transaction};

use crate::engine::double_elim::{
    higher_winners_loser_drop, losers_round_size, total_losers_rounds, total_winners_rounds,
    wr1_loser_drop,
};
use crate::engine::seeding::{is_power_of_two, seed_slot_order};
use crate::engine::slots::BracketSlot;
use crate::error::{TournamentError, TournamentResult};

use super::bracket::{build_seeded_bracket, DoubleElimInput};
use super::insert_bracket_match;

/// Baut das Double-Elimination-Bracket. Liefert die Anzahl erzeugter Matches.
/// Fällt für Nicht-Power-of-2 / < 4 Teams auf Single-Elim zurück (wie im
/// Original, mit Warnung).
pub(crate) async fn build_double_elimination_bracket(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    input: DoubleElimInput,
) -> TournamentResult<i64> {
    // Round-One-Paare und die flache Fallback-Liste bestimmen.
    let (mut round_one_pairs, fallback_entries): (
        Vec<(BracketSlot, BracketSlot)>,
        Vec<BracketSlot>,
    ) = match input {
        DoubleElimInput::Pairs(pairs) => {
            if pairs.is_empty() {
                return Err(TournamentError::validation(
                    "Mindestens 2 Teams für Bracket benötigt",
                ));
            }
            let flat: Vec<BracketSlot> = pairs.iter().flat_map(|(l, r)| [*l, *r]).collect();
            (pairs, flat)
        }
        DoubleElimInput::Entries(entries) => {
            if entries.is_empty() {
                return Err(TournamentError::validation(
                    "Mindestens 2 Teams für Bracket benötigt",
                ));
            }
            (Vec::new(), entries)
        }
    };

    let num_teams = fallback_entries.len();
    if num_teams < 4 || !is_power_of_two(num_teams) {
        tracing::warn!(
            tournament_id,
            num_teams,
            "Double-Elimination nur für Power-of-2-Qualifier unterstützt, fallback auf Single-Elim"
        );
        return build_seeded_bracket(tx, tournament_id, fallback_entries).await;
    }

    if round_one_pairs.is_empty() {
        round_one_pairs = seeded_round_one_pairs(&fallback_entries);
    }

    let mut winners_rounds: Vec<Vec<i64>> = Vec::new();
    let mut losers_rounds: Vec<Vec<i64>> = Vec::new();
    let mut match_count = 0i64;

    // Winners-Runde 1.
    let mut winners_round_one: Vec<i64> = Vec::new();
    for (position, (left, right)) in round_one_pairs.iter().enumerate() {
        let match_id = insert_bracket_match(
            tx,
            tournament_id,
            1,
            position as i64,
            *left,
            *right,
            None,
            "winners",
            None,
            None,
        )
        .await?;
        winners_round_one.push(match_id);
        match_count += 1;
    }
    winners_rounds.push(winners_round_one);

    // Weitere Winners-Runden.
    let total_w = total_winners_rounds(num_teams);
    for winners_round_num in 2..=total_w {
        let previous = winners_rounds.last().unwrap().clone();
        let mut current: Vec<i64> = Vec::new();
        let mut position = 0;
        while position < previous.len() {
            let match_id = insert_bracket_match(
                tx,
                tournament_id,
                winners_round_num as i64,
                (position / 2) as i64,
                BracketSlot::FromMatch(previous[position]),
                BracketSlot::FromMatch(previous[position + 1]),
                None,
                "winners",
                None,
                None,
            )
            .await?;
            current.push(match_id);
            match_count += 1;
            position += 2;
        }
        winners_rounds.push(current);
    }

    // Losers-Runden.
    let total_l = total_losers_rounds(num_teams);
    for losers_round_num in 1..=total_l {
        let round_size = losers_round_size(num_teams, losers_round_num);
        let mut current: Vec<i64> = Vec::new();

        if losers_round_num == 1 {
            for position in 0..round_size {
                let match_id = insert_bracket_match(
                    tx,
                    tournament_id,
                    losers_round_num as i64,
                    position as i64,
                    BracketSlot::Empty,
                    BracketSlot::Empty,
                    None,
                    "losers",
                    None,
                    None,
                )
                .await?;
                current.push(match_id);
                match_count += 1;
            }
            losers_rounds.push(current);
            continue;
        }

        let previous = losers_rounds.last().unwrap().clone();
        if losers_round_num % 2 == 1 {
            for position in 0..round_size {
                let match_id = insert_bracket_match(
                    tx,
                    tournament_id,
                    losers_round_num as i64,
                    position as i64,
                    BracketSlot::FromMatch(previous[position * 2]),
                    BracketSlot::FromMatch(previous[(position * 2) + 1]),
                    None,
                    "losers",
                    None,
                    None,
                )
                .await?;
                current.push(match_id);
                match_count += 1;
            }
        } else {
            // Index-Form bewusst beibehalten (Parität mit dem ungeraden Zweig,
            // der `previous[position*2]` nutzt und nicht enumerierbar ist).
            #[allow(clippy::needless_range_loop)]
            for position in 0..round_size {
                let match_id = insert_bracket_match(
                    tx,
                    tournament_id,
                    losers_round_num as i64,
                    position as i64,
                    BracketSlot::FromMatch(previous[position]),
                    BracketSlot::Empty,
                    None,
                    "losers",
                    None,
                    None,
                )
                .await?;
                current.push(match_id);
                match_count += 1;
            }
        }
        losers_rounds.push(current);
    }

    // Loser-Drops aus Winners-Runde 1 -> Losers-Runde 1.
    for (index, winners_match_id) in winners_rounds[0].iter().enumerate() {
        let (dest_idx, slot) = wr1_loser_drop(index);
        let dest_match = losers_rounds[0][dest_idx];
        sqlx::query(
            "UPDATE turnier.bracket_matches \
             SET loser_to_match_id = $1, loser_to_slot = $2 WHERE id = $3",
        )
        .bind(dest_match)
        .bind(slot)
        .bind(winners_match_id)
        .execute(&mut **tx)
        .await?;
    }

    // Loser-Drops aus höheren Winners-Runden -> Losers-Runde (2*wr - 3), Slot 2.
    for winners_round_num in 2..=total_w {
        let destination_matches = &losers_rounds[(2 * winners_round_num) - 3];
        let destination_count = destination_matches.len();
        for (position, winners_match_id) in winners_rounds[winners_round_num - 1].iter().enumerate()
        {
            let dest_idx = higher_winners_loser_drop(position, destination_count);
            let dest_match = destination_matches[dest_idx];
            sqlx::query(
                "UPDATE turnier.bracket_matches \
                 SET loser_to_match_id = $1, loser_to_slot = 2 WHERE id = $2",
            )
            .bind(dest_match)
            .bind(winners_match_id)
            .execute(&mut **tx)
            .await?;
        }
    }

    // Grand-Final (Slot1 = Winners-Finalist, Slot2 = Losers-Finalist).
    let grand_final_round = (total_w + 1) as i64;
    insert_bracket_match(
        tx,
        tournament_id,
        grand_final_round,
        0,
        BracketSlot::FromMatch(*winners_rounds.last().unwrap().last().unwrap()),
        BracketSlot::FromMatch(*losers_rounds.last().unwrap().last().unwrap()),
        None,
        "grand_final",
        None,
        None,
    )
    .await?;
    let grand_final_reset_id = insert_bracket_match(
        tx,
        tournament_id,
        grand_final_round + 1,
        0,
        BracketSlot::Empty,
        BracketSlot::Empty,
        None,
        "grand_final",
        None,
        None,
    )
    .await?;
    match_count += 2;

    sqlx::query(
        "UPDATE turnier.bracket_matches \
         SET status = 'pending', team1_id = NULL, team2_id = NULL WHERE id = $1",
    )
    .bind(grand_final_reset_id)
    .execute(&mut **tx)
    .await?;

    // Stream-Heuristik: alle Losers-Runden AUSSER der letzten laufen off-stream.
    let mut off_stream_ids: Vec<i64> = Vec::new();
    for losers_round in losers_rounds
        .iter()
        .take(losers_rounds.len().saturating_sub(1))
    {
        off_stream_ids.extend(losers_round.iter().copied());
    }
    for match_id in &off_stream_ids {
        sqlx::query("UPDATE turnier.bracket_matches SET on_stream = false WHERE id = $1")
            .bind(match_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(match_count)
}

/// Standard-Seed-Paarung der ersten Runde aus einer flachen Seed-Liste.
/// Portiert `_seeded_round_one_pairs`.
fn seeded_round_one_pairs(entries: &[BracketSlot]) -> Vec<(BracketSlot, BracketSlot)> {
    let order = seed_slot_order(entries.len());
    let ordered: Vec<BracketSlot> = order.iter().map(|seed| entries[seed - 1]).collect();
    let mut pairs = Vec::new();
    let mut idx = 0;
    while idx < ordered.len() {
        pairs.push((ordered[idx], ordered[idx + 1]));
        idx += 2;
    }
    pairs
}
