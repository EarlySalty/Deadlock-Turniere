//! Repository-Abstraktion über `bracket_matches`/`group_matches`.
//!
//! Ersetzt die stringly-typed Helfer `_match_table`/`_match_scope_column`/
//! `_match_scope_value` des Originals. Jede Operation hat ZWEI feste Query-Zweige
//! (Bracket vs. Group) — Tabellennamen tauchen nirgends als interpolierter String
//! im SQL auf. Das kapselt die Lobby-/Start-Reads, die Scope-gebundenen Updates,
//! Teilnehmer-/Context-/Caster-Reads sowie die Group-Ergebnis-Persistenz.

use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, Row};

use turnier_core::{discord_id_to_string, parse_discord_id};
use turnier_db::dynamic_sql::push_i64_bind_list;
use turnier_db::Pool;

use crate::error::{MatchError, MatchResult};
use crate::kind::MatchKind;

/// Snapshot einer Match-Zeile, vereinheitlicht für Bracket und Group.
///
/// `scope_value` ist der Wert der Scope-Spalte (`tournament_id` für Bracket,
/// `group_id` für Group) — er ersetzt `_match_scope_value`. `tournament_id` ist
/// für beide Arten aufgelöst (bei Group via JOIN über `groups`).
#[derive(Debug, Clone)]
pub struct MatchRow {
    pub id: i64,
    pub tournament_id: i64,
    /// Scope-Wert für das Scoped-Update: Bracket → tournament_id, Group → group_id.
    pub scope_value: i64,
    pub team1_id: Option<i64>,
    pub team2_id: Option<i64>,
    pub winner_id: Option<i64>,
    pub status: String,
    pub steam_party_id: Option<String>,
    pub party_code: Option<String>,
    pub deadlock_match_id: Option<String>,
    pub discord_channel_id: Option<String>,
    pub match_duration_s: Option<i64>,
    pub match_stats: Option<String>,
    pub team1_name: Option<String>,
    pub team2_name: Option<String>,
}

impl MatchRow {
    /// Team-Name mit demselben Fallback wie `_load_match_context`
    /// (`team1_name` ∨ `"Team {team1_id}"`).
    pub fn team1_label(&self) -> String {
        self.team1_name
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Team {}", id_or_dash(self.team1_id)))
    }

    /// Wie [`MatchRow::team1_label`] für Team 2.
    pub fn team2_label(&self) -> String {
        self.team2_name
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Team {}", id_or_dash(self.team2_id)))
    }
}

fn id_or_dash(value: Option<i64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "None".to_string())
}

/// Ein Teilnehmer eines Matches (eine Zeile aus `team_members` + Team-Bezug).
#[derive(Debug, Clone)]
pub struct Participant {
    pub discord_id: Option<String>,
    pub discord_name: Option<String>,
    pub steam_id: Option<String>,
    pub team_id: i64,
    pub team_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ParticipantRow {
    discord_id: i64,
    discord_name: Option<String>,
    steam_id: Option<String>,
    team_id: i64,
    team_name: Option<String>,
}

/// Lädt ein Match (mit Team-Namen) im gegebenen Scope.
///
/// Bracket: `WHERE bm.id = ? AND bm.tournament_id = ?`.
/// Group: `WHERE gm.id = ? AND g.tournament_id = ?` (JOIN `groups`).
/// Match nicht gefunden → [`MatchError::NotFound`] (wie `MatchNotFoundError`).
pub async fn get_match(
    pool: &Pool,
    kind: MatchKind,
    tournament_id: i64,
    match_id: i64,
) -> MatchResult<MatchRow> {
    let row = match kind {
        MatchKind::Bracket => {
            sqlx::query(
                "SELECT bm.id, bm.tournament_id, bm.tournament_id AS scope_value, \
                    bm.team1_id, bm.team2_id, bm.winner_id, bm.status, \
                    bm.steam_party_id, bm.party_code, bm.deadlock_match_id, \
                    bm.discord_channel_id, bm.match_duration_s, bm.match_stats, \
                    t1.name AS team1_name, t2.name AS team2_name \
             FROM turnier.bracket_matches bm \
             LEFT JOIN turnier.teams t1 ON t1.id = bm.team1_id \
             LEFT JOIN turnier.teams t2 ON t2.id = bm.team2_id \
             WHERE bm.id = $1 AND bm.tournament_id = $2",
            )
            .bind(match_id)
            .bind(tournament_id)
            .fetch_optional(pool)
            .await?
        }
        MatchKind::Group => {
            sqlx::query(
                "SELECT gm.id, g.tournament_id AS tournament_id, gm.group_id AS scope_value, \
                    gm.team1_id, gm.team2_id, gm.winner_id, gm.status, \
                    gm.steam_party_id, gm.party_code, gm.deadlock_match_id, \
                    gm.discord_channel_id, gm.match_duration_s, gm.match_stats, \
                    t1.name AS team1_name, t2.name AS team2_name \
             FROM turnier.group_matches gm \
             JOIN turnier.groups g ON g.id = gm.group_id \
             LEFT JOIN turnier.teams t1 ON t1.id = gm.team1_id \
             LEFT JOIN turnier.teams t2 ON t2.id = gm.team2_id \
             WHERE gm.id = $1 AND g.tournament_id = $2",
            )
            .bind(match_id)
            .bind(tournament_id)
            .fetch_optional(pool)
            .await?
        }
    };

    let Some(row) = row else {
        return Err(MatchError::not_found(not_found_msg(
            kind,
            match_id,
            tournament_id,
        )));
    };

    let discord_channel_id: Option<i64> = row.get("discord_channel_id");
    let match_stats: Option<Value> = row.get("match_stats");

    Ok(MatchRow {
        id: row.get("id"),
        tournament_id: row.get("tournament_id"),
        scope_value: row.get("scope_value"),
        team1_id: row.get("team1_id"),
        team2_id: row.get("team2_id"),
        winner_id: row.get("winner_id"),
        status: row.get("status"),
        steam_party_id: row.get("steam_party_id"),
        party_code: row.get("party_code"),
        deadlock_match_id: row.get("deadlock_match_id"),
        discord_channel_id: discord_channel_id.map(discord_id_to_string),
        match_duration_s: row.get("match_duration_s"),
        match_stats: match_stats.map(|value| value.to_string()),
        team1_name: row.get("team1_name"),
        team2_name: row.get("team2_name"),
    })
}

fn not_found_msg(kind: MatchKind, match_id: i64, tournament_id: i64) -> String {
    match kind {
        MatchKind::Bracket => {
            format!("Bracket-Match {match_id} im Turnier {tournament_id} nicht gefunden")
        }
        MatchKind::Group => {
            format!("Group-Match {match_id} im Turnier {tournament_id} nicht gefunden")
        }
    }
}

/// Schreibt nach erfolgreicher Lobby-Erstellung `steam_party_id`, `party_code`,
/// `status = 'lobby_created'` und `hero_assignments` — scope-gebunden.
///
/// Entspricht dem ersten UPDATE in `_create_lobby_for_match`.
pub async fn set_lobby_created(
    pool: &Pool,
    kind: MatchKind,
    match_id: i64,
    scope_value: i64,
    party_id: &str,
    party_code: &str,
    hero_assignments: Option<&Value>,
) -> MatchResult<()> {
    match kind {
        MatchKind::Bracket => sqlx::query(
            "UPDATE turnier.bracket_matches \
             SET steam_party_id = $1, party_code = $2, status = 'lobby_created', hero_assignments = $3::jsonb \
             WHERE id = $4 AND tournament_id = $5",
        ),
        MatchKind::Group => sqlx::query(
            "UPDATE turnier.group_matches \
             SET steam_party_id = $1, party_code = $2, status = 'lobby_created', hero_assignments = $3::jsonb \
             WHERE id = $4 AND group_id = $5",
        ),
    }
    .bind(party_id)
    .bind(party_code)
    .bind(hero_assignments)
    .bind(match_id)
    .bind(scope_value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Schreibt die `discord_channel_id` scope-gebunden (zweites UPDATE im Lobby-Flow).
pub async fn set_discord_channel_id(
    pool: &Pool,
    kind: MatchKind,
    match_id: i64,
    scope_value: i64,
    channel_id: &str,
) -> MatchResult<()> {
    let channel_id = parse_discord_id(channel_id).map_err(|_| {
        MatchError::invalid("discord_channel_id muss eine numerische Discord-ID sein")
    })?;
    match kind {
        MatchKind::Bracket => sqlx::query(
            "UPDATE turnier.bracket_matches SET discord_channel_id = $1 WHERE id = $2 AND tournament_id = $3",
        ),
        MatchKind::Group => sqlx::query(
            "UPDATE turnier.group_matches SET discord_channel_id = $1 WHERE id = $2 AND group_id = $3",
        ),
    }
    .bind(channel_id)
    .bind(match_id)
    .bind(scope_value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Setzt `status = 'in_progress'` und (per COALESCE) die `deadlock_match_id`.
/// Entspricht dem UPDATE in `_start_match_for_match`.
pub async fn set_in_progress(
    pool: &Pool,
    kind: MatchKind,
    match_id: i64,
    scope_value: i64,
    deadlock_match_id: Option<&str>,
) -> MatchResult<()> {
    match kind {
        MatchKind::Bracket => sqlx::query(
            "UPDATE turnier.bracket_matches \
             SET status = 'in_progress', deadlock_match_id = COALESCE($1::text, deadlock_match_id) \
             WHERE id = $2 AND tournament_id = $3",
        ),
        MatchKind::Group => sqlx::query(
            "UPDATE turnier.group_matches \
             SET status = 'in_progress', deadlock_match_id = COALESCE($1::text, deadlock_match_id) \
             WHERE id = $2 AND group_id = $3",
        ),
    }
    .bind(deadlock_match_id)
    .bind(match_id)
    .bind(scope_value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Liest `lobby_settings` eines Turniers als JSON-Objekt.
///
/// Entspricht `_get_tournament_lobby_settings`: leer/`""`/`"{}"` → leeres Objekt,
/// ungültiges JSON oder Nicht-Objekt → Fehler (im Original `SteamTaskError`; hier
/// als [`MatchError::State`], der Aufrufer mappt). Turnier fehlt → NotFound.
pub async fn get_lobby_settings(
    pool: &Pool,
    tournament_id: i64,
) -> MatchResult<serde_json::Map<String, serde_json::Value>> {
    let row = sqlx::query("SELECT lobby_settings FROM turnier.tournaments WHERE id = $1")
        .bind(tournament_id)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Err(MatchError::not_found(format!(
            "Turnier {tournament_id} nicht gefunden"
        )));
    };
    let parsed: Option<Value> = row.get("lobby_settings");
    let Some(parsed) = parsed else {
        return Ok(serde_json::Map::new());
    };
    match parsed {
        serde_json::Value::Null => Ok(serde_json::Map::new()),
        serde_json::Value::Object(map) => Ok(map),
        _ => Err(MatchError::state(
            "lobby_settings muss ein JSON-Objekt sein",
        )),
    }
}

/// `True`, wenn das Turnier ein Test-Turnier ist (`is_test`). Fehlt das Turnier
/// → `false` (wie `_is_test_tournament`).
pub async fn is_test_tournament(pool: &Pool, tournament_id: i64) -> MatchResult<bool> {
    let row = sqlx::query("SELECT is_test FROM turnier.tournaments WHERE id = $1")
        .bind(tournament_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<bool, _>("is_test")).unwrap_or(false))
}

/// Lädt die Teilnehmer beider Teams in stabiler Reihenfolge
/// (`ORDER BY t.id, tm.joined_at, tm.id`). Kein Team gesetzt → leere Liste.
/// Entspricht `_load_match_participants`.
pub async fn load_participants(pool: &Pool, m: &MatchRow) -> MatchResult<Vec<Participant>> {
    let team_ids: Vec<i64> = [m.team1_id, m.team2_id].into_iter().flatten().collect();
    if team_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT tm.discord_id, tm.discord_name, tm.steam_id, t.id AS team_id, t.name AS team_name \
         FROM turnier.team_members tm \
         JOIN turnier.teams t ON t.id = tm.team_id \
         WHERE t.id IN ",
    );
    push_i64_bind_list(&mut query, team_ids);
    query.push(" ORDER BY t.id, tm.joined_at, tm.id");
    let rows: Vec<ParticipantRow> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| Participant {
            discord_id: Some(discord_id_to_string(row.discord_id)),
            discord_name: row.discord_name,
            steam_id: row.steam_id,
            team_id: row.team_id,
            team_name: row.team_name,
        })
        .collect())
}

/// Lädt die Caster-Discord-IDs für ein Match: zuerst die turnierweiten Caster
/// (`tournament_casters`), und NUR falls keine existieren, die match-spezifischen
/// (`match_casters`). Entspricht `_load_match_casters`.
pub async fn load_match_casters(
    pool: &Pool,
    kind: MatchKind,
    match_id: i64,
) -> MatchResult<Vec<String>> {
    // tournament_id des Matches auflösen.
    let tid_row = match kind {
        MatchKind::Group => sqlx::query(
            "SELECT g.tournament_id FROM turnier.group_matches gm \
             JOIN turnier.groups g ON g.id = gm.group_id WHERE gm.id = $1",
        ),
        MatchKind::Bracket => {
            sqlx::query("SELECT tournament_id FROM turnier.bracket_matches WHERE id = $1")
        }
    }
    .bind(match_id)
    .fetch_optional(pool)
    .await?;
    let tournament_id: Option<i64> = tid_row.map(|r| r.get::<i64, _>("tournament_id"));

    if let Some(tid) = tournament_id {
        let rows = sqlx::query(
            "SELECT discord_id FROM turnier.tournament_casters \
             WHERE tournament_id = $1 ORDER BY assigned_at, discord_id",
        )
        .bind(tid)
        .fetch_all(pool)
        .await?;
        if !rows.is_empty() {
            return Ok(rows
                .into_iter()
                .map(|r| discord_id_to_string(r.get::<i64, _>("discord_id")))
                .collect());
        }
    }

    let rows = sqlx::query(
        "SELECT discord_id FROM turnier.match_casters \
         WHERE match_type = $1 AND match_id = $2 ORDER BY assigned_at, discord_id",
    )
    .bind(kind.as_str())
    .bind(match_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| discord_id_to_string(r.get::<i64, _>("discord_id")))
        .collect())
}

/// `(match_objective, team_size)` eines Turniers für die Objective-Auflösung;
/// `None`, wenn das Turnier fehlt. Entspricht dem Inline-SELECT im Lobby-Flow.
pub async fn get_objective_inputs(
    pool: &Pool,
    tournament_id: i64,
) -> MatchResult<Option<(Option<String>, i64)>> {
    let row =
        sqlx::query("SELECT match_objective, team_size FROM turnier.tournaments WHERE id = $1")
            .bind(tournament_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| {
        (
            r.get::<Option<String>, _>("match_objective"),
            r.get::<i64, _>("team_size"),
        )
    }))
}
