//! Die öffentliche, fachliche Schicht: dieselben Operationen wie
//! `discord_notifier.py` (Channel anlegen/löschen, Embeds posten, DMs,
//! Voice/Rollen-Abfragen), 1:1 im Verhalten, mit `turnier-db`-Pool + [`BrokerClient`].
//!
//! Alle Snowflake-IDs werden EINMAL beim Eintritt geparst; ungültige IDs landen
//! deterministisch in der jeweiligen `failed`-Bucket statt zu panicken.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{Postgres, QueryBuilder};

use turnier_core::{discord_id_to_string, parse_discord_id};
use turnier_db::Pool;

use crate::broker::BrokerClient;
use crate::embed::Embed;
use crate::error::{BrokerError, BrokerResult, INVALID_ID_ERROR};
use crate::event::{NotificationEvent, NOTIFY_DM_DEFAULT};
use crate::ids::{idempotency_key, parse_snowflake, unique_preserve_order};
use crate::tasks::{self, TaskType};

/// Broker-Endpunkt-Pfade (zentral, statt verstreut wie im Original).
mod path {
    pub const CREATE_CHANNEL: &str = "/internal/master/v1/discord/create-channel";
    pub const SEND_RICH_MESSAGE: &str = "/internal/master/v1/discord/send-rich-message";
    pub const SEND_MESSAGE: &str = "/internal/master/v1/discord/send-message";
    pub const DELETE_CHANNEL: &str = "/internal/master/v1/discord/delete-channel";
    pub const MOVE_VOICE: &str = "/internal/master/v1/discord/member/move-voice";
    pub const VOICE_MEMBERS: &str = "/internal/master/v1/discord/voice-channel/members";
    pub const ROLE_MEMBERS: &str = "/internal/master/v1/discord/role/members";
}

/// Ergebnis von [`notify_users`]: dieselbe Wire-Shape wie das Original
/// (`{sent, skipped, failed}`), damit Aufrufer unverändert bleiben.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct NotifyUsersResult {
    pub sent: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<FailedId>,
}

/// Ergebnis von [`move_users_to_voice_channel`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct MoveVoiceResult {
    pub moved: Vec<String>,
    pub failed: Vec<FailedId>,
}

/// Ergebnis von [`notify_casters_match_created`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct CasterNotifyResult {
    pub sent: Vec<String>,
    pub failed: Vec<FailedId>,
}

/// Eine fehlgeschlagene ID mit Fehlertext (`{discord_id, error}` im Original).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FailedId {
    pub discord_id: String,
    pub error: String,
}

impl FailedId {
    fn new(discord_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            discord_id: discord_id.into(),
            error: error.into(),
        }
    }
}

/// Der fachliche Notifier. Hält Broker-Client + DB-Pool + die für einige
/// Operationen benötigten Konfigurationswerte.
#[derive(Debug, Clone)]
pub struct DiscordNotifier {
    broker: BrokerClient,
    pool: Pool,
    match_channel_category_id: i64,
    tournament_lobby_channel_id: i64,
    /// `DISCORD_GUILD_ID` als String (im Original ebenfalls String, für die
    /// Channel-URL). Leer = keine URL.
    guild_id: String,
    /// Standard-Verzögerung für [`delete_match_channel_later`] (aus Config).
    delete_delay_seconds: i64,
}

impl DiscordNotifier {
    /// Baut den Notifier aus Broker, Pool und der aufgelösten Config.
    pub fn new(broker: BrokerClient, pool: Pool, config: &turnier_config::Config) -> Self {
        Self {
            broker,
            pool,
            match_channel_category_id: config.discord_match_channel_category_id,
            tournament_lobby_channel_id: config.discord_tournament_lobby_channel_id,
            guild_id: config.discord_guild_id.clone(),
            delete_delay_seconds: config.discord_match_channel_delete_delay_seconds,
        }
    }

    /// Direkter Zugriff auf den unterliegenden Broker-Client (z. B. für Tests
    /// oder Aufrufer, die roh posten müssen).
    pub fn broker(&self) -> &BrokerClient {
        &self.broker
    }

    /// Kuendigt ein automatisch geoeffnetes Turnier im konfigurierten Kanal an.
    /// Erfolgreiche Tasks deduplizieren weitere Scheduler-Ticks; fehlgeschlagene
    /// werden mit derselben Broker-Idempotency erneut versucht.
    pub async fn announce_routine_tournament(
        &self,
        tournament_id: i64,
        channel_id: i64,
        content: &str,
    ) -> BrokerResult<Value> {
        let done: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM turnier.discord_tasks \
             WHERE type = 'ANNOUNCE_TOURNAMENT' AND status = 'DONE' \
               AND payload->>'tournament_id' = $1)",
        )
        .bind(tournament_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(map_db_err)?;
        if done {
            return Ok(json!({ "deduplicated": true }));
        }

        let task_payload = json!({
            "tournament_id": tournament_id,
            "channel_id": channel_id,
            "content": content,
        });
        let task_id =
            tasks::create_running(&self.pool, TaskType::AnnounceTournament, &task_payload)
                .await
                .map_err(map_db_err)?;
        let payload = routine_announcement_payload(tournament_id, channel_id, content);
        match self
            .broker
            .post_internal(path::SEND_RICH_MESSAGE, &payload)
            .await
        {
            Ok(result) => {
                let _ = tasks::mark_done(&self.pool, task_id, Some(&result)).await;
                Ok(result)
            }
            Err(err) => {
                let _ = tasks::mark_failed(&self.pool, task_id, &tasks::error_text(&err)).await;
                Err(err)
            }
        }
    }

    // --- Match-Channel-Verwaltung ---------------------------------------

    /// Legt einen Discord-Match-Channel via Broker an, protokolliert als
    /// `CREATE_CHANNEL` und gibt die `channel_id` (als String) zurück.
    pub async fn create_match_channel(
        &self,
        match_id: i64,
        team1_name: &str,
        team2_name: &str,
    ) -> BrokerResult<String> {
        let task_id = tasks::create_running(
            &self.pool,
            TaskType::CreateChannel,
            &json!({ "match_id": match_id, "team1_name": team1_name, "team2_name": team2_name }),
        )
        .await
        .map_err(map_db_err)?;

        let payload = json!({
            "name": crate::channel_name::build_match_channel_name(team1_name, team2_name),
            "category_id": self.match_channel_category_id,
            "topic": format!("Match {match_id}: {team1_name} vs {team2_name}"),
        });

        match self.create_channel_inner(&payload).await {
            Ok(channel_id) => {
                let _ = tasks::mark_done(
                    &self.pool,
                    task_id,
                    Some(&json!({ "channel_id": channel_id })),
                )
                .await;
                Ok(channel_id)
            }
            Err(err) => {
                let _ = tasks::mark_failed(&self.pool, task_id, &tasks::error_text(&err)).await;
                Err(err)
            }
        }
    }

    /// Postet das Broker-Create und extrahiert die nicht-leere `channel_id`.
    async fn create_channel_inner(&self, payload: &Value) -> BrokerResult<String> {
        let result: Value = self
            .broker
            .post_internal(path::CREATE_CHANNEL, payload)
            .await?;
        let channel_id = result
            .get("channel_id")
            .and_then(value_to_id_string)
            .unwrap_or_default();
        if channel_id.is_empty() {
            return Err(BrokerError::BadJson(
                "Discord-Broker lieferte keine channel_id",
            ));
        }
        Ok(channel_id)
    }

    /// Postet das Lobby-Embed (Code + Teilnehmer-Mentions) in den Match-Channel
    /// und protokolliert als `SEND_MATCH_INFO`.
    ///
    /// `channel_id` wird als Snowflake erwartet; eine ungültige ID führt — wie
    /// im Original (`int(channel_id)` warf) — zu einem Fehler (kein Versand),
    /// hier deterministisch als [`BrokerError::Http`]-freier `BadJson`-Fehler
    /// über die Validierung statt Panic.
    pub async fn send_match_lobby_info(
        &self,
        channel_id: &str,
        party_code: &str,
        participant_discord_ids: &[String],
    ) -> BrokerResult<Value> {
        let participant_ids = unique_preserve_order(participant_discord_ids);

        let task_id = tasks::create_running(
            &self.pool,
            TaskType::SendMatchInfo,
            &json!({
                "channel_id": channel_id,
                "party_code": party_code,
                "participant_discord_ids": participant_ids,
            }),
        )
        .await
        .map_err(map_db_err)?;

        let outcome = self
            .send_match_lobby_info_inner(channel_id, party_code, &participant_ids)
            .await;
        match outcome {
            Ok(result) => {
                let _ = tasks::mark_done(&self.pool, task_id, Some(&result)).await;
                Ok(result)
            }
            Err(err) => {
                let _ = tasks::mark_failed(&self.pool, task_id, &tasks::error_text(&err)).await;
                Err(err)
            }
        }
    }

    async fn send_match_lobby_info_inner(
        &self,
        channel_id: &str,
        party_code: &str,
        participant_ids: &[String],
    ) -> BrokerResult<Value> {
        let channel = require_snowflake(channel_id)?;
        let mention_line = mentions(participant_ids);

        let embed = Embed::new()
            .title("Lobby wurde erstellt")
            .description("Die Steam-Lobby ist bereit.")
            .field("Lobby-Code", format!("`{party_code}`"), false)
            .field(
                "Teilnehmer",
                if mention_line.is_empty() {
                    "Keine Teilnehmer mit Discord-ID".to_string()
                } else {
                    mention_line.clone()
                },
                false,
            );

        // allowed_user_ids durchgängig als u64-Snowflakes (Original mischte
        // str/int; Befund discord_notifier.py:194/209 — "behavior-change", hier
        // vereinheitlicht auf int wie vom Port vorgegeben).
        let allowed = parse_all(participant_ids);
        let payload = json!({
            "channel_id": channel,
            "content": option_str(&mention_line),
            "embed": embed,
            "allowed_user_ids": allowed,
        });
        self.broker
            .post_internal(path::SEND_RICH_MESSAGE, &payload)
            .await
    }

    /// Löscht einen Match-Channel via Broker, Task `DELETE_CHANNEL`.
    pub async fn delete_match_channel(&self, channel_id: &str) -> BrokerResult<Value> {
        let task_id = tasks::create_running(
            &self.pool,
            TaskType::DeleteChannel,
            &json!({ "channel_id": channel_id }),
        )
        .await
        .map_err(map_db_err)?;

        match self.delete_match_channel_inner(channel_id).await {
            Ok(result) => {
                let _ = tasks::mark_done(&self.pool, task_id, Some(&result)).await;
                Ok(result)
            }
            Err(err) => {
                let _ = tasks::mark_failed(&self.pool, task_id, &tasks::error_text(&err)).await;
                Err(err)
            }
        }
    }

    async fn delete_match_channel_inner(&self, channel_id: &str) -> BrokerResult<Value> {
        let channel = require_snowflake(channel_id)?;
        let payload = json!({ "channel_id": channel });
        self.broker
            .post_internal(path::DELETE_CHANNEL, &payload)
            .await
    }

    /// Wartet `delay_seconds` (Default aus Config) und löscht dann den Channel.
    /// Fehler werden nur geloggt — wie das `fire-and-forget` des Originals.
    ///
    /// Hinweis (needs-decision, Befund discord_notifier.py:308-313): diese
    /// Verzögerung lebt im Prozess; bei Neustart/Crash geht sie verloren. Das
    /// Verhalten wird hier 1:1 erhalten (kein DB-gestützter Scheduler).
    pub async fn delete_match_channel_later(&self, channel_id: &str, delay_seconds: Option<f64>) {
        let delay = delay_seconds
            .unwrap_or(self.delete_delay_seconds as f64)
            .max(0.0);
        tokio::time::sleep(std::time::Duration::from_secs_f64(delay)).await;
        if let Err(err) = self.delete_match_channel(channel_id).await {
            tracing::error!(channel_id, error = %err, "Verzögertes Löschen des Discord-Match-Channels fehlgeschlagen");
        }
    }

    // --- Direktnachrichten ----------------------------------------------

    /// Sendet eine Event-DM an Spieler unter Beachtung von `tournament_dm_optout`,
    /// des DM-Master-Schalters und des event-spezifischen Flags.
    /// Rückgabe: `{sent, skipped, failed}`.
    pub async fn notify_users(
        &self,
        discord_ids: &[String],
        event: NotificationEvent,
        message: &str,
    ) -> BrokerResult<NotifyUsersResult> {
        let unique_ids = unique_preserve_order(discord_ids);
        if unique_ids.is_empty() {
            return Ok(NotifyUsersResult::default());
        }

        let flags = self
            .load_notify_flags(&unique_ids, event)
            .await
            .map_err(map_db_err)?;
        let optout_ids = self
            .load_tournament_dm_optout_ids(&unique_ids)
            .await
            .map_err(map_db_err)?;

        let mut summary = NotifyUsersResult::default();
        for discord_id in &unique_ids {
            if optout_ids.contains(discord_id) {
                summary.skipped.push(discord_id.clone());
                continue;
            }

            // Profil-loses Verhalten EXAKT wie im Original (Befund
            // discord_notifier.py:283-284 — "behavior-change", bewusst erhalten):
            // fehlt das Profil, gilt für BEIDE Schalter das EVENT-Default.
            let (dm_master, event_flag) = match flags.iter().find(|(id, _, _)| id == discord_id) {
                Some((_, dm, ev)) => (*dm, *ev),
                None => (event.default_flag(), event.default_flag()),
            };
            // NOTIFY_DM_DEFAULT existiert als eigene Konstante, wird aber für
            // profil-lose User absichtlich NICHT verwendet (Original-Semantik).
            let _ = NOTIFY_DM_DEFAULT;

            if !dm_master || !event_flag {
                summary.skipped.push(discord_id.clone());
                continue;
            }

            let task_id = tasks::create_running(
                &self.pool,
                TaskType::SendDm,
                &json!({ "discord_id": discord_id, "event_type": event.as_str(), "message": message }),
            )
            .await
            .map_err(map_db_err)?;

            match self.send_dm_inner(discord_id, message).await {
                Ok(result) => {
                    let _ = tasks::mark_done(&self.pool, task_id, Some(&result)).await;
                    summary.sent.push(discord_id.clone());
                }
                Err(err) => {
                    let text = tasks::error_text(&err);
                    let _ = tasks::mark_failed(&self.pool, task_id, &text).await;
                    summary.failed.push(FailedId::new(discord_id.clone(), text));
                }
            }
        }
        Ok(summary)
    }

    /// Sendet eine einzelne DM. Ungültige `discord_id` → Fehler statt Panic
    /// (im Original warf `int(discord_id)`; hier in die `failed`-Bucket
    /// geroutet, da der Aufrufer pro ID fängt).
    async fn send_dm_inner(&self, discord_id: &str, message: &str) -> BrokerResult<Value> {
        let user_id = require_snowflake(discord_id)?;
        let payload = json!({ "user_id": user_id, "content": message });
        self.broker
            .post_internal(path::SEND_MESSAGE, &payload)
            .await
    }

    /// Lädt für die gegebenen IDs den DM-Master-Schalter und das event-relevante
    /// Flag. Nur die ZWEI benötigten Spalten werden selektiert (Befund
    /// discord_notifier.py:269-276 — "safe"). Rückgabe je gefundenem Profil:
    /// `(discord_id, notify_discord_dm, notify_event)`.
    async fn load_notify_flags(
        &self,
        ids: &[String],
        event: NotificationEvent,
    ) -> sqlx::Result<Vec<(String, bool, bool)>> {
        // event.column() ist whitelisted (festes Enum) — keine User-Eingabe in
        // der Spalten-Konkatenation.
        let parsed_ids = ids
            .iter()
            .filter_map(|id| parse_discord_id(id).ok().map(|value| (value, id.as_str())))
            .collect::<Vec<_>>();
        if parsed_ids.is_empty() {
            return Ok(Vec::new());
        }

        let column = event.column();
        let mut query = QueryBuilder::<Postgres>::new(format!(
            "SELECT discord_id, notify_discord_dm AS dm, {column} AS ev \
             FROM turnier.user_profiles WHERE discord_id IN ("
        ));
        let mut separated = query.separated(", ");
        for (id, _) in &parsed_ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(")");

        let rows = query.build().fetch_all(&self.pool).await?;

        use sqlx::Row;
        Ok(rows
            .into_iter()
            .map(|row| {
                let id = discord_id_to_string(row.get::<i64, _>("discord_id"));
                let dm: bool = row.get("dm");
                let ev: bool = row.get("ev");
                (id, dm, ev)
            })
            .collect())
    }

    /// Lädt alle IDs, die eine Turnier-DM-Unterdrückung gesetzt haben.
    async fn load_tournament_dm_optout_ids(&self, ids: &[String]) -> sqlx::Result<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }

        let parsed_ids = ids
            .iter()
            .filter_map(|id| parse_discord_id(id).ok().map(|value| (value, id.as_str())))
            .collect::<Vec<_>>();
        if parsed_ids.is_empty() {
            return Ok(HashSet::new());
        }

        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT DISTINCT discord_id FROM turnier.tournament_dm_optout WHERE discord_id IN (",
        );
        let mut separated = query.separated(", ");
        for (id, _) in &parsed_ids {
            separated.push_bind(*id);
        }
        separated.push_unseparated(")");

        let rows: Vec<i64> = query.build_query_scalar().fetch_all(&self.pool).await?;
        let opted_out = rows.into_iter().collect::<HashSet<_>>();
        Ok(parsed_ids
            .into_iter()
            .filter(|(value, _)| opted_out.contains(value))
            .map(|(_, original)| original.to_string())
            .collect())
    }

    /// DM an jeden Caster + Sammel-Mention-Embed im Match-Channel.
    pub async fn notify_casters_match_created(
        &self,
        match_id: i64,
        channel_id: &str,
        caster_discord_ids: &[String],
    ) -> BrokerResult<CasterNotifyResult> {
        let unique_ids = unique_preserve_order(caster_discord_ids);
        let optout_ids = self
            .load_tournament_dm_optout_ids(&unique_ids)
            .await
            .map_err(map_db_err)?;
        let dm_ids = unique_ids
            .into_iter()
            .filter(|discord_id| !optout_ids.contains(discord_id))
            .collect::<Vec<_>>();
        let mut summary = CasterNotifyResult::default();

        // channel_url nur, wenn guild_id gesetzt UND channel_id numerisch ist.
        // Im Original: int(channel_id) im f-String — eine ungültige channel_id
        // hätte dort eine Exception geworfen. Hier: ungültige channel_id ⇒ keine
        // URL (Fallback "#<channel_id>"), kein Panic.
        let channel_num = parse_snowflake(channel_id);
        let channel_url = if !self.guild_id.is_empty() {
            channel_num.map(|c| format!("https://discord.com/channels/{}/{}", self.guild_id, c))
        } else {
            None
        };

        for discord_id in &dm_ids {
            let content = format!(
                "Du bist als Caster für Match #{match_id} eingetragen. Match-Channel: {}",
                channel_url
                    .clone()
                    .unwrap_or_else(|| format!("#{channel_id}"))
            );
            match self.send_dm_inner(discord_id, &content).await {
                Ok(_) => summary.sent.push(discord_id.clone()),
                Err(err) => summary
                    .failed
                    .push(FailedId::new(discord_id.clone(), tasks::error_text(&err))),
            }
        }

        if !dm_ids.is_empty() {
            let channel = require_snowflake(channel_id)?;
            let embed = Embed::new()
                .title("Caster informiert")
                .description("Die zugewiesenen Caster wurden benachrichtigt.");
            let payload = json!({
                "channel_id": channel,
                "content": mentions(&dm_ids),
                "embed": embed,
                "allowed_user_ids": parse_all(&dm_ids),
            });
            let _: Value = self
                .broker
                .post_internal(path::SEND_RICH_MESSAGE, &payload)
                .await?;
        }

        Ok(summary)
    }

    // --- Lobby-/Stats-Embeds --------------------------------------------

    /// Postet das Lobby-Embed mit Team-Split in den zentralen
    /// `DISCORD_TOURNAMENT_LOBBY_CHANNEL_ID`.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_lobby_announcement(
        &self,
        match_id: i64,
        party_code: &str,
        team1_name: &str,
        team2_name: &str,
        team1_discord_ids: &[String],
        team2_discord_ids: &[String],
        hero_assignments_text: Option<&[String]>,
        objective_text: Option<&str>,
    ) -> BrokerResult<Value> {
        let team1_mentions = mentions_or_dash(team1_discord_ids);
        let team2_mentions = mentions_or_dash(team2_discord_ids);

        // all_ids = beide Teams zusammen, gültige Snowflakes (Original ließ
        // ungültige via `if uid`-Filter heraus; hier verwerfen wir nicht
        // parsebare zusätzlich — dieselbe Wirkung: nur gültige int-IDs).
        let combined: Vec<String> = team1_discord_ids
            .iter()
            .chain(team2_discord_ids.iter())
            .cloned()
            .collect();
        let all_ids = parse_all(&combined);

        let mut embed = Embed::new()
            .title(format!("Match {match_id} — Lobby bereit"))
            .field("Lobby-Code", format!("`{party_code}`"), false)
            .field(format!("🔵 {team1_name}"), team1_mentions, true)
            .field(format!("🔴 {team2_name}"), team2_mentions, true);

        if let Some(heroes) = hero_assignments_text.filter(|h| !h.is_empty()) {
            let joined = heroes
                .iter()
                .take(20)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            embed = embed.field("Hero-Zuteilung", joined, false);
        }
        if let Some(obj) = objective_text.filter(|o| !o.is_empty()) {
            embed = embed.field("Wertung", obj.to_string(), false);
        }

        let content = mentions(&combined);
        let payload = json!({
            "channel_id": self.tournament_lobby_channel_id,
            "content": option_str(&content),
            "embed": embed,
            "allowed_user_ids": all_ids,
            "idempotency_key": idempotency_key(&format!("lobby-ann-{match_id}")),
        });
        self.broker
            .post_internal(path::SEND_RICH_MESSAGE, &payload)
            .await
    }

    /// Postet das Ergebnis-Embed (Sieger, Dauer, K/D/A) in den Match-Channel.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_match_stats_to_channel(
        &self,
        channel_id: &str,
        match_id: i64,
        deadlock_match_id: Option<&str>,
        team1_name: &str,
        team2_name: &str,
        winner_name: &str,
        duration_s: Option<i64>,
        player_stats: Option<&[PlayerStat]>,
    ) -> BrokerResult<Value> {
        let channel = require_snowflake(channel_id)?;
        let duration_str = match duration_s {
            Some(s) if s != 0 => format!("{}m {}s", s / 60, s % 60),
            _ => "unbekannt".to_string(),
        };

        // Befund discord_notifier.py:431/452 ("safe"): doppeltes Slicing [:12]
        // dann [:10]. Hier auf EIN Limit (take(10)) reduziert.
        let stats_lines: Vec<String> = player_stats
            .unwrap_or(&[])
            .iter()
            .take(10)
            .map(|p| {
                let name = p.display_name();
                format!("**{}** — {}/{}/{}", name, p.kills, p.deaths, p.assists)
            })
            .collect();

        let mut embed = Embed::new()
            .title(format!("Match {match_id} — Ergebnis"))
            .description(format!("**Sieger: {winner_name}**\nDauer: {duration_str}"))
            .field(
                "Match ID (Deadlock)",
                deadlock_match_id.unwrap_or("—").to_string(),
                true,
            )
            .field("Teams", format!("{team1_name} vs {team2_name}"), true);

        if !stats_lines.is_empty() {
            embed = embed.field("Spieler-Stats (K/D/A)", stats_lines.join("\n"), false);
        }

        let payload = json!({
            "channel_id": channel,
            "content": Value::Null,
            "embed": embed,
            "allowed_user_ids": Vec::<u64>::new(),
            "idempotency_key": idempotency_key(&format!("stats-{match_id}")),
        });
        self.broker
            .post_internal(path::SEND_RICH_MESSAGE, &payload)
            .await
    }

    // --- Voice/Rollen-Abfragen ------------------------------------------

    /// Verschiebt User einzeln in einen Voice-Channel (idempotency_key pro Call).
    pub async fn move_users_to_voice_channel(
        &self,
        discord_ids: &[String],
        channel_id: i64,
        guild_id: i64,
    ) -> MoveVoiceResult {
        let unique_ids = unique_preserve_order(discord_ids);
        let mut results = MoveVoiceResult::default();
        for discord_id in &unique_ids {
            let user_id = match parse_snowflake(discord_id) {
                Some(id) => id,
                None => {
                    tracing::warn!(discord_id, "move_voice: ungültige Discord-ID");
                    results.failed.push(FailedId::new(
                        discord_id.clone(),
                        INVALID_ID_ERROR.to_string(),
                    ));
                    continue;
                }
            };
            let payload = json!({
                "guild_id": guild_id,
                "user_id": user_id,
                "channel_id": channel_id,
                "idempotency_key": idempotency_key(&format!("move-{discord_id}-{channel_id}")),
            });
            match self
                .broker
                .post_internal::<Value, _>(path::MOVE_VOICE, &payload)
                .await
            {
                Ok(_) => results.moved.push(discord_id.clone()),
                Err(err) => {
                    tracing::warn!(discord_id, error = %err, "move_voice fehlgeschlagen");
                    results
                        .failed
                        .push(FailedId::new(discord_id.clone(), tasks::error_text(&err)));
                }
            }
        }
        results
    }

    /// Liest die aktuellen Voice-Channel-Mitglieder über den Broker. Fehlendes
    /// oder nicht-Listen-`members` → leere Liste (wie im Original).
    pub async fn get_voice_channel_members(&self, channel_id: i64) -> BrokerResult<Vec<Value>> {
        let result: Value = self
            .broker
            .post_internal(path::VOICE_MEMBERS, &json!({ "channel_id": channel_id }))
            .await?;
        Ok(members_list(&result))
    }

    /// Liest Guild-Mitglieder einer Rolle über den Broker.
    pub async fn get_role_members(&self, guild_id: i64, role_id: i64) -> BrokerResult<Vec<Value>> {
        let result: Value = self
            .broker
            .post_internal(
                path::ROLE_MEMBERS,
                &json!({ "guild_id": guild_id, "role_id": role_id }),
            )
            .await?;
        Ok(members_list(&result))
    }
}

/// Eine Spieler-Statistik-Zeile für [`DiscordNotifier::send_match_stats_to_channel`].
#[derive(Debug, Clone, Default)]
pub struct PlayerStat {
    pub hero: Option<String>,
    pub player_name: Option<String>,
    pub discord_name: Option<String>,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
}

impl PlayerStat {
    /// Anzeigename mit derselben Präzedenz wie das Original
    /// (`hero` ∨ `player_name` ∨ `discord_name` ∨ `"?"`).
    fn display_name(&self) -> &str {
        for value in [&self.hero, &self.player_name, &self.discord_name]
            .into_iter()
            .flatten()
        {
            if !value.is_empty() {
                return value;
            }
        }
        "?"
    }
}

// --- freie Helfer --------------------------------------------------------

/// `<@id> <@id> …` für die gegebenen (bereits getrimmten) IDs.
fn mentions(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("<@{id}>"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn routine_announcement_payload(tournament_id: i64, channel_id: i64, content: &str) -> Value {
    json!({
        "channel_id": channel_id,
        "content": content,
        "embed": {},
        "allowed_user_ids": [],
        "allowed_role_ids": [],
        "idempotency_key": format!("routine-announcement-{tournament_id}"),
    })
}

/// Wie [`mentions`], aber leere Liste → `"—"` (für die Team-Felder).
fn mentions_or_dash(ids: &[String]) -> String {
    let raw: Vec<String> = ids.iter().map(|id| format!("<@{id}>")).collect();
    if raw.is_empty() {
        "—".to_string()
    } else {
        raw.join(" ")
    }
}

/// `Some(s)` falls nicht leer, sonst JSON-`null` (Original: `mention_line or None`).
fn option_str(value: &str) -> Value {
    if value.is_empty() {
        Value::Null
    } else {
        Value::String(value.to_string())
    }
}

/// Parst alle IDs zu u64-Snowflakes; nicht parsebare werden verworfen
/// (entspricht dem `if uid`-Filter des Originals, das leere Strings übersprang).
fn parse_all(ids: &[String]) -> Vec<u64> {
    ids.iter().filter_map(|id| parse_snowflake(id)).collect()
}

/// Verlangt eine gültige Snowflake; ungültig → deterministischer Fehler statt
/// Panic. Spiegelt den `int(...)`-Wurf des Originals, ohne den Prozess zu töten.
fn require_snowflake(value: &str) -> BrokerResult<u64> {
    parse_snowflake(value).ok_or(BrokerError::BadJson("Ungültige Discord-Snowflake"))
}

/// Liest `result["members"]` als Liste, sonst leere Liste.
fn members_list(result: &Value) -> Vec<Value> {
    result
        .get("members")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Wandelt einen JSON-Wert (`channel_id`) in einen getrimmten ID-String:
/// String wird getrimmt, Zahl wird stringifiziert (Original: `str(... or "").strip()`).
fn value_to_id_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Übersetzt einen DB-Fehler in einen [`BrokerError`]. Das Original ließ
/// DB-Fehler beim Task-Logging durchschlagen; hier kapseln wir sie sichtbar.
fn map_db_err(err: sqlx::Error) -> BrokerError {
    BrokerError::Http {
        status: 500,
        detail: format!("discord_tasks-DB-Fehler: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_baut_korrekt() {
        assert_eq!(mentions(&["1".into(), "2".into()]), "<@1> <@2>");
        assert_eq!(mentions(&[]), "");
        assert_eq!(mentions_or_dash(&[]), "—");
    }

    #[test]
    fn option_str_leer_ist_null() {
        assert_eq!(option_str(""), Value::Null);
        assert_eq!(option_str("x"), Value::String("x".into()));
    }

    #[test]
    fn parse_all_verwirft_ungueltige() {
        let ids = vec!["1".into(), "nope".into(), "".into(), "42".into()];
        assert_eq!(parse_all(&ids), vec![1u64, 42u64]);
    }

    #[test]
    fn require_snowflake_ungueltig_ist_fehler() {
        assert!(require_snowflake("nope").is_err());
        assert_eq!(require_snowflake("123").unwrap(), 123);
    }

    #[test]
    fn members_list_robust() {
        assert_eq!(
            members_list(&json!({ "members": [1, 2] })),
            vec![json!(1), json!(2)]
        );
        assert_eq!(
            members_list(&json!({ "members": "nope" })),
            Vec::<Value>::new()
        );
        assert_eq!(members_list(&json!({})), Vec::<Value>::new());
    }

    #[test]
    fn value_to_id_string_varianten() {
        assert_eq!(value_to_id_string(&json!("  42 ")), Some("42".to_string()));
        assert_eq!(value_to_id_string(&json!(99)), Some("99".to_string()));
        assert_eq!(value_to_id_string(&Value::Null), None);
    }

    #[test]
    fn player_stat_display_praezedenz() {
        let p = PlayerStat {
            hero: Some("Abrams".into()),
            player_name: Some("x".into()),
            ..Default::default()
        };
        assert_eq!(p.display_name(), "Abrams");
        let p2 = PlayerStat {
            player_name: Some("Spieler".into()),
            ..Default::default()
        };
        assert_eq!(p2.display_name(), "Spieler");
        let p3 = PlayerStat::default();
        assert_eq!(p3.display_name(), "?");
        // Leerer hero-String fällt durch auf den nächsten Kandidaten.
        let p4 = PlayerStat {
            hero: Some("".into()),
            discord_name: Some("D".into()),
            ..Default::default()
        };
        assert_eq!(p4.display_name(), "D");
    }

    #[test]
    fn routine_ankuendigung_payload_ist_idempotent() {
        let payload = routine_announcement_payload(42, 123, "PLATZHALTER");
        assert_eq!(payload["channel_id"], 123);
        assert_eq!(payload["content"], "PLATZHALTER");
        assert_eq!(payload["idempotency_key"], "routine-announcement-42");
    }
}
