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

use chrono::{DateTime, Duration, Utc};
use rand::rngs::StdRng;
use rand::seq::IteratorRandom;
use rand::{Rng, SeedableRng};
use sqlx::types::Json;
use sqlx::{Postgres, QueryBuilder, Transaction};
use turnier_core::{discord_id_to_string, now_utc, parse_discord_id};
use turnier_db::Pool;

use crate::error::{DraftError, DraftResult};
use crate::heroes::is_valid_hero;
use crate::heroes_provider::{load_heroes, Hero, HeroesProvider, ReqwestHeroFetcher};
use crate::sequence::SequenceStep;
use crate::sequence::{self, sequence_for_bans, DEFAULT_SEQUENCE, SEQUENCE_LEN};
use crate::state::{ActionOutcome, DraftAction, DraftSession, DraftState};

/// Eingaben zum Anlegen einer freien Draft-Lobby.
#[derive(Debug, Clone)]
pub struct CreateLobbyOptions {
    pub team1_name: String,
    pub team2_name: String,
    pub sequence: Vec<SequenceStep>,
    pub round_seconds: Option<i32>,
    pub reserve_seconds: Option<i32>,
}

/// Geheimnisse, die nur beim Anlegen einer Lobby zurückgegeben werden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LobbyCredentials {
    pub code: String,
    pub team1_token: String,
    pub team2_token: String,
}

#[derive(Debug, sqlx::FromRow)]
struct DraftSessionRow {
    id: i64,
    bracket_match_id: Option<i64>,
    code: Option<String>,
    team1_name: Option<String>,
    team2_name: Option<String>,
    sequence: Option<Json<Vec<SequenceStep>>>,
    round_seconds: Option<i32>,
    reserve_seconds: Option<i32>,
    team1_reserve_left: Option<i32>,
    team2_reserve_left: Option<i32>,
    deadline_at: Option<DateTime<Utc>>,
    status: String,
    current_action_index: i64,
    started_by: Option<i64>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    bans_per_team: i32,
    team1_claimed_at: Option<DateTime<Utc>>,
    team2_claimed_at: Option<DateTime<Utc>>,
    team1_ready: bool,
    team2_ready: bool,
    lobby_status: String,
    lobby_join_code: Option<String>,
    lobby_error: Option<String>,
    lobby_match_id: Option<String>,
    lobby_result: Option<serde_json::Value>,
}

impl From<DraftSessionRow> for DraftSession {
    fn from(row: DraftSessionRow) -> Self {
        Self {
            id: row.id,
            bracket_match_id: row.bracket_match_id,
            code: row.code,
            team1_name: row.team1_name,
            team2_name: row.team2_name,
            sequence: effective_sequence(row.sequence),
            round_seconds: row.round_seconds,
            reserve_seconds: row.reserve_seconds,
            team1_reserve_left: row.team1_reserve_left,
            team2_reserve_left: row.team2_reserve_left,
            deadline_at: row.deadline_at.map(|value| value.to_rfc3339()),
            status: row.status,
            current_action_index: row.current_action_index,
            started_by: row.started_by.map(discord_id_to_string),
            started_at: row.started_at.map(|value| value.to_rfc3339()),
            completed_at: row.completed_at.map(|value| value.to_rfc3339()),
            created_at: row.created_at.to_rfc3339(),
            bans_per_team: row.bans_per_team,
            team1_claimed: row.team1_claimed_at.is_some(),
            team2_claimed: row.team2_claimed_at.is_some(),
            team1_ready: row.team1_ready,
            team2_ready: row.team2_ready,
            lobby_status: row.lobby_status,
            lobby_join_code: row.lobby_join_code,
            lobby_error: row.lobby_error,
            lobby_match_id: row.lobby_match_id,
            lobby_result: row.lobby_result,
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
    is_auto: bool,
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
            is_auto: row.is_auto,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct LobbySessionRow {
    id: i64,
    status: String,
    current_action_index: i64,
    team1_token: String,
    team2_token: String,
    sequence: Json<Vec<SequenceStep>>,
    round_seconds: Option<i32>,
    team1_reserve_left: Option<i32>,
    team2_reserve_left: Option<i32>,
    deadline_at: Option<DateTime<Utc>>,
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

/// Legt eine freie Draft-Lobby mit eigener Sequenz und optionalem Timer an.
pub async fn create_lobby(pool: &Pool, opts: CreateLobbyOptions) -> DraftResult<LobbyCredentials> {
    let now = now_utc();
    let reserve = opts.reserve_seconds.unwrap_or(0).max(0);
    let deadline = opts
        .round_seconds
        .map(|round| now + Duration::seconds(i64::from(round.max(0)) + i64::from(reserve)));
    let mut rng = StdRng::from_entropy();
    let code = random_string(&mut rng, 8);
    let team1_token = random_string(&mut rng, 48);
    let team2_token = random_string(&mut rng, 48);
    let flip = rng.gen_bool(0.5);
    let (slot1_name, slot1_token, slot2_name, slot2_token) = if flip {
        (
            &opts.team2_name,
            &team2_token,
            &opts.team1_name,
            &team1_token,
        )
    } else {
        (
            &opts.team1_name,
            &team1_token,
            &opts.team2_name,
            &team2_token,
        )
    };
    let mut tx = pool.begin().await?;

    let (session_id,): (i64,) = sqlx::query_as(
        "INSERT INTO turnier.draft_sessions \
             (bracket_match_id, code, team1_name, team2_name, team1_token, team2_token, \
              sequence, round_seconds, reserve_seconds, team1_reserve_left, \
              team2_reserve_left, deadline_at, status, current_action_index, \
              started_at, created_at) \
         VALUES (NULL, $1, $2, $3, $4, $5, $6, $7, $8, $8, $8, $9, \
                 'in_progress', 0, $10, $10) \
         RETURNING id",
    )
    .bind(&code)
    .bind(slot1_name)
    .bind(slot2_name)
    .bind(slot1_token)
    .bind(slot2_token)
    .bind(Json(&opts.sequence))
    .bind(opts.round_seconds)
    .bind(reserve)
    .bind(deadline)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;

    materialize_actions(&mut tx, session_id, &opts.sequence).await?;
    tx.commit().await?;
    Ok(LobbyCredentials {
        code,
        team1_token,
        team2_token,
    })
}

pub struct CreateRoomOptions {
    pub team1_name: String,
    pub team2_name: String,
    pub sequence: Vec<SequenceStep>,
    pub bans_per_team: i32,
    pub round_seconds: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimOutcome {
    pub team: i64,
    pub token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyOutcome {
    pub started: bool,
}

pub async fn create_room(pool: &Pool, opts: CreateRoomOptions) -> DraftResult<String> {
    let now = now_utc();
    let mut rng = StdRng::from_entropy();
    let code = random_string(&mut rng, 8);
    let team1_token = random_string(&mut rng, 48);
    let team2_token = random_string(&mut rng, 48);
    let flip = rng.gen_bool(0.5);
    let (slot1_name, slot1_token, slot2_name, slot2_token) = if flip {
        (
            &opts.team2_name,
            &team2_token,
            &opts.team1_name,
            &team1_token,
        )
    } else {
        (
            &opts.team1_name,
            &team1_token,
            &opts.team2_name,
            &team2_token,
        )
    };
    let mut tx = pool.begin().await?;

    let (session_id,): (i64,) = sqlx::query_as(
        "INSERT INTO turnier.draft_sessions \
             (bracket_match_id, code, team1_name, team2_name, team1_token, team2_token, \
              sequence, bans_per_team, round_seconds, reserve_seconds, team1_reserve_left, \
              team2_reserve_left, deadline_at, status, current_action_index, created_at) \
         VALUES (NULL, $1, $2, $3, $4, $5, $6, $7, $8, 0, 0, 0, NULL, \
                 'warteraum', 0, $9) \
         RETURNING id",
    )
    .bind(&code)
    .bind(slot1_name)
    .bind(slot2_name)
    .bind(slot1_token)
    .bind(slot2_token)
    .bind(Json(&opts.sequence))
    .bind(opts.bans_per_team)
    .bind(opts.round_seconds)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;

    materialize_actions(&mut tx, session_id, &opts.sequence).await?;
    tx.commit().await?;
    Ok(code)
}

pub async fn claim_room(pool: &Pool, code: &str, team: i64) -> DraftResult<ClaimOutcome> {
    let now = now_utc();
    let mut tx = pool.begin().await?;
    let (status, team1_token, team2_token): (String, String, String) = sqlx::query_as(
        "SELECT status, team1_token, team2_token FROM turnier.draft_sessions \
         WHERE code = $1 FOR UPDATE",
    )
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DraftError::LobbyNotFound)?;
    if status != "warteraum" {
        return Err(DraftError::RoomNotOpen);
    }
    let (column, token) = match team {
        1 => ("team1_claimed_at", team1_token),
        2 => ("team2_claimed_at", team2_token),
        _ => return Err(DraftError::InvalidTeam),
    };
    let query = format!(
        "UPDATE turnier.draft_sessions SET {column} = $1 \
         WHERE code = $2 AND {column} IS NULL AND status = 'warteraum'"
    );
    let result = sqlx::query(&query)
        .bind(now)
        .bind(code)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() != 1 {
        return Err(DraftError::SlotTaken);
    }
    tx.commit().await?;
    Ok(ClaimOutcome { team, token })
}

pub async fn room_ready(pool: &Pool, code: &str, token: &str) -> DraftResult<ReadyOutcome> {
    let mut tx = pool.begin().await?;
    let (status, team1_token, team2_token, team1_claimed_at, team2_claimed_at): (
        String,
        String,
        String,
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
    ) = sqlx::query_as(
        "SELECT status, team1_token, team2_token, team1_claimed_at, team2_claimed_at \
         FROM turnier.draft_sessions WHERE code = $1 FOR UPDATE",
    )
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DraftError::LobbyNotFound)?;
    if status != "warteraum" {
        return Err(DraftError::RoomNotOpen);
    }
    let team = if token == team1_token {
        1
    } else if token == team2_token {
        2
    } else {
        return Err(DraftError::InvalidToken);
    };
    let claimed = if team == 1 {
        team1_claimed_at.is_some()
    } else {
        team2_claimed_at.is_some()
    };
    if !claimed {
        return Err(DraftError::InvalidToken);
    }
    let (ready_column, claimed_column) = if team == 1 {
        ("team1_ready", "team1_claimed_at")
    } else {
        ("team2_ready", "team2_claimed_at")
    };
    let query = format!("UPDATE turnier.draft_sessions SET {ready_column} = TRUE WHERE code = $1 AND {claimed_column} IS NOT NULL AND status = 'warteraum'");
    let result = sqlx::query(&query).bind(code).execute(&mut *tx).await?;
    if result.rows_affected() != 1 {
        return Err(DraftError::RoomNotOpen);
    }

    let started = sqlx::query_scalar::<_, bool>(
        "SELECT team1_ready AND team2_ready FROM turnier.draft_sessions WHERE code = $1",
    )
    .bind(code)
    .fetch_one(&mut *tx)
    .await?;
    if started {
        let (round_seconds, reserve_left1, reserve_left2): (Option<i32>, Option<i32>, Option<i32>) =
            sqlx::query_as(
                "SELECT round_seconds, team1_reserve_left, team2_reserve_left \
                 FROM turnier.draft_sessions WHERE code = $1",
            )
            .bind(code)
            .fetch_one(&mut *tx)
            .await?;
        let now = now_utc();
        let deadline = round_seconds.filter(|round| *round > 0).map(|round| {
            let reserve = reserve_left1.unwrap_or(0).max(reserve_left2.unwrap_or(0));
            now + Duration::seconds(i64::from(round) + i64::from(reserve.max(0)))
        });
        let result = sqlx::query(
            "UPDATE turnier.draft_sessions \
             SET status = 'in_progress', started_at = $1, deadline_at = $2 \
             WHERE code = $3 AND status = 'warteraum' AND team1_ready AND team2_ready",
        )
        .bind(now)
        .bind(deadline)
        .bind(code)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(DraftError::RoomNotOpen);
        }
    }
    tx.commit().await?;
    Ok(ReadyOutcome { started })
}

pub async fn leave_room(pool: &Pool, code: &str, token: &str) -> DraftResult<()> {
    let mut tx = pool.begin().await?;
    let (status, team1_token, team2_token): (String, String, String) = sqlx::query_as(
        "SELECT status, team1_token, team2_token FROM turnier.draft_sessions \
         WHERE code = $1 FOR UPDATE",
    )
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DraftError::LobbyNotFound)?;
    if status != "warteraum" {
        return Err(DraftError::RoomNotOpen);
    }
    let (claimed_column, ready_column, token_column) = if token == team1_token {
        ("team1_claimed_at", "team1_ready", "team1_token")
    } else if token == team2_token {
        ("team2_claimed_at", "team2_ready", "team2_token")
    } else {
        return Err(DraftError::InvalidToken);
    };
    // Token rotieren: der Verlassende behält sonst sein bekanntes Token und
    // damit die vollen Captain-Rechte über den nächsten Claimer (BLOCK-Fund
    // 2026-09-11). Im Warteraum läuft noch kein Draft, ein neues Token ist
    // gefahrlos möglich.
    let mut rng = StdRng::from_entropy();
    let neues_token = random_string(&mut rng, 48);
    let query = format!(
        "UPDATE turnier.draft_sessions \
         SET {claimed_column} = NULL, {ready_column} = FALSE, {token_column} = $2 \
         WHERE code = $1 AND {claimed_column} IS NOT NULL AND status = 'warteraum'"
    );
    let result = sqlx::query(&query)
        .bind(code)
        .bind(neues_token)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() != 1 {
        return Err(DraftError::InvalidToken);
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct RematchSourceRow {
    status: String,
    team1_token: String,
    team2_token: String,
    team1_name: Option<String>,
    team2_name: Option<String>,
    sequence: Option<Json<Vec<SequenceStep>>>,
    bans_per_team: i32,
    round_seconds: Option<i32>,
}

pub async fn rematch_room(pool: &Pool, code: &str, token: &str) -> DraftResult<String> {
    let now = now_utc();
    let mut tx = pool.begin().await?;
    let source: RematchSourceRow = sqlx::query_as::<_, RematchSourceRow>(
        "SELECT status, team1_token, team2_token, team1_name, team2_name, sequence, \
                bans_per_team, round_seconds \
         FROM turnier.draft_sessions WHERE code = $1 FOR UPDATE",
    )
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DraftError::LobbyNotFound)?;
    let RematchSourceRow {
        status,
        team1_token,
        team2_token,
        team1_name,
        team2_name,
        sequence,
        bans_per_team,
        round_seconds,
    } = source;
    if token != team1_token && token != team2_token {
        return Err(DraftError::InvalidToken);
    }
    if status != "completed" {
        return Err(DraftError::RematchUnavailable);
    }
    let team1_name = team1_name.unwrap_or_else(|| "Team 1".to_string());
    let team2_name = team2_name.unwrap_or_else(|| "Team 2".to_string());
    let sequence = sequence
        .map(|stored| stored.0)
        .unwrap_or_else(|| sequence_for_bans(bans_per_team));
    let mut rng = StdRng::from_entropy();
    let new_code = random_string(&mut rng, 8);
    let new_team1_token = random_string(&mut rng, 48);
    let new_team2_token = random_string(&mut rng, 48);

    let (session_id,): (i64,) = sqlx::query_as(
        "INSERT INTO turnier.draft_sessions \
             (bracket_match_id, code, team1_name, team2_name, team1_token, team2_token, \
              sequence, bans_per_team, round_seconds, reserve_seconds, team1_reserve_left, \
              team2_reserve_left, deadline_at, status, current_action_index, created_at, \
              rematch_of_code) \
         VALUES (NULL, $1, $2, $3, $4, $5, $6, $7, $8, 0, 0, 0, NULL, \
                 'warteraum', 0, $9, $10) \
         RETURNING id",
    )
    .bind(&new_code)
    .bind(&team2_name)
    .bind(&team1_name)
    .bind(&new_team1_token)
    .bind(&new_team2_token)
    .bind(Json(&sequence))
    .bind(bans_per_team)
    .bind(round_seconds)
    .bind(now)
    .bind(code)
    .fetch_one(&mut *tx)
    .await?;

    materialize_actions(&mut tx, session_id, &sequence).await?;
    tx.commit().await?;
    Ok(new_code)
}

pub async fn retry_lobby_request(pool: &Pool, code: &str, token: &str) -> DraftResult<()> {
    let mut tx = pool.begin().await?;
    let (lobby_status, team1_token, team2_token): (String, String, String) = sqlx::query_as(
        "SELECT lobby_status, team1_token, team2_token \
         FROM turnier.draft_sessions WHERE code = $1 FOR UPDATE",
    )
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DraftError::LobbyNotFound)?;
    if token != team1_token && token != team2_token {
        return Err(DraftError::InvalidToken);
    }
    match lobby_status.as_str() {
        "fehler" => {}
        "angefordert" => {
            tx.commit().await?;
            return Ok(());
        }
        _ => return Err(DraftError::RoomNotOpen),
    }
    sqlx::query(
        "UPDATE turnier.draft_sessions \
         SET lobby_status = 'angefordert', lobby_error = NULL WHERE code = $1",
    )
    .bind(code)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn materialize_actions(
    tx: &mut Transaction<'_, Postgres>,
    session_id: i64,
    steps: &[SequenceStep],
) -> DraftResult<()> {
    let mut query = QueryBuilder::<Postgres>::new(
        "INSERT INTO turnier.draft_actions \
             (session_id, sequence_index, action_type, team_slot, is_admin_forced) ",
    );
    query.push_values(steps.iter().enumerate(), |mut row, (idx, step)| {
        row.push_bind(session_id)
            .push_bind(idx as i64)
            .push_bind(step.action_type.as_str())
            .push_bind(step.team_slot.as_i64())
            .push_bind(false);
    });
    query.build().execute(&mut **tx).await?;
    Ok(())
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
    take_action_validated(pool, session_id, hero_name, taken_by, force).await
}

pub async fn take_action_with_heroes(
    pool: &Pool,
    session_id: i64,
    hero_name: &str,
    taken_by: &str,
    force: bool,
    provider: &HeroesProvider<ReqwestHeroFetcher>,
) -> DraftResult<ActionOutcome> {
    let valid = provider
        .cached()
        .map(|heroes| heroes.iter().any(|hero| hero.name == hero_name))
        .unwrap_or_else(|| crate::heroes::DEADLOCK_HEROES.contains(&hero_name));
    if !valid {
        return Err(DraftError::UnknownHero(hero_name.to_string()));
    }
    take_action_validated(pool, session_id, hero_name, taken_by, force).await
}

async fn take_action_validated(
    pool: &Pool,
    session_id: i64,
    hero_name: &str,
    taken_by: &str,
    force: bool,
) -> DraftResult<ActionOutcome> {
    let now = now_utc();
    let taken_by = parse_numeric_id(taken_by)?;
    let mut tx = pool.begin().await?;

    // (1) Index der aktuellen Session lesen — nur wenn in_progress.
    let session: Option<(i64, Option<Json<Vec<SequenceStep>>>)> = sqlx::query_as(
        "SELECT current_action_index, sequence FROM turnier.draft_sessions \
         WHERE id = $1 AND status = 'in_progress'",
    )
    .bind(session_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((idx, stored_sequence)) = session else {
        return Err(DraftError::SessionNotActive);
    };
    let session_sequence = effective_sequence(stored_sequence);

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
    let result = sqlx::query(
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
    if result.rows_affected() != 1 {
        return Err(DraftError::ActionConflict);
    }

    // (4) Index per Compare-and-Swap vorrücken + ggf. abschließen.
    let next_idx = idx + 1;
    let complete = sequence::is_complete(&session_sequence, next_idx as usize);
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
        sequence::step_at(&session_sequence, next_idx as usize)
    };
    Ok(ActionOutcome {
        is_complete: complete,
        next_action_type: next_step.map(|s| s.action_type),
        next_team_slot: next_step.map(|s| s.team_slot.as_i64()),
    })
}

/// Lädt eine freie Lobby per Code und löst vorher alle abgelaufenen Züge auf.
pub async fn get_state_by_code(pool: &Pool, code: &str) -> DraftResult<DraftState> {
    let heroes = load_heroes().await;
    get_state_by_code_loaded(pool, code, heroes).await
}

pub async fn get_state_by_code_with_heroes(
    pool: &Pool,
    code: &str,
    provider: &HeroesProvider<ReqwestHeroFetcher>,
) -> DraftResult<DraftState> {
    get_state_by_code_loaded(pool, code, provider.heroes().await).await
}

async fn get_state_by_code_loaded(
    pool: &Pool,
    code: &str,
    heroes: Vec<Hero>,
) -> DraftResult<DraftState> {
    let mut tx = pool.begin().await?;
    let mut session = load_lobby_for_update(&mut tx, code).await?;
    let mut rng = StdRng::from_entropy();
    settle_expired(&mut tx, &mut session, &heroes, &mut rng).await?;
    let state = load_state(&mut tx, session.id).await?;
    tx.commit().await?;
    Ok(state)
}

/// Führt eine Lobby-Aktion aus, sofern das Captain-Token zum aktiven Team gehört.
pub async fn take_lobby_action(
    pool: &Pool,
    code: &str,
    token: &str,
    hero_name: &str,
) -> DraftResult<ActionOutcome> {
    take_lobby_action_loaded(pool, code, token, hero_name, load_heroes().await).await
}

pub async fn take_lobby_action_with_heroes(
    pool: &Pool,
    code: &str,
    token: &str,
    hero_name: &str,
    provider: &HeroesProvider<ReqwestHeroFetcher>,
) -> DraftResult<ActionOutcome> {
    take_lobby_action_loaded(pool, code, token, hero_name, provider.heroes().await).await
}

async fn take_lobby_action_loaded(
    pool: &Pool,
    code: &str,
    token: &str,
    hero_name: &str,
    heroes: Vec<Hero>,
) -> DraftResult<ActionOutcome> {
    if !heroes.iter().any(|hero| hero.name == hero_name) {
        return Err(DraftError::UnknownHero(hero_name.to_string()));
    }

    let mut tx = pool.begin().await?;
    let mut session = load_lobby_for_update(&mut tx, code).await?;
    let mut rng = StdRng::from_entropy();
    settle_expired(&mut tx, &mut session, &heroes, &mut rng).await?;
    if session.status != "in_progress" {
        return Err(DraftError::SessionNotActive);
    }

    let token_team = if token == session.team1_token {
        1
    } else if token == session.team2_token {
        2
    } else {
        return Err(DraftError::InvalidToken);
    };
    let idx = session.current_action_index;
    let Some(step) = usize::try_from(idx)
        .ok()
        .and_then(|index| sequence::step_at(&session.sequence, index))
    else {
        return Err(DraftError::SessionNotActive);
    };
    if token_team != step.team_slot.as_i64() {
        return Err(DraftError::NotYourTurn);
    }

    let already: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM turnier.draft_actions WHERE session_id = $1 AND hero_name = $2",
    )
    .bind(session.id)
    .bind(hero_name)
    .fetch_optional(&mut *tx)
    .await?;
    if already.is_some() {
        return Err(DraftError::HeroAlreadyTaken(hero_name.to_string()));
    }

    let now = now_utc();
    let reserve_left = consume_reserve(&session, step.team_slot.as_i64(), now);
    if step.team_slot.as_i64() == 1 {
        session.team1_reserve_left = reserve_left;
    } else {
        session.team2_reserve_left = reserve_left;
    }
    let result = sqlx::query(
        "UPDATE turnier.draft_actions \
         SET hero_name = $1, taken_at = $2, is_admin_forced = FALSE, is_auto = FALSE \
         WHERE session_id = $3 AND sequence_index = $4 AND hero_name IS NULL",
    )
    .bind(hero_name)
    .bind(now)
    .bind(session.id)
    .bind(idx)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DraftError::ActionConflict);
    }

    let outcome = advance_lobby_session(&mut tx, &mut session, now, now).await?;
    tx.commit().await?;
    Ok(outcome)
}

async fn load_lobby_for_update(
    tx: &mut Transaction<'_, Postgres>,
    code: &str,
) -> DraftResult<LobbySessionRow> {
    sqlx::query_as::<_, LobbySessionRow>(
        "SELECT id, status, current_action_index, team1_token, team2_token, sequence, \
                round_seconds, team1_reserve_left, team2_reserve_left, deadline_at \
         FROM turnier.draft_sessions WHERE code = $1 FOR UPDATE",
    )
    .bind(code)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(DraftError::LobbyNotFound)
}

/// Löst abgelaufene Züge in derselben Transaktion nacheinander auf.
async fn settle_expired<R: Rng + ?Sized>(
    tx: &mut Transaction<'_, Postgres>,
    session: &mut LobbySessionRow,
    heroes: &[Hero],
    rng: &mut R,
) -> DraftResult<()> {
    if !session.round_seconds.is_some_and(|round| round > 0) {
        return Ok(());
    };
    let now = now_utc();
    while session.status == "in_progress" {
        let Some(deadline) = session.deadline_at else {
            break;
        };
        if now <= deadline {
            break;
        }
        let idx = session.current_action_index;
        let Some(step) = usize::try_from(idx)
            .ok()
            .and_then(|index| sequence::step_at(&session.sequence, index))
        else {
            break;
        };
        let used: Vec<String> = sqlx::query_scalar(
            "SELECT hero_name FROM turnier.draft_actions \
             WHERE session_id = $1 AND hero_name IS NOT NULL",
        )
        .bind(session.id)
        .fetch_all(&mut **tx)
        .await?;
        let hero = choose_auto_hero(heroes, &used, rng).ok_or(DraftError::ActionConflict)?;

        let result = sqlx::query(
            "UPDATE turnier.draft_actions \
             SET hero_name = $1, taken_at = $2, is_admin_forced = FALSE, is_auto = TRUE \
             WHERE session_id = $3 AND sequence_index = $4 AND hero_name IS NULL",
        )
        .bind(&hero.name)
        .bind(now)
        .bind(session.id)
        .bind(idx)
        .execute(&mut **tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(DraftError::ActionConflict);
        }
        if step.team_slot.as_i64() == 1 {
            session.team1_reserve_left = Some(0);
        } else {
            session.team2_reserve_left = Some(0);
        }
        advance_lobby_session(tx, session, now, deadline).await?;
    }
    Ok(())
}

fn consume_reserve(session: &LobbySessionRow, team: i64, now: DateTime<Utc>) -> Option<i32> {
    let round = session.round_seconds?;
    if round <= 0 {
        return None;
    }
    let deadline = session.deadline_at?;
    let reserve = if team == 1 {
        session.team1_reserve_left
    } else {
        session.team2_reserve_left
    }?;
    let started_at =
        deadline - Duration::seconds(i64::from(round.max(0)) + i64::from(reserve.max(0)));
    let elapsed = now.signed_duration_since(started_at).num_seconds().max(0);
    let used = (elapsed - i64::from(round.max(0))).max(0);
    Some((i64::from(reserve) - used).max(0) as i32)
}

async fn advance_lobby_session(
    tx: &mut Transaction<'_, Postgres>,
    session: &mut LobbySessionRow,
    completed_at: DateTime<Utc>,
    deadline_base: DateTime<Utc>,
) -> DraftResult<ActionOutcome> {
    let idx = session.current_action_index;
    let next_idx = idx + 1;
    let complete = sequence::is_complete(&session.sequence, next_idx as usize);
    let next_step = if complete {
        None
    } else {
        sequence::step_at(&session.sequence, next_idx as usize)
    };
    let next_deadline = match (session.round_seconds, next_step) {
        (Some(round), Some(step)) if round > 0 => {
            let reserve = if step.team_slot.as_i64() == 1 {
                session.team1_reserve_left.unwrap_or(0)
            } else {
                session.team2_reserve_left.unwrap_or(0)
            };
            Some(
                deadline_base
                    + Duration::seconds(i64::from(round.max(0)) + i64::from(reserve.max(0))),
            )
        }
        _ => None,
    };
    let result = sqlx::query(
        "UPDATE turnier.draft_sessions \
         SET current_action_index = $1, \
             status = CASE WHEN $2 THEN 'completed' ELSE status END, \
             completed_at = CASE WHEN $2 THEN $3 ELSE completed_at END, \
             team1_reserve_left = $4, team2_reserve_left = $5, deadline_at = $6, \
             lobby_status = CASE WHEN $2 AND lobby_status <> 'keine' \
                            THEN 'angefordert' ELSE lobby_status END \
         WHERE id = $7 AND current_action_index = $8 AND status = 'in_progress'",
    )
    .bind(next_idx)
    .bind(complete)
    .bind(completed_at)
    .bind(session.team1_reserve_left)
    .bind(session.team2_reserve_left)
    .bind(next_deadline)
    .bind(session.id)
    .bind(idx)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(DraftError::ActionConflict);
    }
    session.current_action_index = next_idx;
    session.deadline_at = next_deadline;
    if complete {
        session.status = "completed".to_string();
    }
    Ok(ActionOutcome {
        is_complete: complete,
        next_action_type: next_step.map(|step| step.action_type),
        next_team_slot: next_step.map(|step| step.team_slot.as_i64()),
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
        sequence::step_at(&session.sequence, idx as usize)
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

fn effective_sequence(stored: Option<Json<Vec<SequenceStep>>>) -> Vec<SequenceStep> {
    stored
        .map(|sequence| sequence.0)
        .unwrap_or_else(|| DEFAULT_SEQUENCE.to_vec())
}

fn random_string<R: Rng + ?Sized>(rng: &mut R, len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    (0..len)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

fn choose_auto_hero<'a, R: Rng + ?Sized>(
    heroes: &'a [Hero],
    used: &[String],
    rng: &mut R,
) -> Option<&'a Hero> {
    heroes
        .iter()
        .filter(|hero| !used.contains(&hero.name))
        .choose(rng)
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::*;

    #[test]
    fn auto_heldenwahl_ist_seedbar_und_ueberspringt_vergebene_helden() {
        let heroes = [
            Hero {
                id: 1,
                name: "Vergeben".to_string(),
                image_url: String::new(),
                card_image_url: String::new(),
            },
            Hero {
                id: 2,
                name: "Frei A".to_string(),
                image_url: String::new(),
                card_image_url: String::new(),
            },
            Hero {
                id: 3,
                name: "Frei B".to_string(),
                image_url: String::new(),
                card_image_url: String::new(),
            },
        ];
        let used = vec!["Vergeben".to_string()];
        let mut first_rng = StdRng::seed_from_u64(7);
        let mut second_rng = StdRng::seed_from_u64(7);

        let first = choose_auto_hero(&heroes, &used, &mut first_rng).expect("freier Held");
        let second = choose_auto_hero(&heroes, &used, &mut second_rng).expect("freier Held");

        assert_eq!(first.name, second.name);
        assert_ne!(first.name, "Vergeben");
    }
}
