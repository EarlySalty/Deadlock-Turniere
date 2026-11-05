use std::collections::{HashMap, HashSet};

use boon::{Entity, FieldValue, Serializer};
use prost::Message;

use crate::hud::{BossKill, DecodedTick, PlayerRow, RulesRow};

const CONTROLLER_CLASS: &str = "CCitadelPlayerController";
const RULES_CLASS: &str = "CCitadelGameRulesProxy";
const BOSS_KILLED_MESSAGE: u32 = 347;
const ENTITY_INDEX_MASK: u32 = 0x3fff;
const MAX_VECTOR_ITEMS: i64 = 64;
const PLAYER_DATA: &str = "m_PlayerDataGlobal.";
const RULES_DATA: &str = "m_pGameRules.";

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("Demo-Decoder: {0}")]
    Parser(String),
}

#[derive(Debug, Default)]
pub struct DecodedWindow {
    pub ticks: Vec<DecodedTick>,
    pub boss_kills: Vec<BossKill>,
}

#[derive(Clone, PartialEq, Message)]
struct BossKilledMessage {
    #[prost(int32, optional, tag = "1")]
    objective_team: Option<i32>,
    #[prost(uint32, optional, tag = "3")]
    entity_killed: Option<u32>,
}

pub fn decode_window(
    demo: Vec<u8>,
    after_tick: Option<i64>,
    sample_ticks: i64,
) -> Result<DecodedWindow, DecodeError> {
    let parser = boon::Parser::from_bytes(demo);
    let mut ticks = Vec::new();
    let mut boss_classes: HashMap<u32, String> = HashMap::new();
    let mut last_sampled: Option<i64> = None;
    parser
        .run_to_end(|ctx| {
            let tick = i64::from(ctx.tick());
            if after_tick.is_some_and(|after| tick <= after) {
                return;
            }
            if last_sampled.is_some_and(|last| tick - last < sample_ticks) {
                return;
            }
            last_sampled = Some(tick);
            let mut decoded = DecodedTick {
                tick,
                rules: None,
                players: Vec::new(),
            };
            for (index, entity) in ctx.entities().iter() {
                let Some(serializer) = ctx.serializers().get(&entity.class_name) else {
                    continue;
                };
                match entity.class_name.as_ref() {
                    CONTROLLER_CLASS => decoded.players.push(player_row(index, entity, serializer)),
                    RULES_CLASS => decoded.rules = Some(rules_row(entity, serializer)),
                    name if objective_name(name).is_some() => {
                        boss_classes.insert(index as u32, name.to_string());
                    }
                    _ => {}
                }
            }
            ticks.push(decoded);
        })
        .map_err(|err| DecodeError::Parser(err.to_string()))?;
    let boss_kills = boss_kills(&parser, after_tick, &boss_classes)?;
    Ok(DecodedWindow { ticks, boss_kills })
}

fn boss_kills(
    parser: &boon::Parser,
    after_tick: Option<i64>,
    classes: &HashMap<u32, String>,
) -> Result<Vec<BossKill>, DecodeError> {
    let wanted: HashSet<u32> = [BOSS_KILLED_MESSAGE].into_iter().collect();
    let events = parser
        .events_filtered(None, &wanted)
        .map_err(|err| DecodeError::Parser(err.to_string()))?;
    Ok(events
        .into_iter()
        .filter(|event| !after_tick.is_some_and(|after| i64::from(event.tick) <= after))
        .filter_map(|event| {
            let message = BossKilledMessage::decode(event.payload.as_slice()).ok()?;
            let index = message.entity_killed? & ENTITY_INDEX_MASK;
            let objective = classes.get(&index).and_then(|name| objective_name(name))?;
            Some(BossKill {
                tick: i64::from(event.tick),
                team_num: message.objective_team.map(i64::from),
                objective: objective.to_string(),
            })
        })
        .collect())
}

fn objective_name(class_name: &str) -> Option<&'static str> {
    match class_name {
        "CNPC_Boss_Tier2" => Some("walker"),
        "CNPC_Boss_Tier3" => Some("patron"),
        "CNPC_TrooperBoss" => Some("guardian"),
        "CNPC_BarrackBoss" => Some("base_guardian"),
        "CNPC_MidBoss" => Some("mid_boss"),
        _ => None,
    }
}

fn player_row(index: i32, entity: &Entity, serializer: &Serializer) -> PlayerRow {
    let field = |name: &str| entity.get_by_name(name, serializer);
    let data = |name: &str| field(&format!("{PLAYER_DATA}{name}"));
    PlayerRow {
        slot: index,
        account_id: field("m_unEconAccountID")
            .and_then(as_i64)
            .and_then(|value| u32::try_from(value).ok()),
        name: field("m_iszPlayerName").and_then(as_string),
        team_num: field("m_iTeamNum").and_then(as_i64),
        hero_id: data("m_nHeroID")
            .and_then(as_i64)
            .and_then(|value| u32::try_from(value).ok()),
        net_worth: data("m_iGoldNetWorth").and_then(as_i64),
        kills: data("m_iPlayerKills").and_then(as_i64),
        deaths: data("m_iDeaths").and_then(as_i64),
        assists: data("m_iPlayerAssists").and_then(as_i64),
        alive: data("m_bAlive").and_then(as_bool),
        respawn_time: data("m_flRespawnTime").and_then(as_f64),
        ult_cooldown_end: data("m_flUltimateCooldownEnd").and_then(as_f64),
        ult_trained: data("m_bUltimateTrained").and_then(as_bool),
        health: data("m_iHealth").and_then(as_i64),
        health_max: data("m_iHealthMax").and_then(as_i64),
        hero_damage: data("m_iHeroDamage").and_then(as_i64),
        hero_healing: data("m_iHeroHealing").and_then(as_i64),
        objective_damage: data("m_iObjectiveDamage").and_then(as_i64),
        level: data("m_iLevel").and_then(as_i64),
        upgrades: u32_vector(entity, serializer, &format!("{PLAYER_DATA}m_vecUpgrades")),
    }
}

fn rules_row(entity: &Entity, serializer: &Serializer) -> RulesRow {
    let rules = |name: &str| entity.get_by_name(&format!("{RULES_DATA}{name}"), serializer);
    RulesRow {
        match_id: rules("m_unMatchID")
            .and_then(as_i64)
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value != 0),
        game_start_time: rules("m_flGameStartTime").and_then(as_f64),
        total_paused_ticks: rules("m_nTotalPausedTicks").and_then(as_i64),
        pause_start_tick: rules("m_nPauseStartTick").and_then(as_i64),
        paused: rules("m_bGamePaused").and_then(as_bool),
        game_state: rules("m_eGameState").and_then(as_i64),
        winning_team: rules("m_iWinningTeam").and_then(as_i64),
        banned_heroes: u32_vector(
            entity,
            serializer,
            &format!("{RULES_DATA}m_vecBannedHeroes"),
        ),
    }
}

fn u32_vector(entity: &Entity, serializer: &Serializer, path: &str) -> Vec<u32> {
    let len = entity
        .get_by_name(path, serializer)
        .and_then(as_i64)
        .unwrap_or(0)
        .clamp(0, MAX_VECTOR_ITEMS);
    (0..len)
        .filter_map(|index| entity.get_by_name(&format!("{path}.{index}"), serializer))
        .filter_map(as_i64)
        .filter_map(|value| u32::try_from(value).ok())
        .filter(|value| *value != 0)
        .collect()
}

fn as_i64(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Bool(v) => Some(i64::from(*v)),
        FieldValue::I32(v) => Some(i64::from(*v)),
        FieldValue::I64(v) => Some(*v),
        FieldValue::U32(v) => Some(i64::from(*v)),
        FieldValue::U64(v) => i64::try_from(*v).ok(),
        FieldValue::F32(v) => Some(*v as i64),
        _ => None,
    }
}

fn as_f64(value: &FieldValue) -> Option<f64> {
    match value {
        FieldValue::F32(v) => Some(f64::from(*v)),
        other => as_i64(other).map(|v| v as f64),
    }
}

fn as_bool(value: &FieldValue) -> Option<bool> {
    match value {
        FieldValue::Bool(v) => Some(*v),
        other => as_i64(other).map(|v| v != 0),
    }
}

fn as_string(value: &FieldValue) -> Option<String> {
    match value {
        FieldValue::String(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}
