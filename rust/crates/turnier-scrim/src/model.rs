use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod wire_id {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn i32<S>(value: &i32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn i64<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn opt_i32<S>(value: &Option<i32>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn opt_i64<S>(value: &Option<i64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn de_i32<'de, D>(deserializer: D) -> Result<i32, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_i32(&value).map_err(serde::de::Error::custom)
    }

    pub fn de_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_i64(&value).map_err(serde::de::Error::custom)
    }

    pub fn de_opt_i32<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)?
            .map(|value| parse_i32(&value).map_err(serde::de::Error::custom))
            .transpose()
    }

    pub fn de_opt_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer)?
            .map(|value| parse_i64(&value).map_err(serde::de::Error::custom))
            .transpose()
    }

    pub fn parse_i32(value: &str) -> Result<i32, String> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("ID must be a positive decimal string".to_string());
        }
        value
            .parse::<i32>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or_else(|| "ID must fit into a positive int32".to_string())
    }

    pub fn parse_i64(value: &str) -> Result<i64, String> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("ID must be a positive decimal string".to_string());
        }
        value
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or_else(|| "ID must fit into a positive int64".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AvailabilityStatus {
    Available,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailabilitySlot {
    pub status: AvailabilityStatus,
    pub from: Option<u16>,
    pub to: Option<u16>,
}

impl Default for AvailabilitySlot {
    fn default() -> Self {
        Self {
            status: AvailabilityStatus::Unknown,
            from: None,
            to: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeeklyAvailability {
    #[serde(default)]
    pub mon: Option<AvailabilitySlot>,
    #[serde(default)]
    pub tue: Option<AvailabilitySlot>,
    #[serde(default)]
    pub wed: Option<AvailabilitySlot>,
    #[serde(default)]
    pub thu: Option<AvailabilitySlot>,
    #[serde(default)]
    pub fri: Option<AvailabilitySlot>,
    #[serde(default)]
    pub sat: Option<AvailabilitySlot>,
    #[serde(default)]
    pub sun: Option<AvailabilitySlot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrimDay {
    #[serde(rename = "mon")]
    Monday,
    #[serde(rename = "tue")]
    Tuesday,
    #[serde(rename = "wed")]
    Wednesday,
    #[serde(rename = "thu")]
    Thursday,
    #[serde(rename = "fri")]
    Friday,
    #[serde(rename = "sat")]
    Saturday,
    #[serde(rename = "sun")]
    Sunday,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrimSlot {
    pub day: ScrimDay,
    pub from: u16,
    pub to: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchRequestTemplate {
    RegularScrim,
    Testmatch,
    Training,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRequestPairingInput {
    pub team_a_id: i32,
    pub team_b_id: Option<i32>,
    pub slots: Option<Vec<ScrimSlot>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRequestBatchInput {
    pub template: MatchRequestTemplate,
    pub deadline_at: DateTime<Utc>,
    pub slots: Option<Vec<ScrimSlot>>,
    pub matches: Vec<MatchRequestPairingInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMatchRequest {
    pub team_a_id: i32,
    pub team_b_id: Option<i32>,
    pub slots: Vec<ScrimSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedMatchRequestBatch {
    pub template: MatchRequestTemplate,
    pub deadline_at: DateTime<Utc>,
    pub matches: Vec<ValidatedMatchRequest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Participant {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub id: i32,
    pub discord_id: Option<String>,
    pub display_name: String,
    pub rank: Option<String>,
    pub rank_source: String,
    pub rank_verified: bool,
    pub roles: Option<String>,
    pub availability: Option<String>,
    pub availability_slots: Option<WeeklyAvailability>,
    pub notes: Option<String>,
    pub status: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamMember {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub team_id: i32,
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub participant_id: i32,
    pub display_name: String,
    pub rank: Option<String>,
    pub discord_id: Option<String>,
    pub roles: Option<String>,
    pub availability: Option<String>,
    pub availability_slots: Option<WeeklyAvailability>,
    pub notes: Option<String>,
    pub role: Option<String>,
    pub is_captain: bool,
    pub is_bench: bool,
    pub substitute_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub id: i32,
    pub name: String,
    pub coach: Option<String>,
    pub coach_discord_id: Option<String>,
    pub discord_role_id: Option<String>,
    pub discord_channel_id: Option<String>,
    pub default_from: Option<i32>,
    pub default_to: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub members: Vec<TeamMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamRef {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coach {
    pub discord_user_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseChoice {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRequestResponse {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub request_id: i32,
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub team_id: i32,
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub participant_id: i32,
    pub discord_user_id: String,
    pub slot_index: i32,
    pub response: ResponseChoice,
    pub source: String,
    pub message_id: Option<String>,
    pub channel_id: Option<String>,
    pub responded_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchRequest {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub id: i32,
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub batch_id: i32,
    pub team_a: TeamRef,
    pub team_b: Option<TeamRef>,
    pub status: String,
    pub slots: Vec<ScrimSlot>,
    pub released_slot_index: Option<i32>,
    pub released_slot: Option<ScrimSlot>,
    pub released_at: Option<DateTime<Utc>>,
    pub released_by_user_id: Option<String>,
    pub released_by_display_name: Option<String>,
    pub override_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub responses: Vec<MatchRequestResponse>,
    pub facts: MatchRequestFacts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchRequestBatch {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub id: i32,
    pub template: MatchRequestTemplate,
    pub deadline_at: DateTime<Utc>,
    pub status: String,
    pub created_by_user_id: String,
    pub created_by_display_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub requests: Vec<MatchRequest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedMatchResult {
    #[serde(serialize_with = "wire_id::i64", deserialize_with = "wire_id::de_i64")]
    pub result_ref_id: i64,
    pub steam_match_id: String,
    #[serde(
        serialize_with = "wire_id::opt_i32",
        deserialize_with = "wire_id::de_opt_i32"
    )]
    pub winner_team_id: Option<i32>,
    pub source: String,
    pub selected_at: DateTime<Utc>,
    pub selected_by_user_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrimMatch {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub id: i32,
    pub team_a: Option<TeamRef>,
    pub team_b: Option<TeamRef>,
    pub when_text: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub status: String,
    pub lobby_state: Option<String>,
    pub party_id: Option<String>,
    pub join_code: Option<String>,
    pub lobby_code_source_user_id: Option<String>,
    pub lobby_code_source_display_name: Option<String>,
    pub lobby_code_updated_at: Option<DateTime<Utc>>,
    pub coach_spectator_discord_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub selected_result: Option<SelectedMatchResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LagebildEvidenceRef {
    #[serde(serialize_with = "wire_id::i64", deserialize_with = "wire_id::de_i64")]
    pub id: i64,
    pub evidence_type: String,
    pub label: String,
    pub url: Option<String>,
    pub reference_id: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LagebildSnapshotRef {
    #[serde(serialize_with = "wire_id::i64", deserialize_with = "wire_id::de_i64")]
    pub id: i64,
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub team_id: i32,
    pub generated_at: DateTime<Utc>,
    pub generated_for: String,
    pub source: String,
    pub status: String,
    pub model: Option<String>,
    pub error: Option<String>,
    pub evidences: Vec<LagebildEvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterMember {
    pub team_id: i32,
    pub participant_id: i32,
    pub display_name: String,
    pub is_bench: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotFacts {
    pub index: usize,
    pub available_count: u32,
    pub starter_available_count: u32,
    pub team_available_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacementNeed {
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub request_id: i32,
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub team_id: i32,
    #[serde(serialize_with = "wire_id::i32", deserialize_with = "wire_id::de_i32")]
    pub participant_id: i32,
    pub display_name: String,
    pub slot_index: usize,
    pub reason: String,
    pub is_bench: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRequestFacts {
    pub slots: Vec<SlotFacts>,
    pub missing_response_count: u32,
    pub no_slot_count: u32,
    pub recommended_slot_index: Option<usize>,
    pub selected_slot_index: Option<usize>,
    pub replacement_needs: Vec<ReplacementNeed>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrimReadModel {
    pub participants: Vec<Participant>,
    pub teams: Vec<Team>,
    pub matches: Vec<ScrimMatch>,
    pub match_request_batches: Vec<MatchRequestBatch>,
    pub lagebild_refs: Vec<LagebildSnapshotRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrimMe {
    pub participant: Option<Participant>,
    pub team: Option<Team>,
    pub next_match: Option<ScrimMatch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamBoard {
    pub team: Team,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamTimeline {
    pub team: TeamRef,
    pub matches: Vec<ScrimMatch>,
    pub lagebild_refs: Vec<LagebildSnapshotRef>,
}
