use std::cmp::Ordering;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::protocol::CameraAction;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub account_id: u32,
    pub hero_id: Option<u32>,
    pub team: u8,
    pub alive: bool,
    pub health: Option<f64>,
    pub max_health: Option<f64>,
    pub position: Option<[f64; 3]>,
    pub net_worth: Option<i64>,
    pub kills_recent: u8,
    pub assists_recent: u8,
    pub deaths_recent: u8,
    pub damage_dealt_recent: f64,
    pub damage_taken_recent: f64,
    pub enemies_nearby: u8,
    pub allies_nearby: u8,
    pub objective_pressure: f64,
    pub high_impact_active: bool,
    pub in_combat: bool,
}

impl PlayerSnapshot {
    pub fn health_fraction(&self) -> Option<f64> {
        let max = self.max_health?;
        if max <= 0.0 {
            return None;
        }
        self.health.map(|hp| (hp / max).clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchFrame {
    pub observed_at: DateTime<Utc>,
    pub players: Vec<PlayerSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ScoreBreakdown {
    pub combat: f64,
    pub nearby_enemies: f64,
    pub nearby_allies: f64,
    pub low_health_pressure: f64,
    pub recent_kills: f64,
    pub recent_assists: f64,
    pub recent_damage: f64,
    pub objective: f64,
    pub high_impact: f64,
    pub clutch: f64,
    pub hold_penalty: f64,
}

impl ScoreBreakdown {
    pub fn total(self) -> f64 {
        self.combat
            + self.nearby_enemies
            + self.nearby_allies
            + self.low_health_pressure
            + self.recent_kills
            + self.recent_assists
            + self.recent_damage
            + self.objective
            + self.high_impact
            + self.clutch
            - self.hold_penalty
    }
}

#[derive(Debug, Clone)]
pub struct DirectorConfig {
    pub min_hold: Duration,
    pub normal_switch_delta: f64,
    pub emergency_switch_delta: f64,
    pub stale_after: Duration,
    pub minimum_interesting_score: f64,
    pub player_view_score: f64,
}

impl Default for DirectorConfig {
    fn default() -> Self {
        Self {
            min_hold: Duration::milliseconds(4_500),
            normal_switch_delta: 12.0,
            emergency_switch_delta: 25.0,
            stale_after: Duration::milliseconds(2_500),
            minimum_interesting_score: 18.0,
            player_view_score: 58.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectorDecision {
    pub action: CameraAction,
    pub account_id: Option<u32>,
    pub hero_id: Option<u32>,
    pub score: f64,
    pub current_score: Option<f64>,
    pub switched: bool,
    pub reason: String,
    pub breakdown: ScoreBreakdown,
    pub frame_age_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Director {
    config: DirectorConfig,
    current_account_id: Option<u32>,
    current_since: Option<DateTime<Utc>>,
}

impl Default for Director {
    fn default() -> Self {
        Self::new(DirectorConfig::default())
    }
}

impl Director {
    pub fn new(config: DirectorConfig) -> Self {
        Self {
            config,
            current_account_id: None,
            current_since: None,
        }
    }

    pub fn current_account_id(&self) -> Option<u32> {
        self.current_account_id
    }

    pub fn release_to_directed(
        &mut self,
        now: DateTime<Utc>,
        reason: impl Into<String>,
    ) -> DirectorDecision {
        let switched = self.current_account_id.take().is_some();
        self.current_since = Some(now);
        DirectorDecision {
            action: CameraAction::Directed,
            account_id: None,
            hero_id: None,
            score: 0.0,
            current_score: None,
            switched,
            reason: reason.into(),
            breakdown: ScoreBreakdown::default(),
            frame_age_ms: 0,
        }
    }

    pub fn decide(&mut self, frame: &MatchFrame, now: DateTime<Utc>) -> DirectorDecision {
        let age = now.signed_duration_since(frame.observed_at);
        if age > self.config.stale_after || age < Duration::milliseconds(-250) {
            let mut decision = self.release_to_directed(now, "live_feed_stale");
            decision.frame_age_ms = age.num_milliseconds();
            return decision;
        }

        let hold = self
            .current_since
            .map(|since| now.signed_duration_since(since))
            .unwrap_or_else(Duration::zero);

        let mut ranked = frame
            .players
            .iter()
            .filter(|p| p.alive)
            .map(|p| {
                let breakdown =
                    score_player(p, self.current_account_id == Some(p.account_id), hold);
                (p, breakdown, breakdown.total())
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal));

        let Some((candidate, candidate_breakdown, candidate_score)) = ranked.first().copied()
        else {
            let mut decision = self.release_to_directed(now, "no_alive_players");
            decision.frame_age_ms = age.num_milliseconds();
            return decision;
        };

        let current = self.current_account_id.and_then(|id| {
            ranked
                .iter()
                .find(|(p, _, _)| p.account_id == id)
                .map(|(_, breakdown, score)| (*breakdown, *score))
        });
        let current_score = current.map(|(_, score)| score);

        if candidate_score < self.config.minimum_interesting_score {
            let mut decision = self.release_to_directed(now, "scene_low_interest");
            decision.score = candidate_score;
            decision.current_score = current_score;
            decision.breakdown = candidate_breakdown;
            decision.frame_age_ms = age.num_milliseconds();
            return decision;
        }

        let same = self.current_account_id == Some(candidate.account_id);
        if same {
            return DirectorDecision {
                action: camera_action(candidate.account_id, candidate_score, &self.config),
                account_id: Some(candidate.account_id),
                hero_id: candidate.hero_id,
                score: candidate_score,
                current_score,
                switched: false,
                reason: primary_reason(candidate, candidate_breakdown),
                breakdown: candidate_breakdown,
                frame_age_ms: age.num_milliseconds(),
            };
        }

        let current_is_alive = self
            .current_account_id
            .and_then(|id| frame.players.iter().find(|p| p.account_id == id))
            .is_some_and(|p| p.alive);
        let delta = candidate_score - current_score.unwrap_or(0.0);
        let switch_allowed = if !current_is_alive || self.current_account_id.is_none() {
            true
        } else if hold < self.config.min_hold {
            delta >= self.config.emergency_switch_delta
        } else {
            delta >= self.config.normal_switch_delta
        };

        if !switch_allowed {
            if let Some(current_id) = self.current_account_id {
                if let Some((current_player, breakdown, score)) = ranked
                    .iter()
                    .find(|(player, _, _)| player.account_id == current_id)
                    .copied()
                {
                    return DirectorDecision {
                        action: camera_action(current_id, score, &self.config),
                        account_id: Some(current_id),
                        hero_id: current_player.hero_id,
                        score,
                        current_score: Some(score),
                        switched: false,
                        reason: "hold_hysteresis".to_string(),
                        breakdown,
                        frame_age_ms: age.num_milliseconds(),
                    };
                }
            }
        }

        self.current_account_id = Some(candidate.account_id);
        self.current_since = Some(now);
        DirectorDecision {
            action: camera_action(candidate.account_id, candidate_score, &self.config),
            account_id: Some(candidate.account_id),
            hero_id: candidate.hero_id,
            score: candidate_score,
            current_score,
            switched: true,
            reason: primary_reason(candidate, candidate_breakdown),
            breakdown: candidate_breakdown,
            frame_age_ms: age.num_milliseconds(),
        }
    }
}

fn camera_action(account_id: u32, score: f64, config: &DirectorConfig) -> CameraAction {
    if score >= config.player_view_score {
        CameraAction::PlayerView { account_id }
    } else {
        CameraAction::HeroChase { account_id }
    }
}

pub fn score_player(
    player: &PlayerSnapshot,
    is_current: bool,
    current_hold: Duration,
) -> ScoreBreakdown {
    let mut score = ScoreBreakdown::default();
    if player.in_combat {
        score.combat = 18.0;
    }
    score.nearby_enemies = f64::from(player.enemies_nearby.min(5)) * 8.0;
    score.nearby_allies = f64::from(player.allies_nearby.min(4)) * 2.0;
    if let Some(hp) = player.health_fraction() {
        if player.in_combat && hp < 0.50 {
            score.low_health_pressure = ((0.50 - hp) / 0.50 * 24.0).clamp(0.0, 24.0);
        }
    }
    score.recent_kills = f64::from(player.kills_recent.min(4)) * 16.0;
    score.recent_assists = f64::from(player.assists_recent.min(5)) * 4.0;
    score.recent_damage = (player.damage_dealt_recent / 125.0).clamp(0.0, 20.0)
        + (player.damage_taken_recent / 250.0).clamp(0.0, 8.0);
    score.objective = player.objective_pressure.clamp(0.0, 1.0) * 15.0;
    if player.high_impact_active {
        score.high_impact = 18.0;
    }
    if player.in_combat && player.enemies_nearby >= 2 && player.allies_nearby == 0 {
        score.clutch += 12.0;
    }
    if player.in_combat && player.enemies_nearby >= 3 {
        score.clutch += 8.0;
    }
    if is_current {
        let over = (current_hold.num_milliseconds() - 12_000).max(0) as f64 / 1000.0;
        score.hold_penalty = over.min(18.0);
    }
    score
}

fn primary_reason(player: &PlayerSnapshot, score: ScoreBreakdown) -> String {
    if player.kills_recent >= 2 {
        return "multikill_window".to_string();
    }
    if score.clutch >= 12.0 && player.health_fraction().is_some_and(|hp| hp < 0.35) {
        return "clutch_low_hp".to_string();
    }
    if player.high_impact_active && player.enemies_nearby >= 2 {
        return "high_impact_teamfight".to_string();
    }
    if player.enemies_nearby >= 3 {
        return "teamfight_cluster".to_string();
    }
    if score.objective >= 7.5 {
        return "objective_pressure".to_string();
    }
    if player.in_combat {
        return "active_fight".to_string();
    }
    "best_available_pov".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(id: u32, enemies: u8, hp: f64, combat: bool) -> PlayerSnapshot {
        PlayerSnapshot {
            account_id: id,
            hero_id: Some(id),
            team: if id % 2 == 0 { 0 } else { 1 },
            alive: true,
            health: Some(hp),
            max_health: Some(1000.0),
            position: None,
            net_worth: None,
            kills_recent: 0,
            assists_recent: 0,
            deaths_recent: 0,
            damage_dealt_recent: 0.0,
            damage_taken_recent: 0.0,
            enemies_nearby: enemies,
            allies_nearby: 1,
            objective_pressure: 0.0,
            high_impact_active: false,
            in_combat: combat,
        }
    }

    #[test]
    fn teamfight_beats_idle_player() {
        let now = Utc::now();
        let frame = MatchFrame {
            observed_at: now,
            players: vec![player(1, 0, 1000.0, false), player(2, 4, 650.0, true)],
        };
        let mut director = Director::default();
        let decision = director.decide(&frame, now);
        assert_eq!(decision.account_id, Some(2));
        assert!(decision.switched);
        assert_eq!(decision.reason, "teamfight_cluster");
    }

    #[test]
    fn low_hp_clutch_gets_priority() {
        let now = Utc::now();
        let mut clutch = player(7, 3, 140.0, true);
        clutch.allies_nearby = 0;
        clutch.damage_dealt_recent = 500.0;
        let frame = MatchFrame {
            observed_at: now,
            players: vec![player(8, 2, 900.0, true), clutch],
        };
        let mut director = Director::default();
        let decision = director.decide(&frame, now);
        assert_eq!(decision.account_id, Some(7));
        assert_eq!(decision.reason, "clutch_low_hp");
    }

    #[test]
    fn small_score_wobble_does_not_camera_spam() {
        let now = Utc::now();
        let mut director = Director::default();
        let first = MatchFrame {
            observed_at: now,
            players: vec![player(1, 3, 700.0, true), player(2, 2, 700.0, true)],
        };
        assert_eq!(director.decide(&first, now).account_id, Some(1));
        let mut challenger = player(2, 3, 700.0, true);
        challenger.damage_dealt_recent = 100.0;
        let second = MatchFrame {
            observed_at: now + Duration::seconds(1),
            players: vec![player(1, 3, 700.0, true), challenger],
        };
        let decision = director.decide(&second, now + Duration::seconds(1));
        assert_eq!(decision.account_id, Some(1));
        assert_eq!(decision.reason, "hold_hysteresis");
    }

    #[test]
    fn emergency_event_can_break_min_hold() {
        let now = Utc::now();
        let mut director = Director::default();
        let first = MatchFrame {
            observed_at: now,
            players: vec![player(1, 3, 700.0, true)],
        };
        director.decide(&first, now);
        let mut emergency = player(2, 5, 120.0, true);
        emergency.kills_recent = 2;
        emergency.high_impact_active = true;
        emergency.allies_nearby = 0;
        let second = MatchFrame {
            observed_at: now + Duration::seconds(1),
            players: vec![player(1, 2, 900.0, true), emergency],
        };
        let decision = director.decide(&second, now + Duration::seconds(1));
        assert_eq!(decision.account_id, Some(2));
        assert!(decision.switched);
    }

    #[test]
    fn stale_feed_releases_to_native_director() {
        let now = Utc::now();
        let mut director = Director::default();
        let frame = MatchFrame {
            observed_at: now - Duration::seconds(4),
            players: vec![player(1, 5, 100.0, true)],
        };
        let decision = director.decide(&frame, now);
        assert_eq!(decision.action, CameraAction::Directed);
        assert_eq!(decision.reason, "live_feed_stale");
    }
}
