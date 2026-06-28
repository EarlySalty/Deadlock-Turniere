//! Best-of-N-Serien-Logik für Bracket-Matches (Bo1/Bo3/Bo5).
//!
//! Portiert `match/series_manager.py`. Verwaltet `match_games` je Serie: Spiel N
//! sicherstellen, Ergebnis eintragen, Serien-Format ermitteln (Finale nutzt
//! `final_series_format`, sonst `series_format`) und prüfen, ob die Serie
//! entschieden ist (`wins_needed = series_format / 2 + 1`).
//!
//! Safe-Fix (Befund series_manager.py:11-31/63-72): `record_game_result` nutzt
//! [`ensure_game_exists`] wieder, statt die Insert-Logik zu duplizieren.

use serde_json::Value;
use sqlx::Row;

use turnier_db::Pool;

use crate::error::MatchError;

/// Aktueller UTC-Zeitstempel im ISO-8601-Format (entspricht
/// `datetime.now(timezone.utc).isoformat()`).
fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
}

/// Stellt sicher, dass Spiel `game_number` der Serie existiert; gibt `game.id`
/// zurück. Idempotent (SELECT, sonst INSERT). Portiert `ensure_game_exists`.
pub async fn ensure_game_exists(
    pool: &Pool,
    bracket_match_id: i64,
    game_number: i64,
) -> Result<i64, MatchError> {
    let now = now_iso();
    let existing =
        sqlx::query("SELECT id FROM match_games WHERE bracket_match_id = ? AND game_number = ?")
            .bind(bracket_match_id)
            .bind(game_number)
            .fetch_optional(pool)
            .await?;
    if let Some(row) = existing {
        return Ok(row.get::<i64, _>("id"));
    }
    let result = sqlx::query(
        "INSERT INTO match_games (bracket_match_id, game_number, status, created_at) \
         VALUES (?, ?, 'pending', ?)",
    )
    .bind(bracket_match_id)
    .bind(game_number)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Ergebnis von [`record_game_result`] (Wire-Form `{series_done,
/// series_winner_team, wins_team1, wins_team2, next_game_number}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameResultOutcome {
    pub series_done: bool,
    pub series_winner_team: Option<i64>,
    pub wins_team1: i64,
    pub wins_team2: i64,
    pub next_game_number: Option<i64>,
}

impl GameResultOutcome {
    /// JSON-Repräsentation, identisch zum Python-Rückgabe-Dict.
    pub fn to_value(&self) -> Value {
        serde_json::json!({
            "series_done": self.series_done,
            "series_winner_team": self.series_winner_team,
            "wins_team1": self.wins_team1,
            "wins_team2": self.wins_team2,
            "next_game_number": self.next_game_number,
        })
    }
}

/// Eingabe-Stats für ein Spielergebnis (optional).
#[derive(Debug, Clone, Default)]
pub struct GameStats<'a> {
    pub steam_party_id: Option<&'a str>,
    pub deadlock_match_id: Option<&'a str>,
    pub duration_s: Option<i64>,
    pub match_stats: Option<&'a Value>,
}

/// Trägt das Ergebnis für Spiel `game_number` ein und prüft, ob die Serie
/// entschieden ist. `winner_team` muss 1 oder 2 sein. Portiert
/// `record_game_result`.
///
/// Bug-preserved (Befund series_manager.py:108-132): `series_format` wird ohne
/// Null-Guard als ganze Zahl gelesen. Die Spalte ist `NOT NULL DEFAULT 1`, daher
/// in der Praxis nie NULL — ein dennoch NULL-er Wert führte im Original zu einem
/// `TypeError`; hier scheitert die Dekodierung analog (kein stiller Default).
pub async fn record_game_result(
    pool: &Pool,
    bracket_match_id: i64,
    game_number: i64,
    winner_team: i64,
    stats: &GameStats<'_>,
) -> Result<GameResultOutcome, MatchError> {
    if winner_team != 1 && winner_team != 2 {
        return Err(MatchError::invalid("winner_team muss 1 oder 2 sein"));
    }

    let now = now_iso();
    let game_id = ensure_game_exists(pool, bracket_match_id, game_number).await?;

    let match_stats_json = match stats.match_stats {
        Some(v) => Some(serde_json::to_string(v).map_err(|_| {
            MatchError::invalid("match_stats enthält nicht serialisierbare Daten")
        })?),
        None => None,
    };

    sqlx::query(
        "UPDATE match_games \
         SET winner_team = ?, \
             steam_party_id = COALESCE(?, steam_party_id), \
             deadlock_match_id = COALESCE(?, deadlock_match_id), \
             duration_s = COALESCE(?, duration_s), \
             match_stats = COALESCE(?, match_stats), \
             status = 'completed', \
             completed_at = ? \
         WHERE id = ?",
    )
    .bind(winner_team)
    .bind(stats.steam_party_id)
    .bind(stats.deadlock_match_id)
    .bind(stats.duration_s)
    .bind(match_stats_json)
    .bind(&now)
    .bind(game_id)
    .execute(pool)
    .await?;

    // Alle abgeschlossenen Spiele dieser Serie.
    let completed_rows = sqlx::query(
        "SELECT winner_team FROM match_games \
         WHERE bracket_match_id = ? AND status = 'completed'",
    )
    .bind(bracket_match_id)
    .fetch_all(pool)
    .await?;
    let wins1 = completed_rows
        .iter()
        .filter(|r| r.get::<Option<i64>, _>("winner_team") == Some(1))
        .count() as i64;
    let wins2 = completed_rows
        .iter()
        .filter(|r| r.get::<Option<i64>, _>("winner_team") == Some(2))
        .count() as i64;

    // Serien-Format ermitteln (Finale → final_series_format, sonst series_format).
    let fmt_row = sqlx::query(
        "SELECT t.series_format AS series_format, t.final_series_format AS final_series_format, \
                bm.bracket_type AS bracket_type, bm.round AS round, \
                (SELECT MAX(round) FROM bracket_matches \
                 WHERE tournament_id = t.id AND bracket_type = 'winners') AS max_winners_round \
         FROM tournaments t \
         JOIN bracket_matches bm ON bm.tournament_id = t.id \
         WHERE bm.id = ?",
    )
    .bind(bracket_match_id)
    .fetch_optional(pool)
    .await?;

    let series_format = if let Some(row) = fmt_row {
        let bracket_type: String = row.get("bracket_type");
        let round: i64 = row.get("round");
        let max_winners_round: Option<i64> = row.get("max_winners_round");
        let is_final = bracket_type == "grand_final"
            || (bracket_type == "winners" && Some(round) == max_winners_round);
        let final_fmt: Option<i64> = row.get("final_series_format");
        // Bug-preserved: kein Null-Guard auf series_format (NOT NULL DEFAULT 1).
        let base_fmt: i64 = row.get("series_format");
        if is_final {
            final_fmt.unwrap_or(base_fmt)
        } else {
            base_fmt
        }
    } else {
        1
    };

    let wins_needed = series_format / 2 + 1;
    let series_done = wins1 >= wins_needed || wins2 >= wins_needed;
    let series_winner = if wins1 >= wins_needed {
        Some(1)
    } else if wins2 >= wins_needed {
        Some(2)
    } else {
        None
    };

    Ok(GameResultOutcome {
        series_done,
        series_winner_team: series_winner,
        wins_team1: wins1,
        wins_team2: wins2,
        next_game_number: if series_done { None } else { Some(game_number + 1) },
    })
}

/// Alle Spiele einer Serie (`ORDER BY game_number`). Portiert `get_series_games`.
pub async fn get_series_games(
    pool: &Pool,
    bracket_match_id: i64,
) -> Result<Vec<turnier_core::MatchGame>, MatchError> {
    let rows = sqlx::query(
        "SELECT id, bracket_match_id, game_number, status, steam_party_id, party_code, \
                deadlock_match_id, winner_team, duration_s, match_stats, created_at, completed_at \
         FROM match_games WHERE bracket_match_id = ? ORDER BY game_number",
    )
    .bind(bracket_match_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let match_stats: Option<String> = r.get("match_stats");
            turnier_core::MatchGame {
                id: r.get("id"),
                bracket_match_id: r.get("bracket_match_id"),
                game_number: r.get("game_number"),
                status: r.get("status"),
                steam_party_id: r.get("steam_party_id"),
                party_code: r.get("party_code"),
                deadlock_match_id: r.get("deadlock_match_id"),
                winner_team: r.get("winner_team"),
                duration_s: r.get("duration_s"),
                match_stats: match_stats.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: r.get("created_at"),
                completed_at: r.get("completed_at"),
            }
        })
        .collect())
}

/// Berechnet `wins_needed` aus dem Serien-Format (`format / 2 + 1`).
/// Ausgelagert für DB-freie Unit-Tests.
pub fn wins_needed(series_format: i64) -> i64 {
    series_format / 2 + 1
}

impl crate::MatchManager {
    /// Stellt Spiel `game_number` einer Serie sicher; gibt `game.id` zurück.
    /// Façade über [`ensure_game_exists`].
    pub async fn ensure_game_exists(
        &self,
        bracket_match_id: i64,
        game_number: i64,
    ) -> Result<i64, MatchError> {
        ensure_game_exists(&self.pool, bracket_match_id, game_number).await
    }

    /// Trägt das Ergebnis für Spiel `game_number` ein und meldet, ob die Serie
    /// entschieden ist. Façade über [`record_game_result`].
    pub async fn record_game_result(
        &self,
        bracket_match_id: i64,
        game_number: i64,
        winner_team: i64,
        stats: &GameStats<'_>,
    ) -> Result<GameResultOutcome, MatchError> {
        record_game_result(&self.pool, bracket_match_id, game_number, winner_team, stats).await
    }

    /// Liefert alle Spiele einer Serie. Façade über [`get_series_games`].
    pub async fn get_series_games(
        &self,
        bracket_match_id: i64,
    ) -> Result<Vec<turnier_core::MatchGame>, MatchError> {
        get_series_games(&self.pool, bracket_match_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wins_needed_pro_format() {
        assert_eq!(wins_needed(1), 1); // Bo1
        assert_eq!(wins_needed(3), 2); // Bo3
        assert_eq!(wins_needed(5), 3); // Bo5
        assert_eq!(wins_needed(7), 4); // Bo7
    }
}
