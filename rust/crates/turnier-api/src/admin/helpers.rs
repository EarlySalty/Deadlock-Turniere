//! Gemeinsame Helfer der Admin-Routen: Audit-Schreiber, 404-Loader, die
//! Single-Active-Tournament-Invariante, Team-Kapazitäts-/Lock-Prüfungen,
//! Lobby-Settings-/Reminder-Serialisierung und die Tree-Lösch-Sequenzen.
//!
//! Portiert die `_*`-Helfer aus `tournament/admin_routes.py`. Alle Loader/Checks
//! arbeiten gegen einen `Executor`, damit sie sowohl auf dem Pool als auch
//! innerhalb einer Transaktion laufen (TOCTOU-Checks in EINER Tx wie gefordert).

use serde_json::Value;
use sqlx::postgres::PgRow;
use sqlx::{Executor, PgConnection, Postgres, QueryBuilder, Row};

use turnier_core::LobbySettingsPreset;

use crate::db;
use crate::error::{WebError, WebResult};

/// Status, in denen ein Turnier als „aktiv" gilt (Single-Active-Gate).
pub const ACTIVE_TOURNAMENT_STATUSES: [&str; 5] =
    ["draft", "registration", "checkin", "group_phase", "bracket"];

/// Default-Reminder-Offsets (Magic-Numbers aus dem Original als Konstante).
const DEFAULT_REMINDER_OFFSETS: [i64; 3] = [1440, 120, 15];

/// Schreibt einen Audit-Log-Eintrag über den gegebenen Executor (Pool oder Tx).
pub async fn audit<'e, E>(executor: E, action: &str, user_id: &str, details: Value) -> WebResult<()>
where
    E: Executor<'e, Database = Postgres>,
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

/// Lädt die `tournaments`-Zeile als generische Row oder liefert 404.
pub async fn load_tournament_or_404<'e, E>(executor: E, tournament_id: i64) -> WebResult<PgRow>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(r#"SELECT * FROM turnier."tournaments" WHERE id = $1"#)
        .bind(tournament_id)
        .fetch_optional(executor)
        .await?
        .ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))
}

/// Lädt eine `teams`-Zeile (an das Turnier gebunden) oder liefert 404.
pub async fn load_team_or_404<'e, E>(
    executor: E,
    tournament_id: i64,
    team_id: i64,
) -> WebResult<PgRow>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(r#"SELECT * FROM turnier."teams" WHERE id = $1 AND tournament_id = $2"#)
        .bind(team_id)
        .bind(tournament_id)
        .fetch_optional(executor)
        .await?
        .ok_or_else(|| WebError::not_found("Team nicht gefunden"))
}

/// Lädt eine `team_applications`-Zeile (an das Team gebunden) oder liefert 404.
pub async fn load_team_application_or_404<'e, E>(
    executor: E,
    team_id: i64,
    application_id: i64,
) -> WebResult<PgRow>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query(r#"SELECT * FROM turnier."team_applications" WHERE id = $1 AND team_id = $2"#)
        .bind(application_id)
        .bind(team_id)
        .fetch_optional(executor)
        .await?
        .ok_or_else(|| WebError::not_found("Bewerbung nicht gefunden"))
}

/// Prüft, dass ein Bracket-Match (an das Turnier gebunden) existiert (sonst 404).
pub async fn ensure_bracket_match_exists<'e, E>(
    executor: E,
    tournament_id: i64,
    match_id: i64,
) -> WebResult<()>
where
    E: Executor<'e, Database = Postgres>,
{
    let row = sqlx::query(
        r#"SELECT 1 FROM turnier."bracket_matches" WHERE id = $1 AND tournament_id = $2"#,
    )
    .bind(match_id)
    .bind(tournament_id)
    .fetch_optional(executor)
    .await?;
    if row.is_none() {
        return Err(WebError::not_found("Match nicht gefunden"));
    }
    Ok(())
}

/// Stellt sicher, dass nur ein aktives, nicht-Test-Turnier existiert.
///
/// `ignore_tournament_id` schließt das eigene Turnier aus (Update/Advance). Bei
/// Verletzung → 409 mit der Original-Detailmeldung (`#<id> <name> (<status>)`).
pub async fn ensure_single_active_tournament(
    conn: &mut PgConnection,
    ignore_tournament_id: Option<i64>,
) -> WebResult<()> {
    turnier_scheduler::acquire_single_active_tournament_lock(&mut *conn).await?;

    let mut query = QueryBuilder::<Postgres>::new(
        r#"SELECT id, name, status FROM turnier."tournaments" WHERE status IN ("#,
    );
    let mut separated = query.separated(", ");
    for status in ACTIVE_TOURNAMENT_STATUSES {
        separated.push_bind(status);
    }
    query.push(") AND is_test = false");
    if let Some(id) = ignore_tournament_id {
        query.push(" AND id != ");
        query.push_bind(id);
    }

    if let Some(row) = query.build().fetch_optional(&mut *conn).await? {
        let id: i64 = row.get("id");
        let name: String = row.get("name");
        let status: String = row.get("status");
        return Err(WebError::conflict(format!(
            "Es gibt bereits ein aktives Turnier: #{id} {name} ({status})"
        )));
    }
    Ok(())
}

/// Erzwingt, dass Teilnehmerverwaltung nur in aktiven Turnierstatus erlaubt ist.
pub fn ensure_participant_management_allowed(tournament_status: &str) -> WebResult<()> {
    if !ACTIVE_TOURNAMENT_STATUSES.contains(&tournament_status) {
        return Err(WebError::bad_request(
            "Teilnehmerverwaltung ist nur für aktive Turniere möglich",
        ));
    }
    Ok(())
}

/// Zählt die Mitglieder eines Teams.
pub async fn count_team_members<'e, E>(executor: E, team_id: i64) -> WebResult<i64>
where
    E: Executor<'e, Database = Postgres>,
{
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM turnier."team_members" WHERE team_id = $1"#)
            .bind(team_id)
            .fetch_one(executor)
            .await?;
    Ok(count)
}

/// Liefert 400, wenn das Team bereits voll ist (`>= team_size`).
pub async fn ensure_team_has_capacity<'e, E>(
    executor: E,
    team_id: i64,
    team_size: i64,
) -> WebResult<()>
where
    E: Executor<'e, Database = Postgres>,
{
    if count_team_members(executor, team_id).await? >= team_size {
        return Err(WebError::bad_request("Team ist bereits voll"));
    }
    Ok(())
}

/// Verhindert destruktive Team-Löschung bei bestehender Turnier-Historie.
///
/// Das Original prüft 9 (Tabelle, Spalte)-Paare sequenziell; hier in EINER
/// `EXISTS`-Abfrage zusammengefasst (safe-Fix, identisches Verhalten).
pub async fn ensure_team_not_locked(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    team_id: i64,
) -> WebResult<()> {
    let locked: Option<i64> = sqlx::query_scalar(
        r#"SELECT 1::BIGINT WHERE EXISTS (SELECT 1 FROM turnier."group_teams" WHERE team_id = $1)
            OR EXISTS (SELECT 1 FROM turnier."group_matches" WHERE team1_id = $2 OR team2_id = $3 OR winner_id = $4)
            OR EXISTS (SELECT 1 FROM turnier."bracket_matches" WHERE team1_id = $5 OR team2_id = $6 OR winner_id = $7)
            OR EXISTS (SELECT 1 FROM turnier."match_results" WHERE winning_team = $8)
            OR EXISTS (SELECT 1 FROM turnier."checkins" WHERE team_id = $9)"#,
    )
    .bind(team_id) // group_teams
    .bind(team_id) // group_matches team1
    .bind(team_id) // group_matches team2
    .bind(team_id) // group_matches winner
    .bind(team_id) // bracket_matches team1
    .bind(team_id) // bracket_matches team2
    .bind(team_id) // bracket_matches winner
    .bind(team_id) // match_results winning_team
    .bind(team_id) // checkins
    .fetch_optional(&mut **tx)
    .await?;
    if locked.is_some() {
        return Err(WebError::bad_request(
            "Team kann nicht gelöscht werden, weil es bereits in Turnierdaten verwendet wird",
        ));
    }
    Ok(())
}

/// Wechselt oder leert den Captain eines Teams nach Mitglieder-Mutation.
///
/// Wählt das früheste (`joined_at`) verbleibende Mitglied als neuen Captain;
/// fehlt eines, wird `captain_discord_id = ''`. Portiert `_reassign_or_clear_captain`.
pub async fn reassign_or_clear_captain(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    team_id: i64,
) -> WebResult<()> {
    let next_captain: Option<i64> = sqlx::query_scalar(
        r#"SELECT discord_id FROM turnier."team_members" WHERE team_id = $1 ORDER BY joined_at LIMIT 1"#,
    )
    .bind(team_id)
    .fetch_optional(&mut **tx)
    .await?;
    let next_captain = next_captain.unwrap_or(0);

    sqlx::query(r#"UPDATE turnier."team_members" SET role = 'member' WHERE team_id = $1"#)
        .bind(team_id)
        .execute(&mut **tx)
        .await?;
    if next_captain != 0 {
        sqlx::query(r#"UPDATE turnier."team_members" SET role = 'captain' WHERE team_id = $1 AND discord_id = $2"#)
            .bind(team_id)
            .bind(next_captain)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query(r#"UPDATE turnier."teams" SET captain_discord_id = $1 WHERE id = $2"#)
        .bind(next_captain)
        .bind(team_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Mitgliedsdaten für die Signup-Upsert-Helfer.
pub struct MemberSnapshot {
    pub discord_id: String,
    pub discord_name: Option<String>,
    pub steam_id: Option<String>,
    pub rank: Option<String>,
    pub rank_score: i64,
}

/// Legt für ein entferntes Mitglied wieder ein team-loses Solo-Signup an
/// (UPDATE wenn vorhanden, sonst INSERT). Portiert `_upsert_signup_from_member`.
pub async fn upsert_signup_from_member(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    member: &MemberSnapshot,
) -> WebResult<()> {
    let discord_id = db::parse_discord_id(&member.discord_id)?;
    let existing: Option<i64> = sqlx::query_scalar(
        r#"SELECT id FROM turnier."tournament_signups" WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(discord_id)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(id) = existing {
        sqlx::query(
            r#"UPDATE turnier."tournament_signups" SET discord_name = $1, steam_id = $2, rank = $3,
             rank_score = $4, team_id = NULL WHERE id = $5"#,
        )
        .bind(&member.discord_name)
        .bind(&member.steam_id)
        .bind(&member.rank)
        .bind(member.rank_score)
        .bind(id)
        .execute(&mut **tx)
        .await?;
        return Ok(());
    }

    sqlx::query(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at)
         VALUES ($1, $2, $3, $4, $5, $6, NULL, now())"#,
    )
    .bind(tournament_id)
    .bind(discord_id)
    .bind(&member.discord_name)
    .bind(&member.steam_id)
    .bind(&member.rank)
    .bind(member.rank_score)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Upsert eines team-gebundenen Signups. Portiert `_upsert_signup_for_team`.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_signup_for_team(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
    discord_id: &str,
    discord_name: Option<&str>,
    steam_id: Option<&str>,
    rank: Option<&str>,
    rank_score: i64,
    team_id: i64,
) -> WebResult<()> {
    let discord_id_i64 = db::parse_discord_id(discord_id)?;
    let existing: Option<i64> = sqlx::query_scalar(
        r#"SELECT id FROM turnier."tournament_signups" WHERE tournament_id = $1 AND discord_id = $2"#,
    )
    .bind(tournament_id)
    .bind(discord_id_i64)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(id) = existing {
        sqlx::query(
            r#"UPDATE turnier."tournament_signups" SET discord_name = $1, steam_id = $2, rank = $3,
             rank_score = $4, team_id = $5 WHERE id = $6"#,
        )
        .bind(discord_name)
        .bind(steam_id)
        .bind(rank)
        .bind(rank_score)
        .bind(team_id)
        .bind(id)
        .execute(&mut **tx)
        .await?;
        return Ok(());
    }

    sqlx::query(
        r#"INSERT INTO turnier."tournament_signups"
         (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id, signed_up_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())"#,
    )
    .bind(tournament_id)
    .bind(discord_id_i64)
    .bind(discord_name)
    .bind(steam_id)
    .bind(rank)
    .bind(rank_score)
    .bind(team_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Prüft, ob in der Gruppenphase bereits gespielte Matches existieren.
pub async fn group_phase_has_played_matches<'e, E>(
    executor: E,
    tournament_id: i64,
) -> WebResult<bool>
where
    E: Executor<'e, Database = Postgres>,
{
    let row = sqlx::query(
        r#"SELECT 1 FROM turnier."group_matches" gm JOIN turnier."groups" g ON gm.group_id = g.id
         WHERE g.tournament_id = $1
         AND (gm.status != 'pending' OR gm.winner_id IS NOT NULL OR gm.played_at IS NOT NULL) \
         LIMIT 1"#,
    )
    .bind(tournament_id)
    .fetch_optional(executor)
    .await?;
    Ok(row.is_some())
}

/// Normalisiert/serialisiert Reminder-Offsets als JSON-Array (sortiert, eindeutig,
/// `>= 0`). Leere Eingabe und leeres Ergebnis fallen auf [1440, 120, 15] zurück.
/// Portiert `_serialize_reminder_offsets`.
pub fn serialize_reminder_offsets(offsets: &[i64]) -> String {
    let source: &[i64] = if offsets.is_empty() {
        &DEFAULT_REMINDER_OFFSETS
    } else {
        offsets
    };
    let mut cleaned: Vec<i64> = source.iter().copied().filter(|&o| o >= 0).collect();
    cleaned.sort_unstable();
    cleaned.dedup();
    cleaned.reverse();
    let cleaned = if cleaned.is_empty() {
        DEFAULT_REMINDER_OFFSETS.to_vec()
    } else {
        cleaned
    };
    serde_json::to_string(&cleaned).unwrap_or_else(|_| "[1440,120,15]".to_string())
}

/// Mapping der ConVar-Presets (`_LOBBY_SETTINGS_PRESET_MAP`). `Standard`/`Custom`
/// liefern keinen Preset-Payload.
fn preset_convars(preset: LobbySettingsPreset) -> Option<Value> {
    use LobbySettingsPreset::*;
    let map = match preset {
        Standard | Custom => return None,
        FastMode => serde_json::json!({ "citadel_enable_fast_cooldowns": 1 }),
        HighDamage => serde_json::json!({ "citadel_dps_multiplier": 2 }),
        LowGravity => serde_json::json!({ "sv_gravity": 200 }),
        SpeedMode => serde_json::json!({
            "citadel_player_move_speed_scale": 2.0,
            "citadel_enable_fast_cooldowns": 1,
        }),
        GlassCannon => serde_json::json!({
            "citadel_weapon_damage_multiplier": 5,
            "citadel_dps_multiplier": 3,
            "citadel_melee_damage_scale": 3.0,
        }),
        RichStart => serde_json::json!({ "citadel_player_starting_gold": 10000 }),
        ChaosMode => serde_json::json!({
            "sv_gravity": 400,
            "citadel_player_move_speed_scale": 1.5,
            "citadel_weapon_damage_multiplier": 2,
            "citadel_enable_fast_cooldowns": 1,
            "citadel_player_starting_gold": 5000,
            "citadel_trooper_gold_reward": 200,
        }),
        AllSameHero => serde_json::json!({ "citadel_allow_duplicate_heroes": 1 }),
        Immortal => serde_json::json!({ "citadel_enable_no_hero_death": 1 }),
    };
    Some(map)
}

/// Serialisiert die Lobby-Settings für ein Preset (+ optionale Custom-Settings)
/// als JSON-String oder `None`. Portiert `_serialize_lobby_settings` inkl. der
/// 400-Validierungen.
pub fn serialize_lobby_settings(
    preset: LobbySettingsPreset,
    custom_settings: Option<&Value>,
) -> WebResult<Option<String>> {
    if preset == LobbySettingsPreset::Custom {
        let custom = custom_settings.ok_or_else(|| {
            WebError::bad_request(
                "Für lobby_settings_preset=custom ist lobby_settings erforderlich",
            )
        })?;
        if !custom.is_object() {
            return Err(WebError::bad_request(
                "lobby_settings muss ein JSON-Objekt sein",
            ));
        }
        return Ok(Some(custom.to_string()));
    }

    if custom_settings.is_some() {
        return Err(WebError::bad_request(
            "lobby_settings ist nur mit lobby_settings_preset=custom erlaubt",
        ));
    }

    Ok(preset_convars(preset).map(|payload| payload.to_string()))
}

// --- Tree-Lösch-Sequenzen (route-level Daten-SQL, 1:1 portiert) -------------

/// Löscht alle Gruppen-Artefakte eines Turniers (gemeinsamer Helfer für die
/// beiden Tree-Lösch-Funktionen — DRY-Fix gegenüber dem duplizierten Original).
async fn delete_groups_for_tournament(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<()> {
    let group_ids: Vec<i64> =
        sqlx::query_scalar(r#"SELECT id FROM turnier."groups" WHERE tournament_id = $1"#)
            .bind(tournament_id)
            .fetch_all(&mut **tx)
            .await?;
    if group_ids.is_empty() {
        return Ok(());
    }
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
    Ok(())
}

/// Löscht nur die Gruppenphase (für den Mode-Wechsel). Portiert
/// `_delete_group_phase_tree`.
pub async fn delete_group_phase_tree(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<()> {
    delete_groups_for_tournament(tx, tournament_id).await
}

/// Löscht den gesamten Turnier-Tree (Gruppen, Bracket, Teams, Signups, Turnier).
/// Portiert `_delete_tournament_tree`.
pub async fn delete_tournament_tree(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<()> {
    delete_groups_for_tournament(tx, tournament_id).await?;

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

    clear_bracket_tree(tx, tournament_id).await?;

    sqlx::query(
        r#"DELETE FROM turnier."team_members" WHERE team_id IN (SELECT id FROM turnier."teams" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"DELETE FROM turnier."team_applications" WHERE team_id IN (SELECT id FROM turnier."teams" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"DELETE FROM turnier."team_invitations" WHERE team_id IN (SELECT id FROM turnier."teams" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    for sql in [
        r#"DELETE FROM turnier."match_result_reports" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."tournament_casters" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."tournament_checkins" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."tournament_signups" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."teams" WHERE tournament_id = $1"#,
        r#"DELETE FROM turnier."tournaments" WHERE id = $1"#,
    ] {
        sqlx::query(sql)
            .bind(tournament_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Löscht Bracket-Matches + Mini-Group-Verdrahtung eines Turniers. Portiert
/// `tournament.engine._clear_bracket_tree` 1:1 (FK-Referenzen nullen, dann löschen;
/// `match_games` hängt per `ON DELETE CASCADE` an `bracket_matches`).
pub async fn clear_bracket_tree(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    tournament_id: i64,
) -> WebResult<()> {
    sqlx::query(r#"UPDATE turnier."bracket_mini_groups" SET advances_to_match_id = NULL WHERE tournament_id = $1"#)
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
    sqlx::query(r#"DELETE FROM turnier."match_games" WHERE bracket_match_id IN (SELECT id FROM turnier."bracket_matches" WHERE tournament_id = $1)"#)
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(r#"DELETE FROM turnier."bracket_matches" WHERE tournament_id = $1"#)
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        r#"DELETE FROM turnier."bracket_mini_group_teams" WHERE mini_group_id IN
         (SELECT id FROM turnier."bracket_mini_groups" WHERE tournament_id = $1)"#,
    )
    .bind(tournament_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(r#"DELETE FROM turnier."bracket_mini_groups" WHERE tournament_id = $1"#)
        .bind(tournament_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Liest die Discord-IDs aller Mitglieder eines Teams (für Voice-Moves).
pub async fn team_member_discord_ids<'e, E>(executor: E, team_id: i64) -> WebResult<Vec<String>>
where
    E: Executor<'e, Database = Postgres>,
{
    let rows: Vec<(i64,)> =
        sqlx::query_as(r#"SELECT discord_id FROM turnier."team_members" WHERE team_id = $1"#)
            .bind(team_id)
            .fetch_all(executor)
            .await?;
    Ok(rows
        .into_iter()
        .map(|r| db::discord_id_to_string(r.0))
        .collect())
}
