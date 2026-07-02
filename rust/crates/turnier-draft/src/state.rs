//! Wire-DTOs des Draft-Zustands.
//!
//! Bildet das Rückgabe-Dict von `get_draft_state` (Python) typisiert ab: die
//! Session-Felder, die materialisierten Aktionszeilen und die daraus abgeleiteten
//! Listen (`bans`, `picks_team1`, `picks_team2`) plus die aktuelle Position.
//! Feldnamen entsprechen 1:1 dem JSON, das die `/api/draft`-Routen ausliefern.

use serde::Serialize;

use crate::sequence::ActionType;

/// Eine einzelne Aktionszeile (`draft_actions`), so wie das Python-`dict(action)`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DraftAction {
    pub id: i64,
    pub session_id: i64,
    pub sequence_index: i64,
    /// `'ban'` | `'pick'`.
    pub action_type: ActionType,
    /// `1` | `2`.
    pub team_slot: i64,
    /// `None`, solange die Position noch nicht ausgeführt wurde.
    pub hero_name: Option<String>,
    pub taken_by: Option<String>,
    pub taken_at: Option<String>,
    pub is_admin_forced: bool,
}

/// Die Session-Stammzeile (`draft_sessions`).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DraftSession {
    pub id: i64,
    pub bracket_match_id: i64,
    pub status: String,
    pub current_action_index: i64,
    pub started_by: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}

/// Vollständiger Draft-Zustand — Rückgabe von [`crate::get_draft_state`].
///
/// Entspricht dem zusammengesetzten Dict des Originals: alle Session-Felder
/// (über `session`), die Aktionsliste und die abgeleiteten Sichten.
#[derive(Debug, Clone, Serialize)]
pub struct DraftState {
    #[serde(flatten)]
    pub session: DraftSession,
    pub actions: Vec<DraftAction>,
    /// Typ der aktuell anstehenden Aktion, `None` bei abgeschlossener Sequenz.
    pub current_action_type: Option<ActionType>,
    /// Team-Slot der aktuell anstehenden Aktion, `None` bei abgeschlossener Sequenz.
    pub current_team_slot: Option<i64>,
    /// Alle gebannten Helden (Reihenfolge nach `sequence_index`).
    pub bans: Vec<String>,
    /// Picks von Team 1 (Reihenfolge nach `sequence_index`).
    pub picks_team1: Vec<String>,
    /// Picks von Team 2 (Reihenfolge nach `sequence_index`).
    pub picks_team2: Vec<String>,
}

/// Folgezustand nach einer Aktion — Rückgabe von [`crate::take_action`].
///
/// Entspricht dem `{is_complete, next_action_type, next_team_slot}`-Dict, das
/// `submit_action` über den Vollzustand merged.
#[derive(Debug, Clone, Serialize)]
pub struct ActionOutcome {
    pub is_complete: bool,
    /// Typ der nächsten Aktion, `None` wenn die Sequenz abgeschlossen ist.
    pub next_action_type: Option<ActionType>,
    /// Team-Slot der nächsten Aktion, `None` wenn die Sequenz abgeschlossen ist.
    pub next_team_slot: Option<i64>,
}
