//! Mapping einer `tournaments`-Zeile auf das [`Tournament`]-DTO.
//!
//! Die Reminder-Offset-Spalten liegen in Postgres als JSONB und werden hier zu
//! `Vec<i64>` geparst. Die Wire-Form bleibt bei Zeitstempeln und Discord-IDs
//! String-kompatibel.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;

use turnier_core::{AdminTournamentSummary, Tournament};
use turnier_db::Pool;

use crate::db;
use crate::error::{WebError, WebResult};

/// Typisierte Rohzeile eines Turniers. Enum-Spalten werden — wie überall in der
/// Codebase — als `String` dekodiert und im Mapping konvertiert.
#[derive(sqlx::FromRow)]
struct TournamentRow {
    id: i64,
    name: String,
    status: String,
    description: Option<String>,
    team_size: i64,
    series_format: i64,
    final_series_format: Option<i64>,
    registration_start: Option<DateTime<Utc>>,
    registration_end: Option<DateTime<Utc>>,
    checkin_start: Option<DateTime<Utc>>,
    group_phase_start: Option<DateTime<Utc>>,
    bracket_start: Option<DateTime<Utc>>,
    bracket_format: String,
    tournament_mode: String,
    tournament_game_mode: String,
    auto_lobby_enabled: bool,
    created_by: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    invite_mode: String,
    invite_window_start: Option<DateTime<Utc>>,
    invite_window_end: Option<DateTime<Utc>>,
    lobby_settings: Option<Value>,
    exclude_from_leaderboard: bool,
    reminder_offsets: Option<Value>,
    start_reminder_offsets: Option<Value>,
    match_objective: String,
    no_show_grace_minutes: i64,
    rules: Option<String>,
    is_test: bool,
}

/// String → Domänen-Enum (snake_case-serde); Fehler → 500.
fn parse_enum<T: serde::de::DeserializeOwned>(value: &str) -> WebResult<T> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| WebError::internal("Ungültiger Enum-Wert in der DB"))
}

impl TournamentRow {
    fn into_dto(self) -> WebResult<Tournament> {
        Ok(Tournament {
            id: self.id,
            name: self.name,
            status: parse_enum(&self.status)?,
            description: self.description,
            team_size: self.team_size,
            series_format: self.series_format,
            final_series_format: self.final_series_format,
            registration_start: db::opt_ts_to_string(self.registration_start),
            registration_end: db::opt_ts_to_string(self.registration_end),
            checkin_start: db::opt_ts_to_string(self.checkin_start),
            group_phase_start: db::opt_ts_to_string(self.group_phase_start),
            bracket_start: db::opt_ts_to_string(self.bracket_start),
            bracket_format: self.bracket_format,
            tournament_mode: parse_enum(&self.tournament_mode)?,
            tournament_game_mode: parse_enum(&self.tournament_game_mode)?,
            auto_lobby_enabled: self.auto_lobby_enabled,
            created_by: db::discord_id_to_string(self.created_by),
            created_at: db::ts_to_string(self.created_at),
            updated_at: db::ts_to_string(self.updated_at),
            invite_mode: parse_enum(&self.invite_mode)?,
            invite_window_start: db::opt_ts_to_string(self.invite_window_start),
            invite_window_end: db::opt_ts_to_string(self.invite_window_end),
            lobby_settings: db::json_to_wire(self.lobby_settings),
            exclude_from_leaderboard: self.exclude_from_leaderboard,
            reminder_offsets: turnier_core::json::parse_offsets(
                db::json_to_wire(self.reminder_offsets).as_deref(),
                &turnier_core::json::default_reminder_offsets(),
            ),
            start_reminder_offsets: turnier_core::json::parse_offsets(
                db::json_to_wire(self.start_reminder_offsets).as_deref(),
                &turnier_core::json::default_start_reminder_offsets(),
            ),
            match_objective: self.match_objective,
            no_show_grace_minutes: self.no_show_grace_minutes,
            rules: self.rules,
            is_test: self.is_test,
        })
    }
}

const TOURNAMENT_SELECT: &str =
    "SELECT id, name, status, description, team_size, series_format, final_series_format, \
            registration_start, registration_end, checkin_start, group_phase_start, bracket_start, \
            bracket_format, tournament_mode, tournament_game_mode, auto_lobby_enabled, created_by, \
            created_at, updated_at, invite_mode, invite_window_start, invite_window_end, \
            lobby_settings, exclude_from_leaderboard, reminder_offsets, start_reminder_offsets, \
            match_objective, no_show_grace_minutes, rules, is_test \
     FROM turnier.\"tournaments\"";

/// Lädt ein Turnier als DTO (oder 404).
pub async fn load_tournament_dto(pool: &Pool, tournament_id: i64) -> WebResult<Tournament> {
    let row: Option<TournamentRow> = sqlx::query_as(&format!("{TOURNAMENT_SELECT} WHERE id = $1"))
        .bind(tournament_id)
        .fetch_optional(pool)
        .await?;
    row.ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))?
        .into_dto()
}

/// Lädt alle Turniere als DTO-Liste (`created_at DESC`).
pub async fn load_all_tournaments_dto(pool: &Pool) -> WebResult<Vec<Tournament>> {
    let rows: Vec<TournamentRow> =
        sqlx::query_as(&format!("{TOURNAMENT_SELECT} ORDER BY created_at DESC"))
            .fetch_all(pool)
            .await?;
    rows.into_iter().map(TournamentRow::into_dto).collect()
}

#[derive(sqlx::FromRow)]
struct TournamentCountRow {
    id: i64,
    team_count: i64,
    player_count: i64,
    signup_count: i64,
    open_signup_count: i64,
}

const TOURNAMENT_COUNTS_SELECT: &str = r#"SELECT t.id,
       (SELECT COUNT(*) FROM turnier."teams" tm WHERE tm.tournament_id = t.id) AS team_count,
       (SELECT COUNT(*) FROM turnier."team_members" m JOIN turnier."teams" tm ON tm.id = m.team_id
         WHERE tm.tournament_id = t.id) AS player_count,
       (SELECT COUNT(*) FROM turnier."tournament_signups" s WHERE s.tournament_id = t.id) AS signup_count,
       (SELECT COUNT(*) FROM turnier."tournament_signups" s
         WHERE s.tournament_id = t.id AND s.team_id IS NULL) AS open_signup_count
  FROM turnier."tournaments" t"#;

pub async fn load_admin_tournament_summaries(
    pool: &Pool,
) -> WebResult<Vec<AdminTournamentSummary>> {
    let tournaments = load_all_tournaments_dto(pool).await?;
    let counts: Vec<TournamentCountRow> = sqlx::query_as(TOURNAMENT_COUNTS_SELECT)
        .fetch_all(pool)
        .await?;
    let by_id: HashMap<i64, TournamentCountRow> =
        counts.into_iter().map(|row| (row.id, row)).collect();
    Ok(tournaments
        .into_iter()
        .map(|tournament| {
            let counts = by_id.get(&tournament.id);
            summarize(tournament, counts)
        })
        .collect())
}

fn summarize(
    tournament: Tournament,
    counts: Option<&TournamentCountRow>,
) -> AdminTournamentSummary {
    AdminTournamentSummary {
        team_count: counts.map_or(0, |c| c.team_count),
        player_count: counts.map_or(0, |c| c.player_count),
        signup_count: counts.map_or(0, |c| c.signup_count),
        open_signup_count: counts.map_or(0, |c| c.open_signup_count),
        tournament,
    }
}
