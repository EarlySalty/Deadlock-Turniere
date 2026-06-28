//! Turnier-CRUD + Status-Maschine (portiert die Tournament-Endpunkte aus
//! `admin_routes.py`). Statuswechsel laufen über
//! [`turnier_scheduler::advance_tournament_status`] mit `source="manual"`.

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use sqlx::Row;

use turnier_core::{
    LobbySettingsPreset, Tournament, TournamentCreate, TournamentDetail, TournamentMode,
    TournamentStatus, TournamentUpdate,
};
use turnier_engine::{determine_tournament_mode, is_valid_transition, valid_next_statuses};

use crate::error::{WebError, WebResult};
use crate::extract::{AdminUser, ModUser};
use crate::state::AppState;

use super::helpers::{
    self, audit, delete_group_phase_tree, delete_tournament_tree, ensure_single_active_tournament,
    group_phase_has_played_matches, load_tournament_or_404, serialize_lobby_settings,
    serialize_reminder_offsets,
};
use super::loaders::{
    load_bracket_matches, load_groups_for_tournament, load_mini_groups_for_tournament,
    load_signups_for_tournament, load_teams_for_tournament,
};
use super::tournament_row::{load_all_tournaments_dto, load_tournament_dto};

/// Router der Turnier-CRUD- und Status-Endpunkte.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/tournaments", get(list_tournaments).post(create_tournament))
        .route(
            "/api/admin/tournaments/{tournament_id}",
            get(get_tournament).put(update_tournament).delete(delete_tournament),
        )
        .route("/api/admin/tournaments/{tournament_id}/mini-groups", get(get_mini_groups))
        .route("/api/admin/tournaments/{tournament_id}/auto-lobby/run", post(run_auto_lobby))
        .route("/api/admin/tournaments/{tournament_id}/open-checkin", post(open_checkin))
        .route("/api/admin/tournaments/{tournament_id}/revert-checkin", post(revert_checkin))
        .route("/api/admin/tournaments/{tournament_id}/advance", post(advance_tournament))
}

/// Hilfsfunktion: serde-String eines Status (für SQL-Vergleiche/Audit).
fn status_str(status: TournamentStatus) -> &'static str {
    match status {
        TournamentStatus::Draft => "draft",
        TournamentStatus::Registration => "registration",
        TournamentStatus::Checkin => "checkin",
        TournamentStatus::GroupPhase => "group_phase",
        TournamentStatus::Bracket => "bracket",
        TournamentStatus::Completed => "completed",
        TournamentStatus::Archived => "archived",
    }
}

fn mode_str(mode: TournamentMode) -> &'static str {
    match mode {
        TournamentMode::GroupStage => "group_stage",
        TournamentMode::BracketOnly => "bracket_only",
    }
}

/// `GET /api/admin/tournaments` — alle Turniere inkl. Drafts.
async fn list_tournaments(
    State(state): State<AppState>,
    _mod: ModUser,
) -> WebResult<Json<Vec<Tournament>>> {
    Ok(Json(load_all_tournaments_dto(&state.pool).await?))
}

/// `GET /api/admin/tournaments/{id}` — Turnier-Detail inkl. aller Unterobjekte.
async fn get_tournament(
    State(state): State<AppState>,
    _mod: ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<TournamentDetail>> {
    let tournament = load_tournament_dto(&state.pool, tournament_id).await?;
    let teams = load_teams_for_tournament(&state, tournament_id).await?;
    let groups = load_groups_for_tournament(&state.pool, tournament_id).await?;
    let bracket_matches = load_bracket_matches(&state.pool, tournament_id).await?;
    let mini_groups = load_mini_groups_for_tournament(&state.pool, tournament_id).await?;
    let signups = load_signups_for_tournament(&state, tournament_id).await?;
    Ok(Json(TournamentDetail {
        tournament,
        teams,
        groups,
        bracket_matches,
        mini_groups,
        signups,
    }))
}

/// `GET /api/admin/tournaments/{id}/mini-groups` — Mini-Groups + zugehörige
/// Bracket-Matches (eingebettet).
async fn get_mini_groups(
    State(state): State<AppState>,
    _mod: ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Vec<Value>>> {
    load_tournament_or_404(&state.pool, tournament_id).await?;
    let mini_groups = load_mini_groups_for_tournament(&state.pool, tournament_id).await?;
    let bracket_matches = load_bracket_matches(&state.pool, tournament_id).await?;

    // Bracket-Matches nach mini_group_id gruppieren.
    let mut by_group: BTreeMap<i64, Vec<Value>> = BTreeMap::new();
    for m in &bracket_matches {
        if let Some(gid) = m.mini_group_id {
            by_group.entry(gid).or_default().push(serde_json::to_value(m).unwrap_or_default());
        }
    }

    let out = mini_groups
        .into_iter()
        .map(|mg| {
            let matches = by_group.get(&mg.id).cloned().unwrap_or_default();
            let mut obj = serde_json::to_value(&mg).unwrap_or_default();
            if let Value::Object(map) = &mut obj {
                map.insert("matches".to_string(), Value::Array(matches));
            }
            obj
        })
        .collect();
    Ok(Json(out))
}

/// `POST /api/admin/tournaments/{id}/auto-lobby/run` — Auto-Lobby anstoßen.
async fn run_auto_lobby(
    State(state): State<AppState>,
    _mod: ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    load_tournament_or_404(&state.pool, tournament_id).await?;
    state
        .match_manager
        .schedule_auto_lobbies_for_tournament(tournament_id)
        .await?;
    Ok(Json(json!({ "ok": true, "tournament_id": tournament_id })))
}

/// `POST /api/admin/tournaments` — neues Turnier anlegen (201).
async fn create_tournament(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Json(body): Json<TournamentCreate>,
) -> WebResult<(StatusCode, Json<Tournament>)> {
    let body = body.validated().map_err(WebError::bad_request)?;

    let lobby_settings =
        serialize_lobby_settings(body.lobby_settings_preset, body.lobby_settings.as_ref())?;

    // Auto-Mode (Befund admin_routes.py:835: team_size statt Teamanzahl — 1:1
    // erhalten, siehe bugs_preserved).
    let tournament_mode =
        determine_tournament_mode(body.team_size as usize, body.force_tournament_mode);

    let mut tx = state.pool.begin().await?;
    ensure_single_active_tournament(&mut *tx, None).await?;

    // Insert inkl. lobby_settings (safe-Fix: das nachgelagerte UPDATE entfällt).
    let tournament_id: i64 = sqlx::query(
        "INSERT INTO tournaments \
         (name, description, team_size, bracket_format, registration_start, registration_end, \
          checkin_start, group_phase_start, bracket_start, created_by, invite_mode, \
          invite_window_start, invite_window_end, tournament_mode, tournament_game_mode, \
          auto_lobby_enabled, exclude_from_leaderboard, is_test, reminder_offsets, rules, \
          series_format, final_series_format, match_objective, no_show_grace_minutes, \
          start_reminder_offsets, lobby_settings) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING id",
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(body.team_size)
    .bind(serde_value_str(&body.bracket_format))
    .bind(&body.registration_start)
    .bind(&body.registration_end)
    .bind(&body.checkin_start)
    .bind(&body.group_phase_start)
    .bind(&body.bracket_start)
    .bind(&user.discord_id)
    .bind(serde_value_str(&body.invite_mode))
    .bind(&body.invite_window_start)
    .bind(&body.invite_window_end)
    .bind(mode_str(tournament_mode))
    .bind(serde_value_str(&body.tournament_game_mode))
    .bind(i64::from(body.auto_lobby_enabled))
    .bind(i64::from(body.exclude_from_leaderboard))
    .bind(i64::from(body.is_test))
    .bind(serialize_reminder_offsets(&body.reminder_offsets))
    .bind(&body.rules)
    .bind(body.series_format)
    .bind(body.final_series_format)
    .bind(&body.match_objective)
    .bind(body.no_show_grace_minutes)
    .bind(serialize_reminder_offsets(&body.start_reminder_offsets))
    .bind(&lobby_settings)
    .fetch_one(&mut *tx)
    .await?
    .get("id");

    audit(
        &mut *tx,
        "tournament_create",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "name": body.name }),
    )
    .await?;
    tx.commit().await?;

    let dto = load_tournament_dto(&state.pool, tournament_id).await?;

    // Benachrichtigung aller Profile bei Nicht-Test (best-effort).
    if !body.is_test {
        let profile_ids: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT discord_id FROM user_profiles")
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        let profile_ids: Vec<String> = profile_ids.into_iter().filter(|s| !s.is_empty()).collect();
        if let Err(err) = state
            .notifier
            .notify_users(
                &profile_ids,
                turnier_discord::NotificationEvent::TournamentNews,
                &format!("Ein neues Turnier wurde angelegt: `{}`.", body.name),
            )
            .await
        {
            tracing::error!(tournament_id, error = %err, "Tournament-News-Benachrichtigung fehlgeschlagen");
        }
    }

    Ok((StatusCode::CREATED, Json(dto)))
}

/// serde-String eines String-Enums (z. B. `bracket_format` → `single_elimination`).
fn serde_value_str<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// `PUT /api/admin/tournaments/{id}` — Turnier aktualisieren.
async fn update_tournament(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
    Json(body): Json<TournamentUpdate>,
) -> WebResult<Json<Tournament>> {
    let body = body.validated().map_err(WebError::bad_request)?;

    let mut tx = state.pool.begin().await?;
    let existing = sqlx::query("SELECT status, tournament_mode FROM tournaments WHERE id = ?")
        .bind(tournament_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))?;
    let current_status: String = existing.get("status");
    let existing_mode: String = existing.get("tournament_mode");

    // Status-Übergang validieren (nur wenn ein abweichender Status gesetzt wird).
    if let Some(new_status) = body.status {
        let new_status_s = status_str(new_status);
        if new_status_s != current_status {
            let allowed: Vec<&'static str> = parse_status(&current_status)
                .map(|s| valid_next_statuses(s).iter().map(|t| status_str(*t)).collect())
                .unwrap_or_default();
            let transition_ok = parse_status(&current_status)
                .map(|f| is_valid_transition(f, new_status))
                .unwrap_or(false);
            if !transition_ok {
                let allowed_text = if allowed.is_empty() { "keine".to_string() } else { allowed.join(", ") };
                return Err(WebError::bad_request(format!(
                    "Ungültiger Status-Übergang: {current_status} -> {new_status_s}. Erlaubt: {allowed_text}"
                )));
            }
            if current_status == "registration" && new_status_s == "checkin" {
                return Err(WebError::bad_request("Check-in bitte über den dedizierten Endpoint öffnen"));
            }
            if current_status == "checkin" && new_status_s == "group_phase" {
                return Err(WebError::bad_request("Check-in bitte über finalize-checkin abschließen"));
            }
            if helpers::ACTIVE_TOURNAMENT_STATUSES.contains(&new_status_s) {
                ensure_single_active_tournament(&mut *tx, Some(tournament_id)).await?;
            }
        }
    }

    // Update-Felder einsammeln (nur gesetzte). Reihenfolge wie im Original.
    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<Value> = Vec::new();
    let mut changes = serde_json::Map::new();

    macro_rules! push {
        ($col:literal, $val:expr) => {{
            sets.push(format!("{} = ?", $col));
            let v: Value = $val;
            changes.insert($col.to_string(), v.clone());
            binds.push(v);
        }};
    }

    if let Some(v) = &body.name {
        push!("name", json!(v));
    }
    if let Some(v) = &body.description {
        push!("description", json!(v));
    }
    if let Some(v) = body.status {
        push!("status", json!(status_str(v)));
    }
    if let Some(v) = body.team_size {
        push!("team_size", json!(v));
    }
    if let Some(v) = body.bracket_format {
        push!("bracket_format", json!(serde_value_str(&v)));
    }
    if let Some(v) = body.series_format {
        push!("series_format", json!(v));
    }
    if let Some(v) = body.final_series_format {
        push!("final_series_format", json!(v));
    }
    if let Some(v) = &body.registration_start {
        push!("registration_start", json!(v));
    }
    if let Some(v) = &body.registration_end {
        push!("registration_end", json!(v));
    }
    if let Some(v) = &body.checkin_start {
        push!("checkin_start", json!(v));
    }
    if let Some(v) = &body.group_phase_start {
        push!("group_phase_start", json!(v));
    }
    if let Some(v) = &body.bracket_start {
        push!("bracket_start", json!(v));
    }
    if let Some(v) = &body.match_objective {
        push!("match_objective", json!(v));
    }
    if let Some(v) = body.no_show_grace_minutes {
        push!("no_show_grace_minutes", json!(v));
    }
    if let Some(v) = &body.rules {
        push!("rules", json!(v));
    }

    // Mode-Wechsel (nur in draft/checkin/group_phase, group_phase nur ungespielt).
    let mut mode_changed_to_bracket_only = false;
    if let Some(force_mode) = body.force_tournament_mode {
        if !matches!(current_status.as_str(), "draft" | "checkin" | "group_phase") {
            return Err(WebError::bad_request(
                "Turnier-Modus kann nur in Draft-, Check-in- oder ungespielter Gruppenphase geändert werden",
            ));
        }
        if current_status == "group_phase"
            && group_phase_has_played_matches(&mut *tx, tournament_id).await?
        {
            return Err(WebError::bad_request(
                "Turnier-Modus kann nach Start der Gruppenmatches nicht mehr geändert werden",
            ));
        }
        let mode_s = mode_str(force_mode);
        push!("tournament_mode", json!(mode_s));
        mode_changed_to_bracket_only = mode_s == "bracket_only" && existing_mode != "bracket_only";
    }

    // Bool-/Enum-Spezialfelder.
    if let Some(v) = body.invite_mode {
        push!("invite_mode", json!(serde_value_str(&v)));
    }
    if let Some(v) = body.exclude_from_leaderboard {
        push!("exclude_from_leaderboard", json!(i64::from(v)));
    }
    if let Some(v) = body.auto_lobby_enabled {
        push!("auto_lobby_enabled", json!(i64::from(v)));
    }
    if let Some(v) = body.is_test {
        push!("is_test", json!(i64::from(v)));
    }
    if let Some(v) = body.tournament_game_mode {
        push!("tournament_game_mode", json!(serde_value_str(&v)));
    }
    if let Some(v) = &body.reminder_offsets {
        push!("reminder_offsets", json!(serialize_reminder_offsets(v)));
    }
    if let Some(v) = &body.start_reminder_offsets {
        push!("start_reminder_offsets", json!(serialize_reminder_offsets(v)));
    }
    if body.invite_window_start.is_some() {
        push!("invite_window_start", json!(body.invite_window_start));
    }
    if body.invite_window_end.is_some() {
        push!("invite_window_end", json!(body.invite_window_end));
    }

    // Lobby-Settings: Preset oder Custom (Custom impliziert preset=custom).
    if body.lobby_settings_preset.is_some() || body.lobby_settings.is_some() {
        let preset = match (body.lobby_settings_preset, &body.lobby_settings) {
            (Some(p), _) => Some(p),
            (None, Some(_)) => Some(LobbySettingsPreset::Custom),
            (None, None) => None,
        };
        if let Some(p) = preset {
            let serialized = serialize_lobby_settings(p, body.lobby_settings.as_ref())?;
            push!("lobby_settings", json!(serialized));
        } else if body.lobby_settings.is_some() {
            return Err(WebError::bad_request(
                "lobby_settings_preset ist erforderlich, wenn lobby_settings gesetzt wird",
            ));
        }
    }

    if sets.is_empty() {
        return Err(WebError::bad_request("Keine Änderungen angegeben"));
    }

    let mut sql = format!("UPDATE tournaments SET {}", sets.join(", "));
    sql.push_str(", updated_at = datetime('now') WHERE id = ?");
    let mut q = sqlx::query(&sql);
    for v in &binds {
        q = bind_json(q, v);
    }
    q.bind(tournament_id).execute(&mut *tx).await?;

    // Mode-Wechsel group_phase → bracket_only: Tree-Rebuild (Befund
    // admin_routes.py:1042 — 1:1 erhalten, needs-decision).
    let rebuild_bracket_only = current_status == "group_phase" && mode_changed_to_bracket_only;
    if rebuild_bracket_only {
        delete_group_phase_tree(&mut tx, tournament_id).await?;
        helpers::clear_bracket_tree(&mut tx, tournament_id).await?;
    }

    audit(
        &mut *tx,
        "tournament_update",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "changes": Value::Object(changes) }),
    )
    .await?;
    tx.commit().await?;

    // Vollständigen Single-Elim-Tree aus den Teams neu bauen + auf `bracket`
    // setzen. `generate_bracket` leert das (bereits geleerte) Bracket erneut und
    // baut bei fehlender Gruppenphase aus allen Teams den kompletten Seed-Tree —
    // entspricht dem `_build_seeded_bracket`-Rebuild des Originals. Läuft nach dem
    // Commit, da `generate_bracket` eine eigene Transaktion öffnet.
    if rebuild_bracket_only {
        turnier_engine::generate_bracket(&state.pool, tournament_id).await?;
        sqlx::query(
            "UPDATE tournaments SET status = 'bracket', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(tournament_id)
        .execute(&state.pool)
        .await?;
    }

    Ok(Json(load_tournament_dto(&state.pool, tournament_id).await?))
}

/// Bindet einen JSON-Wert typgerecht an eine SQL-Query (Null/Int/Bool/String).
fn bind_json<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    v: &'q Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    match v {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(i64::from(*b)),
        Value::Number(n) if n.is_i64() => q.bind(n.as_i64().unwrap()),
        Value::Number(n) => q.bind(n.as_f64().unwrap()),
        Value::String(s) => q.bind(s.as_str()),
        other => q.bind(other.to_string()),
    }
}

/// `DELETE /api/admin/tournaments/{id}` — Turnier komplett löschen (Admin).
async fn delete_tournament(
    State(state): State<AppState>,
    AdminUser(user): AdminUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let existing = load_tournament_or_404(&mut *tx, tournament_id).await?;
    let name: String = existing.get("name");

    delete_tournament_tree(&mut tx, tournament_id).await?;
    audit(
        &mut *tx,
        "tournament_delete",
        &user.discord_id,
        json!({ "tournament_id": tournament_id, "name": name }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(json!({ "status": "gelöscht", "tournament_id": tournament_id })))
}

/// `POST /api/admin/tournaments/{id}/open-checkin` — Check-in öffnen.
async fn open_checkin(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Tournament>> {
    {
        let existing = load_tournament_or_404(&state.pool, tournament_id).await?;
        let status: String = existing.get("status");
        if status != "registration" {
            return Err(WebError::bad_request(
                "Check-in kann nur aus der Registration geöffnet werden",
            ));
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM teams WHERE tournament_id = ?")
            .bind(tournament_id)
            .fetch_one(&state.pool)
            .await?;
        if count == 0 {
            return Err(WebError::bad_request(
                "Check-in kann erst geöffnet werden, wenn mindestens ein Team existiert",
            ));
        }
    }

    advance_status_shared(&state, tournament_id, "registration", "checkin", &user.discord_id).await?;
    Ok(Json(load_tournament_dto(&state.pool, tournament_id).await?))
}

/// `POST /api/admin/tournaments/{id}/revert-checkin` — Check-in zurücksetzen.
async fn revert_checkin(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Tournament>> {
    let mut tx = state.pool.begin().await?;
    let existing = load_tournament_or_404(&mut *tx, tournament_id).await?;
    let status: String = existing.get("status");
    if status != "checkin" {
        return Err(WebError::bad_request(
            "Check-in kann nur aus der Check-in-Phase zurückgesetzt werden",
        ));
    }

    sqlx::query("DELETE FROM tournament_checkins WHERE tournament_id = ?")
        .bind(tournament_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE tournaments SET status = ?, registration_end = NULL, checkin_start = NULL, \
         updated_at = datetime('now') WHERE id = ?",
    )
    .bind("registration")
    .bind(tournament_id)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut *tx,
        "tournament_revert_checkin",
        &user.discord_id,
        json!({
            "tournament_id": tournament_id,
            "from_status": "checkin",
            "to_status": "registration",
            "cleared_schedule_fields": ["registration_end", "checkin_start"],
        }),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(load_tournament_dto(&state.pool, tournament_id).await?))
}

/// `POST /api/admin/tournaments/{id}/advance` — Phase weiterschalten.
async fn advance_tournament(
    State(state): State<AppState>,
    ModUser(user): ModUser,
    Path(tournament_id): Path<i64>,
) -> WebResult<Json<Tournament>> {
    let (current_status, next_status) = {
        let mut tx = state.pool.begin().await?;
        let existing = sqlx::query("SELECT status FROM tournaments WHERE id = ?")
            .bind(tournament_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| WebError::not_found("Turnier nicht gefunden"))?;
        let current_status: String = existing.get("status");

        if current_status == "registration" {
            return Err(WebError::bad_request("Check-in bitte über open-checkin öffnen"));
        }
        if current_status == "checkin" {
            return Err(WebError::bad_request("Check-in bitte über finalize-checkin abschließen"));
        }
        let allowed: Vec<&'static str> = parse_status(&current_status)
            .map(|s| valid_next_statuses(s).iter().map(|t| status_str(*t)).collect())
            .unwrap_or_default();
        let Some(&next_status) = allowed.first() else {
            return Err(WebError::bad_request(format!(
                "Keine weitere Phase möglich (aktuell: {current_status})"
            )));
        };
        if helpers::ACTIVE_TOURNAMENT_STATUSES.contains(&next_status) {
            ensure_single_active_tournament(&mut *tx, Some(tournament_id)).await?;
        }
        tx.commit().await?;
        (current_status, next_status)
    };

    advance_status_shared(&state, tournament_id, &current_status, next_status, &user.discord_id)
        .await?;
    Ok(Json(load_tournament_dto(&state.pool, tournament_id).await?))
}

/// String-Status → Enum (für die Übergangstabelle).
fn parse_status(value: &str) -> Option<TournamentStatus> {
    serde_json::from_value(Value::String(value.to_string())).ok()
}

/// Ruft `advance_tournament_status` mit `source="manual"` und mappt die
/// Scheduler-Fehler exakt wie das Original (ValueError→400, RuntimeError→409).
async fn advance_status_shared(
    state: &AppState,
    tournament_id: i64,
    current_status: &str,
    next_status: &str,
    actor_id: &str,
) -> WebResult<()> {
    turnier_scheduler::advance_tournament_status(
        &state.pool,
        &state.match_manager,
        &state.notifier,
        tournament_id,
        current_status,
        next_status,
        "manual",
        Some(actor_id),
    )
    .await
    .map(|_| ())
    .map_err(|err| match err {
        turnier_scheduler::SchedulerError::InvalidTransition(msg) => WebError::bad_request(msg),
        turnier_scheduler::SchedulerError::StatusConflict => {
            WebError::conflict("Turnierstatus wurde parallel geändert")
        }
        other => other.into(),
    })
}
