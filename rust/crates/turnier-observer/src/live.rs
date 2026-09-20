use std::collections::HashMap;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::director::{MatchFrame, PlayerSnapshot};

pub const DEFAULT_CONTROLLER_QUERY: &str = "SELECT * FROM CCitadelPlayerController";
pub const DEFAULT_PAWN_QUERY: &str = "SELECT * FROM CCitadelPlayerPawn";

#[derive(Debug, thiserror::Error)]
pub enum LiveError {
    #[error("Deadlock API transport: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Deadlock API antwortete mit {0}")]
    Status(reqwest::StatusCode),
    #[error("Live-Broadcast-URL fehlt in der Antwort")]
    BroadcastUrlMissing,
    #[error("Live-SSE: {0}")]
    Sse(String),
}

#[derive(Clone)]
pub struct DeadlockLiveClient {
    http: reqwest::Client,
    base_url: String,
}

impl DeadlockLiveClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, LiveError> {
        Self::with_network(base_url, &turnier_config::NetworkConfig::default())
    }

    pub fn with_network(
        base_url: impl Into<String>,
        network: &turnier_config::NetworkConfig,
    ) -> Result<Self, LiveError> {
        let http = reqwest::Client::builder()
            // deadlock-api.com akzeptiert normale API-Clients, blockt aber derzeit
            // Requests ohne brauchbaren User-Agent teilweise bereits am Edge mit
            // HTTP 403. Fester, ehrlicher Produkt-UA statt Browser-Imitation.
            .user_agent("DeutscheDeadlockCommunity-Observer/1.0")
            .connect_timeout(StdDuration::from_secs(network.observer_connect_seconds))
            .tcp_keepalive(StdDuration::from_secs(network.observer_keepalive_seconds))
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        })
    }

    pub async fn resolve_broadcast_url(&self, match_id: u64) -> Result<String, LiveError> {
        let response = self
            .http
            .get(format!("{}/v1/matches/{match_id}/live/url", self.base_url))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(LiveError::Status(response.status()));
        }
        let value: Value = response.json().await?;
        find_string_recursive(
            &value,
            &[
                "broadcast_url",
                "url",
                "watch_url",
                "live_url",
                "spectate_url",
            ],
        )
        .ok_or(LiveError::BroadcastUrlMissing)
    }

    pub async fn stream_rows(
        &self,
        broadcast_url: &str,
        query: &str,
        kind: LiveRowKind,
        tx: mpsc::Sender<LiveRow>,
    ) -> Result<(), LiveError> {
        let response = self
            .http
            .get(format!("{}/v1/matches/demo/live/query", self.base_url))
            .query(&[("query", query), ("broadcast_url", broadcast_url)])
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(LiveError::Status(response.status()));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut event = String::new();
        let mut data = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(newline) = buffer.find('\n') {
                let mut line = buffer[..newline].to_string();
                buffer.drain(..=newline);
                if line.ends_with('\r') {
                    line.pop();
                }
                if line.is_empty() {
                    // Nach SSE ist ein fehlendes `event:` implizit ein
                    // `message`-Event. deadlock-api.com liefert seine Live-Rows
                    // aktuell genau in dieser Default-Form (`data:` + Leerzeile).
                    if is_message_event(&event) && !data.is_empty() {
                        let value: Value = serde_json::from_str(&data)
                            .map_err(|e| LiveError::Sse(format!("ungueltiges JSON: {e}")))?;
                        let received_at = Utc::now();
                        for value in normalize_live_rows(value) {
                            if tx
                                .send(LiveRow {
                                    kind,
                                    received_at,
                                    value,
                                })
                                .await
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                    } else if event == "end" {
                        return Ok(());
                    } else if event == "error" {
                        return Err(LiveError::Sse(data.clone()));
                    }
                    event.clear();
                    data.clear();
                    continue;
                }
                if let Some(value) = line.strip_prefix("event:") {
                    event = value.trim().to_string();
                } else if let Some(value) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(value.trim_start());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRowKind {
    Controller,
    Pawn,
}

#[derive(Debug, Clone)]
pub struct LiveRow {
    pub kind: LiveRowKind,
    pub received_at: DateTime<Utc>,
    pub value: Value,
}

#[derive(Debug, Clone, Default)]
struct ControllerState {
    entity_index: Option<i64>,
    account_id: Option<u32>,
    hero_id: Option<u32>,
    team: Option<u8>,
    net_worth: Option<i64>,
    kills: i64,
    assists: i64,
    deaths: i64,
    previous_kills: i64,
    previous_assists: i64,
    previous_deaths: i64,
    last_stat_change_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
struct PawnState {
    controller_index: Option<i64>,
    health: Option<f64>,
    max_health: Option<f64>,
    previous_health: Option<f64>,
    position: Option<[f64; 3]>,
    last_health_change_at: Option<DateTime<Utc>>,
    damage_taken_recent: f64,
}

/// Normalisiert die schemavarianten Live-Zeilen in den kleinen stabilen
/// Observer-Vertrag. Unbekannte Felder werden ignoriert, nie als Null-Fakten
/// erfunden.
#[derive(Debug, Default)]
pub struct LiveAccumulator {
    controllers_by_entity: HashMap<i64, ControllerState>,
    controllers_by_account: HashMap<u32, ControllerState>,
    pawns_by_controller: HashMap<i64, PawnState>,
    last_observed_at: Option<DateTime<Utc>>,
}

impl LiveAccumulator {
    pub fn apply(&mut self, row: LiveRow) {
        self.last_observed_at = Some(row.received_at);
        match row.kind {
            LiveRowKind::Controller => self.apply_controller(&row.value, row.received_at),
            LiveRowKind::Pawn => self.apply_pawn(&row.value, row.received_at),
        }
    }

    pub fn frame(&self, now: DateTime<Utc>) -> MatchFrame {
        let observed_at = self.last_observed_at.unwrap_or(now);
        let mut players = Vec::with_capacity(self.controllers_by_account.len());
        for (&account_id, controller) in &self.controllers_by_account {
            let pawn = controller
                .entity_index
                .and_then(|idx| self.pawns_by_controller.get(&idx));
            let recent_window = controller
                .last_stat_change_at
                .is_some_and(|at| now.signed_duration_since(at).num_seconds() <= 15);
            let kills_recent = if recent_window {
                (controller.kills - controller.previous_kills)
                    .max(0)
                    .min(255) as u8
            } else {
                0
            };
            let assists_recent = if recent_window {
                (controller.assists - controller.previous_assists)
                    .max(0)
                    .min(255) as u8
            } else {
                0
            };
            let deaths_recent = if recent_window {
                (controller.deaths - controller.previous_deaths)
                    .max(0)
                    .min(255) as u8
            } else {
                0
            };
            let position = pawn.and_then(|p| p.position);
            let alive = pawn.and_then(|p| p.health).is_none_or(|hp| hp > 0.0);
            let in_combat = pawn
                .and_then(|p| p.last_health_change_at)
                .is_some_and(|at| now.signed_duration_since(at).num_seconds() <= 5)
                || kills_recent > 0
                || assists_recent > 0;
            let mut snapshot = PlayerSnapshot {
                account_id,
                hero_id: controller.hero_id,
                team: controller.team.unwrap_or(255),
                alive,
                health: pawn.and_then(|p| p.health),
                max_health: pawn.and_then(|p| p.max_health),
                position,
                net_worth: controller.net_worth,
                kills_recent,
                assists_recent,
                deaths_recent,
                damage_dealt_recent: 0.0,
                damage_taken_recent: pawn.map_or(0.0, |p| {
                    if p.last_health_change_at
                        .is_some_and(|at| now.signed_duration_since(at).num_seconds() <= 8)
                    {
                        p.damage_taken_recent
                    } else {
                        0.0
                    }
                }),
                enemies_nearby: 0,
                allies_nearby: 0,
                objective_pressure: 0.0,
                high_impact_active: false,
                in_combat,
            };
            if position.is_none() {
                snapshot.enemies_nearby = if in_combat { 1 } else { 0 };
            }
            players.push(snapshot);
        }
        enrich_proximity(&mut players);
        MatchFrame {
            observed_at,
            players,
        }
    }

    fn apply_controller(&mut self, raw: &Value, at: DateTime<Utc>) {
        let value = unwrap_row(raw);
        let entity_index = find_i64(
            value,
            &[
                "entity_index",
                "_entity_index",
                "index",
                "entity",
                "entindex",
            ],
        );
        let account_id = find_i64(
            value,
            &["account_id", "m_accountID", "m_steamID", "steam_id"],
        )
        .and_then(normalize_account_id);
        let mut state = account_id
            .and_then(|id| self.controllers_by_account.get(&id).cloned())
            .or_else(|| entity_index.and_then(|idx| self.controllers_by_entity.get(&idx).cloned()))
            .unwrap_or_default();
        state.entity_index = entity_index.or(state.entity_index);
        state.account_id = account_id.or(state.account_id);
        state.hero_id = find_i64(value, &["hero_id", "m_nHeroID", "m_unHeroID", "m_nHeroId"])
            .and_then(|v| u32::try_from(v).ok())
            .or(state.hero_id);
        state.team = find_i64(value, &["team", "team_num", "m_iTeamNum", "m_nTeamNum"])
            .and_then(|v| u8::try_from(v).ok())
            .or(state.team);
        state.net_worth =
            find_i64(value, &["net_worth", "networth", "m_iNetWorth"]).or(state.net_worth);

        let next_kills = find_i64(value, &["kills", "m_iKills", "m_nKills"]);
        let next_assists = find_i64(value, &["assists", "m_iAssists", "m_nAssists"]);
        let next_deaths = find_i64(value, &["deaths", "m_iDeaths", "m_nDeaths"]);
        if next_kills.is_some_and(|v| v != state.kills)
            || next_assists.is_some_and(|v| v != state.assists)
            || next_deaths.is_some_and(|v| v != state.deaths)
        {
            state.previous_kills = state.kills;
            state.previous_assists = state.assists;
            state.previous_deaths = state.deaths;
            state.last_stat_change_at = Some(at);
        }
        if let Some(value) = next_kills {
            state.kills = value;
        }
        if let Some(value) = next_assists {
            state.assists = value;
        }
        if let Some(value) = next_deaths {
            state.deaths = value;
        }

        if let Some(idx) = state.entity_index {
            self.controllers_by_entity.insert(idx, state.clone());
        }
        if let Some(id) = state.account_id {
            self.controllers_by_account.insert(id, state);
        }
    }

    fn apply_pawn(&mut self, raw: &Value, at: DateTime<Utc>) {
        let value = unwrap_row(raw);
        let Some(controller_index) = find_i64(
            value,
            &[
                "controller_index",
                "m_hController",
                "controller",
                "controller_handle",
            ],
        )
        .map(entity_index_from_handle) else {
            return;
        };
        let mut state = self
            .pawns_by_controller
            .get(&controller_index)
            .cloned()
            .unwrap_or_default();
        state.controller_index = Some(controller_index);
        let health = find_f64(value, &["health", "m_iHealth", "m_flHealth"]);
        if let Some(next) = health {
            if let Some(previous) = state.health {
                if next < previous {
                    state.damage_taken_recent =
                        (state.damage_taken_recent + (previous - next)).min(10_000.0);
                    state.last_health_change_at = Some(at);
                }
            }
            state.previous_health = state.health;
            state.health = Some(next);
        }
        state.max_health =
            find_f64(value, &["max_health", "m_iMaxHealth", "m_flMaxHealth"]).or(state.max_health);
        state.position = find_vec3(value).or(state.position);
        self.pawns_by_controller.insert(controller_index, state);
    }
}

fn enrich_proximity(players: &mut [PlayerSnapshot]) {
    let snapshot = players.to_vec();
    for player in players {
        let Some(position) = player.position else {
            continue;
        };
        let mut enemies = 0_u8;
        let mut allies = 0_u8;
        for other in &snapshot {
            if other.account_id == player.account_id || !other.alive {
                continue;
            }
            let Some(other_pos) = other.position else {
                continue;
            };
            if distance(position, other_pos) > 2_500.0 {
                continue;
            }
            if other.team == player.team {
                allies = allies.saturating_add(1);
            } else {
                enemies = enemies.saturating_add(1);
            }
        }
        player.enemies_nearby = enemies;
        player.allies_nearby = allies;
        if enemies > 0 {
            player.in_combat = true;
        }
    }
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn entity_index_from_handle(value: i64) -> i64 {
    value & 0x7fff
}

fn normalize_account_id(value: i64) -> Option<u32> {
    if value <= 0 {
        return None;
    }
    Some((value as u64 & 0xffff_ffff) as u32).filter(|v| *v > 0)
}

fn is_message_event(event: &str) -> bool {
    event.is_empty() || event == "message"
}

fn normalize_live_rows(value: Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items,
        Value::Object(mut map) => {
            for key in ["rows", "data", "result"] {
                if let Some(Value::Array(items)) = map.remove(key) {
                    return items;
                }
            }
            vec![Value::Object(map)]
        }
        other => vec![other],
    }
}

fn unwrap_row(value: &Value) -> &Value {
    value
        .get("row")
        .or_else(|| value.get("data"))
        .unwrap_or(value)
}

fn find_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        lookup_path(value, key).and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
    })
}

fn find_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        lookup_path(value, key).and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_i64().map(|n| n as f64))
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
    })
}

fn find_vec3(value: &Value) -> Option<[f64; 3]> {
    for key in [
        "position",
        "origin",
        "m_vecAbsOrigin",
        "m_vOldOrigin",
        "CBodyComponent.m_vecAbsOrigin",
        "m_pGameSceneNode.m_vecAbsOrigin",
    ] {
        let Some(candidate) = lookup_path(value, key) else {
            continue;
        };
        if let Some(array) = candidate.as_array() {
            if array.len() >= 3 {
                if let (Some(x), Some(y), Some(z)) =
                    (array[0].as_f64(), array[1].as_f64(), array[2].as_f64())
                {
                    return Some([x, y, z]);
                }
            }
        }
        if let Some(object) = candidate.as_object() {
            let x = object.get("x").and_then(Value::as_f64);
            let y = object.get("y").and_then(Value::as_f64);
            let z = object.get("z").and_then(Value::as_f64);
            if let (Some(x), Some(y), Some(z)) = (x, y, z) {
                return Some([x, y, z]);
            }
        }
    }
    None
}

fn lookup_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if let Some(direct) = value.get(path) {
        return Some(direct);
    }
    let mut current = value;
    for part in path.split('.') {
        current = current.get(part)?;
    }
    Some(current)
}

fn find_string_recursive(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(found) = map.get(*key).and_then(Value::as_str) {
                    if !found.trim().is_empty() {
                        return Some(found.to_string());
                    }
                }
            }
            map.values()
                .find_map(|child| find_string_recursive(child, keys))
        }
        Value::Array(items) => items
            .iter()
            .find_map(|child| find_string_recursive(child, keys)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sse_batches_are_flattened_to_rows() {
        let rows = normalize_live_rows(json!({"rows":[{"account_id":1},{"account_id":2}]}));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["account_id"], 1);
        assert_eq!(rows[1]["account_id"], 2);
    }

    #[test]
    fn sse_default_event_is_a_message() {
        assert!(is_message_event(""));
        assert!(is_message_event("message"));
        assert!(!is_message_event("status"));
        assert!(!is_message_event("error"));
    }

    #[test]
    fn steam64_is_normalized_to_account_id() {
        let id = 76561198000000000_i64;
        assert_eq!(
            normalize_account_id(id),
            Some((id as u64 & 0xffff_ffff) as u32)
        );
    }

    #[test]
    fn controller_and_pawn_rows_form_a_frame() {
        let now = Utc::now();
        let mut acc = LiveAccumulator::default();
        acc.apply(LiveRow {
            kind: LiveRowKind::Controller,
            received_at: now,
            value: json!({
                "entity_index": 17,
                "m_steamID": 76561198000000042_i64,
                "m_nHeroID": 7,
                "m_iTeamNum": 0,
                "m_iKills": 3
            }),
        });
        acc.apply(LiveRow {
            kind: LiveRowKind::Pawn,
            received_at: now,
            value: json!({
                "m_hController": 17,
                "m_iHealth": 350,
                "m_iMaxHealth": 1000,
                "m_vecAbsOrigin": [10.0, 20.0, 30.0]
            }),
        });
        let frame = acc.frame(now);
        assert_eq!(frame.players.len(), 1);
        assert_eq!(frame.players[0].hero_id, Some(7));
        assert_eq!(frame.players[0].health, Some(350.0));
        assert_eq!(frame.players[0].position, Some([10.0, 20.0, 30.0]));
    }

    #[test]
    fn nearby_enemy_is_derived_from_positions() {
        let mut players = vec![
            PlayerSnapshot {
                account_id: 1,
                hero_id: None,
                team: 0,
                alive: true,
                health: None,
                max_health: None,
                position: Some([0.0, 0.0, 0.0]),
                net_worth: None,
                kills_recent: 0,
                assists_recent: 0,
                deaths_recent: 0,
                damage_dealt_recent: 0.0,
                damage_taken_recent: 0.0,
                enemies_nearby: 0,
                allies_nearby: 0,
                objective_pressure: 0.0,
                high_impact_active: false,
                in_combat: false,
            },
            PlayerSnapshot {
                account_id: 2,
                hero_id: None,
                team: 1,
                alive: true,
                health: None,
                max_health: None,
                position: Some([100.0, 0.0, 0.0]),
                net_worth: None,
                kills_recent: 0,
                assists_recent: 0,
                deaths_recent: 0,
                damage_dealt_recent: 0.0,
                damage_taken_recent: 0.0,
                enemies_nearby: 0,
                allies_nearby: 0,
                objective_pressure: 0.0,
                high_impact_active: false,
                in_combat: false,
            },
        ];
        enrich_proximity(&mut players);
        assert_eq!(players[0].enemies_nearby, 1);
        assert!(players[0].in_combat);
    }
}
