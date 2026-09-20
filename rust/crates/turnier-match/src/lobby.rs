//! Lobby-Lebenszyklus: Steam-Custom-Lobby erstellen/starten/verlassen,
//! Spectator/Ready setzen, Live-ConVars/Presets anwenden.
//!
//! Portiert die Lobby-/Start-/Leave-/ConVar-Pfade aus `match/manager.py`. Alle
//! Methoden hängen am [`MatchManager`]. Discord-Versand ist best-effort (Fehler
//! geloggt, Flow läuft weiter) — exakt je Aufrufstelle wie im Original.
//!
//! Status-Mengen (Befund manager.py): `VALID_LOBBY_STATUSES={pending,checkin}`,
//! `VALID_START_STATUSES={lobby_created}`, `VALID_RESULT_STATUSES={in_progress}`,
//! `VALID_LEAVE_STATUSES={lobby_created,in_progress}`.

use serde_json::{json, Value};

use turnier_discord::{NotificationEvent, PlayerStat};

use crate::error::{SteamTaskError, SteamTaskResult};
use crate::kind::MatchKind;
use crate::modes::{self, resolve_match_objective};
use crate::presets;
use crate::repo::{self, MatchRow, Participant};
use crate::steam_bridge::{ActiveTaskFilter, TaskOutcome};
use crate::MatchManager;

const VALID_LOBBY_STATUSES: [&str; 2] = ["pending", "checkin"];
const VALID_START_STATUSES: [&str; 1] = ["lobby_created"];
const VALID_LEAVE_STATUSES: [&str; 2] = ["lobby_created", "in_progress"];

impl MatchManager {
    // --- Öffentliche Lobby-API (Bracket + Group) ------------------------

    /// Holt ein Scrim-Ergebnis über dieselbe Steam-Bridge wie Turnier-Matches.
    pub async fn fetch_scrim_match_result(
        &self,
        steam_match_id: Option<i64>,
        party_id: Option<&str>,
    ) -> SteamTaskResult<Value> {
        let mut payload = serde_json::Map::new();
        if let Some(steam_match_id) = steam_match_id {
            payload.insert("match_id".to_string(), json!(steam_match_id));
        }
        if let Some(party_id) = party_id.map(str::trim).filter(|value| !value.is_empty()) {
            payload.insert("party_id".to_string(), json!(party_id));
        }
        if payload.is_empty() {
            return Err(SteamTaskError::state(
                "Scrim-Ergebnis braucht eine Match-ID oder Party-ID",
            ));
        }
        self.run_steam_task(
            "Scrim-Ergebnis abrufen",
            "GC_GET_MATCH_RESULT",
            &Value::Object(payload),
            self.bridge_settings.result_timeout_seconds as f64,
        )
        .await
    }

    /// Erstellt eine Steam-Custom-Lobby für ein Bracket-Match.
    /// Entspricht `create_lobby`.
    pub async fn create_lobby(&self, tournament_id: i64, match_id: i64) -> SteamTaskResult<Value> {
        self.create_lobby_for_match(MatchKind::Bracket, tournament_id, match_id)
            .await
    }

    /// Erstellt eine Steam-Custom-Lobby für ein Group-Match.
    /// Entspricht `create_group_lobby`.
    pub async fn create_group_lobby(
        &self,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        self.create_lobby_for_match(MatchKind::Group, tournament_id, match_id)
            .await
    }

    /// Setzt den Bot auf den Spectator-Slot (nur Bracket). Entspricht
    /// `set_bot_spectator`.
    pub async fn set_bot_spectator(
        &self,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        let party_id = self
            .get_party_id(MatchKind::Bracket, tournament_id, match_id)
            .await?;
        self.run_steam_task(
            "Spectator-Slot setzen",
            "GC_LOBBY_SET_SPECTATOR",
            &json!({ "party_id": party_id }),
            self.bridge_settings.control_timeout_seconds as f64,
        )
        .await
    }

    /// Setzt den Bot in der Lobby auf ready (nur Bracket). Entspricht
    /// `set_bot_ready`.
    pub async fn set_bot_ready(&self, tournament_id: i64, match_id: i64) -> SteamTaskResult<Value> {
        let party_id = self
            .get_party_id(MatchKind::Bracket, tournament_id, match_id)
            .await?;
        self.run_steam_task(
            "Ready-Status setzen",
            "GC_LOBBY_READY",
            &json!({ "party_id": party_id }),
            self.bridge_settings.control_timeout_seconds as f64,
        )
        .await
    }

    /// Startet ein Bracket-Match. Entspricht `start_match`.
    pub async fn start_match(&self, tournament_id: i64, match_id: i64) -> SteamTaskResult<Value> {
        self.start_match_for_match(MatchKind::Bracket, tournament_id, match_id)
            .await
    }

    /// Startet ein Group-Match. Entspricht `start_group_match`.
    pub async fn start_group_match(
        &self,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        self.start_match_for_match(MatchKind::Group, tournament_id, match_id)
            .await
    }

    /// Lässt den Bot eine Bracket-Lobby verlassen. Entspricht `leave_lobby`.
    pub async fn leave_lobby(&self, tournament_id: i64, match_id: i64) -> SteamTaskResult<Value> {
        self.leave_lobby_for_match(MatchKind::Bracket, tournament_id, match_id)
            .await
    }

    /// Lässt den Bot eine Group-Lobby verlassen. Entspricht `leave_group_lobby`.
    pub async fn leave_group_lobby(
        &self,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        self.leave_lobby_for_match(MatchKind::Group, tournament_id, match_id)
            .await
    }

    /// Holt das Bracket-Ergebnis vom GC und übernimmt es. Entspricht
    /// `fetch_match_result`.
    pub async fn fetch_match_result(
        &self,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        self.fetch_match_result_for_match(MatchKind::Bracket, tournament_id, match_id)
            .await
    }

    /// Holt das Group-Ergebnis vom GC und übernimmt es. Entspricht
    /// `fetch_group_match_result`.
    pub async fn fetch_group_match_result(
        &self,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        self.fetch_match_result_for_match(MatchKind::Group, tournament_id, match_id)
            .await
    }

    async fn fetch_match_result_for_match(
        &self,
        kind: MatchKind,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        let m = repo::get_match(&self.pool, kind, tournament_id, match_id).await?;
        require_match_ready_for_result_fetch(&m)?;

        let result = self
            .run_steam_task(
                "Match-Ergebnis abrufen",
                "GC_GET_MATCH_RESULT",
                &build_match_result_payload(&m),
                self.bridge_settings.result_timeout_seconds as f64,
            )
            .await?;

        let winning_team = coerce_optional_int(result.get("winning_team"), "winning_team")?;
        let winner_id = coerce_optional_int(result.get("winner_id"), "winner_id")?;
        let duration_s = coerce_optional_int(result.get("duration_s"), "duration_s")?;
        let players = result.get("players").and_then(|v| v.as_array()).cloned();

        // MatchError → SteamTaskError::InvalidResult (wie `except MatchResultError`).
        let applied = match kind {
            MatchKind::Bracket => {
                let params = crate::result::ApplyBracketParams {
                    winning_team,
                    winner_id,
                    duration_s,
                    players,
                    source: "automatic".to_string(),
                    force: false,
                };
                self.apply_bracket_match_result(tournament_id, match_id, params)
                    .await?
            }
            MatchKind::Group => {
                let deadlock_match_id = extract_deadlock_match_id(&result);
                let params = crate::result::ApplyGroupParams {
                    winning_team,
                    winner_id,
                    deadlock_match_id,
                    duration_s,
                    players,
                    source: "automatic".to_string(),
                };
                self.apply_group_match_result(tournament_id, match_id, params)
                    .await?
            }
        };

        let mut out = as_object(result);
        let applied_value = match kind {
            MatchKind::Bracket => applied.to_bracket_value(),
            MatchKind::Group => applied.to_group_value("automatic"),
        };
        if let Value::Object(map) = applied_value {
            for (k, v) in map {
                out.insert(k, v);
            }
        }
        out.insert("success".into(), json!(true));
        Ok(Value::Object(out))
    }

    // --- Live-Event-Presets ---------------------------------------------

    /// Verfügbare Live-Event-Presets fürs Admin-Panel. Entspricht
    /// `list_match_event_presets`.
    pub async fn list_match_event_presets(&self) -> Vec<Value> {
        presets::list_match_event_presets()
    }

    /// Wendet beliebige ConVars live auf eine bestehende Bracket-Lobby an.
    /// Entspricht `apply_match_convars`.
    pub async fn apply_match_convars(
        &self,
        tournament_id: i64,
        match_id: i64,
        convars: &serde_json::Map<String, Value>,
    ) -> SteamTaskResult<Value> {
        let m = repo::get_match(&self.pool, MatchKind::Bracket, tournament_id, match_id).await?;
        require_match_has_live_lobby(&m)?;
        let normalized = presets::normalize_convar_payload(convars)?;
        let party_id = m.steam_party_id.clone().unwrap_or_default();
        let result = self
            .run_steam_task(
                "Match-ConVars anwenden",
                "GC_LOBBY_APPLY_CONVARS",
                &json!({
                    "party_id": party_id,
                    "convars": Value::Object(normalized.clone()),
                    "tournament_id": tournament_id,
                    "match_id": match_id,
                }),
                self.bridge_settings.convars_timeout_seconds as f64,
            )
            .await?;
        let mut out = as_object(result);
        out.insert("success".into(), json!(true));
        out.insert("match_id".into(), json!(match_id));
        out.insert("party_id".into(), json!(party_id));
        out.insert("applied_convars".into(), Value::Object(normalized));
        Ok(Value::Object(out))
    }

    /// Aktiviert/deaktiviert ein Event-Preset auf einer Lobby. Entspricht
    /// `apply_match_event_preset`.
    pub async fn apply_match_event_preset(
        &self,
        tournament_id: i64,
        match_id: i64,
        preset_key: &str,
        enabled: bool,
    ) -> SteamTaskResult<Value> {
        let preset = presets::get_preset(preset_key).ok_or_else(|| {
            SteamTaskError::state(format!("Unbekanntes Event-Preset: {preset_key}"))
        })?;
        let convars = preset.convars_for(enabled);
        let result = self
            .apply_match_convars(tournament_id, match_id, &convars)
            .await?;
        let mut out = as_object(result);
        out.insert("preset_key".into(), json!(preset.key));
        out.insert("enabled".into(), json!(enabled));
        out.insert("label".into(), json!(preset.label));
        out.insert("requires_cheats".into(), json!(preset.requires_cheats));
        Ok(Value::Object(out))
    }

    // --- interne Lobby-Erstellung ---------------------------------------

    async fn create_lobby_for_match(
        &self,
        kind: MatchKind,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        let m = repo::get_match(&self.pool, kind, tournament_id, match_id).await?;
        require_match_ready_for_lobby(&m)?;
        self.ensure_no_duplicate_lobby_request(kind, match_id, &m)
            .await?;

        let lobby_settings = repo::get_lobby_settings(&self.pool, tournament_id).await?;
        let participants = repo::load_participants(&self.pool, &m).await?;
        let participant_discord_ids = non_empty_discord_ids(&participants);
        let steam_ids: Vec<String> = participants
            .iter()
            .filter_map(|p| p.steam_id.as_ref())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let is_test = repo::is_test_tournament(&self.pool, tournament_id).await?;

        let mode_payload =
            modes::prepare_match_assignments(&self.pool, tournament_id, kind, match_id).await?;
        let mut merged_convars = lobby_settings;
        for (k, v) in &mode_payload.convars {
            merged_convars.insert(k.clone(), v.clone());
        }
        let hero_assignments = if mode_payload.hero_assignments_is_empty() {
            None
        } else {
            Some(&mode_payload.hero_assignments)
        };

        let mut create_payload = serde_json::Map::new();
        create_payload.insert("tournament_id".into(), json!(tournament_id));
        create_payload.insert("match_id".into(), json!(match_id));
        create_payload.insert("match_type".into(), json!(kind.as_str()));
        // game_mode/region_mode: Befund manager.py:130-148 — die Parameter wurden
        // nie abweichend gesetzt; der Port reicht die Default-Werte (1/1) durch.
        create_payload.insert("game_mode".into(), json!(1));
        create_payload.insert("region_mode".into(), json!(1));
        if !merged_convars.is_empty() {
            create_payload.insert("convars".into(), Value::Object(merged_convars.clone()));
        }

        let result = self
            .run_steam_task(
                "Lobby-Erstellung",
                "GC_CREATE_CUSTOM_LOBBY",
                &Value::Object(create_payload),
                self.bridge_settings.create_timeout_seconds as f64,
            )
            .await?;

        let party_id = coerce_required_str(result.get("party_id"), "party_id")?;
        let (party_code, join_code) = resolve_party_codes(&result)?;

        repo::set_lobby_created(
            &self.pool,
            kind,
            match_id,
            m.scope_value,
            &party_id,
            &party_code,
            hero_assignments,
        )
        .await?;

        let invite_result = self
            .invite_match_participants(&party_id, &steam_ids)
            .await?;

        let mut discord_channel_id: Option<String> = None;
        if !is_test {
            discord_channel_id = self
                .setup_discord_for_lobby(
                    kind,
                    tournament_id,
                    match_id,
                    &m,
                    &party_code,
                    &participants,
                    &participant_discord_ids,
                    mode_payload.announcement_lines.clone(),
                )
                .await;
        }

        let mut out = as_object(result);
        out.insert("success".into(), json!(true));
        out.insert("party_id".into(), json!(party_id));
        out.insert("party_code".into(), json!(party_code));
        out.insert("join_code".into(), json!(join_code));
        out.insert("lobby_settings".into(), Value::Object(merged_convars));
        out.insert("hero_assignments".into(), mode_payload.hero_assignments);
        out.insert("invite_result".into(), invite_result);
        out.insert(
            "discord_channel_id".into(),
            discord_channel_id.map(Value::String).unwrap_or(Value::Null),
        );
        Ok(Value::Object(out))
    }

    /// Discord-Setup nach erfolgreicher Lobby-Erstellung (best-effort).
    /// Gibt die Channel-ID zurück oder `None` (jeder Fehler → `None`, exakt wie
    /// der äußere `except`-Block im Original, der `discord_channel_id = None` setzt).
    #[allow(clippy::too_many_arguments)]
    async fn setup_discord_for_lobby(
        &self,
        kind: MatchKind,
        tournament_id: i64,
        match_id: i64,
        m: &MatchRow,
        party_code: &str,
        participants: &[Participant],
        participant_discord_ids: &[String],
        announcement_lines: Vec<String>,
    ) -> Option<String> {
        let notifier = self.notifier.as_ref()?;
        let team1_name = m.team1_label();
        let team2_name = m.team2_label();

        let channel_id = match notifier
            .create_match_channel(match_id, &team1_name, &team2_name)
            .await
        {
            Ok(id) => id,
            Err(err) => {
                tracing::error!(
                    tournament_id, match_id, error = %err,
                    "Discord match channel setup failed"
                );
                return None;
            }
        };

        if let Err(err) =
            repo::set_discord_channel_id(&self.pool, kind, match_id, m.scope_value, &channel_id)
                .await
        {
            tracing::error!(match_id, error = %err, "discord_channel_id persistieren fehlgeschlagen");
            return None;
        }

        if let Err(err) = notifier
            .send_match_lobby_info(&channel_id, party_code, participant_discord_ids)
            .await
        {
            tracing::error!(match_id, error = %err, "send_match_lobby_info fehlgeschlagen");
            return None;
        }

        // Caster benachrichtigen (best-effort, im Original innerhalb des try-Blocks).
        match repo::load_match_casters(&self.pool, kind, match_id).await {
            Ok(caster_ids) if !caster_ids.is_empty() => {
                if let Err(err) = notifier
                    .notify_casters_match_created(match_id, &channel_id, &caster_ids)
                    .await
                {
                    tracing::error!(match_id, error = %err, "notify_casters_match_created fehlgeschlagen");
                    return None;
                }
            }
            Ok(_) => {}
            Err(err) => {
                tracing::error!(match_id, error = %err, "Caster-Laden fehlgeschlagen");
                return None;
            }
        }

        // Team-Split der Discord-IDs.
        let team1_ids = team_discord_ids(participants, m.team1_id);
        let team2_ids = team_discord_ids(participants, m.team2_id);

        // Objective-Text (eigener try im Original — Fehler non-critical).
        let objective_text = match repo::get_objective_inputs(&self.pool, tournament_id).await {
            Ok(Some((objective, team_size))) => {
                let (_, text) = resolve_match_objective(objective.as_deref(), team_size);
                Some(text)
            }
            Ok(None) => None,
            Err(err) => {
                tracing::error!(
                    match_id, error = %err,
                    "Objective-Auflösung fehlgeschlagen (non-critical)"
                );
                None
            }
        };

        let hero_lines: Option<&[String]> = if announcement_lines.is_empty() {
            None
        } else {
            Some(&announcement_lines)
        };
        if let Err(err) = notifier
            .send_lobby_announcement(
                match_id,
                party_code,
                &team1_name,
                &team2_name,
                &team1_ids,
                &team2_ids,
                hero_lines,
                objective_text.as_deref(),
            )
            .await
        {
            // Eigener try im Original — non-critical, Channel-ID bleibt erhalten.
            tracing::error!(
                match_id, error = %err,
                "Lobby-Announcement fehlgeschlagen (non-critical)"
            );
        }

        Some(channel_id)
    }

    // --- interner Match-Start -------------------------------------------

    async fn start_match_for_match(
        &self,
        kind: MatchKind,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        let m = repo::get_match(&self.pool, kind, tournament_id, match_id).await?;
        require_match_ready_for_start(&m)?;
        let team1_name = m.team1_label();
        let team2_name = m.team2_label();

        let party_id = m
            .steam_party_id
            .clone()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                SteamTaskError::state(format!(
                    "Für Match {match_id} ist keine Party-ID gespeichert"
                ))
            })?;

        self.run_steam_task(
            "Spectator-Slot setzen",
            "GC_LOBBY_SET_SPECTATOR",
            &json!({ "party_id": party_id }),
            self.bridge_settings.control_timeout_seconds as f64,
        )
        .await?;
        self.run_steam_task(
            "Ready-Status setzen",
            "GC_LOBBY_READY",
            &json!({ "party_id": party_id }),
            self.bridge_settings.control_timeout_seconds as f64,
        )
        .await?;
        let result = self
            .run_steam_task(
                "Match-Start",
                "GC_LOBBY_START_MATCH",
                &json!({ "party_id": party_id, "match_type": kind.as_str(), "match_id": match_id }),
                self.bridge_settings.start_timeout_seconds as f64,
            )
            .await?;

        let participants = repo::load_participants(&self.pool, &m).await?;
        let participant_discord_ids = non_empty_discord_ids(&participants);
        let is_test = repo::is_test_tournament(&self.pool, tournament_id).await?;

        if !is_test {
            if let Some(notifier) = self.notifier.as_ref() {
                let code = m
                    .party_code
                    .clone()
                    .filter(|s| !s.is_empty())
                    .or_else(|| m.steam_party_id.clone())
                    .unwrap_or_default();
                let message = format!(
                    "Euer Match zwischen {team1_name} und {team2_name} läuft jetzt. Lobby-Code: {code}"
                );
                if let Err(err) = notifier
                    .notify_users(
                        &participant_discord_ids,
                        NotificationEvent::MatchStart,
                        &message,
                    )
                    .await
                {
                    tracing::error!(
                        tournament_id, match_id, error = %err,
                        "Match start notification failed"
                    );
                }
            }
        }

        // Caster in den Voice-Channel ziehen (best-effort).
        let caster_ids = repo::load_match_casters(&self.pool, kind, match_id).await?;
        if !caster_ids.is_empty() && !is_test {
            if let Some(notifier) = self.notifier.as_ref() {
                let move_result = notifier
                    .move_users_to_voice_channel(
                        &caster_ids,
                        self.caster_voice_channel_id,
                        self.guild_id,
                    )
                    .await;
                if !move_result.failed.is_empty() {
                    tracing::warn!(
                        match_kind = %kind, match_id,
                        failed = move_result.failed.len(),
                        "Caster voice move teilweise fehlgeschlagen"
                    );
                }
            }
        }

        // `result.get("match_id") or result.get("deadlock_match_id")`, dann zu
        // optional int coercen; in der DB als String, im Rückgabe-Dict als int.
        let raw = first_truthy(&result, &["match_id", "deadlock_match_id"]);
        let match_id_value = coerce_optional_int(raw.as_ref(), "match_id")?;
        let deadlock_str = match_id_value.map(|i| i.to_string());

        repo::set_in_progress(
            &self.pool,
            kind,
            match_id,
            m.scope_value,
            deadlock_str.as_deref(),
        )
        .await?;

        let mut out = as_object(result);
        out.insert("success".into(), json!(true));
        out.insert(
            "match_id".into(),
            match_id_value.map(|i| json!(i)).unwrap_or(Value::Null),
        );
        Ok(Value::Object(out))
    }

    // --- interner Lobby-Leave -------------------------------------------

    async fn leave_lobby_for_match(
        &self,
        kind: MatchKind,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<Value> {
        let m = repo::get_match(&self.pool, kind, tournament_id, match_id).await?;
        require_match_ready_for_leave(&m)?;
        let party_id = m.steam_party_id.clone().unwrap_or_default();
        let result = self
            .run_steam_task(
                "Lobby verlassen",
                "GC_LOBBY_LEAVE",
                &json!({ "party_id": party_id }),
                self.bridge_settings.control_timeout_seconds as f64,
            )
            .await?;
        let mut out = as_object(result);
        out.insert("success".into(), json!(true));
        Ok(Value::Object(out))
    }

    // --- gemeinsame Helfer ----------------------------------------------

    /// Holt die gespeicherte Party-ID eines Matches. Entspricht `_get_party_id`.
    async fn get_party_id(
        &self,
        kind: MatchKind,
        tournament_id: i64,
        match_id: i64,
    ) -> SteamTaskResult<String> {
        let m = repo::get_match(&self.pool, kind, tournament_id, match_id).await?;
        m.steam_party_id.filter(|s| !s.is_empty()).ok_or_else(|| {
            SteamTaskError::state(format!(
                "Für Match {match_id} ist keine Party-ID gespeichert"
            ))
        })
    }

    /// Verhindert eine doppelte Lobby-Anfrage. Entspricht
    /// `_ensure_no_duplicate_lobby_request` (prüft Party-ID UND aktiven Task).
    async fn ensure_no_duplicate_lobby_request(
        &self,
        kind: MatchKind,
        match_id: i64,
        m: &MatchRow,
    ) -> SteamTaskResult<()> {
        if m.steam_party_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .is_some()
        {
            return Err(SteamTaskError::state(
                "Für dieses Match existiert bereits eine Lobby",
            ));
        }
        if let Some(bridge) = self.bridge.as_ref() {
            let active = bridge
                .has_active_task(&ActiveTaskFilter {
                    match_id: Some(match_id),
                    match_type: Some(kind.as_str()),
                    ..ActiveTaskFilter::new("GC_CREATE_CUSTOM_LOBBY")
                })
                .await?;
            if active {
                return Err(SteamTaskError::state(
                    "Für dieses Match läuft bereits eine Lobby-Erstellung",
                ));
            }
        }
        Ok(())
    }

    /// Lädt die Teilnehmer per Steam-Bridge in die Lobby ein. Leere Steam-ID-Liste
    /// → trivialer Erfolg (wie `_invite_match_participants_to_lobby`).
    async fn invite_match_participants(
        &self,
        party_id: &str,
        steam_ids: &[String],
    ) -> SteamTaskResult<Value> {
        if steam_ids.is_empty() {
            return Ok(json!({
                "success": true,
                "party_id": party_id,
                "steam_ids": [],
                "invited": [],
                "failed": [],
                "skipped": [],
            }));
        }
        let bridge = self.require_bridge()?;
        let result = bridge.invite_players_to_lobby(party_id, steam_ids).await?;
        Ok(result.to_value())
    }

    /// Legt einen Steam-Task an, pollt das Ergebnis und übersetzt Fehler.
    /// Portiert `_run_steam_task` (Timeout schlägt durch, FAILED → `SteamTaskError`,
    /// `success:false` im Result → `SteamTaskError`).
    pub(crate) async fn run_steam_task(
        &self,
        action: &str,
        task_type: &str,
        payload: &Value,
        timeout_s: f64,
    ) -> SteamTaskResult<Value> {
        let bridge = self.require_bridge()?;
        let task_id = bridge.create_task(task_type, payload).await?;
        let outcome = bridge.poll_task_result(task_id, timeout_s).await?;
        match outcome {
            TaskOutcome::TimedOut { task_id, timeout_s } => {
                Err(SteamTaskError::Timeout { task_id, timeout_s })
            }
            TaskOutcome::Failed { error, .. } => Err(SteamTaskError::failed(format!(
                "{action} fehlgeschlagen: {error}"
            ))),
            TaskOutcome::Done(value) => {
                // success:false im DONE-Ergebnis → Fehler (Original: result.get
                // ("success") is False).
                if value.get("success") == Some(&Value::Bool(false)) {
                    let err = value
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unbekannter Fehler");
                    return Err(SteamTaskError::failed(format!(
                        "{action} fehlgeschlagen: {err}"
                    )));
                }
                Ok(value)
            }
        }
    }

    /// Postet die Match-Stats best-effort in den Discord-Channel (aus dem
    /// Result-Pfad aufgerufen). Fehler werden geloggt, nicht propagiert.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn post_match_stats(
        &self,
        channel_id: &str,
        match_id: i64,
        deadlock_match_id: Option<&str>,
        team1_name: &str,
        team2_name: &str,
        winner_name: &str,
        duration_s: Option<i64>,
        players: &[PlayerStat],
    ) {
        let Some(notifier) = self.notifier.as_ref() else {
            return;
        };
        if let Err(err) = notifier
            .send_match_stats_to_channel(
                channel_id,
                match_id,
                deadlock_match_id,
                team1_name,
                team2_name,
                winner_name,
                duration_s,
                if players.is_empty() {
                    None
                } else {
                    Some(players)
                },
            )
            .await
        {
            tracing::error!(match_id, error = %err, "send_match_stats_to_channel fehlgeschlagen");
        }
    }

    /// Plant das verzögerte Löschen des Match-Channels (fire-and-forget, wie das
    /// `asyncio.create_task(delete_match_channel_later(...))` des Originals).
    pub(crate) fn spawn_delete_channel_later(&self, channel_id: String) {
        let Some(notifier) = self.notifier.clone() else {
            return;
        };
        let delay = self.channel_delete_delay_seconds as f64;
        tokio::spawn(async move {
            notifier
                .delete_match_channel_later(&channel_id, Some(delay))
                .await;
        });
    }
}

// --- freie Statusprüfungen (DB-frei) -------------------------------------

fn require_match_ready_for_lobby(m: &MatchRow) -> SteamTaskResult<()> {
    if m.winner_id.is_some() || !VALID_LOBBY_STATUSES.contains(&m.status.as_str()) {
        return Err(SteamTaskError::state(
            "Für dieses Match kann keine Lobby erstellt werden",
        ));
    }
    if m.team1_id.is_none() || m.team2_id.is_none() {
        return Err(SteamTaskError::state(
            "Beide Teams müssen gesetzt sein, bevor eine Lobby erstellt wird",
        ));
    }
    if m.steam_party_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_some()
    {
        return Err(SteamTaskError::state(
            "Für dieses Match existiert bereits eine Lobby",
        ));
    }
    Ok(())
}

fn require_match_ready_for_start(m: &MatchRow) -> SteamTaskResult<()> {
    if !VALID_START_STATUSES.contains(&m.status.as_str()) {
        return Err(SteamTaskError::state(
            "Ein Match kann nur aus dem Status 'lobby_created' gestartet werden",
        ));
    }
    if m.steam_party_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err(SteamTaskError::state(
            "Für dieses Match existiert noch keine Lobby",
        ));
    }
    if m.deadlock_match_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_some()
    {
        return Err(SteamTaskError::state(
            "Für dieses Match wurde bereits eine Deadlock-Match-ID gespeichert",
        ));
    }
    Ok(())
}

fn require_match_ready_for_leave(m: &MatchRow) -> SteamTaskResult<()> {
    if !VALID_LEAVE_STATUSES.contains(&m.status.as_str()) {
        return Err(SteamTaskError::state(
            "Die Lobby kann nur im Status 'lobby_created' oder 'in_progress' verlassen werden",
        ));
    }
    if m.steam_party_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err(SteamTaskError::state(
            "Für dieses Match ist keine Party-ID gespeichert",
        ));
    }
    Ok(())
}

fn require_match_ready_for_result_fetch(m: &MatchRow) -> SteamTaskResult<()> {
    if m.status != "in_progress" {
        return Err(SteamTaskError::state(
            "Match-Ergebnisse können nur aus laufenden Matches abgerufen werden",
        ));
    }
    let has_party = m
        .steam_party_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_some();
    let has_deadlock = m
        .deadlock_match_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_some();
    if !has_party && !has_deadlock {
        return Err(SteamTaskError::state(
            "Für dieses Match ist weder eine Party-ID noch eine Deadlock-Match-ID gespeichert",
        ));
    }
    Ok(())
}

/// Baut das GC-Result-Payload (`match_id`/`party_id`, jeweils nur wenn gesetzt).
/// Portiert `_build_match_result_payload`.
fn build_match_result_payload(m: &MatchRow) -> Value {
    let mut payload = serde_json::Map::new();
    if let Some(deadlock) = m.deadlock_match_id.as_deref().filter(|s| !s.is_empty()) {
        payload.insert("match_id".into(), json!(deadlock));
    }
    if let Some(party) = m.steam_party_id.as_deref().filter(|s| !s.is_empty()) {
        payload.insert("party_id".into(), json!(party));
    }
    Value::Object(payload)
}

fn require_match_has_live_lobby(m: &MatchRow) -> SteamTaskResult<()> {
    if !VALID_LEAVE_STATUSES.contains(&m.status.as_str()) {
        return Err(SteamTaskError::state(
            "Live-Events koennen nur fuer Matches mit aktiver oder laufender Lobby gesetzt werden",
        ));
    }
    if m.steam_party_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err(SteamTaskError::state(
            "Fuer dieses Match ist keine Party-ID gespeichert",
        ));
    }
    Ok(())
}

// --- freie Wert-Helfer ---------------------------------------------------

/// Discord-IDs aller Teilnehmer mit nicht-leerer ID, in Teilnehmer-Reihenfolge.
fn non_empty_discord_ids(participants: &[Participant]) -> Vec<String> {
    participants
        .iter()
        .filter_map(|p| p.discord_id.as_deref())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Discord-IDs der Teilnehmer eines bestimmten Teams (für den Team-Split).
fn team_discord_ids(participants: &[Participant], team_id: Option<i64>) -> Vec<String> {
    let Some(team_id) = team_id else {
        return Vec::new();
    };
    participants
        .iter()
        .filter(|p| p.team_id == team_id)
        .filter_map(|p| p.discord_id.as_deref())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Wandelt ein JSON-Result in ein Map-Objekt (leeres Objekt bei Nicht-Objekt).
/// Entspricht `dict(result)`, das im Original immer auf ein Dict angewandt wird.
fn as_object(value: Value) -> serde_json::Map<String, Value> {
    match value {
        Value::Object(m) => m,
        _ => serde_json::Map::new(),
    }
}

/// Verlangt einen nicht-leeren String aus dem Steam-Result. Entspricht
/// `_coerce_required_str`.
fn coerce_required_str(value: Option<&Value>, field_name: &str) -> SteamTaskResult<String> {
    let s = match value {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    if s.is_empty() {
        return Err(SteamTaskError::failed(format!(
            "{field_name} fehlt im Steam-Ergebnis"
        )));
    }
    Ok(s)
}

/// Coerce zu einem optionalen i64 (`_coerce_optional_int`). String/Number werden
/// geparst; nicht-parsebar → Fehler.
fn coerce_optional_int(value: Option<&Value>, field_name: &str) -> SteamTaskResult<Option<i64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .map(Some)
            .ok_or_else(|| {
                SteamTaskError::failed(format!("{field_name} muss eine ganze Zahl sein"))
            }),
        Some(Value::String(s)) => {
            s.trim().parse::<i64>().map(Some).map_err(|_| {
                SteamTaskError::failed(format!("{field_name} muss eine ganze Zahl sein"))
            })
        }
        _ => Err(SteamTaskError::failed(format!(
            "{field_name} muss eine ganze Zahl sein"
        ))),
    }
}

/// Erster „truthy" JSON-Wert unter den Schlüsseln (`a or b`-Semantik): leere
/// Strings/`null`/`0`/`false` gelten als falsy und werden übersprungen.
fn first_truthy(result: &Value, keys: &[&str]) -> Option<Value> {
    for k in keys {
        match result.get(*k) {
            Some(Value::String(s)) if !s.is_empty() => return Some(Value::String(s.clone())),
            Some(Value::Number(n)) if n.as_f64() != Some(0.0) => {
                return Some(Value::Number(n.clone()))
            }
            Some(Value::Bool(true)) => return Some(Value::Bool(true)),
            _ => {}
        }
    }
    None
}

/// `result["match_id"]` ∨ `result["deadlock_match_id"]` als String (Befund
/// manager.py:411-414 — einheitlicher Helfer). Liefert den ersten nicht-leeren Wert.
fn extract_deadlock_match_id(result: &Value) -> Option<String> {
    for key in ["match_id", "deadlock_match_id"] {
        match result.get(key) {
            Some(Value::String(s)) if !s.is_empty() => return Some(s.clone()),
            Some(Value::Number(n)) => return Some(n.to_string()),
            _ => {}
        }
    }
    None
}

/// Löst `party_code`/`join_code` aus den Result-Feldern auf (Befund
/// manager.py:201-209). Geordnete Kandidaten-Prüfung mit gegenseitiger Auffüllung;
/// fehlen beide → Fehler.
fn resolve_party_codes(result: &Value) -> SteamTaskResult<(String, String)> {
    let pick = |keys: &[&str]| -> Option<String> {
        for k in keys {
            if let Some(Value::String(s)) = result.get(*k) {
                if !s.is_empty() {
                    return Some(s.clone());
                }
            }
            if let Some(Value::Number(n)) = result.get(*k) {
                return Some(n.to_string());
            }
        }
        None
    };
    let party_code = pick(&["party_code", "party_code_display", "join_code"]);
    let join_code = pick(&["join_code", "party_code", "party_code_display"]);
    match (party_code, join_code) {
        (None, None) => Err(SteamTaskError::failed(
            "Lobby-Erstellung lieferte keinen Party-Code",
        )),
        (Some(pc), None) => Ok((pc.clone(), pc)),
        (None, Some(jc)) => Ok((jc.clone(), jc)),
        (Some(pc), Some(jc)) => Ok((pc, jc)),
    }
}
