use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObserverMode {
    Shadow,
    Assist,
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CameraAction {
    SpectateLobby { lobby_id: u64 },
    Directed,
    HeroChase { account_id: u32 },
    PlayerView { account_id: u32 },
}

impl CameraAction {
    pub fn account_id(&self) -> Option<u32> {
        match self {
            Self::SpectateLobby { .. } | Self::Directed => None,
            Self::HeroChase { account_id } | Self::PlayerView { account_id } => Some(*account_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraCommand {
    pub id: i64,
    pub session_key: String,
    pub action: CameraAction,
    pub reason: String,
    pub score: Option<f64>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentHeartbeat {
    pub agent_version: String,
    pub bot_account_id: i16,
    pub vconsole_connected: bool,
    pub game_connected: bool,
    pub current_action: Option<CameraAction>,
    pub last_command_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAck {
    pub ok: bool,
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_actions_are_typed_and_never_raw_console() {
        let value = serde_json::to_value(CameraAction::PlayerView { account_id: 42 }).unwrap();
        assert_eq!(value["action"], "player_view");
        assert_eq!(value["account_id"], 42);
        assert!(value.get("command").is_none());
    }
}
