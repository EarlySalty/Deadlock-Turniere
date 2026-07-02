//! Persistenz-Schicht des Draft-Subsystems (sqlx::PgPool).
//!
//! Portiert `start_draft`, `take_action`, `get_draft_state` aus
//! `backend/draft/engine.py`. Die reine Sequenz-Logik lebt in [`crate::sequence`];
//! hier sitzt nur der DB-Zugriff.
//!
//! Wichtigste Korrektheits-Verbesserung gegenüber dem Original: [`take_action`]
//! läuft als EINE Transaktion mit optimistischem Compare-and-Swap auf
//! `current_action_index`. Im Python-Original lagen SELECT (Index lesen),
//! Doppel-Pick-Prüfung und die beiden UPDATEs ohne Isolation nebeneinander —
//! zwei gleichzeitige Aktionen konnten denselben Index lesen und sich gegenseitig
//! überschreiben (last-write-wins). Hier scheitert die zweite Aktion mit
//! [`DraftError::ActionConflict`]. Welche Picks gültig sind, bleibt unverändert.

use chrono::{DateTime, Utc};
use sqlx::{Postgres, QueryBuilder, Transaction};
use turnier_core::{discord_id_to_string, now_utc, parse_discord_id};
use turnier_db::Pool;

use crate::error::{DraftError, DraftResult};
use crate::heroes::is_valid_hero;
use crate::sequence::{self, DEFAULT_SEQUENCE, SEQUENCE_LEN};
use crate::state::{ActionOutcome, DraftAction, DraftSession, DraftState};

#[derive(Debug, sqlx::FromRow)]
struct DraftSessionRow {
    id: i64,
    bracket_match_id: i64,
    status: String,
    current_action_index: i64,
    started_by: Option<i64>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl From<DraftSessionRow> for DraftSession {
    fn from(row: DraftSessionRow) -> Self {
        Self {
            id: row.id,
            bracket_match_id: row.bracket_match_id,
            status: row.status,
            current_action_index: row.current_action_index,
            started_by: row.started_by.map(discord_id_to_string),
            started_at: row.started_at.map(|value| value.to_rfc3339()),
            completed_at: row.completed_at.map(|value| value.to_rfc3339()),
            created_at: row.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct DraftActionRow {
    id: i64,
    session_id: i64,
    sequence_index: i64,
    action_type: sequence::ActionType,
    team_slot: i64,
    hero_name: Option<String>,
    taken_by: Option<i64>,
    taken_at: Option<DateTime<Utc>>,
    is_admin_forced: bool,
}

impl From<DraftActionRow> for DraftAction {
    fn from(row: DraftActionRow) -> Self {
        Self {
            id: row.id,
            session_id: row.session_id,
            sequence_index: row.sequence_index,
            action_type: row.action_type,
            team_slot: row.team_slot,
            hero_name: row.hero_name,
            taken_by: row.taken_by.map(discord_id_to_string),
            taken_at: row.taken_at.map(|value| value.to_rfc3339()),
            is_admin_forced: row.is_admin_forced,
        }
    }
}

/// Erstellt eine neue Draft-Session für `bracket_match_id` oder gibt eine
/// bestehende (`pending`/`in_progress`) zurück. Idempotent — wie `start_draft`.
///
/// Bei Neuanlage werden alle 18 Aktionszeilen der Standard-Sequenz in einem
/// Multi-Row-INSERT materialisiert (Original: 18 Einzel-INSERTs in einer
/// Schleife). Das Ergebnis ist identisch; nur effizienter (safe-Fix).
///
/// `started_by` ist im Original ein freier String; turnier-api leitet ihn aus der
/// Admin-Identität (`user.discord_id`) ab.
pub async fn start_draft(pool: &Pool, bracket_match_id: i64, started_by: &str) -> DraftResult<i64> {
    let now = now_utc();
    let started_by = parse_numeric_id(started_by)?;
    let mut tx = pool.begin().await?;

    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM turnier.draft_sessions \
         WHERE bracket_match_id = $1 AND status IN ('pending', 'in_progress')",
    )
    .bind(bracket_match_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((id,)) = existing {
        tx.commit().await?;
        return Ok(id);
    }

    let session_row: (i64,) = sqlx::query_as(
        "INSERT INTO turnier.draft_sessions \
             (bracket_match_id, status, current_action_index, started_by, started_at, created_at) \
         VALUES ($1, 'in_progress', 0, $2, $3, $4) RETURNING id",
    )
    .bind(bracket_match_id)
    .bind(started_by)
    .bind(now)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    let session_id = session_row.0;

    let mut query = QueryBuilder::<Postgres>::new(
        "INSERT INTO turnier.draft_actions \
             (session_id, sequence_index, action_type, team_slot, is_admin_forced) ",
    );
    query.push_values(
        DEFAULT_SEQUENCE.iter().enumerate(),
        |mut row, (idx, step)| {
            row.push_bind(session_id)
                .push_bind(idx as i64)
                .push_bind(step.action_type.as_str())
                .push_bind(step.team_slot.as_i64())
                .push_bind(false);
        },
    );
    query.push(" RETURNING id");
    let action_ids: Vec<(i64,)> = query.build_query_as().fetch_all(&mut *tx).await?;
    debug_assert_eq!(action_ids.len(), SEQUENCE_LEN);

    tx.commit().await?;
    Ok(session_id)
}

/// Führt die aktuell anstehende Draft-Aktion aus: schreibt `hero_name` an die
/// Position `current_action_index`, rückt den Index vor und schließt die Session
/// bei Sequenz-Ende ab. Portiert `take_action`.
///
/// Reihenfolge der Prüfungen 1:1 zum Original:
/// 1. Held gültig? sonst [`DraftError::UnknownHero`].
/// 2. Session `in_progress`? sonst [`DraftError::SessionNotActive`].
/// 3. Held in dieser Session schon vergeben? sonst [`DraftError::HeroAlreadyTaken`].
///
/// Alles in EINER Postgres-Transaktion. Zusätzlich rückt der Index per Compare-and-Swap vor
/// (`WHERE current_action_index = ? AND status = 'in_progress'`); 0 betroffene
/// Zeilen ⇒ [`DraftError::ActionConflict`].
pub async fn take_action(
    pool: &Pool,
    session_id: i64,
    hero_name: &str,
    taken_by: &str,
    force: bool,
) -> DraftResult<ActionOutcome> {
    if !is_valid_hero(hero_name) {
        return Err(DraftError::UnknownHero(hero_name.to_string()));
    }

    let now = now_utc();
    let taken_by = parse_numeric_id(taken_by)?;
    let mut tx = pool.begin().await?;

    // (1) Index der aktuellen Session lesen — nur wenn in_progress.
    let session: Option<(i64,)> = sqlx::query_as(
        "SELECT current_action_index FROM turnier.draft_sessions \
         WHERE id = $1 AND status = 'in_progress'",
    )
    .bind(session_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((idx,)) = session else {
        return Err(DraftError::SessionNotActive);
    };

    // (2) Doppel-Pick-Prüfung: ist der Held in dieser Session schon vergeben?
    let already: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM turnier.draft_actions WHERE session_id = $1 AND hero_name = $2",
    )
    .bind(session_id)
    .bind(hero_name)
    .fetch_optional(&mut *tx)
    .await?;
    if already.is_some() {
        return Err(DraftError::HeroAlreadyTaken(hero_name.to_string()));
    }

    // (3) Held an die aktuelle Position schreiben.
    sqlx::query(
        "UPDATE turnier.draft_actions \
         SET hero_name = $1, taken_by = $2, taken_at = $3, is_admin_forced = $4 \
         WHERE session_id = $5 AND sequence_index = $6",
    )
    .bind(hero_name)
    .bind(taken_by)
    .bind(now)
    .bind(force)
    .bind(session_id)
    .bind(idx)
    .execute(&mut *tx)
    .await?;

    // (4) Index per Compare-and-Swap vorrücken + ggf. abschließen.
    let next_idx = idx + 1;
    let complete = sequence::is_complete(next_idx as usize);
    let res = if complete {
        sqlx::query(
            "UPDATE turnier.draft_sessions \
             SET current_action_index = $1, status = 'completed', completed_at = $2 \
             WHERE id = $3 AND current_action_index = $4 AND status = 'in_progress'",
        )
        .bind(next_idx)
        .bind(now)
        .bind(session_id)
        .bind(idx)
        .execute(&mut *tx)
        .await?
    } else {
        // completed_at NICHT anfassen, solange nicht abgeschlossen (im Original
        // wurde es bei jeder Zwischenaktion unnötig auf NULL gesetzt — safe-Fix).
        sqlx::query(
            "UPDATE turnier.draft_sessions \
             SET current_action_index = $1 \
             WHERE id = $2 AND current_action_index = $3 AND status = 'in_progress'",
        )
        .bind(next_idx)
        .bind(session_id)
        .bind(idx)
        .execute(&mut *tx)
        .await?
    };
    if res.rows_affected() == 0 {
        // CAS fehlgeschlagen → parallele Aktion hat den Index verschoben. tx wird
        // beim Drop zurückgerollt, die Aktionszeile aus (3) verfällt mit.
        return Err(DraftError::ActionConflict);
    }

    tx.commit().await?;

    let next_step = if complete {
        None
    } else {
        sequence::step_at(next_idx as usize)
    };
    Ok(ActionOutcome {
        is_complete: complete,
        next_action_type: next_step.map(|s| s.action_type),
        next_team_slot: next_step.map(|s| s.team_slot.as_i64()),
    })
}

/// Lädt Session + alle Aktionen und leitet `bans`/`picks_team1`/`picks_team2`
/// sowie die aktuelle Position ab. Portiert `get_draft_state`.
pub async fn get_draft_state(pool: &Pool, session_id: i64) -> DraftResult<DraftState> {
    let mut tx = pool.begin().await?;
    let state = load_state(&mut tx, session_id).await?;
    tx.commit().await?;
    Ok(state)
}

/// Zustandsaufbau innerhalb einer bestehenden Transaktion. Von [`get_draft_state`]
/// genutzt; als eigene Funktion gehalten, damit turnier-api den Vollzustand bei Bedarf
/// in derselben Transaktion wie eine Aktion aufbauen kann.
async fn load_state(
    tx: &mut Transaction<'_, Postgres>,
    session_id: i64,
) -> DraftResult<DraftState> {
    let session: Option<DraftSessionRow> =
        sqlx::query_as::<_, DraftSessionRow>("SELECT * FROM turnier.draft_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(session) = session else {
        return Err(DraftError::SessionNotFound);
    };
    let session = DraftSession::from(session);

    let actions: Vec<DraftActionRow> = sqlx::query_as::<_, DraftActionRow>(
        "SELECT * FROM turnier.draft_actions WHERE session_id = $1 ORDER BY sequence_index",
    )
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await?;
    let actions = actions
        .into_iter()
        .map(DraftAction::from)
        .collect::<Vec<_>>();

    let idx = session.current_action_index;
    let current = if idx >= 0 {
        sequence::step_at(idx as usize)
    } else {
        None
    };

    let bans: Vec<String> = actions
        .iter()
        .filter(|a| a.action_type == sequence::ActionType::Ban)
        .filter_map(|a| a.hero_name.clone())
        .collect();
    let picks_team1: Vec<String> = actions
        .iter()
        .filter(|a| a.action_type == sequence::ActionType::Pick && a.team_slot == 1)
        .filter_map(|a| a.hero_name.clone())
        .collect();
    let picks_team2: Vec<String> = actions
        .iter()
        .filter(|a| a.action_type == sequence::ActionType::Pick && a.team_slot == 2)
        .filter_map(|a| a.hero_name.clone())
        .collect();

    Ok(DraftState {
        session,
        actions,
        current_action_type: current.map(|s| s.action_type),
        current_team_slot: current.map(|s| s.team_slot.as_i64()),
        bans,
        picks_team1,
        picks_team2,
    })
}

fn parse_numeric_id(value: &str) -> DraftResult<i64> {
    parse_discord_id(value).map_err(|_| DraftError::InvalidDiscordId(value.to_string()))
}
