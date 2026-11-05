use std::collections::{BTreeMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

pub const TICKS_PER_SECOND: f64 = 64.0;
const AMBER_TEAM_NUM: i64 = 2;
const SAPPHIRE_TEAM_NUM: i64 = 3;
const MAX_SOUL_POINTS: usize = 3600;
const MAX_ITEM_EVENTS: usize = 24;
const MAX_FEED_EVENTS: usize = 12;
const REVIVE_MARGIN_SECONDS: f64 = 2.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerRow {
    pub slot: i32,
    pub account_id: Option<u32>,
    pub name: Option<String>,
    pub team_num: Option<i64>,
    pub hero_id: Option<u32>,
    pub net_worth: Option<i64>,
    pub kills: Option<i64>,
    pub deaths: Option<i64>,
    pub assists: Option<i64>,
    pub alive: Option<bool>,
    pub respawn_time: Option<f64>,
    pub ult_cooldown_end: Option<f64>,
    pub ult_trained: Option<bool>,
    pub health: Option<i64>,
    pub health_max: Option<i64>,
    pub hero_damage: Option<i64>,
    pub hero_healing: Option<i64>,
    pub objective_damage: Option<i64>,
    pub level: Option<i64>,
    #[serde(default)]
    pub upgrades: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RulesRow {
    pub match_id: Option<u64>,
    pub game_start_time: Option<f64>,
    pub total_paused_ticks: Option<i64>,
    pub pause_start_tick: Option<i64>,
    pub paused: Option<bool>,
    pub game_state: Option<i64>,
    pub winning_team: Option<i64>,
    #[serde(default)]
    pub banned_heroes: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DecodedTick {
    pub tick: i64,
    pub rules: Option<RulesRow>,
    #[serde(default)]
    pub players: Vec<PlayerRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BossKill {
    pub tick: i64,
    pub team_num: Option<i64>,
    pub objective: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudPhase {
    Waiting,
    Live,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HudTeam {
    pub team: u8,
    pub souls: Option<i64>,
    pub kills: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HudPlayer {
    pub slot: i32,
    pub account_id: Option<u32>,
    pub name: Option<String>,
    pub hero_id: Option<u32>,
    pub team: u8,
    pub net_worth: Option<i64>,
    pub kills: Option<i64>,
    pub deaths: Option<i64>,
    pub assists: Option<i64>,
    pub alive: Option<bool>,
    pub respawn_seconds: Option<f64>,
    pub ult_ready: Option<bool>,
    pub health_fraction: Option<f64>,
    pub hero_damage: Option<i64>,
    pub healing: Option<i64>,
    pub objective_damage: Option<i64>,
    pub level: Option<i64>,
    pub items: Vec<u32>,
    pub kill_participation: Option<f64>,
    pub net_worth_rank: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SoulPoint {
    pub second: u32,
    pub diff: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HudEvent {
    Item {
        clock_seconds: Option<f64>,
        team: u8,
        slot: i32,
        account_id: Option<u32>,
        hero_id: Option<u32>,
        item_id: u32,
    },
    Revive {
        clock_seconds: Option<f64>,
        team: u8,
        slot: i32,
        account_id: Option<u32>,
        hero_id: Option<u32>,
    },
    Objective {
        clock_seconds: Option<f64>,
        team: Option<u8>,
        objective: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HudSnapshot {
    pub match_id: Option<u64>,
    pub tick: Option<i64>,
    pub clock_seconds: Option<f64>,
    pub paused: Option<bool>,
    pub phase: HudPhase,
    pub winner: Option<u8>,
    pub bans: Vec<u32>,
    pub teams: [HudTeam; 2],
    pub players: Vec<HudPlayer>,
    pub soul_difference: Vec<SoulPoint>,
    pub item_events: Vec<HudEvent>,
    pub feed_events: Vec<HudEvent>,
}

#[derive(Debug, Default)]
pub struct HudAccumulator {
    tick: Option<i64>,
    rules: RulesRow,
    players: BTreeMap<i32, PlayerRow>,
    souls: VecDeque<SoulPoint>,
    item_events: VecDeque<HudEvent>,
    feed_events: VecDeque<HudEvent>,
    last_tick: Option<i64>,
    seen_boss_kills: HashSet<(i64, Option<i64>, String)>,
}

pub fn team_index(team_num: Option<i64>) -> Option<u8> {
    match team_num {
        Some(AMBER_TEAM_NUM) => Some(0),
        Some(SAPPHIRE_TEAM_NUM) => Some(1),
        _ => None,
    }
}

fn curtime(tick: i64) -> f64 {
    tick as f64 / TICKS_PER_SECOND
}

pub fn match_clock(tick: i64, rules: &RulesRow) -> Option<f64> {
    let start = rules.game_start_time.filter(|value| *value > 0.0)?;
    let paused_ticks = rules.total_paused_ticks.unwrap_or(0).max(0);
    let effective_tick = match (rules.paused, rules.pause_start_tick) {
        (Some(true), Some(pause_tick)) if pause_tick > 0 => pause_tick.min(tick),
        _ => tick,
    };
    Some((curtime(effective_tick) - start - curtime(paused_ticks)).max(0.0))
}

impl HudAccumulator {
    pub fn last_tick(&self) -> Option<i64> {
        self.last_tick
    }

    pub fn apply_tick(&mut self, decoded: &DecodedTick) {
        if self.last_tick.is_some_and(|last| decoded.tick <= last) {
            return;
        }
        self.last_tick = Some(decoded.tick);
        self.tick = Some(decoded.tick);
        if let Some(rules) = &decoded.rules {
            self.rules = rules.clone();
        }
        let clock = self.clock();
        for row in decoded
            .players
            .iter()
            .filter(|row| player_team(row).is_some())
        {
            self.track_player(decoded.tick, clock, row);
        }
        self.record_souls(clock);
    }

    pub fn apply_boss_kill(&mut self, kill: &BossKill) {
        let key = (kill.tick, kill.team_num, kill.objective.clone());
        if !self.seen_boss_kills.insert(key) {
            return;
        }
        let clock = match_clock(kill.tick, &self.rules);
        push_bounded(
            &mut self.feed_events,
            HudEvent::Objective {
                clock_seconds: clock,
                team: team_index(kill.team_num),
                objective: kill.objective.clone(),
            },
            MAX_FEED_EVENTS,
        );
    }

    pub fn is_finished(&self) -> bool {
        self.winner().is_some()
    }

    fn clock(&self) -> Option<f64> {
        self.tick.and_then(|tick| match_clock(tick, &self.rules))
    }

    fn winner(&self) -> Option<u8> {
        team_index(self.rules.winning_team)
    }

    fn track_player(&mut self, tick: i64, clock: Option<f64>, row: &PlayerRow) {
        if let Some(previous) = self.players.get(&row.slot) {
            let events = transition_events(tick, clock, previous, row);
            for event in events {
                match event {
                    HudEvent::Item { .. } => {
                        push_bounded(&mut self.item_events, event, MAX_ITEM_EVENTS)
                    }
                    _ => push_bounded(&mut self.feed_events, event, MAX_FEED_EVENTS),
                }
            }
        }
        self.players.insert(row.slot, row.clone());
    }

    fn record_souls(&mut self, clock: Option<f64>) {
        let (Some(clock), Some(amber), Some(sapphire)) =
            (clock, self.team_souls(0), self.team_souls(1))
        else {
            return;
        };
        let second = clock.floor() as u32;
        if self
            .souls
            .back()
            .is_some_and(|point| point.second >= second)
        {
            return;
        }
        push_bounded(
            &mut self.souls,
            SoulPoint {
                second,
                diff: amber - sapphire,
            },
            MAX_SOUL_POINTS,
        );
    }

    fn team_rows(&self, team: u8) -> impl Iterator<Item = &PlayerRow> {
        self.players
            .values()
            .filter(move |row| player_team(row) == Some(team))
    }

    fn team_souls(&self, team: u8) -> Option<i64> {
        sum_known(self.team_rows(team).map(|row| row.net_worth))
    }

    fn team_kills(&self, team: u8) -> Option<i64> {
        sum_known(self.team_rows(team).map(|row| row.kills))
    }

    pub fn snapshot(&self) -> HudSnapshot {
        let clock = self.clock();
        let ranks = net_worth_ranks(&self.players);
        let players = self
            .players
            .values()
            .filter_map(|row| {
                let team = player_team(row)?;
                Some(self.hud_player(row, team, ranks.get(&row.slot).copied()))
            })
            .collect();
        HudSnapshot {
            match_id: self.rules.match_id,
            tick: self.tick,
            clock_seconds: clock,
            paused: self.rules.paused,
            phase: self.phase(clock),
            winner: self.winner(),
            bans: self.rules.banned_heroes.clone(),
            teams: [self.hud_team(0), self.hud_team(1)],
            players,
            soul_difference: self.souls.iter().copied().collect(),
            item_events: self.item_events.iter().cloned().collect(),
            feed_events: self.feed_events.iter().cloned().collect(),
        }
    }

    fn phase(&self, clock: Option<f64>) -> HudPhase {
        if self.is_finished() {
            HudPhase::Ended
        } else if clock.is_some() {
            HudPhase::Live
        } else {
            HudPhase::Waiting
        }
    }

    fn hud_team(&self, team: u8) -> HudTeam {
        HudTeam {
            team,
            souls: self.team_souls(team),
            kills: self.team_kills(team),
        }
    }

    fn hud_player(&self, row: &PlayerRow, team: u8, rank: Option<u32>) -> HudPlayer {
        let now = self.tick.map(curtime);
        HudPlayer {
            slot: row.slot,
            account_id: row.account_id.filter(|id| *id != 0),
            name: row.name.clone().filter(|name| !name.is_empty()),
            hero_id: row.hero_id.filter(|id| *id != 0),
            team,
            net_worth: row.net_worth,
            kills: row.kills,
            deaths: row.deaths,
            assists: row.assists,
            alive: row.alive,
            respawn_seconds: respawn_seconds(row, now),
            ult_ready: ult_ready(row, now),
            health_fraction: health_fraction(row),
            hero_damage: row.hero_damage,
            healing: row.hero_healing,
            objective_damage: row.objective_damage,
            level: row.level,
            items: row.upgrades.clone(),
            kill_participation: kill_participation(row, self.team_kills(team)),
            net_worth_rank: rank,
        }
    }
}

fn player_team(row: &PlayerRow) -> Option<u8> {
    team_index(row.team_num)
}

fn sum_known(values: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    values.fold(None, |acc, value| match (acc, value) {
        (acc, None) => acc,
        (None, Some(value)) => Some(value),
        (Some(acc), Some(value)) => Some(acc + value),
    })
}

fn push_bounded<T>(queue: &mut VecDeque<T>, value: T, cap: usize) {
    queue.push_back(value);
    while queue.len() > cap {
        queue.pop_front();
    }
}

fn transition_events(
    tick: i64,
    clock: Option<f64>,
    previous: &PlayerRow,
    current: &PlayerRow,
) -> Vec<HudEvent> {
    let Some(team) = player_team(current) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    let owned: HashSet<u32> = previous.upgrades.iter().copied().collect();
    for item_id in current.upgrades.iter().filter(|id| !owned.contains(id)) {
        events.push(HudEvent::Item {
            clock_seconds: clock,
            team,
            slot: current.slot,
            account_id: current.account_id,
            hero_id: current.hero_id,
            item_id: *item_id,
        });
    }
    if is_revive(tick, previous, current) {
        events.push(HudEvent::Revive {
            clock_seconds: clock,
            team,
            slot: current.slot,
            account_id: current.account_id,
            hero_id: current.hero_id,
        });
    }
    events
}

fn is_revive(tick: i64, previous: &PlayerRow, current: &PlayerRow) -> bool {
    if previous.alive != Some(false) || current.alive != Some(true) {
        return false;
    }
    previous
        .respawn_time
        .filter(|respawn| *respawn > 0.0)
        .is_some_and(|respawn| respawn - curtime(tick) > REVIVE_MARGIN_SECONDS)
}

fn respawn_seconds(row: &PlayerRow, now: Option<f64>) -> Option<f64> {
    if row.alive != Some(false) {
        return None;
    }
    let respawn = row.respawn_time.filter(|value| *value > 0.0)?;
    Some((respawn - now?).max(0.0).ceil())
}

fn ult_ready(row: &PlayerRow, now: Option<f64>) -> Option<bool> {
    match row.ult_trained? {
        false => Some(false),
        true => Some(row.ult_cooldown_end.unwrap_or(0.0) <= now?),
    }
}

fn health_fraction(row: &PlayerRow) -> Option<f64> {
    let max = row.health_max.filter(|value| *value > 0)?;
    Some((row.health? as f64 / max as f64).clamp(0.0, 1.0))
}

fn kill_participation(row: &PlayerRow, team_kills: Option<i64>) -> Option<f64> {
    let team_kills = team_kills.filter(|kills| *kills > 0)?;
    let involved = row.kills? + row.assists?;
    Some((involved as f64 / team_kills as f64).clamp(0.0, 1.0))
}

fn net_worth_ranks(players: &BTreeMap<i32, PlayerRow>) -> BTreeMap<i32, u32> {
    let mut ranked: Vec<(i32, i64)> = players
        .values()
        .filter(|row| player_team(row).is_some())
        .filter_map(|row| Some((row.slot, row.net_worth?)))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .enumerate()
        .map(|(index, (slot, _))| (slot, index as u32 + 1))
        .collect()
}
