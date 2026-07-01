//! Punkte-Recompute für die globale Rangliste (Persistenz).
//!
//! Idempotenz-Strategie (bewusste strukturelle Abweichung vom Original, siehe
//! algorithm_notes): Statt additiv in bestehende `player_points`-Zeilen zu
//! akkumulieren — was das Original bei Mehrfach-`completed` doppelt zählte —
//! rechnet diese Funktion die GANZE `player_points`-Tabelle deterministisch aus
//! ALLEN abgeschlossenen, nicht ausgeschlossenen Turnieren neu auf.
//!
//! Für den im Original üblichen Fall (genau ein gerade abgeschlossenes Turnier)
//! sind die resultierenden Zahlen identisch zum additiven Lauf; mehrere
//! Turniere summieren sich genauso, aber ein erneuter Aufruf ändert nichts mehr
//! (idempotent). Die Punkte-/Platzierungs-WERTE je Turnier stammen 1:1 aus
//! [`crate::points`] (inkl. der bug-preserved SE-Round-Heuristik und des
//! globalen `matches_played`).

use std::collections::HashMap;

use sqlx::{Pool, Postgres, Transaction};
use turnier_core::now_utc;

use crate::error::TournamentResult;
use crate::points::{
    contribution_for_team, team_placements, team_wins, CompletedMatch, PointsContribution,
};

// ASCII tag "trnpnts1" as a signed i64. Serializes the global player_points
// DELETE+INSERT recompute across all tournaments for one transaction.
const POINTS_RECOMPUTE_ADVISORY_LOCK_KEY: i64 = i64::from_be_bytes(*b"trnpnts1");

#[derive(sqlx::FromRow)]
struct MatchRow {
    round: i64,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
    winner_id: Option<i64>,
}

/// Aggregierter Stand eines Spielers über alle gewerteten Turniere.
#[derive(Default)]
struct PlayerAggregate {
    total_points: i64,
    tournaments_played: i64,
    matches_played: i64,
    matches_won: i64,
    best_placement: Option<i64>,
}

impl PlayerAggregate {
    fn add(&mut self, c: PointsContribution) {
        self.total_points += c.points;
        self.tournaments_played += 1;
        self.matches_played += c.matches_played;
        self.matches_won += c.matches_won;
        self.best_placement = match (self.best_placement, c.placement) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
    }
}

/// Rechnet die globale Rangliste neu auf. Das Argument `tournament_id` bleibt aus
/// Signatur-Parität erhalten; der Recompute selbst ist turnier-übergreifend und
/// damit idempotent. Eigene Transaktion.
pub async fn recalculate_player_points(
    pool: &Pool<Postgres>,
    tournament_id: i64,
) -> TournamentResult<()> {
    let mut tx = pool.begin().await?;
    recalculate_player_points_in_tx(&mut tx, tournament_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Transaktionsfähige Variante von [`recalculate_player_points`].
///
/// Der Aufrufer besitzt Commit/Rollback. So können Statuswechsel, Audit und
/// Punkte-Recompute wieder wie im Python-Original atomar zusammenlaufen.
pub async fn recalculate_player_points_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    _tournament_id: i64,
) -> TournamentResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(POINTS_RECOMPUTE_ADVISORY_LOCK_KEY)
        .execute(&mut **tx)
        .await?;

    // Alle gewerteten Turniere: abgeschlossen UND nicht von der Rangliste
    // ausgeschlossen (der Original-Caller filtert exclude_from_leaderboard vor
    // dem Recalc; hier zentral).
    let tournaments: Vec<(i64,)> = sqlx::query_as(
        "SELECT id FROM turnier.tournaments \
         WHERE status = 'completed' AND exclude_from_leaderboard = false \
         ORDER BY id",
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut aggregates: HashMap<i64, PlayerAggregate> = HashMap::new();

    for (tid,) in &tournaments {
        // Abgeschlossene Bracket-Matches, round DESC (für die Platzierungs-Heuristik).
        let raw_matches: Vec<MatchRow> = sqlx::query_as::<_, MatchRow>(
            "SELECT round, team1_id, team2_id, winner_id FROM turnier.bracket_matches \
             WHERE tournament_id = $1 AND status = 'completed' ORDER BY round DESC, id",
        )
        .bind(tid)
        .fetch_all(&mut **tx)
        .await?;
        let matches: Vec<CompletedMatch> = raw_matches
            .iter()
            .map(|r| CompletedMatch {
                round: r.round,
                team1_id: r.team1_id,
                team2_id: r.team2_id,
                winner_id: r.winner_id,
            })
            .collect();

        let placements = team_placements(&matches);
        let wins = team_wins(&matches);
        let total_completed = matches.len() as i64;

        // Teilnehmer: Mitglieder aller Teams dieses Turniers.
        let participants: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT tm.discord_id, t.id FROM turnier.team_members tm \
             JOIN turnier.teams t ON tm.team_id = t.id \
             WHERE t.tournament_id = $1 ORDER BY t.id, tm.joined_at, tm.id",
        )
        .bind(tid)
        .fetch_all(&mut **tx)
        .await?;

        for (discord_id, team_id) in participants {
            let contribution = contribution_for_team(team_id, &placements, &wins, total_completed);
            aggregates.entry(discord_id).or_default().add(contribution);
        }
    }

    // Tabelle vollständig neu schreiben (idempotent).
    let now = now_utc();
    sqlx::query("DELETE FROM turnier.player_points")
        .execute(&mut **tx)
        .await?;
    for (discord_id, agg) in &aggregates {
        sqlx::query(
            "INSERT INTO turnier.player_points \
             (discord_id, total_points, tournaments_played, matches_played, matches_won, best_placement, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(discord_id)
        .bind(agg.total_points)
        .bind(agg.tournaments_played)
        .bind(agg.matches_played)
        .bind(agg.matches_won)
        .bind(agg.best_placement)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}
