//! Persistenz-Schicht der Turnier-Engine (sqlx).
//!
//! Jede öffentliche Operation läuft in EINER `sqlx::Transaction` — anders als das
//! Python-Original, das `_audit` mitten in offenen Transaktionen committen ließ
//! (siehe bugs_preserved/Port-Notes). Die Bracket-Verdrahtung nutzt die reinen
//! Algorithmen aus [`crate::engine`]; die echten Match-IDs kommen aus der DB
//! (AUTOINCREMENT), weshalb der Aufbau hier — nicht im reinen Layer — sitzt.
//!
//! Laufzeit-geprüfte Queries (`sqlx::query`/`query_as`), keine `query!`-Makros.

mod advance;
mod bracket;
mod checkin;
mod double_elim;
mod groups;
mod mini_group_complete;
mod points;

pub use advance::advance_bracket_winner;
pub use bracket::{generate_bracket, generate_bracket_in_tx};
pub use checkin::{
    assign_random_teams, build_checkin_snapshot_token, finalize_checkin, AddedPlayer, CreatedTeam,
    FinalizeCheckinParams, FinalizeCheckinResult, NoShuffle, RemovedPlayer, RngShuffler,
    SoloPlayer, SoloShuffler, TeamWarning,
};
pub use groups::{
    generate_group_matches, generate_group_matches_in_tx, generate_groups, generate_groups_in_tx,
};
pub use mini_group_complete::complete_mini_group_round_robin;
pub use points::{recalculate_player_points, recalculate_player_points_in_tx};

use serde_json::Value;
use sqlx::{Postgres, Transaction};
use turnier_core::{now_utc, parse_discord_id};

use crate::engine::slots::BracketSlot;
use crate::error::{TournamentError, TournamentResult};

pub(crate) fn parse_db_discord_id(value: &str) -> TournamentResult<i64> {
    parse_discord_id(value).map_err(|err| TournamentError::validation(err.to_string()))
}

/// Fügt ein Bracket-Match ein und verdrahtet bei Mini-Group-Quellen das
/// `advances_to_*` der Quell-Mini-Group. Liefert die neue Match-ID.
///
/// Portiert `_insert_bracket_match` 1:1 (gleiche Spalten, gleiche Reihenfolge,
/// Status `'pending'`). `slot1`/`slot2` projizieren je nach Variante team_id,
/// source_match_id oder source_mini_group_id in die richtigen Spalten.
#[allow(clippy::too_many_arguments)]
async fn insert_bracket_match(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
    round_num: i64,
    position: i64,
    slot1: BracketSlot,
    slot2: BracketSlot,
    mini_group_id: Option<i64>,
    bracket_type: &str,
    loser_to_match_id: Option<i64>,
    loser_to_slot: Option<i64>,
) -> TournamentResult<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO turnier.bracket_matches \
         (tournament_id, round, position, bracket_type, mini_group_id, \
          team1_id, team2_id, source_match1_id, source_match2_id, loser_to_match_id, loser_to_slot, \
          source_mini_group1_id, source_mini_group2_id, status, on_stream) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'pending', true) \
         RETURNING id",
    )
    .bind(tournament_id)
    .bind(round_num)
    .bind(position)
    .bind(bracket_type)
    .bind(mini_group_id)
    .bind(slot1.team_id())
    .bind(slot2.team_id())
    .bind(slot1.source_match_id())
    .bind(slot2.source_match_id())
    .bind(loser_to_match_id)
    .bind(loser_to_slot)
    .bind(slot1.source_mini_group_id())
    .bind(slot2.source_mini_group_id())
    .fetch_one(&mut **tx)
    .await?;
    let match_id = row.0;

    // Verdrahtung: Wenn dieser Slot Sieger einer Mini-Group ist (und das Match
    // selbst nicht TEIL einer Mini-Group ist), trägt die Mini-Group ihr Ziel ein.
    if mini_group_id.is_none() {
        if let Some(mg) = slot1.source_mini_group_id() {
            sqlx::query(
                "UPDATE turnier.bracket_mini_groups \
                 SET advances_to_match_id = $1, advances_to_slot = 1 WHERE id = $2",
            )
            .bind(match_id)
            .bind(mg)
            .execute(&mut **tx)
            .await?;
        }
        if let Some(mg) = slot2.source_mini_group_id() {
            sqlx::query(
                "UPDATE turnier.bracket_mini_groups \
                 SET advances_to_match_id = $1, advances_to_slot = 2 WHERE id = $2",
            )
            .bind(match_id)
            .bind(mg)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(match_id)
}

/// Leert den Bracket-Baum eines Turniers (Matches + Mini-Groups) und trennt
/// deren Verdrahtung. Portiert `_clear_bracket_tree`.
async fn clear_bracket_tree(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
) -> TournamentResult<()> {
    sqlx::query(
        "UPDATE turnier.bracket_mini_groups SET advances_to_match_id = NULL \
         WHERE tournament_id = $1",
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE turnier.bracket_mini_group_teams SET source_match_id = NULL \
         WHERE mini_group_id IN \
             (SELECT id FROM turnier.bracket_mini_groups WHERE tournament_id = $1)",
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM turnier.bracket_matches WHERE tournament_id = $1")
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;

    let mini_group_ids: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM turnier.bracket_mini_groups WHERE tournament_id = $1")
            .bind(tournament_id)
            .fetch_all(&mut **tx)
            .await?;
    if !mini_group_ids.is_empty() {
        // Einzeln löschen — vermeidet dynamische IN-Platzhalter und ist für die
        // hier üblichen kleinen Mengen unkritisch.
        for (mg_id,) in &mini_group_ids {
            sqlx::query("DELETE FROM turnier.bracket_mini_group_teams WHERE mini_group_id = $1")
                .bind(mg_id)
                .execute(&mut **tx)
                .await?;
        }
        sqlx::query("DELETE FROM turnier.bracket_mini_groups WHERE tournament_id = $1")
            .bind(tournament_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Schreibt einen Audit-Log-Eintrag in DERSELBEN Transaktion (kein eigenes
/// Commit — das ist der bewusste Unterschied zum Original-`_audit`).
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    action: &str,
    user_id: Option<&str>,
    details: &str,
) -> TournamentResult<()> {
    let user_id = user_id.map(parse_db_discord_id).transpose()?;
    let details: Value =
        serde_json::from_str(details).unwrap_or_else(|_| Value::String(details.to_string()));
    sqlx::query(
        "INSERT INTO turnier.audit_log (action, user_id, details, created_at) \
         VALUES ($1, $2, $3::jsonb, $4)",
    )
    .bind(action)
    .bind(user_id)
    .bind(details)
    .bind(now_utc())
    .execute(&mut **tx)
    .await?;
    Ok(())
}
