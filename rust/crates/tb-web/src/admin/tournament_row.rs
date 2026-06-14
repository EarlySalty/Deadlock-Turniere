//! Mapping einer `tournaments`-Zeile auf das [`Tournament`]-DTO.
//!
//! Die Reminder-Offset-Spalten liegen als JSON-String in der DB und werden hier
//! zu `Vec<i64>` geparst (Default `[]` bei Null/Parse-Fehler — wie das Original,
//! das ein leeres bzw. fehlerhaftes Feld toleriert).

use tb_core::Tournament;
use tb_db::Pool;

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
    registration_start: Option<String>,
    registration_end: Option<String>,
    checkin_start: Option<String>,
    group_phase_start: Option<String>,
    bracket_start: Option<String>,
    bracket_format: String,
    tournament_mode: String,
    tournament_game_mode: String,
    auto_lobby_enabled: i64,
    created_by: String,
    created_at: String,
    updated_at: String,
    invite_mode: String,
    invite_window_start: Option<String>,
    invite_window_end: Option<String>,
    lobby_settings: Option<String>,
    exclude_from_leaderboard: i64,
    reminder_offsets: Option<String>,
    start_reminder_offsets: Option<String>,
    match_objective: String,
    no_show_grace_minutes: i64,
    rules: Option<String>,
    is_test: i64,
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
            registration_start: self.registration_start,
            registration_end: self.registration_end,
            checkin_start: self.checkin_start,
            group_phase_start: self.group_phase_start,
            bracket_start: self.bracket_start,
            bracket_format: self.bracket_format,
            tournament_mode: parse_enum(&self.tournament_mode)?,
            tournament_game_mode: parse_enum(&self.tournament_game_mode)?,
            auto_lobby_enabled: self.auto_lobby_enabled != 0,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            invite_mode: parse_enum(&self.invite_mode)?,
            invite_window_start: self.invite_window_start,
            invite_window_end: self.invite_window_end,
            lobby_settings: self.lobby_settings,
            exclude_from_leaderboard: self.exclude_from_leaderboard != 0,
            reminder_offsets: tb_core::json::parse_offsets(
                self.reminder_offsets.as_deref(),
                &tb_core::json::default_reminder_offsets(),
            ),
            start_reminder_offsets: tb_core::json::parse_offsets(
                self.start_reminder_offsets.as_deref(),
                &tb_core::json::default_start_reminder_offsets(),
            ),
            match_objective: self.match_objective,
            no_show_grace_minutes: self.no_show_grace_minutes,
            rules: self.rules,
            is_test: self.is_test != 0,
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
     FROM tournaments";

/// Lädt ein Turnier als DTO (oder 404).
pub async fn load_tournament_dto(pool: &Pool, tournament_id: i64) -> WebResult<Tournament> {
    let row: Option<TournamentRow> =
        sqlx::query_as(&format!("{TOURNAMENT_SELECT} WHERE id = ?"))
            .bind(tournament_id)
            .fetch_optional(pool)
            .await?;
    row.ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))?.into_dto()
}

/// Lädt alle Turniere als DTO-Liste (`created_at DESC`).
pub async fn load_all_tournaments_dto(pool: &Pool) -> WebResult<Vec<Tournament>> {
    let rows: Vec<TournamentRow> =
        sqlx::query_as(&format!("{TOURNAMENT_SELECT} ORDER BY created_at DESC"))
            .fetch_all(pool)
            .await?;
    rows.into_iter().map(TournamentRow::into_dto).collect()
}
