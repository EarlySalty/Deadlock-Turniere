//! Auto-Lobby-Planung: erstellt automatisch Lobbys für spielbereite Matches.
//!
//! Portiert `match/auto_lobby.py`. Zwei Einstiege: turnierweit (nach Mini-Group-
//! Abschluss) und gezielt für Folge-Matches einer abgeschlossenen Bracket-Partie.
//! Beide sind über `tournaments.auto_lobby_enabled` und `is_test` gated.
//!
//! Der Zirkel-Import des Originals (`auto_lobby` ↔ `manager`) entfällt durch die
//! Crate-Schichtung: diese Methoden hängen am [`MatchManager`] und rufen direkt
//! `create_lobby`/`create_group_lobby` ([`crate::lobby`]).

use sqlx::Row;

use crate::error::MatchResult;
use crate::MatchManager;

impl MatchManager {
    /// Plant Auto-Lobbys für alle spielbereiten Bracket- und Group-Matches eines
    /// Turniers. Gated über `auto_lobby_enabled` & `is_test`. Pro Match wird ein
    /// Fehler nur geloggt (best-effort), nicht propagiert. Portiert
    /// `schedule_auto_lobbies_for_tournament`.
    pub async fn schedule_auto_lobbies_for_tournament(
        &self,
        tournament_id: i64,
    ) -> MatchResult<()> {
        if !self.auto_lobby_active(tournament_id).await? {
            return Ok(());
        }

        let bracket_ids: Vec<i64> = sqlx::query(
            "SELECT id FROM turnier.bracket_matches \
             WHERE tournament_id = $1 \
               AND team1_id IS NOT NULL AND team2_id IS NOT NULL \
               AND status IN ('pending', 'checkin') AND steam_party_id IS NULL \
             ORDER BY round, position, id",
        )
        .bind(tournament_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.get::<i64, _>("id"))
        .collect();

        let group_ids: Vec<i64> = sqlx::query(
            "SELECT gm.id FROM turnier.group_matches gm \
             JOIN turnier.groups g ON g.id = gm.group_id \
             WHERE g.tournament_id = $1 \
               AND gm.team1_id IS NOT NULL AND gm.team2_id IS NOT NULL \
               AND gm.status IN ('pending', 'checkin') AND gm.steam_party_id IS NULL \
             ORDER BY gm.id",
        )
        .bind(tournament_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.get::<i64, _>("id"))
        .collect();

        for match_id in bracket_ids {
            if let Err(err) = self.create_lobby(tournament_id, match_id).await {
                tracing::error!(
                    tournament_id, match_id, error = %err,
                    "Auto-Lobby für Bracket-Match fehlgeschlagen"
                );
            }
        }
        for match_id in group_ids {
            if let Err(err) = self.create_group_lobby(tournament_id, match_id).await {
                tracing::error!(
                    tournament_id, match_id, error = %err,
                    "Auto-Lobby für Group-Match fehlgeschlagen"
                );
            }
        }
        Ok(())
    }

    /// Plant Auto-Lobbys für die Folge-Matches eines abgeschlossenen Bracket-
    /// Matches. Gated wie oben. Portiert `schedule_auto_lobby_for_next_round`.
    pub async fn schedule_auto_lobby_for_next_round(
        &self,
        tournament_id: i64,
        completed_match_id: i64,
    ) -> MatchResult<()> {
        if !self.auto_lobby_active(tournament_id).await? {
            return Ok(());
        }

        let next_ids: Vec<i64> = sqlx::query(
            "SELECT id FROM turnier.bracket_matches \
             WHERE tournament_id = $1 \
               AND (source_match1_id = $2 OR source_match2_id = $3) \
               AND team1_id IS NOT NULL AND team2_id IS NOT NULL \
               AND status IN ('pending', 'checkin') AND steam_party_id IS NULL \
             ORDER BY round, position, id",
        )
        .bind(tournament_id)
        .bind(completed_match_id)
        .bind(completed_match_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|r| r.get::<i64, _>("id"))
        .collect();

        for match_id in next_ids {
            if let Err(err) = self.create_lobby(tournament_id, match_id).await {
                tracing::error!(
                    tournament_id, completed_match_id, next_match = match_id, error = %err,
                    "Auto-Lobby für Folge-Match fehlgeschlagen"
                );
            }
        }
        Ok(())
    }

    /// `true`, wenn das Turnier existiert, kein Test ist und `auto_lobby_enabled`
    /// gesetzt hat. Entspricht dem gemeinsamen Gate beider Funktionen.
    async fn auto_lobby_active(&self, tournament_id: i64) -> MatchResult<bool> {
        let row = sqlx::query(
            "SELECT auto_lobby_enabled, is_test FROM turnier.tournaments WHERE id = $1",
        )
        .bind(tournament_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let is_test = row.get::<bool, _>("is_test");
        let enabled = row.get::<bool, _>("auto_lobby_enabled");
        Ok(!is_test && enabled)
    }
}
