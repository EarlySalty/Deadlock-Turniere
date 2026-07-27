use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::{AvailabilitySlot, MatchRequestTemplate, ScrimSlot};

pub const MATCH_REQUEST_RESPONSE_SCHEMA_VERSION: &str = "turnier-scrim-match-request-response:v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchRequestAction {
    Slot,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchRequestResponseRequest {
    pub schema_version: String,
    pub event: String,
    pub idempotency: String,
    pub action: MatchRequestAction,
    pub request: String,
    pub team: String,
    pub slot: Option<u32>,
    pub interaction: String,
    pub guild: String,
    pub channel: String,
    pub message: Option<String>,
    pub actor: String,
    pub actor_role_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionReceipt {
    pub accepted: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityReceipt {
    pub available: bool,
    pub verified: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PatchValue<T> {
    #[default]
    Omitted,
    Null,
    Value(T),
}

impl<T> PatchValue<T> {
    pub fn is_omitted(&self) -> bool {
        matches!(self, Self::Omitted)
    }
}

impl<T: Serialize> Serialize for PatchValue<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Omitted | Self::Null => serializer.serialize_none(),
            Self::Value(value) => serializer.serialize_some(value),
        }
    }
}

fn deserialize_patch_value<'de, D, T>(deserializer: D) -> Result<PatchValue<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    Option::<T>::deserialize(deserializer).map(|value| match value {
        Some(value) => PatchValue::Value(value),
        None => PatchValue::Null,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseMatchRequest {
    pub slot_index: Option<u32>,
    #[serde(alias = "note")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningCreateRequest {
    pub technical_template_key: Option<String>,
    pub deadline_at: DateTime<Utc>,
    pub slots: Option<Vec<PlanningSlot>>,
    pub pairings: Vec<PlanningPairing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningSlot {
    pub day: crate::model::ScrimDay,
    pub from_minute: u16,
    pub to_minute: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningPairing {
    pub team_a_id: String,
    pub team_b_id: Option<String>,
    #[serde(default)]
    pub slots: Option<Vec<PlanningSlot>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMatchRequest {
    pub team_a_id: Option<String>,
    pub team_b_id: Option<String>,
    pub match_request_id: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    pub coach_spectator_discord_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeeklyAvailability {
    #[serde(default)]
    pub mon: AvailabilitySlot,
    #[serde(default)]
    pub tue: AvailabilitySlot,
    #[serde(default)]
    pub wed: AvailabilitySlot,
    #[serde(default)]
    pub thu: AvailabilitySlot,
    #[serde(default)]
    pub fri: AvailabilitySlot,
    #[serde(default)]
    pub sat: AvailabilitySlot,
    #[serde(default)]
    pub sun: AvailabilitySlot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignupRequest {
    pub rank: Option<String>,
    pub roles: Option<String>,
    pub availability: Option<String>,
    pub availability_slots: Option<WeeklyAvailability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfServiceParticipant {
    pub id: i32,
    pub display_name: String,
    pub rank: Option<String>,
    pub roles: Option<String>,
    pub availability: Option<String>,
    pub availability_slots: WeeklyAvailability,
    pub availability_confirmed: bool,
    pub status: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTeamRequest {
    pub name: String,
    pub coach: Option<String>,
    pub coach_discord_id: Option<String>,
    pub default_from: Option<i32>,
    pub default_to: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamPatchRequest {
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub name: PatchValue<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub coach: PatchValue<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub coach_discord_id: PatchValue<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub default_from: PatchValue<i32>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub default_to: PatchValue<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantPatchRequest {
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub status: PatchValue<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub team_id: PatchValue<i32>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub is_bench: PatchValue<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub is_captain: PatchValue<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub notes: PatchValue<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub rank: PatchValue<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub roles: PatchValue<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnounceTeamRequest {
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuggestTeamRequest {
    pub window: Option<ScrimSlot>,
    pub size: Option<u32>,
    pub pool: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstituteRequest {
    pub participant_id: i32,
    pub window: ScrimSlot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterTeam {
    pub id: i32,
    pub name: String,
    pub coach: Option<String>,
    pub coach_discord_id: Option<String>,
    pub discord_role_id: Option<i64>,
    pub discord_channel_id: Option<i64>,
    pub default_from: Option<i32>,
    pub default_to: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterParticipant {
    pub id: i32,
    pub display_name: String,
    pub rank: Option<String>,
    pub roles: Option<String>,
    pub availability: Option<String>,
    pub availability_slots: WeeklyAvailability,
    pub availability_confirmed: bool,
    pub discord_linked: bool,
    pub notes: Option<String>,
    pub status: String,
    pub source: String,
    pub team: Option<RosterTeam>,
    pub role: Option<String>,
    pub is_captain: bool,
    pub is_bench: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordSyncStatus {
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamMutationResponse {
    #[serde(flatten)]
    pub team: RosterTeam,
    pub discord_sync: DiscordSyncStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantPatchResponse {
    #[serde(flatten)]
    pub participant: RosterParticipant,
    pub discord_sync: DiscordSyncStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordResyncResponse {
    pub discord_sync: DiscordSyncStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnounceTeamResponse {
    pub message_id: Option<String>,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RosterSuggestionCandidate {
    pub participant_id: i32,
    pub display_name: String,
    pub rank: Option<String>,
    pub roles: Option<String>,
    pub availability: Option<String>,
    pub availability_slots: WeeklyAvailability,
    pub availability_confirmed: bool,
    pub status: String,
    pub source: String,
    pub fit_minutes: u32,
    pub fit_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RosterSuggestResponse {
    pub team: RosterTeam,
    pub requested_size: u32,
    pub fit_count: u32,
    pub best_window: Option<ScrimSlot>,
    pub candidates: Vec<RosterSuggestionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubstituteResponse {
    pub participant: RosterParticipant,
    pub discord_sync: DiscordSyncStatus,
    pub dm: DiscordSyncStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyRequest {}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReminderRequest {
    pub template: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchRequestPatch {
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub status: PatchValue<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_patch_value",
        skip_serializing_if = "PatchValue::is_omitted"
    )]
    pub note: PatchValue<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusPublicationRequest {
    pub channel_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementRequestCreate {
    pub match_id: Option<String>,
    pub team_id: Option<String>,
    pub participant_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplacementRequestAction {
    Accept,
    Decline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementRequestPatch {
    pub action: ReplacementRequestAction,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LobbyCodeRequest {
    pub lobby_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchIdsRequest {
    pub match_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultFetchRequest {
    pub match_id_ref: Option<String>,
    pub winner_team_id: Option<String>,
    pub score: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchIdPatchRequest {
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnouncementPublicationRequest {
    pub title: Option<String>,
    pub channel_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRequestDefaults {
    pub default_deadline_hours: u16,
    pub min_slots: u8,
    pub max_slots: u8,
    pub templates: Vec<MatchRequestTemplate>,
    pub preset_slots: Vec<ScrimSlot>,
}
