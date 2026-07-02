//! Preset-Persistenz fuer wiederverwendbare Turnier-Konfigurationen.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use turnier_core::{
    discord_id_to_string, json::jsonb_to_wire_string, json::wire_string_to_jsonb, now_utc,
    parse_discord_id, BracketFormat, InviteMode, TournamentGameMode, TournamentMode,
};
use turnier_db::Pool;

use crate::error::{AutomatikError, AutomatikResult};

/// Zielkategorie eines Presets und der spaeteren DM-Zielgruppe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum Category {
    /// Fun-/Spass-Turnier.
    Fun,
    /// Competitive-/Grind-Turnier.
    Comp,
}

impl Category {
    /// DB-/Wire-Wert.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Fun => "fun",
            Category::Comp => "comp",
        }
    }
}

/// Wiederverwendbarer Konfigurationsblock eines Presets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetConfig {
    pub team_size: i64,
    pub bracket_format: BracketFormat,
    pub series_format: i64,
    pub final_series_format: Option<i64>,
    pub tournament_mode: TournamentMode,
    pub tournament_game_mode: TournamentGameMode,
    pub match_objective: String,
    pub invite_mode: InviteMode,
    pub reminder_offsets: Option<String>,
    pub start_reminder_offsets: Option<String>,
    pub rules: Option<String>,
    pub description_template: Option<String>,
}

/// Eingabe zum Anlegen eines Presets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPreset {
    pub name: String,
    pub category: Category,
    pub config: PresetConfig,
    pub active: bool,
    pub created_by: String,
}

/// Vollstaendige Aenderung eines Presets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetUpdate {
    pub name: String,
    pub category: Category,
    pub config: PresetConfig,
}

/// DB-Zeile aus `tournament_presets`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Preset {
    pub id: i64,
    pub name: String,
    pub category: Category,
    pub team_size: i64,
    pub bracket_format: BracketFormat,
    pub series_format: i64,
    pub final_series_format: Option<i64>,
    pub tournament_mode: TournamentMode,
    pub tournament_game_mode: TournamentGameMode,
    pub match_objective: String,
    pub invite_mode: InviteMode,
    pub reminder_offsets: Option<String>,
    pub start_reminder_offsets: Option<String>,
    pub rules: Option<String>,
    pub description_template: Option<String>,
    pub active: bool,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct PresetRow {
    pub id: i64,
    pub name: String,
    pub category: Category,
    pub team_size: i64,
    pub bracket_format: BracketFormat,
    pub series_format: i64,
    pub final_series_format: Option<i64>,
    pub tournament_mode: TournamentMode,
    pub tournament_game_mode: TournamentGameMode,
    pub match_objective: String,
    pub invite_mode: InviteMode,
    pub reminder_offsets: Option<Value>,
    pub start_reminder_offsets: Option<Value>,
    pub rules: Option<String>,
    pub description_template: Option<String>,
    pub active: bool,
    pub created_by: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<PresetRow> for Preset {
    fn from(row: PresetRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            category: row.category,
            team_size: row.team_size,
            bracket_format: row.bracket_format,
            series_format: row.series_format,
            final_series_format: row.final_series_format,
            tournament_mode: row.tournament_mode,
            tournament_game_mode: row.tournament_game_mode,
            match_objective: row.match_objective,
            invite_mode: row.invite_mode,
            reminder_offsets: jsonb_to_wire_string(row.reminder_offsets),
            start_reminder_offsets: jsonb_to_wire_string(row.start_reminder_offsets),
            rules: row.rules,
            description_template: row.description_template,
            active: row.active,
            created_by: discord_id_to_string(row.created_by),
            created_at: row.created_at.to_rfc3339(),
            updated_at: row.updated_at.to_rfc3339(),
        }
    }
}

/// Legt ein Preset an und gibt die gespeicherte Zeile zurueck.
pub async fn create(pool: &Pool, preset: &NewPreset) -> AutomatikResult<Preset> {
    let created_by = parse_discord_id(&preset.created_by)
        .map_err(|_| AutomatikError::InvalidNumericId(preset.created_by.clone()))?;
    let now = now_utc();
    let row = sqlx::query_as::<_, PresetRow>(
        "INSERT INTO turnier.tournament_presets \
         (name, category, team_size, bracket_format, series_format, final_series_format, \
          tournament_mode, tournament_game_mode, match_objective, invite_mode, \
          reminder_offsets, start_reminder_offsets, rules, description_template, active, created_by, \
          created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18) \
         RETURNING *",
    )
    .bind(&preset.name)
    .bind(preset.category)
    .bind(preset.config.team_size)
    .bind(preset.config.bracket_format)
    .bind(preset.config.series_format)
    .bind(preset.config.final_series_format)
    .bind(preset.config.tournament_mode)
    .bind(preset.config.tournament_game_mode)
    .bind(&preset.config.match_objective)
    .bind(preset.config.invite_mode)
    .bind(wire_string_to_jsonb(preset.config.reminder_offsets.as_deref()))
    .bind(wire_string_to_jsonb(
        preset.config.start_reminder_offsets.as_deref(),
    ))
    .bind(&preset.config.rules)
    .bind(&preset.config.description_template)
    .bind(preset.active)
    .bind(created_by)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(row.into())
}

/// Listet alle Presets, stabil nach ID sortiert.
pub async fn list(pool: &Pool) -> AutomatikResult<Vec<Preset>> {
    let rows =
        sqlx::query_as::<_, PresetRow>("SELECT * FROM turnier.tournament_presets ORDER BY id")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Listet aktive Presets einer Kategorie.
pub async fn list_active_by_category(
    pool: &Pool,
    category: Category,
) -> AutomatikResult<Vec<Preset>> {
    let rows = sqlx::query_as::<_, PresetRow>(
        "SELECT * FROM turnier.tournament_presets \
         WHERE category = $1 AND active = true ORDER BY id",
    )
    .bind(category)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Laedt ein Preset per ID.
pub async fn get(pool: &Pool, preset_id: i64) -> AutomatikResult<Option<Preset>> {
    let row =
        sqlx::query_as::<_, PresetRow>("SELECT * FROM turnier.tournament_presets WHERE id = $1")
            .bind(preset_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(Into::into))
}

/// Aktualisiert ein Preset vollstaendig und gibt die neue Zeile zurueck.
pub async fn update(
    pool: &Pool,
    preset_id: i64,
    changes: &PresetUpdate,
) -> AutomatikResult<Option<Preset>> {
    let now = now_utc();
    let row = sqlx::query_as::<_, PresetRow>(
        "UPDATE turnier.tournament_presets SET \
             name = $1, category = $2, team_size = $3, bracket_format = $4, series_format = $5, \
             final_series_format = $6, tournament_mode = $7, tournament_game_mode = $8, \
             match_objective = $9, invite_mode = $10, reminder_offsets = $11, start_reminder_offsets = $12, \
             rules = $13, description_template = $14, updated_at = $15 \
         WHERE id = $16 RETURNING *",
    )
    .bind(&changes.name)
    .bind(changes.category)
    .bind(changes.config.team_size)
    .bind(changes.config.bracket_format)
    .bind(changes.config.series_format)
    .bind(changes.config.final_series_format)
    .bind(changes.config.tournament_mode)
    .bind(changes.config.tournament_game_mode)
    .bind(&changes.config.match_objective)
    .bind(changes.config.invite_mode)
    .bind(wire_string_to_jsonb(
        changes.config.reminder_offsets.as_deref(),
    ))
    .bind(wire_string_to_jsonb(
        changes.config.start_reminder_offsets.as_deref(),
    ))
    .bind(&changes.config.rules)
    .bind(&changes.config.description_template)
    .bind(now)
    .bind(preset_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// Aktiviert/deaktiviert ein Preset.
pub async fn set_active(pool: &Pool, preset_id: i64, active: bool) -> AutomatikResult<bool> {
    let now = now_utc();
    let res = sqlx::query(
        "UPDATE turnier.tournament_presets SET active = $1, updated_at = $2 WHERE id = $3",
    )
    .bind(active)
    .bind(now)
    .bind(preset_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Loescht ein Preset.
pub async fn delete(pool: &Pool, preset_id: i64) -> AutomatikResult<bool> {
    let res = sqlx::query("DELETE FROM turnier.tournament_presets WHERE id = $1")
        .bind(preset_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
