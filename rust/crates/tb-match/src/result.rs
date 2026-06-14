//! Ergebnisverarbeitung — Bracket und Group VEREINHEITLICHT.
//!
//! Bündelt, was im Original auf `result_processor.apply_bracket_match_result`
//! und `manager._apply_group_match_result` verteilt war: beide Pfade liegen hier
//! NEBENEINANDER. Der Bracket-Pfad persistiert das Ergebnis, propagiert den
//! Gewinner (über [`tb_tournament::advance_bracket_winner`]), schließt ggf. die
//! Mini-Group ab, triggert die Auto-Lobby für Folge-Runden und postet
//! Stats/Channel-Cleanup nach Discord. Der Group-Pfad aktualisiert zusätzlich die
//! `group_teams`-Tabelle.
//!
//! ## Bewusst erhaltene Befunde (1:1)
//! - `match_results.winning_team`-Spalte: der Bracket-Pfad schreibt die
//!   **winner_id** (Team-PK), der Group-Pfad einen **Slot** (1/2). Diese
//!   uneinheitliche Semantik (Befund result_processor.py:162 / manager.py:955)
//!   wird NICHT vereinheitlicht (needs-decision).
//! - `winning_team`-Konvention: Bracket 0-basiert (0=team1, 1=team2), Group
//!   1-basiert (1/2) — beide Konventionen bleiben (needs-decision).
//! - Status-Guard: der redundante zweite NOT-IN-Teil (Befund
//!   result_processor.py:73-81) bleibt erhalten (behavior-change).
//! - `_reset_bracket_downstream`: rekursiv OHNE Zyklus-/Tiefenschutz (Befund
//!   result_processor.py:252-283) bleibt erhalten (behavior-change).
//! - `discord_channel_id` wird aus dem Pre-Update-Snapshot gelesen (Befund
//!   result_processor.py:166) — bewusst erhalten.

use serde_json::Value;
use sqlx::Row;

use tb_discord::PlayerStat;

use crate::error::{MatchError, MatchResult};
use crate::MatchManager;

/// Terminal-Status, aus denen ohne `force` nicht verarbeitet werden darf.
const TERMINAL_STATUSES: [&str; 3] = ["completed", "cancelled", "forfeit"];
/// Status, in denen manuelle Bracket-Ergebnisse erlaubt sind
/// (`VALID_MANUAL_RESULT_STATUSES`).
const VALID_MANUAL_RESULT_STATUSES: [&str; 4] =
    ["pending", "checkin", "lobby_created", "in_progress"];

/// Ergebnis von [`MatchManager::apply_bracket_match_result`] /
/// [`MatchManager::apply_group_match_result`] (Wire-Form analog zum Python-Dict).
#[derive(Debug, Clone)]
pub struct ApplyResultOutcome {
    pub match_id: i64,
    pub winner_id: i64,
    /// Slot/ID je nach Pfad (Bracket: 0/1, Group: 1/2 — siehe Modul-Doku).
    pub winning_team: i64,
    pub duration_s: Option<i64>,
    /// Spieler-Stats (Bracket: zurückgegebene Liste; Group nutzt dieses Feld nicht).
    pub players: Vec<Value>,
}

impl ApplyResultOutcome {
    /// JSON-Form für den Bracket-Pfad (`match_id, winner_id, winning_team,
    /// duration_s, players`).
    pub fn to_bracket_value(&self) -> Value {
        serde_json::json!({
            "match_id": self.match_id,
            "winner_id": self.winner_id,
            "winning_team": self.winning_team,
            "duration_s": self.duration_s,
            "players": self.players,
        })
    }

    /// JSON-Form für den Group-Pfad (`match_id, winner_id, winning_team,
    /// duration_s, source`). `source` wird vom Aufrufer ergänzt.
    pub fn to_group_value(&self, source: &str) -> Value {
        serde_json::json!({
            "match_id": self.match_id,
            "winner_id": self.winner_id,
            "winning_team": self.winning_team,
            "duration_s": self.duration_s,
            "source": source,
        })
    }
}

/// Eingabe-Parameter für [`MatchManager::apply_bracket_match_result`].
#[derive(Debug, Clone, Default)]
pub struct ApplyBracketParams {
    pub winning_team: Option<i64>,
    pub winner_id: Option<i64>,
    pub duration_s: Option<i64>,
    /// Spieler-Stats als JSON-Liste; `None` → bestehende `match_stats` nutzen.
    pub players: Option<Vec<Value>>,
    /// Quelle (`automatic` | `manual` | `series_manual` | `self_report`).
    pub source: String,
    /// `force` setzt Downstream rekursiv zurück, wenn sich der Gewinner ändert.
    pub force: bool,
}

impl ApplyBracketParams {
    /// Default-Parameter mit der Standard-Quelle `automatic`.
    pub fn automatic() -> Self {
        Self { source: "automatic".to_string(), ..Self::default() }
    }
}

/// Eingabe-Parameter für [`MatchManager::apply_group_match_result`].
#[derive(Debug, Clone, Default)]
pub struct ApplyGroupParams {
    pub winning_team: Option<i64>,
    pub winner_id: Option<i64>,
    pub deadlock_match_id: Option<String>,
    pub duration_s: Option<i64>,
    pub players: Option<Vec<Value>>,
    pub source: String,
}

impl ApplyGroupParams {
    /// Default-Parameter mit der Standard-Quelle `manual`.
    pub fn manual() -> Self {
        Self { source: "manual".to_string(), ..Self::default() }
    }
}

/// Snapshot einer Bracket-Zeile VOR dem Ergebnis-Update (Pre-Update).
#[derive(Debug, Clone, sqlx::FromRow)]
struct BracketResultRow {
    id: i64,
    round: i64,
    position: i64,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
    winner_id: Option<i64>,
    status: String,
    mini_group_id: Option<i64>,
    match_duration_s: Option<i64>,
    match_stats: Option<String>,
    discord_channel_id: Option<String>,
}

impl MatchManager {
    /// Persistiert ein Bracket-Ergebnis und propagiert den Gewinner.
    /// Portiert `apply_bracket_match_result`.
    pub async fn apply_bracket_match_result(
        &self,
        tournament_id: i64,
        match_id: i64,
        params: ApplyBracketParams,
    ) -> MatchResult<ApplyResultOutcome> {
        let match_row: Option<BracketResultRow> = sqlx::query_as::<_, BracketResultRow>(
            "SELECT id, round, position, team1_id, team2_id, winner_id, status, \
                    mini_group_id, match_duration_s, match_stats, discord_channel_id \
             FROM bracket_matches WHERE id = ? AND tournament_id = ?",
        )
        .bind(match_id)
        .bind(tournament_id)
        .fetch_optional(&self.pool)
        .await?;
        let match_row = match_row
            .ok_or_else(|| MatchError::not_found(format!("Bracket-Match {match_id} nicht gefunden")))?;

        if TERMINAL_STATUSES.contains(&match_row.status.as_str()) && !params.force {
            return Err(MatchError::state(format!(
                "Bracket-Match {match_id} kann aus Status {} nicht verarbeitet werden",
                match_row.status
            )));
        }
        // Bug-preserved (behavior-change, result_processor.py:73-81): der zweite
        // NOT-IN-Teil ist faktisch redundant, bleibt aber erhalten.
        if (params.source == "manual" || params.source == "series_manual")
            && !VALID_MANUAL_RESULT_STATUSES.contains(&match_row.status.as_str())
            && !TERMINAL_STATUSES.contains(&match_row.status.as_str())
        {
            return Err(MatchError::state(
                "Manuelle Bracket-Ergebnisse sind nur für ausstehende, eingecheckte, \
                 Lobby-erstellte oder laufende Matches erlaubt",
            ));
        }

        let team1_id = match_row.team1_id;
        let team2_id = match_row.team2_id;
        let (Some(team1_id), Some(team2_id)) = (team1_id, team2_id) else {
            return Err(MatchError::state(format!(
                "Bracket-Match {match_id} hat noch nicht beide Teams gesetzt"
            )));
        };

        let (winner_id_value, winning_team_value) =
            resolve_bracket_winner(team1_id, team2_id, params.winning_team, params.winner_id)?;

        // Force-Reset bei Gewinnerwechsel.
        if params.force
            && match_row.winner_id.is_some()
            && match_row.winner_id != Some(winner_id_value)
        {
            self.reset_bracket_downstream(tournament_id, match_row.id, match_row.round, match_row.position)
                .await?;
        }

        let duration_value = params.duration_s.or(match_row.match_duration_s);
        let (player_stats_json, return_players) =
            resolve_player_stats(params.players.as_ref(), match_row.match_stats.as_deref())?;

        // Persistenz in einer Transaktion (DELETE + UPDATE + INSERT).
        let mut tx = self.pool.begin().await.map_err(tb_db::DbError::from)?;
        sqlx::query("DELETE FROM match_results WHERE bracket_match_id = ?")
            .bind(match_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE bracket_matches \
             SET winner_id = ?, status = 'completed', match_duration_s = ?, \
                 match_stats = ?, played_at = datetime('now') \
             WHERE id = ? AND tournament_id = ?",
        )
        .bind(winner_id_value)
        .bind(duration_value)
        .bind(player_stats_json.as_deref())
        .bind(match_id)
        .bind(tournament_id)
        .execute(&mut *tx)
        .await?;
        // Bug-preserved (needs-decision): hier wird winner_id_value in winning_team
        // geschrieben (nicht der Slot) — exakt wie das Original.
        sqlx::query(
            "INSERT INTO match_results (bracket_match_id, winning_team, duration_s, player_stats, source) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(match_id)
        .bind(winner_id_value)
        .bind(duration_value)
        .bind(player_stats_json.as_deref())
        .bind(&params.source)
        .execute(&mut *tx)
        .await?;
        tx.commit().await.map_err(tb_db::DbError::from)?;

        // Channel-Cleanup (fire-and-forget) — Snapshot-Quelle bewusst Pre-Update.
        let discord_channel_id = match_row.discord_channel_id.clone();
        if let Some(channel_id) = discord_channel_id.clone().filter(|s| !s.is_empty()) {
            self.spawn_delete_channel_later(channel_id);
        }

        // Gewinner propagieren.
        tb_tournament::advance_bracket_winner(&self.pool, tournament_id, match_id, winner_id_value)
            .await?;

        // Mini-Group ggf. abschließen.
        let mut mini_group_winner_id: Option<i64> = None;
        if let Some(mini_group_id) = match_row.mini_group_id {
            mini_group_winner_id =
                tb_tournament::complete_mini_group_round_robin(&self.pool, mini_group_id).await?;
        }

        // Auto-Lobby für Folge-Match (best-effort).
        if let Err(err) = self.schedule_auto_lobby_for_next_round(tournament_id, match_id).await {
            tracing::error!(
                tournament_id, match_id, error = %err,
                "Auto-Lobby für Folge-Match fehlgeschlagen"
            );
        }
        // Auto-Lobby nach Mini-Group-Abschluss (best-effort).
        if mini_group_winner_id.is_some() {
            if let Err(err) = self.schedule_auto_lobbies_for_tournament(tournament_id).await {
                tracing::error!(
                    tournament_id, error = %err,
                    "Auto-Lobby nach Mini-Group-Abschluss fehlgeschlagen"
                );
            }
        }

        // Stats in den Discord-Channel posten (best-effort, nur wenn players da).
        let has_players = params.players.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
        if let (Some(channel_id), true) = (discord_channel_id.filter(|s| !s.is_empty()), has_players) {
            self.post_bracket_stats(
                tournament_id,
                match_id,
                &channel_id,
                duration_value,
                params.players.as_deref().unwrap_or(&[]),
            )
            .await;
        }

        Ok(ApplyResultOutcome {
            match_id,
            winner_id: winner_id_value,
            winning_team: winning_team_value,
            duration_s: duration_value,
            players: return_players,
        })
    }

    /// Persistiert ein Group-Match-Ergebnis und aktualisiert die Gruppen-Tabelle.
    /// Portiert `_apply_group_match_result`.
    pub async fn apply_group_match_result(
        &self,
        tournament_id: i64,
        match_id: i64,
        params: ApplyGroupParams,
    ) -> MatchResult<ApplyResultOutcome> {
        let row = sqlx::query(
            "SELECT gm.group_id, gm.team1_id, gm.team2_id, gm.status \
             FROM group_matches gm \
             JOIN groups g ON g.id = gm.group_id \
             WHERE gm.id = ? AND g.tournament_id = ?",
        )
        .bind(match_id)
        .bind(tournament_id)
        .fetch_optional(&self.pool)
        .await?;
        let row = row.ok_or_else(|| {
            MatchError::not_found(format!(
                "Group-Match {match_id} im Turnier {tournament_id} nicht gefunden"
            ))
        })?;
        let group_id: i64 = row.get("group_id");
        let team1_id: Option<i64> = row.get("team1_id");
        let team2_id: Option<i64> = row.get("team2_id");
        let status: String = row.get("status");

        if TERMINAL_STATUSES.contains(&status.as_str()) {
            return Err(MatchError::state(format!(
                "Group-Match {match_id} kann aus Status {status} nicht verarbeitet werden"
            )));
        }

        // winner_id aus winning_team (1-basiert!) ableiten, falls nicht gesetzt.
        let mut winner_id = params.winner_id;
        if winner_id.is_none() {
            if params.winning_team == Some(1) {
                winner_id = team1_id;
            } else if params.winning_team == Some(2) {
                winner_id = team2_id;
            }
        }
        let winner_id = winner_id
            .filter(|w| Some(*w) == team1_id || Some(*w) == team2_id)
            .ok_or_else(|| MatchError::invalid("winner_id muss eines der beiden Teams im Match sein"))?;

        let winning_team_value = if Some(winner_id) == team1_id { 1 } else { 2 };
        let loser_id = if winning_team_value == 1 { team2_id } else { team1_id };
        let match_stats = params.players.as_ref().map(|players| {
            serde_json::to_string(&serde_json::json!({ "players": players })).unwrap_or_default()
        });
        let player_stats = params
            .players
            .as_ref()
            .map(|players| serde_json::to_string(players).unwrap_or_default());

        let mut tx = self.pool.begin().await.map_err(tb_db::DbError::from)?;
        sqlx::query(
            "UPDATE group_matches \
             SET winner_id = ?, status = 'completed', \
                 deadlock_match_id = COALESCE(?, deadlock_match_id), \
                 match_duration_s = COALESCE(?, match_duration_s), \
                 match_stats = COALESCE(?, match_stats), \
                 played_at = datetime('now') \
             WHERE id = ?",
        )
        .bind(winner_id)
        .bind(params.deadlock_match_id.as_deref())
        .bind(params.duration_s)
        .bind(match_stats.as_deref())
        .bind(match_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE group_teams SET wins = wins + 1, points = points + 3 \
             WHERE group_id = ? AND team_id = ?",
        )
        .bind(group_id)
        .bind(winner_id)
        .execute(&mut *tx)
        .await?;
        if let Some(loser_id) = loser_id {
            sqlx::query(
                "UPDATE group_teams SET losses = losses + 1 WHERE group_id = ? AND team_id = ?",
            )
            .bind(group_id)
            .bind(loser_id)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM match_results WHERE group_match_id = ?")
            .bind(match_id)
            .execute(&mut *tx)
            .await?;
        // Bug-preserved (needs-decision): Group-Pfad schreibt den Slot (1/2) in
        // winning_team — anders als der Bracket-Pfad.
        sqlx::query(
            "INSERT INTO match_results (group_match_id, winning_team, duration_s, player_stats, source) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(match_id)
        .bind(winning_team_value)
        .bind(params.duration_s)
        .bind(player_stats.as_deref())
        .bind(&params.source)
        .execute(&mut *tx)
        .await?;
        tx.commit().await.map_err(tb_db::DbError::from)?;

        Ok(ApplyResultOutcome {
            match_id,
            winner_id,
            winning_team: winning_team_value,
            duration_s: params.duration_s,
            players: Vec::new(),
        })
    }

    /// Setzt den Downstream eines Bracket-Matches rekursiv zurück.
    /// Portiert `_reset_bracket_downstream`. Bug-preserved (behavior-change): KEIN
    /// Zyklus-/Tiefenschutz — bei fehlerhaften Source-Verweisen droht (wie im
    /// Original) Endlosrekursion. Bewusst 1:1 erhalten.
    fn reset_bracket_downstream<'a>(
        &'a self,
        tournament_id: i64,
        match_id: i64,
        round: i64,
        position: i64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = MatchResult<()>> + Send + 'a>> {
        Box::pin(async move {
            let next = load_next_bracket_match(&self.pool, tournament_id, match_id, round, position)
                .await?;
            let Some(next) = next else {
                return Ok(());
            };

            // Erst den Downstream des Folge-Matches (Post-Order wie das Original).
            self.reset_bracket_downstream(tournament_id, next.id, next.round, next.position)
                .await?;

            let slot_is_team1 = resolve_next_slot_is_team1(
                match_id,
                position,
                next.source_match1_id,
                next.source_match2_id,
            );
            // Zwei feste Query-Zweige statt dynamischem Spaltennamen.
            if slot_is_team1 {
                sqlx::query(
                    "UPDATE bracket_matches \
                     SET team1_id = NULL, winner_id = NULL, status = 'pending', \
                         steam_party_id = NULL, party_code = NULL, deadlock_match_id = NULL, \
                         match_duration_s = NULL, match_stats = NULL, played_at = NULL \
                     WHERE id = ?",
                )
                .bind(next.id)
                .execute(&self.pool)
                .await?;
            } else {
                sqlx::query(
                    "UPDATE bracket_matches \
                     SET team2_id = NULL, winner_id = NULL, status = 'pending', \
                         steam_party_id = NULL, party_code = NULL, deadlock_match_id = NULL, \
                         match_duration_s = NULL, match_stats = NULL, played_at = NULL \
                     WHERE id = ?",
                )
                .bind(next.id)
                .execute(&self.pool)
                .await?;
            }
            sqlx::query("DELETE FROM match_results WHERE bracket_match_id = ?")
                .bind(next.id)
                .execute(&self.pool)
                .await?;
            Ok(())
        })
    }

    /// Lädt die Stats-Zeile (Team-/Sieger-Namen) und postet die Match-Stats.
    async fn post_bracket_stats(
        &self,
        tournament_id: i64,
        match_id: i64,
        channel_id: &str,
        duration_s: Option<i64>,
        players: &[Value],
    ) {
        if self.notifier.is_none() {
            return;
        }
        let stats_row = sqlx::query(
            "SELECT bm.deadlock_match_id, t1.name AS team1_name, t2.name AS team2_name, \
                    winner.name AS winner_name \
             FROM bracket_matches bm \
             LEFT JOIN teams t1 ON t1.id = bm.team1_id \
             LEFT JOIN teams t2 ON t2.id = bm.team2_id \
             LEFT JOIN teams winner ON winner.id = bm.winner_id \
             WHERE bm.id = ? AND bm.tournament_id = ?",
        )
        .bind(match_id)
        .bind(tournament_id)
        .fetch_optional(&self.pool)
        .await;

        let (deadlock_match_id, team1_name, team2_name, winner_name) = match stats_row {
            Ok(Some(row)) => (
                row.get::<Option<String>, _>("deadlock_match_id").filter(|s| !s.is_empty()),
                row.get::<Option<String>, _>("team1_name").unwrap_or_else(|| "Team 1".to_string()),
                row.get::<Option<String>, _>("team2_name").unwrap_or_else(|| "Team 2".to_string()),
                row.get::<Option<String>, _>("winner_name").unwrap_or_else(|| "Unbekannt".to_string()),
            ),
            Ok(None) => (None, "Team 1".to_string(), "Team 2".to_string(), "Unbekannt".to_string()),
            Err(err) => {
                tracing::error!(match_id, error = %err, "Stats-Zeile laden fehlgeschlagen");
                (None, "Team 1".to_string(), "Team 2".to_string(), "Unbekannt".to_string())
            }
        };

        let player_stats: Vec<PlayerStat> = players.iter().map(player_stat_from_value).collect();
        self.post_match_stats(
            channel_id,
            match_id,
            deadlock_match_id.as_deref(),
            &team1_name,
            &team2_name,
            &winner_name,
            duration_s,
            &player_stats,
        )
        .await;
    }
}

/// Verkettetes Folge-Match (Snapshot für den Downstream-Reset).
#[derive(Debug, Clone, sqlx::FromRow)]
struct NextBracketMatch {
    id: i64,
    round: i64,
    position: i64,
    source_match1_id: Option<i64>,
    source_match2_id: Option<i64>,
}

/// Lädt das Folge-Match: zuerst über source_match-Verweise, sonst über
/// round+1/position//2. Portiert `_load_next_bracket_match`.
async fn load_next_bracket_match(
    pool: &tb_db::Pool,
    tournament_id: i64,
    match_id: i64,
    round: i64,
    position: i64,
) -> MatchResult<Option<NextBracketMatch>> {
    let by_source: Option<NextBracketMatch> = sqlx::query_as::<_, NextBracketMatch>(
        "SELECT id, round, position, source_match1_id, source_match2_id \
         FROM bracket_matches \
         WHERE tournament_id = ? AND (source_match1_id = ? OR source_match2_id = ?) LIMIT 1",
    )
    .bind(tournament_id)
    .bind(match_id)
    .bind(match_id)
    .fetch_optional(pool)
    .await?;
    if by_source.is_some() {
        return Ok(by_source);
    }
    let legacy: Option<NextBracketMatch> = sqlx::query_as::<_, NextBracketMatch>(
        "SELECT id, round, position, source_match1_id, source_match2_id \
         FROM bracket_matches WHERE tournament_id = ? AND round = ? AND position = ? LIMIT 1",
    )
    .bind(tournament_id)
    .bind(round + 1)
    .bind(position / 2)
    .fetch_optional(pool)
    .await?;
    Ok(legacy)
}

/// Bestimmt, ob der zurückzusetzende Slot `team1_id` ist. Portiert
/// `_resolve_next_slot` (source_match1 → team1, source_match2 → team2, sonst über
/// Position-Parität).
fn resolve_next_slot_is_team1(
    match_id: i64,
    position: i64,
    source_match1_id: Option<i64>,
    source_match2_id: Option<i64>,
) -> bool {
    if source_match1_id == Some(match_id) {
        return true;
    }
    if source_match2_id == Some(match_id) {
        return false;
    }
    position % 2 == 0
}

/// Löst `winner_id` und `winning_team` (0-basiert!) aus den Eingaben auf.
/// Portiert die Winner-Auflösungslogik von `apply_bracket_match_result`.
fn resolve_bracket_winner(
    team1_id: i64,
    team2_id: i64,
    winning_team: Option<i64>,
    winner_id: Option<i64>,
) -> MatchResult<(i64, i64)> {
    if winning_team.is_none() && winner_id.is_none() {
        return Err(MatchError::invalid("winner_id oder winning_team ist erforderlich"));
    }

    let (winner_id_value, winning_team_value) = match (winner_id, winning_team) {
        // Nur winning_team gegeben (0-basiert): 0 → team1, 1 → team2.
        (None, Some(wt)) => match wt {
            0 => (team1_id, 0),
            1 => (team2_id, 1),
            other => {
                return Err(MatchError::invalid(format!("Ungültiger winning_team-Wert: {other}")))
            }
        },
        // Nur winner_id gegeben: Slot ableiten.
        (Some(wid), None) => {
            let wt = resolve_winning_team(team1_id, team2_id, wid)?;
            (wid, wt)
        }
        // Beide gegeben: Konsistenz prüfen.
        (Some(wid), Some(wt)) => {
            if wt != 0 && wt != 1 {
                return Err(MatchError::invalid(format!("Ungültiger winning_team-Wert: {wt}")));
            }
            let expected = resolve_winning_team(team1_id, team2_id, wid)?;
            if wt != expected {
                return Err(MatchError::invalid("winning_team passt nicht zum übergebenen winner_id"));
            }
            (wid, wt)
        }
        (None, None) => unreachable!("bereits oben abgefangen"),
    };

    if winner_id_value != team1_id && winner_id_value != team2_id {
        return Err(MatchError::invalid(format!(
            "winner_id {winner_id_value} gehört nicht zum Match"
        )));
    }
    Ok((winner_id_value, winning_team_value))
}

/// `0` für team1, `1` für team2 (Bracket-Konvention). Portiert
/// `_resolve_winning_team`.
fn resolve_winning_team(team1_id: i64, team2_id: i64, winner_id: i64) -> MatchResult<i64> {
    if winner_id == team1_id {
        return Ok(0);
    }
    if winner_id == team2_id {
        return Ok(1);
    }
    Err(MatchError::invalid(format!("winner_id {winner_id} gehört nicht zu diesem Match")))
}

/// Löst die Spieler-Stats auf: explizite `players`-Liste serialisieren, sonst die
/// bestehenden `match_stats` dekodieren. Portiert `_resolve_player_stats`.
/// Rückgabe: `(player_stats_json, decoded_players)`.
fn resolve_player_stats(
    players: Option<&Vec<Value>>,
    existing_match_stats: Option<&str>,
) -> MatchResult<(Option<String>, Vec<Value>)> {
    if let Some(players) = players {
        let json = serde_json::to_string(players)
            .map_err(|_| MatchError::invalid("players enthält nicht serialisierbare Daten"))?;
        return Ok((Some(json), players.clone()));
    }
    let Some(existing) = existing_match_stats.filter(|s| !s.is_empty()) else {
        return Ok((None, Vec::new()));
    };
    let decoded: Value = serde_json::from_str(existing)
        .map_err(|_| MatchError::invalid("match_stats enthält ungültiges JSON"))?;
    let Value::Array(list) = decoded else {
        return Err(MatchError::invalid("match_stats enthält keine gültige Spielerliste"));
    };
    Ok((Some(existing.to_string()), list))
}

/// Baut einen [`PlayerStat`] aus einem JSON-Spielerobjekt.
fn player_stat_from_value(value: &Value) -> PlayerStat {
    let get_str = |key: &str| value.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());
    let get_int = |key: &str| value.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
    PlayerStat {
        hero: get_str("hero"),
        player_name: get_str("player_name"),
        discord_name: get_str("discord_name"),
        kills: get_int("kills"),
        deaths: get_int("deaths"),
        assists: get_int("assists"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn winner_aus_winning_team_0basiert() {
        // winning_team=0 → team1.
        let (wid, wt) = resolve_bracket_winner(100, 200, Some(0), None).unwrap();
        assert_eq!((wid, wt), (100, 0));
        // winning_team=1 → team2.
        let (wid, wt) = resolve_bracket_winner(100, 200, Some(1), None).unwrap();
        assert_eq!((wid, wt), (200, 1));
        // Ungültiger Slot.
        assert!(resolve_bracket_winner(100, 200, Some(2), None).is_err());
    }

    #[test]
    fn winner_aus_winner_id() {
        let (wid, wt) = resolve_bracket_winner(100, 200, None, Some(200)).unwrap();
        assert_eq!((wid, wt), (200, 1));
        // winner_id fremd.
        assert!(resolve_bracket_winner(100, 200, None, Some(999)).is_err());
    }

    #[test]
    fn winner_beide_konsistent_und_inkonsistent() {
        // Konsistent: winner_id=team1, winning_team=0.
        assert!(resolve_bracket_winner(100, 200, Some(0), Some(100)).is_ok());
        // Inkonsistent: winner_id=team1, winning_team=1.
        assert!(resolve_bracket_winner(100, 200, Some(1), Some(100)).is_err());
        // Ungültiger Slot bei beiden gesetzt.
        assert!(resolve_bracket_winner(100, 200, Some(5), Some(100)).is_err());
    }

    #[test]
    fn winner_fehlt_komplett() {
        assert!(resolve_bracket_winner(100, 200, None, None).is_err());
    }

    #[test]
    fn player_stats_explizit() {
        let players = vec![json!({ "hero": "Abrams" })];
        let (json_str, decoded) = resolve_player_stats(Some(&players), None).unwrap();
        assert!(json_str.is_some());
        assert_eq!(decoded.len(), 1);
    }

    #[test]
    fn player_stats_aus_bestehend() {
        let existing = r#"[{"hero":"Bebop"}]"#;
        let (json_str, decoded) = resolve_player_stats(None, Some(existing)).unwrap();
        assert_eq!(json_str.as_deref(), Some(existing));
        assert_eq!(decoded.len(), 1);
    }

    #[test]
    fn player_stats_leer() {
        let (json_str, decoded) = resolve_player_stats(None, None).unwrap();
        assert!(json_str.is_none());
        assert!(decoded.is_empty());
        // Leerer String → ebenfalls leer.
        let (json_str, decoded) = resolve_player_stats(None, Some("")).unwrap();
        assert!(json_str.is_none());
        assert!(decoded.is_empty());
    }

    #[test]
    fn player_stats_ungueltiges_json_fehler() {
        assert!(resolve_player_stats(None, Some("nicht json")).is_err());
        // Gültiges JSON, aber keine Liste.
        assert!(resolve_player_stats(None, Some(r#"{"x":1}"#)).is_err());
    }

    #[test]
    fn next_slot_aufloesung() {
        // source_match1 → team1.
        assert!(resolve_next_slot_is_team1(5, 0, Some(5), None));
        // source_match2 → team2.
        assert!(!resolve_next_slot_is_team1(5, 0, None, Some(5)));
        // Fallback gerade Position → team1.
        assert!(resolve_next_slot_is_team1(5, 2, None, None));
        // Fallback ungerade Position → team2.
        assert!(!resolve_next_slot_is_team1(5, 3, None, None));
    }

    #[test]
    fn player_stat_aus_value() {
        let v = json!({ "hero": "Haze", "kills": 5, "deaths": 2, "assists": 7 });
        let p = player_stat_from_value(&v);
        assert_eq!(p.hero.as_deref(), Some("Haze"));
        assert_eq!((p.kills, p.deaths, p.assists), (5, 2, 7));
    }
}
