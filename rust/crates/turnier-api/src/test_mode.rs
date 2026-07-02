//! Test-Modus-Router — portiert `admin/test_mode.py`.
//!
//! Stellt Moderatoren ein HTTP-Toolkit bereit, um Turnier-Abläufe ohne echte
//! Spieler zu testen: synthetische Test-User (reservierter Snowflake-Bereich) inkl. Zufallsrang
//! anlegen, vollständige Test-Turniere (Teams, Signups, Check-ins, optional bis
//! Bracket vorgerückt) generieren, Runden mit Zufalls-Gewinnern simulieren und
//! alle Test-Daten wieder aufräumen. Alle erzeugten Turniere tragen `is_test=1`;
//! mutierende Endpunkte verweigern Arbeit an Nicht-Test-Turnieren.
//!
//! ## Gate (Abweichung vom Python-Original)
//! Im Python-Original wird der Router in `main.py` UNBEDINGT gemountet (kein
//! Env-/Prod-Guard — siehe `bugs_preserved`). Der Port hängt die Registrierung
//! an das Env-Flag `TURNIER_ENABLE_TEST_MODE` (Default **true** = wie Python
//! immer an); ist es explizit ausgeschaltet, liefert [`router`] einen leeren
//! Router. So bleibt das Default-Verhalten 1:1, ohne den Prod-Kill-Switch zu
//! verlieren.
//!
//! ## Match-Simulation
//! Läuft über die ECHTEN Engine-Funktionen
//! ([`turnier_engine::finalize_checkin`], [`turnier_scheduler::advance_tournament_status`],
//! [`turnier_match::MatchManager::apply_group_match_result`] /
//! [`turnier_match::MatchManager::apply_bracket_match_result`]) — keine privaten
//! `_`-Funktionen nachgebaut.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::routing::{delete, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, QueryBuilder};

use turnier_config::Config;
use turnier_core::TournamentGameMode;
use turnier_engine::FinalizeCheckinParams;
use turnier_match::{ApplyBracketParams, ApplyGroupParams};

use crate::db;
use crate::error::{WebError, WebResult};
use crate::extract::ModUser;
use crate::state::AppState;

/// Aktive Turnierstatus, in denen Test-User nicht gelöscht werden dürfen
/// (entspricht dem Status-Filter aller drei Lösch-Guards im Original).
const ACTIVE_TOURNAMENT_STATUSES: [&str; 4] = ["registration", "checkin", "group_phase", "bracket"];

/// Match-Status, die als abgeschlossen gelten (nicht mehr simulierbar).
const FINISHED_MATCH_STATUSES: [&str; 3] = ["completed", "forfeit", "cancelled"];

/// Reservierter BIGINT-Bereich fuer synthetische Test-Discord-IDs.
///
/// Historisch nutzte der SQLite-Testmodus `test_123456` als Text-ID. Die zentrale
/// PG-Grenze speichert Discord-IDs als BIGINT; deshalb bleiben Test-User an der
/// HTTP-Grenze Strings, liegen intern aber in diesem hohen, klar reservierten
/// Snowflake-aehnlichen Bereich.
const TEST_DISCORD_ID_BASE: i64 = 9_100_000_000_000_000_000;
const TEST_DISCORD_ID_LIMIT: i64 = TEST_DISCORD_ID_BASE + 1_000_000;

/// Router der Test-Modus-Endpunkte. Liefert einen leeren Router, wenn der
/// Test-Modus per Env ausgeschaltet ist (`TURNIER_ENABLE_TEST_MODE=0`); Default
/// ist an — wie im Python-Original, das den Router immer mountet.
pub fn router(_config: &Config) -> Router<AppState> {
    if !test_mode_enabled() {
        return Router::new();
    }
    Router::new()
        .route(
            "/api/admin/test/users",
            post(create_test_users)
                .get(list_test_users)
                .delete(delete_test_users),
        )
        .route("/api/admin/test/tournaments", post(create_test_tournament))
        .route(
            "/api/admin/test/tournaments/{tournament_id}/simulate-round",
            post(simulate_test_tournament_round),
        )
        .route("/api/admin/test/wipe", delete(wipe_test_data))
}

/// Gate-Flag des Test-Modus. Default **an** (= wie Python, das den Router immer
/// mountet); nur ein explizit „aus"-Wert (`0/false/no/off`) deaktiviert ihn.
fn test_mode_enabled() -> bool {
    let raw = turnier_config::secrets::get_first_string(&["TURNIER_ENABLE_TEST_MODE"], "1");
    !matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

// ---------------------------------------------------------------------------
// Request-/Response-DTOs (1:1 zu den Pydantic-Modellen)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TestUsersCreateRequest {
    count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct TestUserCreated {
    discord_id: String,
    display_name: String,
}

#[derive(Debug, Serialize)]
struct TestUserOut {
    discord_id: String,
    display_name: String,
    rank: Option<String>,
}

#[derive(Debug, Serialize)]
struct TestUsersCreateResponse {
    created: Vec<TestUserCreated>,
}

#[derive(Debug, Serialize)]
struct TestUsersDeleteResponse {
    deleted: i64,
}

#[derive(Debug, Deserialize)]
struct TestTournamentCreateRequest {
    name: String,
    team_size: i64,
    num_teams: i64,
    mode: String,
    #[serde(default = "default_game_mode")]
    tournament_game_mode: TournamentGameMode,
    #[serde(default = "default_advance_to")]
    advance_to: String,
}

fn default_game_mode() -> TournamentGameMode {
    TournamentGameMode::Standard
}
fn default_advance_to() -> String {
    "bracket".to_string()
}

#[derive(Debug, Serialize)]
struct TestTournamentCreateResponse {
    tournament_id: i64,
}

#[derive(Debug, Serialize)]
struct SimulateRoundResponse {
    simulated_matches: i64,
}

#[derive(Debug, Serialize)]
struct TestWipeResponse {
    deleted_tournaments: i64,
    deleted_users: i64,
}

// ---------------------------------------------------------------------------
// Mini-PRNG (kein rand-Dependency in turnier-api)
// ---------------------------------------------------------------------------

/// Globaler Zähler, der mit der Systemzeit zum SplitMix64-Seed gemischt wird —
/// stellt sicher, dass aufeinanderfolgende Aufrufe verschiedene Sequenzen
/// liefern (wie ein ungeseedeter `random`-Aufruf). Test-Tooling, daher genügt
/// einfache Streuung statt kryptografischer Qualität.
static RNG_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Selbstständiger SplitMix64-Generator. Ersetzt `random.randint/choice/shuffle`
/// (1:1 im Effekt: gleichverteilte Zufallsauswahl, ohne Seed/Reproduzierbarkeit
/// — wie im Original, siehe `bugs_preserved`).
struct Rng(u64);

impl Rng {
    /// Neuer Generator, geseedet aus Systemzeit (ns) XOR globalem Zähler.
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let counter = RNG_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(nanos ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// Nächster 64-Bit-Wert (SplitMix64).
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Ganzzahl in `[low, high]` (beide inklusive — wie `random.randint`).
    fn randint(&mut self, low: i64, high: i64) -> i64 {
        debug_assert!(high >= low);
        let span = (high - low + 1) as u64;
        low + (self.next_u64() % span) as i64
    }

    /// Fisher-Yates-Shuffle in-place (wie `random.shuffle`).
    fn shuffle<T>(&mut self, items: &mut [T]) {
        if items.len() < 2 {
            return;
        }
        for i in (1..items.len()).rev() {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            items.swap(i, j);
        }
    }

    /// Eines von zwei Elementen wählen (wie `random.choice([a, b])`).
    fn choice2(&mut self, a: i64, b: i64) -> i64 {
        if (self.next_u64() & 1) == 0 {
            a
        } else {
            b
        }
    }
}

// ---------------------------------------------------------------------------
// Test-User-Helfer
// ---------------------------------------------------------------------------

/// Eine `(discord_id, display_name, rank)`-Zeile aus dem Test-User-JOIN.
#[derive(Debug, Clone, sqlx::FromRow)]
struct TestUserRow {
    discord_id: i64,
    display_name: Option<String>,
    rank: Option<String>,
}

/// Eine Pool-Zeile inkl. `rank_score` für die Turnier-Generierung.
#[derive(Debug, Clone, sqlx::FromRow)]
struct PoolUserRow {
    discord_id: i64,
    display_name: Option<String>,
    rank: Option<String>,
    rank_score: Option<i64>,
}

/// Erzeugt einen eindeutigen 6-stelligen Suffix (`random.randint`, max 1000
/// Versuche; sonst Fehler — 1:1 zum Original `_test_suffix`).
fn test_suffix(rng: &mut Rng, existing_ids: &mut HashSet<i64>) -> WebResult<(String, i64)> {
    for _ in 0..1000 {
        let suffix_number = rng.randint(0, 999_999);
        let suffix = format!("{suffix_number:06}");
        let discord_id = TEST_DISCORD_ID_BASE + suffix_number;
        if !existing_ids.contains(&discord_id) {
            existing_ids.insert(discord_id);
            return Ok((suffix, discord_id));
        }
    }
    Err(WebError::internal(
        "Konnte keine eindeutige Test-Discord-ID erzeugen",
    ))
}

/// Zufälliger Rang: `(name, tier, subrank, score)` mit `tier ∈ 1..=11`,
/// `subrank ∈ 1..=6`, `score = tier*100 + subrank` (1:1 zu `_rank_payload`).
fn rank_payload(rng: &mut Rng) -> (String, i64, i64, i64) {
    let rank_tier = rng.randint(1, 11);
    let subrank = rng.randint(1, 6);
    let rank_name = turnier_steam::rank_name_for_tier(rank_tier)
        .expect("tier 1..=11 hat einen Namen")
        .to_string();
    let rank_score = rank_tier * 100 + subrank;
    (rank_name, rank_tier, subrank, rank_score)
}

/// Legt `count` Test-User in `user_profiles` + `rank_cache` an (auf einer
/// Connection innerhalb der Transaktion). Liefert die erzeugten
/// `(discord_id, display_name)`. Portiert `_create_test_users_in_db`.
async fn create_test_users_in_db(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    rng: &mut Rng,
    count: i64,
) -> WebResult<Vec<TestUserCreated>> {
    ensure_throwaway_test_db_tx(tx).await?;
    let existing: Vec<(i64,)> = sqlx::query_as(
        r#"SELECT discord_id FROM turnier."user_profiles"
           WHERE discord_id >= $1 AND discord_id < $2"#,
    )
    .bind(TEST_DISCORD_ID_BASE)
    .bind(TEST_DISCORD_ID_LIMIT)
    .fetch_all(&mut **tx)
    .await?;
    let mut existing_ids: HashSet<i64> = existing.into_iter().map(|(d,)| d).collect();

    let mut created = Vec::new();

    for _ in 0..count {
        let (suffix, discord_id) = test_suffix(rng, &mut existing_ids)?;
        let display_name = format!("Test User {suffix}");
        let (rank_name, rank_tier, subrank, rank_score) = rank_payload(rng);

        sqlx::query(
            r#"INSERT INTO turnier."user_profiles"
             (discord_id, display_name, invite_auto_accept, notify_discord_dm, notify_browser,
              notify_match_start, notify_checkin, notify_team_invite, notify_tournament_news,
              notify_registration_reminder, updated_at)
             VALUES ($1, $2, false, false, false, false, false, false, false, false, now())"#,
        )
        .bind(discord_id)
        .bind(&display_name)
        .execute(&mut **tx)
        .await?;

        sqlx::query(
            r#"INSERT INTO turnier."rank_cache"
             (discord_id, source, steam_id, rank, rank_tier, subrank, rank_score, cached_at)
             VALUES ($1, 'test_mode', NULL, $2, $3, $4, $5, now())"#,
        )
        .bind(discord_id)
        .bind(&rank_name)
        .bind(rank_tier)
        .bind(subrank)
        .bind(rank_score)
        .execute(&mut **tx)
        .await?;

        created.push(TestUserCreated {
            discord_id: db::discord_id_to_string(discord_id),
            display_name,
        });
    }

    Ok(created)
}

/// Lädt alle Test-User mit Rang (LEFT JOIN ohne Eindeutigkeits-Garantie — kann
/// bei mehrfachen rank_cache-Zeilen duplizieren, 1:1 zum Original
/// `_load_test_users`).
async fn load_test_users<'e, E>(executor: E) -> WebResult<Vec<TestUserRow>>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let rows = sqlx::query_as::<_, TestUserRow>(
        r#"SELECT up.discord_id, up.display_name, rc.rank
         FROM turnier."user_profiles" up
         LEFT JOIN turnier."rank_cache" rc ON rc.discord_id = up.discord_id
         WHERE up.discord_id >= $1 AND up.discord_id < $2
         ORDER BY up.display_name, up.discord_id"#,
    )
    .bind(TEST_DISCORD_ID_BASE)
    .bind(TEST_DISCORD_ID_LIMIT)
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

/// Prüft, ob Test-User noch in aktiven Turnieren referenziert sind → 409.
/// Portiert die drei Guards aus `_ensure_test_user_deletion_allowed` zu EINEM
/// EXISTS-Query (safe: identische Status-Liste, identisches Verhalten).
async fn ensure_test_user_deletion_allowed<'e, E>(executor: E) -> WebResult<()>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let mut query = QueryBuilder::new(
        r#"SELECT 1::BIGINT WHERE EXISTS (
            SELECT 1 FROM turnier."team_members" tm
            JOIN turnier."teams" t ON t.id = tm.team_id
            JOIN turnier."tournaments" tr ON tr.id = t.tournament_id
            WHERE tm.discord_id >= "#,
    );
    query
        .push_bind(TEST_DISCORD_ID_BASE)
        .push(" AND tm.discord_id < ")
        .push_bind(TEST_DISCORD_ID_LIMIT)
        .push(" AND tr.status IN (");
    {
        let mut separated = query.separated(", ");
        for status in ACTIVE_TOURNAMENT_STATUSES {
            separated.push_bind(status);
        }
    }
    query.push(
        r#")
         ) OR EXISTS (
            SELECT 1 FROM turnier."tournament_signups" ts
            JOIN turnier."tournaments" tr ON tr.id = ts.tournament_id
            WHERE ts.discord_id >= "#,
    );
    query
        .push_bind(TEST_DISCORD_ID_BASE)
        .push(" AND ts.discord_id < ")
        .push_bind(TEST_DISCORD_ID_LIMIT)
        .push(" AND tr.status IN (");
    {
        let mut separated = query.separated(", ");
        for status in ACTIVE_TOURNAMENT_STATUSES {
            separated.push_bind(status);
        }
    }
    query.push(
        r#")
         ) OR EXISTS (
            SELECT 1 FROM turnier."tournament_checkins" tc
            JOIN turnier."tournaments" tr ON tr.id = tc.tournament_id
            WHERE tc.discord_id >= "#,
    );
    query
        .push_bind(TEST_DISCORD_ID_BASE)
        .push(" AND tc.discord_id < ")
        .push_bind(TEST_DISCORD_ID_LIMIT)
        .push(" AND tr.status IN (");
    {
        let mut separated = query.separated(", ");
        for status in ACTIVE_TOURNAMENT_STATUSES {
            separated.push_bind(status);
        }
    }
    query.push(") LIMIT 1");
    let referenced: Option<(i64,)> = query.build_query_as().fetch_optional(executor).await?;
    if referenced.is_some() {
        return Err(WebError::conflict(
            "Test-User sind noch in laufenden Turnieren referenziert",
        ));
    }
    Ok(())
}

/// Löscht alle Test-User (zählt vorher) aus sessions/user_consents/
/// player_points/rank_cache/user_profiles. Portiert `_delete_test_users`.
///
/// bug-preserved: räumt NUR diese fünf Tabellen — team_applications,
/// team_invitations, checkins, match_result_reports (reported_by/resolved_by)
/// und turnierfremde Signups/Check-ins bleiben als Orphans liegen (siehe
/// `bugs_preserved`).
async fn delete_test_users_in_db(tx: &mut sqlx::Transaction<'_, Postgres>) -> WebResult<i64> {
    ensure_throwaway_test_db_tx(tx).await?;
    let deleted: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM turnier."user_profiles"
           WHERE discord_id >= $1 AND discord_id < $2"#,
    )
    .bind(TEST_DISCORD_ID_BASE)
    .bind(TEST_DISCORD_ID_LIMIT)
    .fetch_one(&mut **tx)
    .await?;
    for table in [
        r#"turnier."sessions""#,
        r#"turnier."user_consents""#,
        r#"turnier."player_points""#,
        r#"turnier."rank_cache""#,
        r#"turnier."user_profiles""#,
    ] {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE discord_id >= $1 AND discord_id < $2"
        ))
        .bind(TEST_DISCORD_ID_BASE)
        .bind(TEST_DISCORD_ID_LIMIT)
        .execute(&mut **tx)
        .await?;
    }
    Ok(deleted)
}

// ---------------------------------------------------------------------------
// turnier-api-lokale Helfer (Audit / Tournament-Load / Tree-Löschung)
// ---------------------------------------------------------------------------

/// Schreibt einen Audit-Log-Eintrag (Executor = aktuelle Verbindung/Tx).
/// Spiegelt `tournament.admin_routes._audit`.
async fn audit<'e, E>(
    executor: E,
    action: &str,
    user_id: &str,
    details: serde_json::Value,
) -> WebResult<()>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let user_id = db::parse_actor_id(user_id)?;
    sqlx::query(
        r#"INSERT INTO turnier."audit_log" (action, user_id, details, created_at)
           VALUES ($1, $2, $3, now())"#,
    )
    .bind(action)
    .bind(user_id)
    .bind(details)
    .execute(executor)
    .await?;
    Ok(())
}

/// Status + `is_test` eines Turniers.
#[derive(Debug, sqlx::FromRow)]
struct TournamentFlags {
    status: String,
    is_test: bool,
}

/// Lädt Status/`is_test` oder liefert 404. Stellt zusätzlich sicher, dass es
/// sich um ein Test-Turnier handelt (sonst 400). Spiegelt
/// `_load_tournament_or_404` + `_ensure_test_tournament`.
async fn ensure_test_tournament<'e, E>(
    executor: E,
    tournament_id: i64,
) -> WebResult<TournamentFlags>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let row: Option<TournamentFlags> =
        sqlx::query_as(r#"SELECT status, is_test FROM turnier."tournaments" WHERE id = $1"#)
            .bind(tournament_id)
            .fetch_optional(executor)
            .await?;
    let Some(flags) = row else {
        return Err(WebError::not_found("Turnier nicht gefunden"));
    };
    if !flags.is_test {
        return Err(WebError::bad_request(
            "Nur Test-Turniere dürfen über diesen Endpoint verändert werden",
        ));
    }
    Ok(flags)
}

/// Löscht den kompletten Turnier-Baum (Gruppen/Bracket/Teams/Signups/Check-ins)
/// und das Turnier selbst. Der Einstieg ist hart auf die Wegwerf-Test-DB gegatet.
async fn delete_tournament_tree(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<()> {
    ensure_throwaway_test_db_tx(tx).await?;
    let group_ids: Vec<i64> =
        sqlx::query_scalar(r#"SELECT id FROM turnier."groups" WHERE tournament_id = $1"#)
            .bind(tournament_id)
            .fetch_all(&mut **tx)
            .await?;

    if !group_ids.is_empty() {
        sqlx::query(
            r#"DELETE FROM turnier."match_results" WHERE group_match_id IN
             (SELECT id FROM turnier."group_matches" WHERE group_id = ANY($1))"#,
        )
        .bind(&group_ids)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"DELETE FROM turnier."checkins" WHERE match_type = 'group' AND match_id IN
             (SELECT id FROM turnier."group_matches" WHERE group_id = ANY($1))"#,
        )
        .bind(&group_ids)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"DELETE FROM turnier."match_result_reports" WHERE match_type = 'group' AND match_id IN
             (SELECT id FROM turnier."group_matches" WHERE group_id = ANY($1))"#,
        )
        .bind(&group_ids)
        .execute(&mut **tx)
        .await?;
        sqlx::query(r#"DELETE FROM turnier."group_matches" WHERE group_id = ANY($1)"#)
            .bind(&group_ids)
            .execute(&mut **tx)
            .await?;
        sqlx::query(r#"DELETE FROM turnier."group_teams" WHERE group_id = ANY($1)"#)
            .bind(&group_ids)
            .execute(&mut **tx)
            .await?;
        sqlx::query(r#"DELETE FROM turnier."groups" WHERE id = ANY($1)"#)
            .bind(&group_ids)
            .execute(&mut **tx)
            .await?;
    }

    sqlx::query(
        r#"DELETE FROM turnier."match_results" WHERE bracket_match_id IN
         (SELECT id FROM turnier."bracket_matches" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"DELETE FROM turnier."checkins" WHERE match_type = 'bracket' AND match_id IN
         (SELECT id FROM turnier."bracket_matches" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"DELETE FROM turnier."match_result_reports" WHERE match_type = 'bracket' AND match_id IN
         (SELECT id FROM turnier."bracket_matches" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"DELETE FROM turnier."match_casters" WHERE match_type = 'bracket' AND match_id IN
         (SELECT id FROM turnier."bracket_matches" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;

    clear_bracket_tree(tx, tournament_id).await?;

    let proposal_ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT id FROM turnier."tournament_proposals" WHERE tournament_id = $1"#,
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;
    if !proposal_ids.is_empty() {
        sqlx::query(
            r#"DELETE FROM turnier."tournament_proposal_feedback" WHERE proposal_id = ANY($1)"#,
        )
        .bind(&proposal_ids)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"DELETE FROM turnier."tournament_proposal_votes" WHERE proposal_id = ANY($1)"#,
        )
        .bind(&proposal_ids)
        .execute(&mut **tx)
        .await?;
    }

    sqlx::query(
        r#"DELETE FROM turnier."team_members" WHERE team_id IN
         (SELECT id FROM turnier."teams" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"DELETE FROM turnier."team_applications" WHERE team_id IN
         (SELECT id FROM turnier."teams" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(r#"DELETE FROM turnier."team_invitations" WHERE tournament_id = $1"#)
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;

    for sql in [
        r#"DELETE FROM turnier."tournament_checkins" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."tournament_signups" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."tournament_casters" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."sent_tournament_reminders" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."sent_start_reminders" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."tournament_signals" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."tournament_proposals" WHERE tournament_id = $1"#,
    ] {
        sqlx::query(sql)
            .bind(tournament_id)
            .execute(&mut **tx)
            .await?;
    }

    sqlx::query(r#"DELETE FROM turnier."teams" WHERE tournament_id = $1"#)
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"DELETE FROM turnier."tournaments" WHERE id = $1"#)
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Räumt den Bracket-Teilbaum (Matches + Mini-Groups).
async fn clear_bracket_tree(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<()> {
    sqlx::query(
        r#"UPDATE turnier."bracket_mini_groups" SET advances_to_match_id = NULL WHERE tournament_id = $1"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"UPDATE turnier."bracket_mini_group_teams" SET source_match_id = NULL
         WHERE mini_group_id IN (SELECT id FROM turnier."bracket_mini_groups" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"DELETE FROM turnier."match_games" WHERE bracket_match_id IN
         (SELECT id FROM turnier."bracket_matches" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(r#"DELETE FROM turnier."bracket_matches" WHERE tournament_id = $1"#)
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;

    let mini_group_ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT id FROM turnier."bracket_mini_groups" WHERE tournament_id = $1"#,
    )
    .bind(tournament_id)
    .fetch_all(&mut **tx)
    .await?;
    if !mini_group_ids.is_empty() {
        sqlx::query(
            r#"DELETE FROM turnier."bracket_mini_group_teams" WHERE mini_group_id = ANY($1)"#,
        )
        .bind(&mini_group_ids)
        .execute(&mut **tx)
        .await?;
        sqlx::query(r#"DELETE FROM turnier."bracket_mini_groups" WHERE id = ANY($1)"#)
            .bind(&mini_group_ids)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn ensure_throwaway_test_db_tx(tx: &mut sqlx::Transaction<'_, Postgres>) -> WebResult<()> {
    ensure_throwaway_test_db(&mut **tx).await
}

/// Erlaubt Testdaten-Mutationen nur gegen die Wegwerf-Test-DB.
///
/// Neben der DSN-Gleichheit braucht es `TURNIER_TEST_DB_CONFIRM=throwaway-only`.
/// Diese zweite, nur vom Test-Harness gesetzte Bedingung schützt vor reiner
/// Env-Var-Verwechslung, bei der Test- und aktive DSN versehentlich auf eine
/// echte zentrale DB zeigen.
async fn ensure_throwaway_test_db<'e, E>(executor: E) -> WebResult<()>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let central_test_dsn = std::env::var("CENTRAL_TEST_DSN").ok();
    let active_dsn = std::env::var("DEADLOCK_CENTRAL_DSN").ok();
    let env_allows_throwaway =
        matches!((central_test_dsn, active_dsn), (Some(test), Some(active)) if test == active);
    let confirmed_throwaway =
        std::env::var("TURNIER_TEST_DB_CONFIRM").ok().as_deref() == Some("throwaway-only");
    if !env_allows_throwaway || !confirmed_throwaway {
        return Err(WebError::forbidden(
            "Test-Daten-Mutation ist nur gegen die zentrale Wegwerf-Test-DB erlaubt",
        ));
    }

    let table_exists: Option<(i64,)> = sqlx::query_as(
        "SELECT 1::BIGINT FROM information_schema.tables WHERE table_schema = 'turnier' AND table_name = 'tournaments'",
    )
    .fetch_optional(executor)
    .await?;
    if table_exists.is_none() {
        return Err(WebError::forbidden(
            "Test-Daten-Mutation ist nur gegen initialisierte Test-Schema erlaubt",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Endpunkte: Test-User
// ---------------------------------------------------------------------------

/// `POST /api/admin/test/users` — `count` Test-User anlegen (require_mod).
async fn create_test_users(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Json(body): Json<TestUsersCreateRequest>,
) -> WebResult<Json<TestUsersCreateResponse>> {
    validate_range(body.count, 1, 100, "count")?;
    ensure_throwaway_test_db(&state.pool).await?;

    let mut rng = Rng::new();
    let mut tx = state.pool.begin().await?;
    let created = create_test_users_in_db(&mut tx, &mut rng, body.count).await?;
    audit(
        &mut *tx,
        "test_users_create",
        &user.discord_id,
        json!({
            "count": body.count,
            "created_ids": created.iter().map(|c| c.discord_id.clone()).collect::<Vec<_>>(),
        }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(TestUsersCreateResponse { created }))
}

/// `GET /api/admin/test/users` — alle Test-User mit Rang (require_mod). Kein Audit.
async fn list_test_users(
    State(state): State<AppState>,
    _user: ModUser,
) -> WebResult<Json<Vec<TestUserOut>>> {
    let rows = load_test_users(&state.pool).await?;
    let out = rows
        .into_iter()
        .map(|r| TestUserOut {
            discord_id: db::discord_id_to_string(r.discord_id),
            // Pydantic-Modell verlangt display_name (str); im Original kommt er
            // direkt aus der Zeile (kann theoretisch NULL sein → leerer String).
            display_name: r.display_name.unwrap_or_default(),
            rank: r.rank,
        })
        .collect();
    Ok(Json(out))
}

/// `DELETE /api/admin/test/users` — alle Test-User löschen, sofern nicht in
/// aktiven Turnieren referenziert (sonst 409) (require_mod).
async fn delete_test_users(
    State(state): State<AppState>,
    ModUser(user): ModUser,
) -> WebResult<Json<TestUsersDeleteResponse>> {
    ensure_throwaway_test_db(&state.pool).await?;
    let mut tx = state.pool.begin().await?;
    ensure_test_user_deletion_allowed(&mut *tx).await?;
    let deleted = delete_test_users_in_db(&mut tx).await?;
    audit(
        &mut *tx,
        "test_users_delete",
        &user.discord_id,
        json!({ "deleted": deleted }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(TestUsersDeleteResponse { deleted }))
}

// ---------------------------------------------------------------------------
// Endpunkt: Test-Turnier anlegen
// ---------------------------------------------------------------------------

/// `POST /api/admin/test/tournaments` — vollständiges Test-Turnier generieren
/// und optional bis group_phase/bracket vorrücken (require_mod).
async fn create_test_tournament(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Json(body): Json<TestTournamentCreateRequest>,
) -> WebResult<Json<TestTournamentCreateResponse>> {
    ensure_throwaway_test_db(&state.pool).await?;
    validate_range(body.team_size, 1, 12, "team_size")?;
    validate_range(body.num_teams, 2, 128, "num_teams")?;
    if body.mode != "bracket_only" && body.mode != "group_then_bracket" {
        return Err(WebError::unprocessable("mode ist ungültig"));
    }
    if body.advance_to != "bracket"
        && body.advance_to != "group_phase"
        && body.advance_to != "checkin"
    {
        return Err(WebError::unprocessable("advance_to ist ungültig"));
    }

    if body.mode == "bracket_only" && body.advance_to == "group_phase" {
        return Err(WebError::bad_request(
            "group_phase ist für bracket_only nicht verfügbar",
        ));
    }

    let required_users = body.team_size * body.num_teams;
    let registration_start = chrono::Utc::now();
    let registration_end = registration_start + chrono::Duration::hours(1);

    let mut rng = Rng::new();

    // --- Turnier-Erstellung als EINE Transaktion (Rollback automatisch bei Fehler) ---
    let mut tx = state.pool.begin().await?;

    // Pool auffüllen, falls zu wenige Test-User existieren.
    // bug-preserved: missing = required - count(aller bestehenden Test-User),
    // anschließend wird per LIMIT eine beliebige (nach discord_id sortierte)
    // Teilmenge genommen — fragil, aber 1:1 zum Original.
    let existing_users = load_test_users(&mut *tx).await?;
    let missing_users = required_users - existing_users.len() as i64;
    if missing_users > 0 {
        create_test_users_in_db(&mut tx, &mut rng, missing_users).await?;
    }

    let mut user_rows: Vec<PoolUserRow> = sqlx::query_as::<_, PoolUserRow>(
        r#"SELECT up.discord_id, up.display_name, rc.rank, rc.rank_score
         FROM turnier."user_profiles" up
         LEFT JOIN turnier."rank_cache" rc ON rc.discord_id = up.discord_id
         WHERE up.discord_id >= $1 AND up.discord_id < $2
         ORDER BY up.discord_id
         LIMIT $3"#,
    )
    .bind(TEST_DISCORD_ID_BASE)
    .bind(TEST_DISCORD_ID_LIMIT)
    .bind(required_users)
    .fetch_all(&mut *tx)
    .await?;

    if (user_rows.len() as i64) < required_users {
        return Err(WebError::internal(
            "Test-User-Pool konnte nicht vollständig erzeugt werden",
        ));
    }

    rng.shuffle(&mut user_rows);

    let tournament_id: i64 = sqlx::query_scalar(
        r#"INSERT INTO turnier."tournaments" (
            name, status, description, team_size, registration_start, registration_end,
            checkin_start, group_phase_start, bracket_start, bracket_format, created_by,
            tournament_mode, tournament_game_mode, auto_lobby_enabled,
            exclude_from_leaderboard, is_test, reminder_offsets, start_reminder_offsets,
            created_at, updated_at, invite_mode, series_format, match_objective,
            no_show_grace_minutes, source
         )
         VALUES ($1, 'checkin', $2, $3, $4, $5, $6, $7, $8, 'single_elimination', $9,
                 $10, $11, false, true, true, $12, $13, now(), now(), 'always', 1,
                 'match_win', 10, 'test_mode')
         RETURNING id"#,
    )
    .bind(&body.name)
    .bind("Autogenerated test tournament")
    .bind(body.team_size)
    .bind(registration_start)
    .bind(registration_end)
    .bind(registration_start)
    .bind(registration_start)
    .bind(registration_start)
    .bind(db::parse_actor_id(&user.discord_id)?)
    .bind(normalize_test_mode(&body.mode))
    .bind(body.tournament_game_mode)
    // bug-preserved (safe): reminder_offsets wird hart als Default gesetzt
    // (identisch zum Schema-Default), exakt wie im Original.
    .bind(json!([1440, 120, 15]))
    .bind(json!([120, 15]))
    .fetch_one(&mut *tx)
    .await?;

    let mut existing_keys: HashSet<String> = HashSet::new();
    for team_index in 0..body.num_teams {
        let start = (team_index * body.team_size) as usize;
        let end = ((team_index + 1) * body.team_size) as usize;
        let members = &user_rows[start..end];
        let captain = &members[0];
        let captain_base = captain
            .display_name
            .clone()
            .unwrap_or_else(|| db::discord_id_to_string(captain.discord_id));
        let (team_name, name_key) = team_name(&captain_base, &mut existing_keys);

        let team_id: i64 = sqlx::query_scalar(
            r#"INSERT INTO turnier."teams"
             (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status)
             VALUES ($1, $2, $3, $4, now(), 'open') RETURNING id"#,
        )
        .bind(tournament_id)
        .bind(&team_name)
        .bind(&name_key)
        .bind(captain.discord_id)
        .fetch_one(&mut *tx)
        .await?;

        for (member_index, member) in members.iter().enumerate() {
            let role = if member_index == 0 {
                "captain"
            } else {
                "member"
            };
            let display_name = member
                .display_name
                .clone()
                .unwrap_or_else(|| db::discord_id_to_string(member.discord_id));
            let rank_score = member.rank_score.unwrap_or(0);

            sqlx::query(
                r#"INSERT INTO turnier."team_members"
                 (team_id, discord_id, discord_name, steam_id, rank, rank_score, role, joined_at)
                 VALUES ($1, $2, $3, NULL, $4, $5, $6, now())"#,
            )
            .bind(team_id)
            .bind(member.discord_id)
            .bind(&display_name)
            .bind(&member.rank)
            .bind(rank_score)
            .bind(role)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                r#"INSERT INTO turnier."tournament_signups"
                 (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at)
                 VALUES ($1, $2, $3, NULL, $4, $5, $6, now())"#,
            )
            .bind(tournament_id)
            .bind(member.discord_id)
            .bind(&display_name)
            .bind(&member.rank)
            .bind(rank_score)
            .bind(team_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                r#"INSERT INTO turnier."tournament_checkins" (tournament_id, discord_id, checked_in_at)
                 VALUES ($1, $2, now())"#,
            )
            .bind(tournament_id)
            .bind(member.discord_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    audit(
        &mut *tx,
        "test_tournament_create",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "name": body.name,
            "team_size": body.team_size,
            "num_teams": body.num_teams,
            "mode": body.mode,
            "advance_to": body.advance_to,
        }),
    )
    .await?;
    tx.commit().await?;

    // --- Vorrücken über die echte Engine (eigene Transaktionen); bei Fehler Baum löschen ---
    if let Err(err) = advance_test_tournament(&state, &body, tournament_id, &user.discord_id).await
    {
        tracing::error!(
            tournament_id,
            error = %err.detail,
            "Test-Turnier konnte nicht vollständig vorbereitet werden"
        );
        let mut cleanup_tx = state.pool.begin().await?;
        delete_tournament_tree(&mut cleanup_tx, tournament_id).await?;
        cleanup_tx.commit().await?;
        return Err(err);
    }

    Ok(Json(TestTournamentCreateResponse { tournament_id }))
}

/// Rückt ein frisch erzeugtes Test-Turnier je nach `advance_to` vor. Nutzt die
/// echten Engine-Funktionen. Fehler werden vom Aufrufer in den Tree-Rollback
/// gefasst.
async fn advance_test_tournament(
    state: &AppState,
    body: &TestTournamentCreateRequest,
    tournament_id: i64,
    actor_id: &str,
) -> WebResult<()> {
    if body.advance_to == "group_phase" || body.advance_to == "bracket" {
        // Dry-Run für den Snapshot-Token, dann bestätigtes Vorrücken.
        let preview = turnier_engine::finalize_checkin(
            &state.pool,
            tournament_id,
            FinalizeCheckinParams {
                confirm: false,
                ..Default::default()
            },
        )
        .await?;
        // bug-preserved (needs-decision): advance_to_group_phase wird hart True
        // übergeben, auch für bracket_only — 1:1 zum Original.
        turnier_engine::finalize_checkin(
            &state.pool,
            tournament_id,
            FinalizeCheckinParams {
                confirm: true,
                actor_id: Some(actor_id),
                expected_snapshot_token: Some(&preview.snapshot_token),
                advance_to_group_phase: true,
                ..Default::default()
            },
        )
        .await?;
    }

    if body.advance_to == "bracket" {
        let flags = ensure_test_tournament(&state.pool, tournament_id).await?;
        if flags.status == "group_phase" {
            turnier_scheduler::advance_tournament_status(
                &state.pool,
                &state.match_manager,
                &state.notifier,
                tournament_id,
                "group_phase",
                "bracket",
                "manual",
                Some(actor_id),
            )
            .await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Endpunkt: Runden-Simulation
// ---------------------------------------------------------------------------

/// Eine offene Bracket-Match-Zeile für die Simulation.
#[derive(Debug, sqlx::FromRow)]
struct OpenBracketMatch {
    id: i64,
    round: i64,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
}

/// Eine offene Group-Match-Zeile für die Simulation.
#[derive(Debug, sqlx::FromRow)]
struct OpenGroupMatch {
    id: i64,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
}

/// `POST /api/admin/test/tournaments/{tournament_id}/simulate-round` — eine
/// Runde mit Zufalls-Gewinnern über die echte Ergebnis-Pipeline simulieren
/// (require_mod).
async fn simulate_test_tournament_round(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<SimulateRoundResponse>> {
    ensure_throwaway_test_db(&state.pool).await?;
    let flags = ensure_test_tournament(&state.pool, tournament_id).await?;

    let finished_statuses: Vec<&str> = FINISHED_MATCH_STATUSES.to_vec();
    let bracket_rows: Vec<OpenBracketMatch> = sqlx::query_as(
        r#"SELECT id, round, team1_id, team2_id
         FROM turnier."bracket_matches"
         WHERE tournament_id = $1
           AND team1_id IS NOT NULL AND team2_id IS NOT NULL
           AND NOT (status = ANY($2))
         ORDER BY round, position, id"#,
    )
    .bind(tournament_id)
    .bind(&finished_statuses)
    .fetch_all(&state.pool)
    .await?;

    let group_rows: Vec<OpenGroupMatch> = sqlx::query_as(
        r#"SELECT gm.id, gm.team1_id, gm.team2_id
         FROM turnier."group_matches" gm
         JOIN turnier."groups" g ON g.id = gm.group_id
         WHERE g.tournament_id = $1
           AND gm.team1_id IS NOT NULL AND gm.team2_id IS NOT NULL
           AND NOT (gm.status = ANY($2))
         ORDER BY gm.id"#,
    )
    .bind(tournament_id)
    .bind(&finished_statuses)
    .fetch_all(&state.pool)
    .await?;

    let mut rng = Rng::new();
    let mut simulated_matches: i64 = 0;

    if !group_rows.is_empty() && flags.status == "group_phase" {
        for row in &group_rows {
            let (Some(t1), Some(t2)) = (row.team1_id, row.team2_id) else {
                continue;
            };
            let winner_id = rng.choice2(t1, t2);
            state
                .match_manager
                .apply_group_match_result(
                    tournament_id,
                    row.id,
                    ApplyGroupParams {
                        winner_id: Some(winner_id),
                        duration_s: Some(0),
                        players: Some(Vec::new()),
                        source: "manual".to_string(),
                        ..Default::default()
                    },
                )
                .await?;
            simulated_matches += 1;
        }
    } else if !bracket_rows.is_empty() {
        let current_round = bracket_rows.iter().map(|r| r.round).min().unwrap_or(0);
        for row in &bracket_rows {
            if row.round != current_round {
                continue;
            }
            let (Some(t1), Some(t2)) = (row.team1_id, row.team2_id) else {
                continue;
            };
            let winner_id = rng.choice2(t1, t2);
            state
                .match_manager
                .apply_bracket_match_result(
                    tournament_id,
                    row.id,
                    ApplyBracketParams {
                        winner_id: Some(winner_id),
                        duration_s: Some(0),
                        players: Some(Vec::new()),
                        source: "manual".to_string(),
                        ..Default::default()
                    },
                )
                .await?;
            simulated_matches += 1;
        }
    }

    // bug-preserved: Audit in SEPARATER Transaktion nach den Match-Applies —
    // kein Gesamt-Rollback (1:1 zum Original).
    audit(
        &state.pool,
        "test_tournament_simulate_round",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "simulated_matches": simulated_matches }),
    )
    .await?;

    Ok(Json(SimulateRoundResponse { simulated_matches }))
}

// ---------------------------------------------------------------------------
// Endpunkt: Wipe
// ---------------------------------------------------------------------------

/// `DELETE /api/admin/test/wipe` — alle is_test-Turniere + alle Test-User löschen
/// (ignoriert den 409-Guard des Einzel-User-Pfads) (require_mod).
async fn wipe_test_data(
    State(state): State<AppState>,
    ModUser(user): ModUser,
) -> WebResult<Json<TestWipeResponse>> {
    ensure_throwaway_test_db(&state.pool).await?;
    let mut tx = state.pool.begin().await?;

    let tournament_ids: Vec<i64> = sqlx::query_scalar(
        r#"SELECT id FROM turnier."tournaments" WHERE is_test = true ORDER BY id"#,
    )
    .fetch_all(&mut *tx)
    .await?;
    for tournament_id in &tournament_ids {
        delete_tournament_tree(&mut tx, *tournament_id).await?;
    }

    let deleted_users = delete_test_users_in_db(&mut tx).await?;
    audit(
        &mut *tx,
        "test_data_wipe",
        &user.discord_id,
        json!({
            "deleted_tournaments": tournament_ids.len(),
            "deleted_users": deleted_users,
        }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(TestWipeResponse {
        deleted_tournaments: tournament_ids.len() as i64,
        deleted_users,
    }))
}

// ---------------------------------------------------------------------------
// Kleinkram
// ---------------------------------------------------------------------------

/// FastAPI `Field(ge=lo, le=hi)`-Äquivalent: bei Verletzung 422.
fn validate_range(value: i64, lo: i64, hi: i64, field: &str) -> WebResult<()> {
    if value < lo || value > hi {
        return Err(WebError::unprocessable(format!(
            "{field} muss zwischen {lo} und {hi} liegen"
        )));
    }
    Ok(())
}

/// Übersetzt den Request-`mode` in den DB-`tournament_mode` (1:1 zu
/// `_normalize_test_mode`).
fn normalize_test_mode(mode: &str) -> &'static str {
    if mode == "group_then_bracket" {
        "group_stage"
    } else {
        "bracket_only"
    }
}

/// Erzeugt einen eindeutigen Teamnamen aus dem Captain-Basisnamen
/// (case-insensitive, mit `(n)`-Suffix bei Kollision). Portiert `_team_name`.
fn team_name(base_name: &str, existing_keys: &mut HashSet<String>) -> (String, String) {
    let mut candidate = format!("{base_name} Team");
    candidate = candidate.trim().to_string();
    if candidate.is_empty() {
        candidate = "Test Team".to_string();
    }
    let mut team_name = candidate.clone();
    let mut suffix = 1;
    while existing_keys.contains(&team_name.to_lowercase()) {
        suffix += 1;
        team_name = format!("{candidate} ({suffix})");
    }
    let name_key = team_name.to_lowercase();
    existing_keys.insert(name_key.clone());
    (team_name, name_key)
}
