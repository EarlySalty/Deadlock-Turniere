//! Preset-Persistenz fuer wiederverwendbare Turnier-Konfigurationen.

use serde::{Deserialize, Serialize};
use turnier_core::{BracketFormat, InviteMode, TournamentGameMode, TournamentMode};
use turnier_db::Pool;

use crate::error::AutomatikResult;

/// Zielkategorie eines Presets und der spaeteren DM-Zielgruppe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
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

/// Legt ein Preset an und gibt die gespeicherte Zeile zurueck.
pub async fn create(pool: &Pool, preset: &NewPreset) -> AutomatikResult<Preset> {
    let row = sqlx::query_as::<_, Preset>(
        "INSERT INTO tournament_presets \
         (name, category, team_size, bracket_format, series_format, final_series_format, \
          tournament_mode, tournament_game_mode, match_objective, invite_mode, \
          reminder_offsets, start_reminder_offsets, rules, description_template, active, created_by) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
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
    .bind(&preset.config.reminder_offsets)
    .bind(&preset.config.start_reminder_offsets)
    .bind(&preset.config.rules)
    .bind(&preset.config.description_template)
    .bind(preset.active)
    .bind(&preset.created_by)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Listet alle Presets, stabil nach ID sortiert.
pub async fn list(pool: &Pool) -> AutomatikResult<Vec<Preset>> {
    let rows = sqlx::query_as::<_, Preset>("SELECT * FROM tournament_presets ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// Listet aktive Presets einer Kategorie.
pub async fn list_active_by_category(
    pool: &Pool,
    category: Category,
) -> AutomatikResult<Vec<Preset>> {
    let rows = sqlx::query_as::<_, Preset>(
        "SELECT * FROM tournament_presets \
         WHERE category = ? AND active = 1 ORDER BY id",
    )
    .bind(category)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Laedt ein Preset per ID.
pub async fn get(pool: &Pool, preset_id: i64) -> AutomatikResult<Option<Preset>> {
    let row = sqlx::query_as::<_, Preset>("SELECT * FROM tournament_presets WHERE id = ?")
        .bind(preset_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// Aktualisiert ein Preset vollstaendig und gibt die neue Zeile zurueck.
pub async fn update(
    pool: &Pool,
    preset_id: i64,
    changes: &PresetUpdate,
) -> AutomatikResult<Option<Preset>> {
    let row = sqlx::query_as::<_, Preset>(
        "UPDATE tournament_presets SET \
             name = ?, category = ?, team_size = ?, bracket_format = ?, series_format = ?, \
             final_series_format = ?, tournament_mode = ?, tournament_game_mode = ?, \
             match_objective = ?, invite_mode = ?, reminder_offsets = ?, start_reminder_offsets = ?, \
             rules = ?, description_template = ?, updated_at = datetime('now') \
         WHERE id = ? RETURNING *",
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
    .bind(&changes.config.reminder_offsets)
    .bind(&changes.config.start_reminder_offsets)
    .bind(&changes.config.rules)
    .bind(&changes.config.description_template)
    .bind(preset_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Aktiviert/deaktiviert ein Preset.
pub async fn set_active(pool: &Pool, preset_id: i64, active: bool) -> AutomatikResult<bool> {
    let res = sqlx::query(
        "UPDATE tournament_presets SET active = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(active)
    .bind(preset_id)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Loescht ein Preset.
pub async fn delete(pool: &Pool, preset_id: i64) -> AutomatikResult<bool> {
    let res = sqlx::query("DELETE FROM tournament_presets WHERE id = ?")
        .bind(preset_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}
