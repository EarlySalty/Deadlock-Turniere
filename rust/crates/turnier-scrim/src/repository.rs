use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};

use turnier_db::Pool;

use crate::decision::{derive_match_request_facts, rank_replacement_candidates};
use crate::dto::{
    ActionReceipt, AnnouncementPublicationRequest, CreateTeamRequest, MatchRequestAction,
    MatchRequestPatch, MatchRequestResponseRequest, ParticipantPatchRequest, PatchValue,
    ReleaseMatchRequest, ReminderRequest, ReplacementRequestAction, ReplacementRequestCreate,
    ReplacementRequestPatch, RosterParticipant, RosterTeam, SelfServiceParticipant, SignupRequest,
    StatusPublicationRequest, TeamPatchRequest, WeeklyAvailability as SelfServiceAvailability,
};
use crate::model::{
    AnnouncementPreview, AvailabilitySlot, AvailabilityStatus, Coach, LagebildEvidenceRef,
    LagebildSnapshotRef, LobbyStateMutation, MatchMutation, MatchRequest, MatchRequestBatch,
    MatchRequestResponse, MatchRequestTemplate, Participant, ReplacementCandidate, ResponseChoice,
    RosterMember, ScrimAction, ScrimMatch, ScrimReadModel, ScrimSlot, SelectedMatchResult, Team,
    TeamMember, TeamRef, ValidatedMatchRequestBatch, WeeklyAvailability,
};
use crate::{ScrimError, ScrimResult};

const MAIN_GUILD_ID: &str = "1289721245281292288";
const ACTIVE_REQUEST_STATUSES: &[&str] = &["draft", "posting", "open", "post_failed"];
const TEAM_LOCK_NAMESPACE: i32 = 20260725;
const ID_LOCK_NAMESPACE: i32 = 20260726;
const RUNTIME_LOCK_NAMESPACE: i32 = 724060001;
const RUNTIME_LOCK_KEY: i32 = 724060002;
const COMMAND_LEASE_OWNER: &str = "turniere:api";
const SELF_SERVICE_ADVISORY_LOCK: i64 = 0x4451_0008_0004_0001;

#[async_trait]
pub trait ScrimReadRepository: Send + Sync {
    async fn read_model(&self) -> ScrimResult<ScrimReadModel>;
    async fn coaches(&self) -> ScrimResult<Vec<Coach>>;
    async fn is_active_coach(&self, discord_id: i64) -> ScrimResult<bool>;
    async fn existing_team_ids(&self, team_ids: &BTreeSet<i32>) -> ScrimResult<BTreeSet<i32>>;
    async fn active_request_team_ids(&self, team_ids: &BTreeSet<i32>)
        -> ScrimResult<BTreeSet<i32>>;
}

#[derive(Debug, Clone)]
pub struct PgScrimReadRepository {
    pool: Pool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignupMutation {
    pub participant: SelfServiceParticipant,
    pub discord_user_id: Option<u64>,
    pub role_ids: BTreeSet<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleOperation {
    Add,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordRoleAction {
    pub operation: RoleOperation,
    pub role_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordRoleSyncPlan {
    pub subject: String,
    pub discord_user_id: Option<u64>,
    pub actions: Vec<DiscordRoleAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMutation {
    pub team: RosterTeam,
    pub sync_plans: Vec<DiscordRoleSyncPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantMutation {
    pub participant: RosterParticipant,
    pub sync_plan: DiscordRoleSyncPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubstituteMutation {
    pub participant: RosterParticipant,
    pub sync_plan: DiscordRoleSyncPlan,
    pub discord_user_id: Option<u64>,
    pub team_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterPoolCandidate {
    pub participant_id: i32,
    pub discord_id: Option<i64>,
    pub display_name: String,
    pub rank: Option<String>,
    pub roles: Option<String>,
    pub availability: Option<String>,
    pub availability_slots: SelfServiceAvailability,
    pub availability_confirmed: bool,
    pub status: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordDispatch {
    pub record_id: i64,
    pub user_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub content: String,
}

/// Wird vollstaendig im Command-Receipt abgelegt, damit ein Replay auch die offenen
/// Discord-Zustellungen erneut liefert. Der Broker dedupliziert ueber den
/// Idempotenzschluessel, ein zweiter Versuch kann also nichts doppelt zustellen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationDispatch {
    pub receipt: ActionReceipt,
    pub discord: Vec<DiscordDispatch>,
}

impl PgScrimReadRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn runtime_control(&self) -> ScrimResult<RuntimeControl> {
        load_runtime_control(&self.pool).await
    }

    pub async fn roster_team(&self, team_id: i32) -> ScrimResult<RosterTeam> {
        let mut tx = self.pool.begin().await?;
        load_roster_team(&mut tx, team_id).await
    }

    pub async fn create_match(
        &self,
        idempotency_key: &str,
        team_a_id: i32,
        team_b_id: i32,
        scheduled_at: Option<DateTime<Utc>>,
        coach_spectator_discord_id: Option<i64>,
    ) -> ScrimResult<MatchMutation> {
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let payload = json!({
            "team_a_id": team_a_id.to_string(),
            "team_b_id": team_b_id.to_string(),
            "scheduled_at": scheduled_at,
            "coach_spectator_discord_id": coach_spectator_discord_id.map(|id| id.to_string()),
        });
        let receipt_id =
            match begin_command(&mut tx, "match_create", idempotency_key, &payload).await? {
                CommandStart::New(id) => id,
                CommandStart::Replay(response) => return Ok(response),
            };
        lock_id_generation(&mut tx).await?;
        let team_ids = BTreeSet::from([team_a_id, team_b_id]);
        if existing_team_ids_tx(&mut tx, &team_ids).await? != team_ids {
            return Err(ScrimError::InvalidProposal(
                "At least one team was not found".to_string(),
            ));
        }
        let id = next_id(&mut tx, "scrim.matches").await?;
        sqlx::query(
            "INSERT INTO scrim.matches(\
                 id, team_a_id, team_b_id, scheduled_at, status, lobby_state, \
                 coach_spectator_discord_id, created_at, updated_at\
             ) VALUES ($1, $2, $3, $4, 'scheduled', 'draft', $5, now(), now())",
        )
        .bind(id)
        .bind(team_a_id)
        .bind(team_b_id)
        .bind(scheduled_at)
        .bind(coach_spectator_discord_id)
        .execute(&mut *tx)
        .await?;
        let response = MatchMutation {
            scrim_match: load_match_tx(&mut tx, id).await?,
        };
        complete_command(&mut tx, receipt_id, &response).await?;
        tx.commit().await?;
        Ok(response)
    }

    pub async fn set_lobby_code(
        &self,
        idempotency_key: &str,
        match_id: i32,
        code: &str,
        actor_user_id: &str,
        actor_display_name: &str,
    ) -> ScrimResult<MatchMutation> {
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let payload = json!({"match_id": match_id.to_string(), "lobby_code": code});
        let receipt_id =
            match begin_command(&mut tx, "match_lobby_code", idempotency_key, &payload).await? {
                CommandStart::New(id) => id,
                CommandStart::Replay(response) => return Ok(response),
            };
        let row =
            sqlx::query("SELECT lobby_state, join_code FROM scrim.matches WHERE id=$1 FOR UPDATE")
                .bind(match_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| ScrimError::NotFound("Match not found".to_string()))?;
        let lobby_state = row.try_get::<Option<String>, _>("lobby_state")?;
        if lobby_state.as_deref().is_some_and(blocks_lobby_code_write) {
            return Err(ScrimError::Conflict(format!(
                "Lobby state is controlled by bot: {}",
                lobby_state.unwrap_or_default()
            )));
        }
        let previous_code = row.try_get::<Option<String>, _>("join_code")?;
        let corrections = previous_code
            .as_deref()
            .filter(|previous| *previous != code)
            .map(|previous| {
                json!([{
                    "from": previous,
                    "to": code,
                    "source_user_id": actor_user_id,
                    "source_display_name": actor_display_name,
                    "at": Utc::now().timestamp(),
                }])
            })
            .unwrap_or_else(|| json!([]));
        sqlx::query(
            "UPDATE scrim.matches \
                SET join_code=$2, lobby_state='lobby_open', lobby_code_source_user_id=$3, \
                    lobby_code_source_display_name=$4, lobby_code_updated_at=now(), \
                    lobby_code_corrections=COALESCE(lobby_code_corrections, '[]'::jsonb) || $5::jsonb, \
                    updated_at=now() \
              WHERE id=$1",
        )
        .bind(match_id)
        .bind(code)
        .bind(actor_user_id)
        .bind(actor_display_name)
        .bind(corrections)
        .execute(&mut *tx)
        .await?;
        let response = MatchMutation {
            scrim_match: load_match_tx(&mut tx, match_id).await?,
        };
        complete_command(&mut tx, receipt_id, &response).await?;
        tx.commit().await?;
        Ok(response)
    }

    pub async fn add_match_ids(
        &self,
        idempotency_key: &str,
        match_id: i32,
        steam_match_ids: &[i64],
        actor_user_id: &str,
        actor_display_name: &str,
    ) -> ScrimResult<MatchMutation> {
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let payload = json!({
            "match_id": match_id.to_string(),
            "match_ids": steam_match_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
        });
        let receipt_id =
            match begin_command(&mut tx, "match_ids_add", idempotency_key, &payload).await? {
                CommandStart::New(id) => id,
                CommandStart::Replay(response) => return Ok(response),
            };
        if sqlx::query_scalar::<_, i32>("SELECT id FROM scrim.matches WHERE id=$1 FOR UPDATE")
            .bind(match_id)
            .fetch_optional(&mut *tx)
            .await?
            .is_none()
        {
            return Err(ScrimError::NotFound("Match not found".to_string()));
        }
        for steam_match_id in steam_match_ids {
            let inserted = sqlx::query_scalar::<_, i64>(
                "INSERT INTO scrim.match_result_refs(\
                     match_id, steam_match_id, source_user_id, source_display_name, \
                     fetch_status, entered_at, updated_at\
                 ) VALUES ($1, $2, $3, $4, 'pending', now(), now()) \
                 ON CONFLICT(steam_match_id) DO NOTHING RETURNING id",
            )
            .bind(match_id)
            .bind(steam_match_id)
            .bind(actor_user_id)
            .bind(actor_display_name)
            .fetch_optional(&mut *tx)
            .await?;
            if inserted.is_none() {
                return Err(ScrimError::Conflict("Match ID already exists".to_string()));
            }
        }
        sqlx::query(
            "UPDATE scrim.matches SET lobby_state='result_requested', updated_at=now() WHERE id=$1",
        )
        .bind(match_id)
        .execute(&mut *tx)
        .await?;
        let response = MatchMutation {
            scrim_match: load_match_tx(&mut tx, match_id).await?,
        };
        complete_command(&mut tx, receipt_id, &response).await?;
        tx.commit().await?;
        Ok(response)
    }

    pub async fn request_result_fetch(
        &self,
        idempotency_key: &str,
        match_id: i32,
        result_ref_id: Option<i64>,
    ) -> ScrimResult<LobbyStateMutation> {
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let payload = json!({
            "match_id": match_id.to_string(),
            "result_ref_id": result_ref_id.map(|id| id.to_string()),
        });
        let receipt_id =
            match begin_command(&mut tx, "match_result_fetch", idempotency_key, &payload).await? {
                CommandStart::New(id) => id,
                CommandStart::Replay(response) => return Ok(response),
            };
        let lobby_state = sqlx::query_scalar::<_, Option<String>>(
            "SELECT lobby_state FROM scrim.matches WHERE id=$1 FOR UPDATE",
        )
        .bind(match_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ScrimError::NotFound("Match not found".to_string()))?;
        if lobby_state
            .as_deref()
            .is_some_and(|state| state_blocks_lobby_request(state, "result_requested"))
        {
            return Err(ScrimError::Conflict(format!(
                "Lobby state is controlled by bot: {}",
                lobby_state.unwrap_or_default()
            )));
        }
        if let Some(result_ref_id) = result_ref_id {
            let updated = sqlx::query(
                "UPDATE scrim.match_result_refs \
                    SET fetch_status='pending', last_error=NULL, updated_at=now() \
                  WHERE id=$1 AND match_id=$2",
            )
            .bind(result_ref_id)
            .bind(match_id)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(ScrimError::NotFound(
                    "Result reference not found".to_string(),
                ));
            }
        }
        sqlx::query(
            "UPDATE scrim.matches SET lobby_state='result_requested', updated_at=now() WHERE id=$1",
        )
        .bind(match_id)
        .execute(&mut *tx)
        .await?;
        let response = LobbyStateMutation {
            match_id,
            lobby_state: "result_requested".to_string(),
        };
        complete_command(&mut tx, receipt_id, &response).await?;
        tx.commit().await?;
        Ok(response)
    }

    pub async fn select_result_ref(
        &self,
        idempotency_key: &str,
        match_id: i32,
        result_ref_id: i64,
        actor_user_id: &str,
        actor_display_name: &str,
        selection_reason: &str,
    ) -> ScrimResult<MatchMutation> {
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let payload = json!({
            "match_id": match_id.to_string(),
            "result_ref_id": result_ref_id.to_string(),
            "selection_reason": selection_reason,
        });
        let receipt_id =
            match begin_command(&mut tx, "match_result_select", idempotency_key, &payload).await? {
                CommandStart::New(id) => id,
                CommandStart::Replay(response) => return Ok(response),
            };
        let selectable = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(\
                 SELECT 1 FROM scrim.match_result_refs \
                  WHERE id=$1 AND match_id=$2 AND fetch_status='fetched' \
                    AND validation_status='valid' AND winner_team_id IS NOT NULL \
                    AND voided_at IS NULL AND superseded_by_ref_id IS NULL\
             )",
        )
        .bind(result_ref_id)
        .bind(match_id)
        .fetch_one(&mut *tx)
        .await?;
        if !selectable {
            return Err(ScrimError::InvalidProposal(
                "Result reference is not selectable".to_string(),
            ));
        }
        sqlx::query(
            "INSERT INTO scrim.match_result_selections(\
                 match_id, result_ref_id, selected_by_user_id, selected_by_display_name, \
                 selection_reason, selected_at\
             ) VALUES ($1, $2, $3, $4, $5, now()) \
             ON CONFLICT(match_id) DO UPDATE SET \
                 result_ref_id=EXCLUDED.result_ref_id, \
                 selected_by_user_id=EXCLUDED.selected_by_user_id, \
                 selected_by_display_name=EXCLUDED.selected_by_display_name, \
                 selection_reason=EXCLUDED.selection_reason, selected_at=now()",
        )
        .bind(match_id)
        .bind(result_ref_id)
        .bind(actor_user_id)
        .bind(actor_display_name)
        .bind(selection_reason)
        .execute(&mut *tx)
        .await?;
        let response = MatchMutation {
            scrim_match: load_match_tx(&mut tx, match_id).await?,
        };
        complete_command(&mut tx, receipt_id, &response).await?;
        tx.commit().await?;
        Ok(response)
    }

    pub async fn announcement_preview(&self, block_id: &str) -> ScrimResult<AnnouncementPreview> {
        let block_key = announcement_block_key(block_id);
        let row = sqlx::query(
            "SELECT id, title, body, payload->>'channel_id' AS channel_id, status, published_at \
               FROM scrim.announcement_drafts \
              WHERE scope='block' AND block_key=$1 \
              ORDER BY created_at DESC, id DESC LIMIT 1",
        )
        .bind(block_key)
        .fetch_optional(&self.pool)
        .await?;
        Ok(match row {
            Some(row) => announcement_from_row(block_id, &row)?,
            None => AnnouncementPreview {
                id: None,
                block_id: block_id.to_string(),
                title: "Noch keine Ankündigung".to_string(),
                message: "Für diesen Block gibt es noch keinen Ankündigungstext.".to_string(),
                channel_id: None,
                status: "preview".to_string(),
                published_at: None,
            },
        })
    }

    pub async fn create_announcement_publication(
        &self,
        block_id: &str,
        idempotency_key: &str,
        actor_user_id: &str,
        actor_display_name: &str,
        request: &AnnouncementPublicationRequest,
    ) -> ScrimResult<AnnouncementPreview> {
        let title = request.title.as_deref().unwrap_or("Scrim-Ankündigung");
        let payload = json!({"channel_id": request.channel_id});
        let block_key = announcement_block_key(block_id);
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        if let Some(row) = sqlx::query(
            "SELECT id, block_key, title, body, payload->>'channel_id' AS channel_id, \
                    status, published_at \
               FROM scrim.announcement_drafts \
              WHERE idempotency_key=$1 AND idempotency_generation=0 \
              FOR UPDATE",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let existing_block = row.try_get::<Option<String>, _>("block_key")?;
            let existing_title = row.try_get::<String, _>("title")?;
            let existing_body = row.try_get::<String, _>("body")?;
            let existing_channel = row.try_get::<Option<String>, _>("channel_id")?;
            if existing_block.as_deref() != Some(block_key.as_str())
                || existing_title != title
                || existing_body != request.message
                || existing_channel != request.channel_id
            {
                return Err(ScrimError::IdempotencyConflict);
            }
            return announcement_from_row(block_id, &row);
        }
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO scrim.announcement_drafts(\
                 scope, block_key, title, body, payload, payload_hash, status, \
                 idempotency_key, created_by_user_id, created_by_display_name, \
                 approved_by_user_id, approved_by_display_name, approved_at\
             ) VALUES (\
                 'block', $1, $2, $3, $4::jsonb, \
                 scrim.announcement_effect_hash('block', $1, $2, $3, $4::jsonb), \
                 'approved', $5, $6, $7, $6, $7, now()\
             ) RETURNING id",
        )
        .bind(block_key)
        .bind(title)
        .bind(&request.message)
        .bind(&payload)
        .bind(idempotency_key)
        .bind(actor_user_id)
        .bind(actor_display_name)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO scrim.announcement_approvals(\
                 draft_id, decision, decided_by_user_id, decided_by_display_name, decision_data\
             ) VALUES ($1, 'approved', $2, $3, '{}'::jsonb)",
        )
        .bind(id)
        .bind(actor_user_id)
        .bind(actor_display_name)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.announcement_preview(block_id).await
    }

    pub async fn mark_announcement_published(
        &self,
        announcement_id: i64,
        remote_message_id: Option<&str>,
    ) -> ScrimResult<()> {
        sqlx::query(
            "UPDATE scrim.announcement_drafts \
                SET status='published', published_at=now(), remote_system='discord', \
                    remote_message_id=$2, updated_at=now() \
              WHERE id=$1 AND status='approved'",
        )
        .bind(announcement_id)
        .bind(remote_message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn action(&self, id: i64) -> ScrimResult<ScrimAction> {
        let row = sqlx::query(
            "SELECT id, command_scope, state, attempts, next_attempt_at, remote_system, \
                    remote_message_id, remote_task_id, result_payload, last_error_code, \
                    received_at, updated_at, completed_at \
               FROM scrim.command_receipts WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ScrimError::NotFound("Action not found".to_string()))?;
        Ok(ScrimAction {
            id: row.try_get("id")?,
            command_scope: row.try_get("command_scope")?,
            state: row.try_get("state")?,
            attempts: row.try_get("attempts")?,
            next_attempt_at: row.try_get("next_attempt_at")?,
            remote_system: row.try_get("remote_system")?,
            remote_message_id: row.try_get("remote_message_id")?,
            remote_task_id: row.try_get("remote_task_id")?,
            result: row.try_get("result_payload")?,
            last_error_code: row.try_get("last_error_code")?,
            received_at: row.try_get("received_at")?,
            updated_at: row.try_get("updated_at")?,
            completed_at: row.try_get("completed_at")?,
        })
    }

    pub async fn signup(
        &self,
        discord_id: i64,
        display_name: &str,
        request: &SignupRequest,
        signup_role_id: Option<u64>,
        reserve_role_id: Option<u64>,
    ) -> ScrimResult<SignupMutation> {
        let availability_slots = request
            .availability_slots
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| {
                ScrimError::InvalidProposal(
                    "Deine Verfügbarkeit ließ sich nicht speichern. Bitte trag die Zeiten erneut ein.".to_string(),
                )
            })?;
        let mut tx = self.pool.begin().await?;

        // Gleicher Advisory-Key wie der Live-Reaktions-Hook (dl-community/reaction_roles.rs:475) — serialisiert Web-Signup gegen Discord-Reaktion. store.rs nutzt abweichend 42060004001 (Reconcile = separater Bot-Task).
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SELF_SERVICE_ADVISORY_LOCK)
            .execute(&mut *tx)
            .await?;

        let participant_id: Option<i32> = sqlx::query_scalar(
            "SELECT id FROM scrim.participants \
              WHERE discord_id=$1 ORDER BY id ASC LIMIT 1",
        )
        .bind(discord_id)
        .fetch_optional(&mut *tx)
        .await?;
        let participant_id = if let Some(participant_id) = participant_id {
            sqlx::query(
                // rank_source/rank_verified beziehen sich auf den ALTEN rank-Wert: in Postgres
                // lesen alle SET-Ausdruecke die Zeile vor dem Update. Ein selbst gemeldeter Rang
                // darf nie als verifiziert stehen bleiben; bleibt der Rang gleich, bleibt eine
                // bestehende Bestaetigung erhalten.
                "UPDATE scrim.participants \
                    SET display_name=$2, rank=$3, roles=$4, availability=$5, \
                        rank_source=CASE WHEN rank IS DISTINCT FROM $3 THEN 'self' ELSE rank_source END, \
                        rank_verified=CASE WHEN rank IS DISTINCT FROM $3 THEN false ELSE rank_verified END, \
                        availability_slots=COALESCE($6::jsonb, availability_slots), \
                        updated_at=now() \
                  WHERE id=$1",
            )
            .bind(participant_id)
            .bind(display_name)
            .bind(request.rank.as_deref())
            .bind(request.roles.as_deref())
            .bind(request.availability.as_deref())
            .bind(availability_slots.clone())
            .execute(&mut *tx)
            .await?;
            participant_id
        } else if let Some(participant_id) = sqlx::query_scalar(
            "SELECT id FROM scrim.participants \
              WHERE display_name=$1 AND discord_id IS NULL \
              ORDER BY id ASC LIMIT 1",
        )
        .bind(display_name)
        .fetch_optional(&mut *tx)
        .await?
        {
            sqlx::query(
                // Siehe oben: selbst gemeldeter Rang verliert eine bestehende Bestaetigung.
                "UPDATE scrim.participants \
                    SET discord_id=$2, rank=$3, roles=$4, availability=$5, \
                        rank_source=CASE WHEN rank IS DISTINCT FROM $3 THEN 'self' ELSE rank_source END, \
                        rank_verified=CASE WHEN rank IS DISTINCT FROM $3 THEN false ELSE rank_verified END, \
                        availability_slots=COALESCE($6::jsonb, availability_slots), \
                        updated_at=now() \
                  WHERE id=$1",
            )
            .bind(participant_id)
            .bind(discord_id)
            .bind(request.rank.as_deref())
            .bind(request.roles.as_deref())
            .bind(request.availability.as_deref())
            .bind(availability_slots.clone())
            .execute(&mut *tx)
            .await?;
            participant_id
        } else {
            sqlx::query_scalar(
                "INSERT INTO scrim.participants(\
                     id, discord_id, display_name, rank, rank_source, rank_verified, roles, \
                     availability, availability_slots, status, source, created_at, updated_at\
                 ) VALUES (\
                     (SELECT COALESCE(MAX(id), 0) + 1 FROM scrim.participants), \
                     $1, $2, $3, 'self', false, $4, $5, $6::jsonb, \
                     'new', 'web_form', now(), now()\
                 ) RETURNING id",
            )
            .bind(discord_id)
            .bind(display_name)
            .bind(request.rank.as_deref())
            .bind(request.roles.as_deref())
            .bind(request.availability.as_deref())
            .bind(availability_slots)
            .fetch_one(&mut *tx)
            .await?
        };

        let participant = load_self_service_participant(&mut tx, participant_id).await?;
        let team_role_ids = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT t.discord_role_id \
               FROM scrim.team_members tm \
               JOIN scrim.teams t ON t.id=tm.team_id \
              WHERE tm.participant_id=$1 \
              ORDER BY t.discord_role_id ASC NULLS LAST",
        )
        .bind(participant_id)
        .fetch_all(&mut *tx)
        .await?;
        let role_ids = managed_role_ids(
            &participant.status,
            signup_role_id,
            reserve_role_id,
            team_role_ids,
        );
        tx.commit().await?;
        Ok(SignupMutation {
            participant,
            discord_user_id: u64::try_from(discord_id).ok(),
            role_ids,
        })
    }

    pub async fn update_availability(
        &self,
        discord_id: i64,
        availability: &SelfServiceAvailability,
        legacy_availability: &str,
    ) -> ScrimResult<SelfServiceParticipant> {
        let availability_slots = serde_json::to_value(availability).map_err(|_| {
            ScrimError::InvalidProposal(
                "Deine Verfügbarkeit ließ sich nicht speichern. Bitte trag die Zeiten erneut ein."
                    .to_string(),
            )
        })?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SELF_SERVICE_ADVISORY_LOCK)
            .execute(&mut *tx)
            .await?;
        let participant_id: Option<i32> = sqlx::query_scalar(
            "SELECT id FROM scrim.participants \
              WHERE discord_id=$1 ORDER BY id ASC LIMIT 1",
        )
        .bind(discord_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(participant_id) = participant_id else {
            return Err(ScrimError::NotFound(
                "Du bist noch nicht im Scrim-Pool. Melde dich zuerst an, dann kannst du deine Verfügbarkeit pflegen.".to_string(),
            ));
        };
        sqlx::query(
            "UPDATE scrim.participants \
                SET availability_slots=$2::jsonb, availability=$3, updated_at=now() \
              WHERE id=$1",
        )
        .bind(participant_id)
        .bind(availability_slots)
        .bind(legacy_availability)
        .execute(&mut *tx)
        .await?;
        let participant = load_self_service_participant(&mut tx, participant_id).await?;
        tx.commit().await?;
        Ok(participant)
    }

    pub async fn create_team(
        &self,
        idempotency_key: &str,
        request: &CreateTeamRequest,
    ) -> ScrimResult<RosterTeam> {
        let coach_discord_id = request
            .coach_discord_id
            .as_deref()
            .map(parse_positive_i64)
            .transpose()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SELF_SERVICE_ADVISORY_LOCK)
            .execute(&mut *tx)
            .await?;
        let payload = serde_json::to_value(request).map_err(|error| {
            ScrimError::InvalidStoredData(format!("team serialization failed: {error}"))
        })?;
        let receipt_id =
            match begin_command(&mut tx, "roster_team_create", idempotency_key, &payload).await? {
                CommandStart::New(id) => id,
                CommandStart::Replay(team) => {
                    tx.commit().await?;
                    return Ok(team);
                }
            };
        let coach = resolve_coach(
            &mut tx,
            coach_discord_id,
            request.coach.as_deref().and_then(trimmed_nonempty),
        )
        .await?;
        let id: i32 =
            sqlx::query_scalar("SELECT (COALESCE(MAX(id), 0) + 1)::int4 FROM scrim.teams")
                .fetch_one(&mut *tx)
                .await?;
        sqlx::query(
            "INSERT INTO scrim.teams(\
                 id, name, coach, coach_discord_id, default_from, default_to, created_at\
             ) VALUES($1, $2, $3, $4, $5, $6, now())",
        )
        .bind(id)
        .bind(request.name.trim())
        .bind(coach)
        .bind(coach_discord_id)
        .bind(request.default_from)
        .bind(request.default_to)
        .execute(&mut *tx)
        .await?;
        let team = load_roster_team(&mut tx, id).await?;
        complete_command(&mut tx, receipt_id, &team).await?;
        tx.commit().await?;
        Ok(team)
    }

    pub async fn set_team_discord_role(
        &self,
        team_id: i32,
        discord_role_id: Option<i64>,
    ) -> ScrimResult<TeamMutation> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SELF_SERVICE_ADVISORY_LOCK)
            .execute(&mut *tx)
            .await?;
        let team = load_roster_team(&mut tx, team_id).await?;
        let coach_discord_id = team
            .coach_discord_id
            .as_deref()
            .map(parse_positive_i64)
            .transpose()?;
        let before = match coach_discord_id {
            Some(id) => vec![coach_role_snapshot(&mut tx, id).await?],
            None => Vec::new(),
        };
        sqlx::query("UPDATE scrim.teams SET discord_role_id=$2 WHERE id=$1")
            .bind(team_id)
            .bind(discord_role_id)
            .execute(&mut *tx)
            .await?;
        let team = load_roster_team(&mut tx, team_id).await?;
        let sync_plans = coach_sync_plans(&mut tx, before).await?;
        tx.commit().await?;
        Ok(TeamMutation { team, sync_plans })
    }

    pub async fn patch_team(
        &self,
        team_id: i32,
        request: TeamPatchRequest,
    ) -> ScrimResult<TeamMutation> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SELF_SERVICE_ADVISORY_LOCK)
            .execute(&mut *tx)
            .await?;
        let team = load_roster_team(&mut tx, team_id).await?;
        let old_coach_id = team
            .coach_discord_id
            .as_deref()
            .map(parse_positive_i64)
            .transpose()?;
        let requested_coach_id = match &request.coach_discord_id {
            PatchValue::Omitted => None,
            PatchValue::Null => Some(None),
            PatchValue::Value(value) => Some(Some(parse_positive_i64(value)?)),
        };
        let coach_id = requested_coach_id.unwrap_or(old_coach_id);
        let coach = match (requested_coach_id, request.coach) {
            (Some(Some(coach_id)), _) => resolve_coach(&mut tx, Some(coach_id), None).await?,
            (Some(None), _) => team.coach.clone(),
            (None, PatchValue::Value(value)) => trimmed_nonempty(&value).map(str::to_string),
            (None, PatchValue::Null) => None,
            (None, PatchValue::Omitted) => team.coach.clone(),
        };
        let name = match request.name {
            PatchValue::Value(value) => value.trim().to_string(),
            PatchValue::Omitted | PatchValue::Null => team.name.clone(),
        };
        let default_from = patch_option(request.default_from, team.default_from);
        let default_to = patch_option(request.default_to, team.default_to);
        crate::service::validate_team_window(default_from, default_to)?;
        let mut affected = [old_coach_id, coach_id]
            .into_iter()
            .flatten()
            .collect::<BTreeSet<_>>();
        if requested_coach_id.is_none() {
            affected.clear();
        }
        let mut before = Vec::new();
        for coach_id in affected {
            before.push(coach_role_snapshot(&mut tx, coach_id).await?);
        }
        sqlx::query(
            "UPDATE scrim.teams \
             SET name=$2, coach=$3, coach_discord_id=$4, default_from=$5, default_to=$6 \
             WHERE id=$1",
        )
        .bind(team_id)
        .bind(name)
        .bind(coach)
        .bind(coach_id)
        .bind(default_from)
        .bind(default_to)
        .execute(&mut *tx)
        .await?;
        let team = load_roster_team(&mut tx, team_id).await?;
        let sync_plans = coach_sync_plans(&mut tx, before).await?;
        tx.commit().await?;
        Ok(TeamMutation { team, sync_plans })
    }

    pub async fn patch_participant(
        &self,
        participant_id: i32,
        request: ParticipantPatchRequest,
        reserve_role_id: Option<u64>,
        signup_role_id: Option<u64>,
    ) -> ScrimResult<ParticipantMutation> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SELF_SERVICE_ADVISORY_LOCK)
            .execute(&mut *tx)
            .await?;
        let before =
            participant_role_snapshot(&mut tx, participant_id, reserve_role_id, signup_role_id)
                .await?;
        let rank = patch_text_value(request.rank);
        let roles = patch_text_value(request.roles);
        let notes = patch_text_value(request.notes);
        if rank.is_some() || roles.is_some() || notes.is_some() {
            sqlx::query(
                "UPDATE scrim.participants \
                 SET rank=COALESCE($2, rank), roles=COALESCE($3, roles), \
                     notes=COALESCE($4, notes), updated_at=now() \
                 WHERE id=$1",
            )
            .bind(participant_id)
            .bind(rank)
            .bind(roles)
            .bind(notes)
            .execute(&mut *tx)
            .await?;
        }
        if let PatchValue::Value(status) = request.status {
            sqlx::query("UPDATE scrim.participants SET status=$2, updated_at=now() WHERE id=$1")
                .bind(participant_id)
                .bind(&status)
                .execute(&mut *tx)
                .await?;
            if status.trim().eq_ignore_ascii_case("assigned") {
                sqlx::query(
                    "UPDATE scrim.team_members SET substitute_until=NULL WHERE participant_id=$1",
                )
                .bind(participant_id)
                .execute(&mut *tx)
                .await?;
            }
        }
        match request.team_id {
            PatchValue::Value(team_id) => {
                load_roster_team(&mut tx, team_id).await?;
                sqlx::query(
                    "DELETE FROM scrim.team_members WHERE participant_id=$1 AND team_id<>$2",
                )
                .bind(participant_id)
                .bind(team_id)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO scrim.team_members(\
                         team_id, participant_id, role, is_captain, is_bench\
                     ) VALUES($1, $2, NULL, COALESCE($3, false), COALESCE($4, false)) \
                     ON CONFLICT (team_id, participant_id) DO UPDATE SET \
                         is_captain=COALESCE($3, scrim.team_members.is_captain), \
                         is_bench=COALESCE($4, scrim.team_members.is_bench)",
                )
                .bind(team_id)
                .bind(participant_id)
                .bind(patch_bool(request.is_captain))
                .bind(patch_bool(request.is_bench))
                .execute(&mut *tx)
                .await?;
            }
            PatchValue::Null => {
                sqlx::query("DELETE FROM scrim.team_members WHERE participant_id=$1")
                    .bind(participant_id)
                    .execute(&mut *tx)
                    .await?;
            }
            PatchValue::Omitted => {
                if !request.is_captain.is_omitted() || !request.is_bench.is_omitted() {
                    sqlx::query(
                        "UPDATE scrim.team_members \
                         SET is_captain=COALESCE($2, is_captain), \
                             is_bench=COALESCE($3, is_bench) \
                         WHERE participant_id=$1",
                    )
                    .bind(participant_id)
                    .bind(patch_bool(request.is_captain))
                    .bind(patch_bool(request.is_bench))
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        let participant = load_roster_participant(&mut tx, participant_id).await?;
        let after =
            participant_role_snapshot(&mut tx, participant_id, reserve_role_id, signup_role_id)
                .await?;
        let sync_plan = role_diff(&before, &after);
        tx.commit().await?;
        Ok(ParticipantMutation {
            participant,
            sync_plan,
        })
    }

    pub async fn substitute(
        &self,
        idempotency_key: &str,
        payload: &Value,
        team_id: i32,
        participant_id: i32,
        reserve_role_id: Option<u64>,
        signup_role_id: Option<u64>,
    ) -> ScrimResult<SubstituteMutation> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SELF_SERVICE_ADVISORY_LOCK)
            .execute(&mut *tx)
            .await?;
        let receipt_id =
            match begin_command(&mut tx, "roster_team_substitute", idempotency_key, payload).await?
            {
                CommandStart::New(id) => id,
                CommandStart::Replay(mutation) => {
                    tx.commit().await?;
                    return Ok(mutation);
                }
            };
        let team = load_roster_team(&mut tx, team_id).await?;
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM scrim.participants WHERE id=$1")
                .bind(participant_id)
                .fetch_optional(&mut *tx)
                .await?;
        let status =
            status.ok_or_else(|| ScrimError::NotFound("Spieler nicht gefunden.".to_string()))?;
        if !status.trim().eq_ignore_ascii_case("reserve") {
            return Err(ScrimError::InvalidProposal(
                "Nur Auswechselspieler können als Aushilfe einspringen.".to_string(),
            ));
        }
        sqlx::query(
            "INSERT INTO scrim.team_members(\
                 team_id, participant_id, is_bench, is_captain, substitute_until\
             ) VALUES($1, $2, TRUE, FALSE, now() + interval '24 hours') \
             ON CONFLICT (team_id, participant_id) DO UPDATE SET \
                 is_bench=TRUE, substitute_until=now() + interval '24 hours'",
        )
        .bind(team_id)
        .bind(participant_id)
        .execute(&mut *tx)
        .await?;
        let participant = load_roster_participant(&mut tx, participant_id).await?;
        let snapshot =
            participant_role_snapshot(&mut tx, participant_id, reserve_role_id, signup_role_id)
                .await?;
        let discord_user_id = snapshot.discord_user_id;
        let managed = all_managed_role_ids(&mut tx, signup_role_id, reserve_role_id).await?;
        let sync_plan = role_resync(&snapshot, &managed);
        let mutation = SubstituteMutation {
            participant,
            sync_plan,
            discord_user_id,
            team_name: team.name,
        };
        complete_command(&mut tx, receipt_id, &mutation).await?;
        tx.commit().await?;
        Ok(mutation)
    }

    pub async fn participant_resync_plan(
        &self,
        participant_id: i32,
        reserve_role_id: Option<u64>,
        signup_role_id: Option<u64>,
    ) -> ScrimResult<DiscordRoleSyncPlan> {
        let mut tx = self.pool.begin().await?;
        let snapshot =
            participant_role_snapshot(&mut tx, participant_id, reserve_role_id, signup_role_id)
                .await?;
        let managed = all_managed_role_ids(&mut tx, signup_role_id, reserve_role_id).await?;
        Ok(role_resync(&snapshot, &managed))
    }

    pub async fn roster_suggestion_pool(
        &self,
        team_id: i32,
        reserve: bool,
    ) -> ScrimResult<(RosterTeam, Vec<RosterPoolCandidate>)> {
        let mut tx = self.pool.begin().await?;
        let team = load_roster_team(&mut tx, team_id).await?;
        let status_filter = if reserve { "reserve" } else { "players" };
        let rows = sqlx::query(
            "SELECT p.id, p.discord_id, p.display_name, p.rank, p.roles, p.availability, \
                    p.availability_slots, p.status, p.source \
             FROM scrim.participants p \
             WHERE (($1 = 'reserve' AND p.status = 'reserve') OR \
                    ($1 = 'players' AND p.status NOT IN ('inactive', 'reserve'))) \
               AND NOT EXISTS (\
                   SELECT 1 FROM scrim.team_members tm WHERE tm.participant_id=p.id\
               ) \
             ORDER BY p.created_at ASC, p.id ASC",
        )
        .bind(status_filter)
        .fetch_all(&mut *tx)
        .await?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let availability: Option<String> = row.try_get("availability")?;
            let slots: Option<Value> = row.try_get("availability_slots")?;
            let confirmed = slots.is_some();
            candidates.push(RosterPoolCandidate {
                participant_id: row.try_get("id")?,
                discord_id: row.try_get("discord_id")?,
                display_name: row.try_get("display_name")?,
                rank: row.try_get("rank")?,
                roles: row.try_get("roles")?,
                availability: availability.clone(),
                availability_slots: effective_self_service_availability(
                    slots,
                    availability.as_deref(),
                ),
                availability_confirmed: confirmed,
                status: row.try_get("status")?,
                source: row.try_get("source")?,
            });
        }
        Ok((team, candidates))
    }

    pub async fn patch_match_request(
        &self,
        idempotency_key: &str,
        api_request_id: &str,
        payload: &Value,
        request_id: i32,
        request: &MatchRequestPatch,
        actor: (&str, &str),
    ) -> ScrimResult<ActionReceipt> {
        let status = match &request.status {
            PatchValue::Omitted => None,
            PatchValue::Null => {
                return Err(ScrimError::InvalidProposal(
                    "Unbekannter Status für diese Abfrage.".to_string(),
                ));
            }
            PatchValue::Value(status)
                if matches!(
                    status.as_str(),
                    "draft" | "posting" | "open" | "post_failed" | "closed" | "cancelled"
                ) =>
            {
                Some(status.as_str())
            }
            PatchValue::Value(_) => {
                return Err(ScrimError::InvalidProposal(
                    "Ungültiger Slot: er gehört nicht zu dieser Abfrage.".to_string(),
                ));
            }
        };
        let note = match &request.note {
            PatchValue::Omitted => None,
            PatchValue::Null => Some(None),
            PatchValue::Value(note) if note.chars().count() <= 1_000 => Some(Some(note.as_str())),
            PatchValue::Value(_) => {
                return Err(ScrimError::InvalidProposal(
                    "Die Notiz ist zu lang — höchstens 1000 Zeichen.".to_string(),
                ));
            }
        };
        if status.is_none() && note.is_none() {
            return Err(ScrimError::InvalidProposal(
                "Es gibt nichts zu ändern: weder Status noch Notiz angegeben.".to_string(),
            ));
        }

        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let receipt_id =
            match begin_command(&mut tx, "match_request_patch", idempotency_key, payload).await? {
                CommandStart::New(id) => id,
                CommandStart::Replay(receipt) => {
                    tx.commit().await?;
                    return Ok(receipt);
                }
            };
        let exists = sqlx::query_scalar::<_, i32>(
            "SELECT id FROM scrim.match_requests WHERE id=$1 FOR UPDATE",
        )
        .bind(request_id)
        .fetch_optional(&mut *tx)
        .await?;
        if exists.is_none() {
            return Err(ScrimError::NotFound(
                "Diese Abfrage gibt es nicht.".to_string(),
            ));
        }
        sqlx::query(
            "UPDATE scrim.match_requests \
                SET status=COALESCE($2, status), \
                    override_reason=CASE WHEN $3 THEN $4 ELSE override_reason END, \
                    updated_at=now() \
              WHERE id=$1",
        )
        .bind(request_id)
        .bind(status)
        .bind(note.is_some())
        .bind(note.flatten())
        .execute(&mut *tx)
        .await?;
        insert_audit_event(
            &mut tx,
            "match_request_patched",
            ("match_request", i64::from(request_id)),
            actor.0,
            api_request_id,
            idempotency_key,
            json!({"status": status, "note_changed": note.is_some()}),
        )
        .await?;
        let receipt = placeholder_receipt();
        complete_command(&mut tx, receipt_id, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn create_match_request_reminders(
        &self,
        idempotency_key: &str,
        api_request_id: &str,
        payload: &Value,
        request_id: i32,
        request: &ReminderRequest,
        actor: (&str, &str),
    ) -> ScrimResult<MutationDispatch> {
        let content = validated_message(request.message.as_deref())?;
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let receipt_id =
            match begin_command(&mut tx, "match_request_reminders", idempotency_key, payload)
                .await?
            {
                CommandStart::New(id) => id,
                // Replay liefert die Discord-Zustellungen mit: ist der erste Versuch
                // nach dem Commit gescheitert, holt ein erneuter Aufruf ihn nach.
                CommandStart::Replay(dispatch) => {
                    tx.commit().await?;
                    return Ok(dispatch);
                }
            };
        let row = sqlx::query(
            "SELECT team_a_id, team_b_id, status, team_query_message_ids \
               FROM scrim.match_requests WHERE id=$1 FOR UPDATE",
        )
        .bind(request_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ScrimError::NotFound("Diese Abfrage gibt es nicht.".to_string()))?;
        let status = row.try_get::<String, _>("status")?;
        if !matches!(status.as_str(), "open" | "post_failed") {
            return Err(ScrimError::Conflict(
                "Erinnerungen gehen nur an offene Abfragen.".to_string(),
            ));
        }
        let message_ids = row.try_get::<Value, _>("team_query_message_ids")?;
        let team_ids = [
            Some(row.try_get::<i32, _>("team_a_id")?),
            row.try_get::<Option<i32>, _>("team_b_id")?,
        ];
        let mut discord = Vec::new();
        for team_id in team_ids.into_iter().flatten() {
            let team = sqlx::query(
                "SELECT discord_role_id, discord_channel_id FROM scrim.teams WHERE id=$1",
            )
            .bind(team_id)
            .fetch_one(&mut *tx)
            .await?;
            let source = team_query_message(&message_ids, team_id)?;
            let members = sqlx::query(
                "SELECT p.id, p.discord_id \
                   FROM scrim.team_members tm \
                   JOIN scrim.participants p ON p.id=tm.participant_id \
                  WHERE tm.team_id=$1 \
                    AND NOT EXISTS(\
                        SELECT 1 FROM scrim.match_request_responses r \
                         WHERE r.request_id=$2 AND r.team_id=$1 AND r.participant_id=p.id\
                    ) \
                  ORDER BY tm.is_bench ASC, p.display_name ASC, p.id ASC",
            )
            .bind(team_id)
            .bind(request_id)
            .fetch_all(&mut *tx)
            .await?;
            if members.is_empty() {
                continue;
            }
            let participant_ids = members
                .iter()
                .map(|member| member.try_get::<i32, _>("id"))
                .collect::<Result<Vec<_>, _>>()?;
            let discord_ids = members
                .iter()
                .map(|member| member.try_get::<Option<i64>, _>("discord_id"))
                .collect::<Result<Vec<_>, _>>()?;
            let all_have_discord = discord_ids.iter().all(Option::is_some);
            let target_discord_ids = if all_have_discord {
                discord_ids.iter().copied().flatten().collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let channel_id = team
                .try_get::<Option<i64>, _>("discord_channel_id")?
                .ok_or_else(|| {
                    ScrimError::InvalidStoredData("Dem Team fehlt der Discord-Kanal.".to_string())
                })?;
            if source.0 != channel_id {
                return Err(ScrimError::InvalidStoredData(
                    "Die Abfrage gehört zu einem anderen Kanal als das Team.".to_string(),
                ));
            }
            let role_id = team.try_get::<Option<i64>, _>("discord_role_id")?;
            let target_role_id = if all_have_discord {
                None
            } else {
                Some(role_id.ok_or_else(|| {
                    ScrimError::InvalidStoredData(
                        "Zur Abfrage ist keine Nachricht hinterlegt.".to_string(),
                    )
                })?)
            };
            let reminder_id: i64 = sqlx::query_scalar(
                "INSERT INTO scrim.match_request_reminders(\
                     request_id, team_id, template, target_kind, target_participant_ids, \
                     target_discord_user_ids, target_role_id, missing_count, \
                     approved_by_user_id, approved_by_display_name, status, \
                     discord_channel_id, source_message_id\
                 ) VALUES ($1, $2, 'antwort_fehlt', $3, $4, $5, $6, $7, $8, $9, \
                           'approved', $10, $11) RETURNING id",
            )
            .bind(request_id)
            .bind(team_id)
            .bind(if all_have_discord { "members" } else { "team" })
            .bind(&participant_ids)
            .bind(&target_discord_ids)
            .bind(target_role_id)
            .bind(i32::try_from(participant_ids.len()).map_err(|_| {
                ScrimError::InvalidStoredData(
                    "Die hinterlegten Nachrichten-IDs sind unbrauchbar.".to_string(),
                )
            })?)
            .bind(actor.0)
            .bind(actor.1)
            .bind(channel_id)
            .bind(source.1)
            .fetch_one(&mut *tx)
            .await?;
            if all_have_discord {
                discord.extend(
                    target_discord_ids
                        .into_iter()
                        .map(|user_id| DiscordDispatch {
                            record_id: reminder_id,
                            user_id: Some(user_id),
                            channel_id: None,
                            content: content.clone(),
                        }),
                );
            } else {
                discord.push(DiscordDispatch {
                    record_id: reminder_id,
                    user_id: None,
                    channel_id: Some(source.0),
                    content: content.clone(),
                });
            }
        }
        if discord.is_empty() {
            return Err(ScrimError::Conflict(
                "Niemand im Team hat einen verknüpften Discord-Account.".to_string(),
            ));
        }
        insert_audit_event(
            &mut tx,
            "match_request_reminders_approved",
            ("match_request", i64::from(request_id)),
            actor.0,
            api_request_id,
            idempotency_key,
            json!({"dispatch_count": discord.len()}),
        )
        .await?;
        let receipt = placeholder_receipt();
        // Den vollstaendigen Dispatch ablegen, nicht nur die Quittung: sonst geht bei einem
        // Replay verloren, welche Discord-Zustellungen noch offen sind.
        let dispatch = MutationDispatch { receipt, discord };
        complete_command(&mut tx, receipt_id, &dispatch).await?;
        tx.commit().await?;
        Ok(dispatch)
    }

    pub async fn create_status_publication(
        &self,
        idempotency_key: &str,
        api_request_id: &str,
        payload: &Value,
        request_id: i32,
        request: &StatusPublicationRequest,
        actor: (&str, &str),
    ) -> ScrimResult<MutationDispatch> {
        let content = validated_message(request.message.as_deref())?;
        let requested_channel = request
            .channel_id
            .as_deref()
            .map(crate::model::wire_id::parse_i64)
            .transpose()
            .map_err(ScrimError::InvalidProposal)?;
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let receipt_id = match begin_command(
            &mut tx,
            "match_request_status_publication",
            idempotency_key,
            payload,
        )
        .await?
        {
            CommandStart::New(id) => id,
            // Replay liefert die Discord-Zustellungen mit: ist der erste Versuch
            // nach dem Commit gescheitert, holt ein erneuter Aufruf ihn nach.
            CommandStart::Replay(dispatch) => {
                tx.commit().await?;
                return Ok(dispatch);
            }
        };
        let row = sqlx::query(
            "SELECT team_a_id, team_b_id FROM scrim.match_requests WHERE id=$1 FOR UPDATE",
        )
        .bind(request_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ScrimError::NotFound("Diese Abfrage gibt es nicht.".to_string()))?;
        let publication_payload = serde_json::to_value(request).map_err(|_| {
            ScrimError::InvalidProposal(
                "Die Statusmeldung ließ sich nicht verarbeiten.".to_string(),
            )
        })?;
        let publication_id: i64 = sqlx::query_scalar(
            "INSERT INTO scrim.status_publication_approvals(\
                 target_kind, target_id, status_kind, payload, payload_hash, decision, \
                 decided_by_user_id, decided_by_display_name, decided_at\
             ) VALUES (\
                 'match_request', $1, 'match_status', $2::jsonb, \
                 scrim.status_publication_effect_hash('match_request', $1, 'match_status', $2::jsonb), \
                 'approved', $3, $4, now()\
             ) RETURNING id",
        )
        .bind(request_id.to_string())
        .bind(publication_payload)
        .bind(actor.0)
        .bind(actor.1)
        .fetch_one(&mut *tx)
        .await?;
        let channels = if let Some(channel_id) = requested_channel {
            vec![channel_id]
        } else {
            let team_ids = [
                Some(row.try_get::<i32, _>("team_a_id")?),
                row.try_get::<Option<i32>, _>("team_b_id")?,
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT discord_channel_id FROM scrim.teams \
                  WHERE id=ANY($1) ORDER BY id",
            )
            .bind(team_ids)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .flatten()
            .collect()
        };
        if channels.is_empty() {
            return Err(ScrimError::InvalidStoredData(
                "Für diese Abfrage ist kein Kanal hinterlegt.".to_string(),
            ));
        }
        let discord = channels
            .into_iter()
            .map(|channel_id| DiscordDispatch {
                record_id: publication_id,
                user_id: None,
                channel_id: Some(channel_id),
                content: content.clone(),
            })
            .collect::<Vec<_>>();
        insert_audit_event(
            &mut tx,
            "match_request_status_publication_approved",
            ("match_request", i64::from(request_id)),
            actor.0,
            api_request_id,
            idempotency_key,
            json!({"publication_id": publication_id.to_string()}),
        )
        .await?;
        let receipt = placeholder_receipt();
        // Den vollstaendigen Dispatch ablegen, nicht nur die Quittung: sonst geht bei einem
        // Replay verloren, welche Discord-Zustellungen noch offen sind.
        let dispatch = MutationDispatch { receipt, discord };
        complete_command(&mut tx, receipt_id, &dispatch).await?;
        tx.commit().await?;
        Ok(dispatch)
    }

    pub async fn replacement_candidates(
        &self,
        need_id: i64,
    ) -> ScrimResult<Vec<ReplacementCandidate>> {
        let status = sqlx::query_scalar::<_, String>(
            "SELECT status FROM scrim.replacement_needs WHERE id=$1",
        )
        .bind(need_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            ScrimError::NotFound("Dieser Ersatzbedarf ist nicht bekannt.".to_string())
        })?;
        if !matches!(status.as_str(), "open" | "contacting") {
            return Err(ScrimError::Conflict(
                "Für diesen Bedarf wird kein Ersatz mehr gesucht.".to_string(),
            ));
        }
        let rows = sqlx::query(
            "SELECT c.id, c.need_id, c.participant_id, c.discord_user_id, \
                    p.display_name, p.rank, p.roles, p.availability, \
                    c.candidate_data, c.score_data, c.status \
               FROM scrim.replacement_candidates c \
               LEFT JOIN scrim.participants p ON p.id=c.participant_id \
              WHERE c.need_id=$1 \
                AND c.status NOT IN ('expired', 'rejected')",
        )
        .bind(need_id)
        .fetch_all(&self.pool)
        .await?;
        let mut candidates = rows
            .into_iter()
            .map(|row| {
                Ok(ReplacementCandidate {
                    id: row.try_get("id")?,
                    need_id: row.try_get("need_id")?,
                    participant_id: row.try_get("participant_id")?,
                    discord_user_id: row.try_get("discord_user_id")?,
                    display_name: row.try_get("display_name")?,
                    rank: row.try_get("rank")?,
                    roles: row.try_get("roles")?,
                    availability: row.try_get("availability")?,
                    candidate_data: row.try_get("candidate_data")?,
                    score_data: row.try_get("score_data")?,
                    status: row.try_get("status")?,
                })
            })
            .collect::<ScrimResult<Vec<_>>>()?;
        rank_replacement_candidates(&mut candidates);
        Ok(candidates)
    }

    pub async fn create_replacement_request(
        &self,
        idempotency_key: &str,
        api_request_id: &str,
        payload: &Value,
        need_id: i64,
        request: &ReplacementRequestCreate,
        actor: (&str, &str),
    ) -> ScrimResult<MutationDispatch> {
        let participant_id = crate::model::wire_id::parse_i32(&request.participant_id)
            .map_err(ScrimError::InvalidProposal)?;
        if request
            .reason
            .as_ref()
            .is_some_and(|reason| reason.chars().count() > 1_000)
        {
            return Err(ScrimError::InvalidProposal(
                "Es wurde kein Kandidat angegeben.".to_string(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let receipt_id = match begin_command(
            &mut tx,
            "replacement_request_create",
            idempotency_key,
            payload,
        )
        .await?
        {
            CommandStart::New(id) => id,
            // Replay liefert die Discord-Zustellungen mit: ist der erste Versuch
            // nach dem Commit gescheitert, holt ein erneuter Aufruf ihn nach.
            CommandStart::Replay(dispatch) => {
                tx.commit().await?;
                return Ok(dispatch);
            }
        };
        let need = sqlx::query(
            "SELECT match_id, team_id, status FROM scrim.replacement_needs \
              WHERE id=$1 FOR UPDATE",
        )
        .bind(need_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            ScrimError::NotFound("Dieser Ersatzbedarf ist nicht bekannt.".to_string())
        })?;
        if !matches!(
            need.try_get::<String, _>("status")?.as_str(),
            "open" | "contacting"
        ) {
            return Err(ScrimError::Conflict(
                "Für diesen Bedarf wird kein Ersatz mehr gesucht.".to_string(),
            ));
        }
        validate_optional_id(request.match_id.as_deref(), need.try_get("match_id")?)?;
        validate_optional_id(request.team_id.as_deref(), need.try_get("team_id")?)?;
        let candidate = sqlx::query(
            "SELECT id, discord_user_id FROM scrim.replacement_candidates \
              WHERE need_id=$1 AND participant_id=$2 \
                AND status IN ('candidate', 'shortlisted') \
              FOR UPDATE",
        )
        .bind(need_id)
        .bind(participant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            ScrimError::InvalidProposal(
                "Dieser Kandidat steht nicht auf der Liste für diesen Bedarf.".to_string(),
            )
        })?;
        let candidate_id = candidate.try_get::<i64, _>("id")?;
        let discord_user_id = candidate.try_get::<Option<i64>, _>("discord_user_id")?;
        let request_payload = serde_json::to_value(request).map_err(|_| {
            ScrimError::InvalidProposal("Die Anfrage ließ sich nicht verarbeiten.".to_string())
        })?;
        let replacement_request_id: i64 = sqlx::query_scalar(
            "INSERT INTO scrim.replacement_requests(\
                 need_id, candidate_id, participant_id, discord_user_id, status, request_payload, \
                 requested_by_user_id, requested_by_display_name\
             ) VALUES ($1, $2, $3, $4, 'pending', $5::jsonb, $6, $7) RETURNING id",
        )
        .bind(need_id)
        .bind(candidate_id)
        .bind(participant_id)
        .bind(discord_user_id)
        .bind(request_payload)
        .bind(actor.0)
        .bind(actor.1)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE scrim.replacement_candidates SET status='requested', updated_at=now() \
              WHERE id=$1",
        )
        .bind(candidate_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE scrim.replacement_needs SET status='contacting', updated_at=now() WHERE id=$1",
        )
        .bind(need_id)
        .execute(&mut *tx)
        .await?;
        insert_audit_event(
            &mut tx,
            "replacement_request_created",
            ("replacement_request", replacement_request_id),
            actor.0,
            api_request_id,
            idempotency_key,
            json!({"need_id": need_id.to_string(), "participant_id": participant_id.to_string()}),
        )
        .await?;
        let receipt = placeholder_receipt();
        let discord = discord_user_id
            .map(|user_id| DiscordDispatch {
                record_id: replacement_request_id,
                user_id: Some(user_id),
                channel_id: None,
                content: "Hey! 👋 Für ein Scrim wird noch jemand gesucht — du stehst als möglicher Ersatz auf der Liste. Wenn du Zeit und Lust hast, meld dich kurz im Team-Kanal. Danke dir! 🎮".to_string(),
            })
            .into_iter()
            .collect();
        // Den vollstaendigen Dispatch ablegen, nicht nur die Quittung: sonst geht bei einem
        // Replay verloren, welche Discord-Zustellungen noch offen sind.
        let dispatch = MutationDispatch { receipt, discord };
        complete_command(&mut tx, receipt_id, &dispatch).await?;
        tx.commit().await?;
        Ok(dispatch)
    }

    pub async fn patch_replacement_request(
        &self,
        idempotency_key: &str,
        api_request_id: &str,
        payload: &Value,
        replacement_request_id: i64,
        request: &ReplacementRequestPatch,
        actor: (&str, &str),
    ) -> ScrimResult<ActionReceipt> {
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let receipt_id = match begin_command(
            &mut tx,
            "replacement_request_patch",
            idempotency_key,
            payload,
        )
        .await?
        {
            CommandStart::New(id) => id,
            CommandStart::Replay(receipt) => {
                tx.commit().await?;
                return Ok(receipt);
            }
        };
        let row = sqlx::query(
            "SELECT need_id, candidate_id, status FROM scrim.replacement_requests \
              WHERE id=$1 FOR UPDATE",
        )
        .bind(replacement_request_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ScrimError::NotFound("Diese Ersatzanfrage gibt es nicht.".to_string()))?;
        if !matches!(
            row.try_get::<String, _>("status")?.as_str(),
            "pending" | "sent" | "uncertain"
        ) {
            return Err(ScrimError::Conflict(
                "Diese Ersatzanfrage ist schon abgeschlossen.".to_string(),
            ));
        }
        let need_id = row.try_get::<i64, _>("need_id")?;
        let candidate_id = row.try_get::<Option<i64>, _>("candidate_id")?;
        let (request_status, candidate_status) = match request.action {
            ReplacementRequestAction::Accept => ("accepted", "selected"),
            ReplacementRequestAction::Decline => ("declined", "declined"),
        };
        sqlx::query(
            "UPDATE scrim.replacement_requests \
                SET status=$2, responded_at=now(), updated_at=now() WHERE id=$1",
        )
        .bind(replacement_request_id)
        .bind(request_status)
        .execute(&mut *tx)
        .await?;
        if let Some(candidate_id) = candidate_id {
            sqlx::query(
                "UPDATE scrim.replacement_candidates SET status=$2, updated_at=now() WHERE id=$1",
            )
            .bind(candidate_id)
            .bind(candidate_status)
            .execute(&mut *tx)
            .await?;
        }
        if request.action == ReplacementRequestAction::Accept {
            sqlx::query(
                "UPDATE scrim.replacement_needs \
                    SET status='filled', closed_at=now(), updated_at=now() WHERE id=$1",
            )
            .bind(need_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE scrim.replacement_requests \
                    SET status='cancelled', updated_at=now() \
                  WHERE need_id=$1 AND id<>$2 AND status IN ('pending', 'sent', 'uncertain')",
            )
            .bind(need_id)
            .bind(replacement_request_id)
            .execute(&mut *tx)
            .await?;
        }
        insert_audit_event(
            &mut tx,
            "replacement_request_responded",
            ("replacement_request", replacement_request_id),
            actor.0,
            api_request_id,
            idempotency_key,
            json!({"action": request.action}),
        )
        .await?;
        let receipt = placeholder_receipt();
        complete_command(&mut tx, receipt_id, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn create_match_request_batch(
        &self,
        idempotency_key: &str,
        request_id: &str,
        payload: &Value,
        actor_user_id: &str,
        actor_display_name: &str,
        batch: &ValidatedMatchRequestBatch,
    ) -> ScrimResult<ActionReceipt> {
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let receipt_id = match begin_command(
            &mut tx,
            "planning_batch_create",
            idempotency_key,
            payload,
        )
        .await?
        {
            CommandStart::New(id) => id,
            CommandStart::Replay(receipt) => {
                tx.commit().await?;
                return Ok(receipt);
            }
        };

        let team_ids = batch
            .matches
            .iter()
            .flat_map(|request| [Some(request.team_a_id), request.team_b_id])
            .flatten()
            .collect::<BTreeSet<_>>();
        lock_team_ids(&mut tx, &team_ids).await?;
        if existing_team_ids_tx(&mut tx, &team_ids).await? != team_ids {
            return Err(ScrimError::InvalidProposal(
                "At least one team was not found".to_string(),
            ));
        }
        if !active_request_team_ids_tx(&mut tx, &team_ids)
            .await?
            .is_empty()
        {
            return Err(ScrimError::InvalidProposal(
                "Ein Team darf im gleichen aktiven Abfragezeitraum nur in einem Match stecken."
                    .to_string(),
            ));
        }

        lock_id_generation(&mut tx).await?;
        let batch_id = next_id(&mut tx, "scrim.match_request_batches").await?;
        sqlx::query(
            "INSERT INTO scrim.match_request_batches(\
                 id, template, deadline_at, status, created_by_user_id, \
                 created_by_display_name, created_at, updated_at\
             ) VALUES ($1, $2, $3, 'draft', $4, $5, now(), now())",
        )
        .bind(batch_id)
        .bind(template_to_db(batch.template))
        .bind(batch.deadline_at)
        .bind(actor_user_id)
        .bind(actor_display_name)
        .execute(&mut *tx)
        .await?;

        let mut created_request_ids = Vec::with_capacity(batch.matches.len());
        let mut match_request_id = next_id(&mut tx, "scrim.match_requests").await?;
        for request in &batch.matches {
            sqlx::query(
                "INSERT INTO scrim.match_requests(\
                     id, batch_id, team_a_id, team_b_id, status, slot_options, created_at, updated_at\
                 ) VALUES ($1, $2, $3, $4, 'draft', $5::jsonb, now(), now())",
            )
            .bind(match_request_id)
            .bind(batch_id)
            .bind(request.team_a_id)
            .bind(request.team_b_id)
            .bind(serde_json::to_value(&request.slots).map_err(|error| {
                ScrimError::InvalidProposal(format!("slot serialization failed: {error}"))
            })?)
            .execute(&mut *tx)
            .await?;
            created_request_ids.push(match_request_id);
            match_request_id = match_request_id.checked_add(1).ok_or_else(|| {
                ScrimError::InvalidProposal("too many match requests".to_string())
            })?;
        }
        if let Some(entity_id) = created_request_ids.first() {
            insert_audit_event(
                &mut tx,
                "match_request_batch_created",
                ("match_request", i64::from(*entity_id)),
                actor_user_id,
                request_id,
                idempotency_key,
                json!({
                    "batch_id": batch_id.to_string(),
                    "request_ids": created_request_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>(),
                    "template": template_to_db(batch.template),
                    "deadline_at": batch.deadline_at.to_rfc3339(),
                    "match_count": batch.matches.len(),
                }),
            )
            .await?;
        }

        let receipt = ActionReceipt {
            accepted: true,
            message: format!("Match-Request-Batch {batch_id} erstellt."),
        };
        complete_command(&mut tx, receipt_id, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn release_match_request(
        &self,
        idempotency_key: &str,
        api_request_id: &str,
        payload: &Value,
        request_id: i32,
        request: &ReleaseMatchRequest,
        actor: (&str, &str),
    ) -> ScrimResult<ActionReceipt> {
        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let receipt_id = match begin_command(
            &mut tx,
            "match_request_release",
            idempotency_key,
            payload,
        )
        .await?
        {
            CommandStart::New(id) => id,
            CommandStart::Replay(receipt) => {
                tx.commit().await?;
                return Ok(receipt);
            }
        };

        let row = sqlx::query(
            "SELECT mr.batch_id, mr.team_a_id, mr.team_b_id, mr.slot_options, mr.status, \
                    b.deadline_at \
               FROM scrim.match_requests mr \
               JOIN scrim.match_request_batches b ON b.id = mr.batch_id \
              WHERE mr.id = $1 \
              FOR UPDATE OF mr",
        )
        .bind(request_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ScrimError::NotFound("Match request was not found".to_string()))?;

        let status = row.try_get::<String, _>("status")?;
        if !matches!(status.as_str(), "open" | "post_failed") {
            return Err(ScrimError::Conflict(
                "Match request is not open".to_string(),
            ));
        }
        let deadline_at = row.try_get::<DateTime<Utc>, _>("deadline_at")?;
        if deadline_at > Utc::now() {
            return Err(ScrimError::Conflict("Deadline has not passed".to_string()));
        }

        let batch_id = row.try_get::<i32, _>("batch_id")?;
        let team_a_id = row.try_get::<i32, _>("team_a_id")?;
        let team_b_id = row.try_get::<Option<i32>, _>("team_b_id")?;
        let team_ids = [Some(team_a_id), team_b_id]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let slots = parse_slots(row.try_get("slot_options")?)?;
        let roster = load_roster_members_tx(&mut tx, &team_ids).await?;
        let responses = load_match_request_responses_tx(&mut tx, request_id).await?;
        let facts = derive_match_request_facts(
            request_id,
            slots.len(),
            &team_ids,
            &roster,
            &responses,
            true,
            None,
        )?;
        let recommended_slot_index = facts.recommended_slot_index;
        let released_slot_index = request
            .slot_index
            .map(|index| {
                usize::try_from(index).map_err(|_| {
                    ScrimError::InvalidProposal("slot_index is out of range".to_string())
                })
            })
            .transpose()?
            .or(recommended_slot_index)
            .ok_or_else(|| ScrimError::Conflict("No recommended slot available".to_string()))?;
        let released_slot = slots
            .get(released_slot_index)
            .cloned()
            .ok_or_else(|| ScrimError::InvalidProposal("slot_index is out of range".to_string()))?;
        let override_reason = if request.slot_index.is_some()
            && Some(released_slot_index) != recommended_slot_index
        {
            request.reason.clone()
        } else {
            None
        };
        let released_slot_index_i32 = i32::try_from(released_slot_index)
            .map_err(|_| ScrimError::InvalidProposal("slot_index is out of range".to_string()))?;
        sqlx::query(
            "UPDATE scrim.match_requests \
                SET released_slot_index = $2, released_slot = $3::jsonb, released_at = now(), \
                    released_by_user_id = $4, released_by_display_name = $5, \
                    override_reason = $6, status_message_state = 'pending', \
                    status_message_last_error = NULL, status = 'closed', updated_at = now() \
              WHERE id = $1",
        )
        .bind(request_id)
        .bind(released_slot_index_i32)
        .bind(serde_json::to_value(&released_slot).map_err(|error| {
            ScrimError::InvalidProposal(format!("slot serialization failed: {error}"))
        })?)
        .bind(actor.0)
        .bind(actor.1)
        .bind(&override_reason)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE scrim.match_request_batches b \
                SET status = 'closed', updated_at = now() \
              WHERE b.id = $1 \
                AND NOT EXISTS (\
                    SELECT 1 FROM scrim.match_requests mr \
                     WHERE mr.batch_id = b.id \
                       AND mr.status NOT IN ('closed', 'cancelled')\
                )",
        )
        .bind(batch_id)
        .execute(&mut *tx)
        .await?;
        insert_audit_event(
            &mut tx,
            "match_request_released",
            ("match_request", i64::from(request_id)),
            actor.0,
            api_request_id,
            idempotency_key,
            json!({
                "request_id": request_id.to_string(),
                "batch_id": batch_id.to_string(),
                "released_slot_index": released_slot_index,
                "override_reason_present": override_reason.is_some(),
            }),
        )
        .await?;

        let receipt = ActionReceipt {
            accepted: true,
            message: format!("Match-Request {request_id} freigegeben."),
        };
        complete_command(&mut tx, receipt_id, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn record_match_request_response(
        &self,
        idempotency_key: &str,
        api_request_id: &str,
        payload: &Value,
        request: &MatchRequestResponseRequest,
    ) -> ScrimResult<ActionReceipt> {
        let request_id = crate::model::wire_id::parse_i32(&request.request)
            .map_err(ScrimError::InvalidResponse)?;
        let team_id =
            crate::model::wire_id::parse_i32(&request.team).map_err(ScrimError::InvalidResponse)?;
        let actor_id = crate::model::wire_id::parse_i64(&request.actor)
            .map_err(ScrimError::InvalidResponse)?;
        let channel_id = crate::model::wire_id::parse_i64(&request.channel)
            .map_err(ScrimError::InvalidResponse)?;
        let message_id = request
            .message
            .as_ref()
            .map(|message| {
                crate::model::wire_id::parse_i64(message).map_err(ScrimError::InvalidResponse)
            })
            .transpose()?;
        let slot_index = match request.action {
            MatchRequestAction::Slot => i32::try_from(request.slot.ok_or_else(|| {
                ScrimError::InvalidResponse("slot is required for slot action".to_string())
            })?)
            .map_err(|_| ScrimError::InvalidResponse("slot is out of range".to_string()))?,
            MatchRequestAction::None => {
                if request.slot.is_some() {
                    return Err(ScrimError::InvalidResponse(
                        "slot must be null for none action".to_string(),
                    ));
                }
                -1
            }
        };

        let mut tx = self.pool.begin().await?;
        lock_runtime_control(&mut tx).await?;
        require_turniere_runtime(&mut tx).await?;
        let receipt_id =
            match begin_command(&mut tx, "match_request_response", idempotency_key, payload).await?
            {
                CommandStart::New(id) => id,
                CommandStart::Replay(receipt) => {
                    tx.commit().await?;
                    return Ok(receipt);
                }
            };

        let row = sqlx::query(
            "SELECT tm.participant_id::bigint AS participant_id \
               FROM scrim.match_requests mr \
               LEFT JOIN scrim.participants p ON p.discord_id = $3 \
               LEFT JOIN scrim.team_members tm ON tm.participant_id = p.id AND tm.team_id = $2 \
              WHERE mr.id = $1 \
                AND (mr.team_a_id = $2 OR mr.team_b_id = $2)",
        )
        .bind(request_id)
        .bind(team_id)
        .bind(actor_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            ScrimError::InvalidResponse("Diese Terminantwort ist ungültig.".to_string())
        })?;
        let participant_id = row
            .try_get::<Option<i64>, _>("participant_id")?
            .ok_or(ScrimError::ParticipantUnauthorized)?;
        let participant_id_i32 = i32::try_from(participant_id).map_err(|_| {
            ScrimError::InvalidResponse("participant_id is out of range".to_string())
        })?;
        sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
            .bind(request_id)
            .bind(participant_id_i32)
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query(
            "SELECT mr.status, mr.slot_options, mr.team_query_message_ids, \
                    tm.participant_id::bigint AS participant_id \
               FROM scrim.match_requests mr \
               LEFT JOIN scrim.participants p ON p.discord_id = $3 \
               LEFT JOIN scrim.team_members tm ON tm.participant_id = p.id AND tm.team_id = $2 \
              WHERE mr.id = $1 \
                AND (mr.team_a_id = $2 OR mr.team_b_id = $2) \
              FOR UPDATE OF mr",
        )
        .bind(request_id)
        .bind(team_id)
        .bind(actor_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            ScrimError::InvalidResponse("Diese Terminantwort ist ungültig.".to_string())
        })?;
        let status = row.try_get::<String, _>("status")?;
        if !matches!(status.as_str(), "open" | "post_failed") {
            return Err(ScrimError::Conflict(
                "Diese Abstimmung ist nicht mehr offen.".to_string(),
            ));
        }
        row.try_get::<Option<i64>, _>("participant_id")?
            .ok_or(ScrimError::ParticipantUnauthorized)?;
        if slot_index >= 0 {
            let slot_options = row.try_get::<Value, _>("slot_options")?;
            let slot_count = slot_options
                .as_array()
                .ok_or_else(|| {
                    ScrimError::InvalidStoredData("slot_options is not an array".to_string())
                })?
                .len();
            if usize::try_from(slot_index)
                .ok()
                .is_none_or(|index| index >= slot_count)
            {
                return Err(ScrimError::InvalidResponse(
                    "slot is out of range".to_string(),
                ));
            }
        }
        if !posted_message_matches(
            &row.try_get::<Value, _>("team_query_message_ids")?,
            team_id,
            channel_id,
            message_id,
        ) {
            return Err(ScrimError::Conflict(
                "Diese Antwort passt nicht zu dieser Terminabfrage.".to_string(),
            ));
        }
        if slot_index == -1 {
            sqlx::query(
                "DELETE FROM scrim.match_request_responses \
                  WHERE request_id = $1 AND team_id = $2 AND participant_id = $3 \
                    AND slot_index <> -1",
            )
            .bind(request_id)
            .bind(team_id)
            .bind(participant_id_i32)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "DELETE FROM scrim.match_request_responses \
                  WHERE request_id = $1 AND team_id = $2 AND participant_id = $3 \
                    AND slot_index = -1",
            )
            .bind(request_id)
            .bind(team_id)
            .bind(participant_id_i32)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO scrim.match_request_responses(\
                 request_id, team_id, participant_id, discord_user_id, slot_index, response, \
                 source, message_id, channel_id, responded_at, updated_at\
             ) VALUES ($1, $2, $3, $4, $5, $6, 'button', $7, $8, now(), now()) \
             ON CONFLICT(request_id, team_id, participant_id, slot_index) DO UPDATE SET \
                 discord_user_id = excluded.discord_user_id, response = excluded.response, \
                 source = excluded.source, message_id = excluded.message_id, \
                 channel_id = excluded.channel_id, responded_at = now(), updated_at = now()",
        )
        .bind(request_id)
        .bind(team_id)
        .bind(participant_id_i32)
        .bind(actor_id)
        .bind(slot_index)
        .bind(if slot_index == -1 {
            "unavailable"
        } else {
            "available"
        })
        .bind(message_id)
        .bind(channel_id)
        .execute(&mut *tx)
        .await?;
        let actor_user_id = actor_id.to_string();
        insert_audit_event(
            &mut tx,
            "match_request_response_recorded",
            ("match_request", i64::from(request_id)),
            &actor_user_id,
            api_request_id,
            idempotency_key,
            json!({
                "request_id": request_id.to_string(),
                "team_id": team_id.to_string(),
                "slot_index": slot_index,
                "response": if slot_index == -1 { "unavailable" } else { "available" },
            }),
        )
        .await?;

        let receipt = ActionReceipt {
            accepted: true,
            message: if slot_index == -1 {
                "Antwort gespeichert: Kein Slot passt.".to_string()
            } else {
                format!("Antwort gespeichert: Slot {} passt.", slot_index + 1)
            },
        };
        complete_command(&mut tx, receipt_id, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeControl {
    pub mode: String,
    pub operational_writer: String,
    pub epoch: i64,
}

enum CommandStart<T> {
    New(i64),
    Replay(T),
}

#[async_trait]
impl ScrimReadRepository for PgScrimReadRepository {
    async fn read_model(&self) -> ScrimResult<ScrimReadModel> {
        Ok(ScrimReadModel {
            participants: load_participants(&self.pool).await?,
            teams: load_teams(&self.pool).await?,
            matches: load_matches(&self.pool).await?,
            match_request_batches: load_match_request_batches(&self.pool).await?,
            lagebild_refs: load_lagebild_refs(&self.pool).await?,
        })
    }

    async fn coaches(&self) -> ScrimResult<Vec<Coach>> {
        let rows = sqlx::query(
            "SELECT discord_user_id, display_name, avatar_url \
               FROM coaching.coaches \
              WHERE status = 'active' AND discord_user_id IS NOT NULL \
              ORDER BY display_name ASC NULLS LAST, discord_user_id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let discord_user_id = row.try_get::<i64, _>("discord_user_id")?;
                Ok(Coach {
                    discord_user_id: discord_user_id.to_string(),
                    display_name: row
                        .try_get::<Option<String>, _>("display_name")?
                        .unwrap_or_else(|| discord_user_id.to_string()),
                    avatar_url: row.try_get("avatar_url")?,
                })
            })
            .collect()
    }

    async fn is_active_coach(&self, discord_id: i64) -> ScrimResult<bool> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(\
                SELECT 1 FROM coaching.coaches \
                  WHERE discord_user_id = $1 AND status = 'active'\
             )",
        )
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn existing_team_ids(&self, team_ids: &BTreeSet<i32>) -> ScrimResult<BTreeSet<i32>> {
        if team_ids.is_empty() {
            return Ok(BTreeSet::new());
        }
        let team_ids = team_ids.iter().copied().collect::<Vec<_>>();
        Ok(
            sqlx::query_scalar::<_, i32>("SELECT id FROM scrim.teams WHERE id = ANY($1)")
                .bind(team_ids)
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .collect(),
        )
    }

    async fn active_request_team_ids(
        &self,
        team_ids: &BTreeSet<i32>,
    ) -> ScrimResult<BTreeSet<i32>> {
        if team_ids.is_empty() {
            return Ok(BTreeSet::new());
        }
        let team_ids = team_ids.iter().copied().collect::<Vec<_>>();
        Ok(sqlx::query_scalar::<_, i32>(
            "SELECT active.team_id \
               FROM (\
                     SELECT mr.team_a_id AS team_id \
                       FROM scrim.match_requests mr \
                       JOIN scrim.match_request_batches b ON b.id = mr.batch_id \
                      WHERE b.status IN ('draft', 'posting', 'open', 'post_failed') \
                        AND b.deadline_at > now() \
                     UNION \
                     SELECT mr.team_b_id AS team_id \
                       FROM scrim.match_requests mr \
                       JOIN scrim.match_request_batches b ON b.id = mr.batch_id \
                      WHERE mr.team_b_id IS NOT NULL \
                        AND b.status IN ('draft', 'posting', 'open', 'post_failed') \
                        AND b.deadline_at > now()\
               ) active \
              WHERE active.team_id = ANY($1)",
        )
        .bind(team_ids)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect())
    }
}

async fn load_self_service_participant(
    tx: &mut Transaction<'_, Postgres>,
    participant_id: i32,
) -> ScrimResult<SelfServiceParticipant> {
    let row = sqlx::query(
        "SELECT id, display_name, rank, roles, availability, availability_slots, status, source \
           FROM scrim.participants WHERE id=$1",
    )
    .bind(participant_id)
    .fetch_one(&mut **tx)
    .await?;
    let availability = row.try_get::<Option<String>, _>("availability")?;
    let availability_slots = row.try_get::<Option<Value>, _>("availability_slots")?;
    let availability_confirmed = availability_slots.is_some();
    Ok(SelfServiceParticipant {
        id: row.try_get("id")?,
        display_name: row.try_get("display_name")?,
        rank: row.try_get("rank")?,
        roles: row.try_get("roles")?,
        availability_slots: effective_self_service_availability(
            availability_slots,
            availability.as_deref(),
        ),
        availability_confirmed,
        availability,
        status: row.try_get("status")?,
        source: row.try_get("source")?,
    })
}

#[derive(Debug, Clone)]
struct RoleSnapshot {
    subject: String,
    discord_user_id: Option<u64>,
    role_ids: BTreeSet<u64>,
}

async fn load_roster_team(
    tx: &mut Transaction<'_, Postgres>,
    team_id: i32,
) -> ScrimResult<RosterTeam> {
    let row = sqlx::query(
        "SELECT id, name, coach, coach_discord_id, discord_role_id, discord_channel_id, \
                default_from, default_to \
         FROM scrim.teams WHERE id=$1",
    )
    .bind(team_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ScrimError::NotFound("Dieses Team gibt es nicht.".to_string()))?;
    Ok(RosterTeam {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        coach: row.try_get("coach")?,
        coach_discord_id: row
            .try_get::<Option<i64>, _>("coach_discord_id")?
            .map(|id| id.to_string()),
        discord_role_id: row.try_get("discord_role_id")?,
        discord_channel_id: row.try_get("discord_channel_id")?,
        default_from: row.try_get("default_from")?,
        default_to: row.try_get("default_to")?,
    })
}

async fn load_roster_participant(
    tx: &mut Transaction<'_, Postgres>,
    participant_id: i32,
) -> ScrimResult<RosterParticipant> {
    let row = sqlx::query(
        "SELECT p.id, p.display_name, p.rank, p.roles, p.availability, \
                p.availability_slots, (p.discord_id IS NOT NULL) AS discord_linked, \
                p.notes, p.status, p.source, t.id AS team_id, t.name AS team_name, \
                t.coach AS team_coach, t.coach_discord_id AS team_coach_discord_id, \
                t.discord_role_id AS team_discord_role_id, \
                t.discord_channel_id AS team_discord_channel_id, \
                t.default_from AS team_default_from, t.default_to AS team_default_to, \
                tm.role AS team_member_role, \
                COALESCE(tm.is_captain, false) AS is_captain, \
                COALESCE(tm.is_bench, false) AS is_bench \
         FROM scrim.participants p \
         LEFT JOIN LATERAL (\
             SELECT team_id, role, is_captain, is_bench \
             FROM scrim.team_members WHERE participant_id=p.id \
             ORDER BY team_id ASC LIMIT 1\
         ) tm ON true \
         LEFT JOIN scrim.teams t ON t.id=tm.team_id \
         WHERE p.id=$1",
    )
    .bind(participant_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ScrimError::NotFound("Diesen Spieler gibt es nicht.".to_string()))?;
    let availability: Option<String> = row.try_get("availability")?;
    let slots: Option<Value> = row.try_get("availability_slots")?;
    let availability_confirmed = slots.is_some();
    let team_id: Option<i32> = row.try_get("team_id")?;
    let team = match team_id {
        Some(id) => Some(RosterTeam {
            id,
            name: row.try_get("team_name")?,
            coach: row.try_get("team_coach")?,
            coach_discord_id: row
                .try_get::<Option<i64>, _>("team_coach_discord_id")?
                .map(|id| id.to_string()),
            discord_role_id: row.try_get("team_discord_role_id")?,
            discord_channel_id: row.try_get("team_discord_channel_id")?,
            default_from: row.try_get("team_default_from")?,
            default_to: row.try_get("team_default_to")?,
        }),
        None => None,
    };
    Ok(RosterParticipant {
        id: row.try_get("id")?,
        display_name: row.try_get("display_name")?,
        rank: row.try_get("rank")?,
        roles: row.try_get("roles")?,
        availability_slots: effective_self_service_availability(slots, availability.as_deref()),
        availability_confirmed,
        availability,
        discord_linked: row.try_get("discord_linked")?,
        notes: row.try_get("notes")?,
        status: row.try_get("status")?,
        source: row.try_get("source")?,
        team,
        role: row.try_get("team_member_role")?,
        is_captain: row.try_get("is_captain")?,
        is_bench: row.try_get("is_bench")?,
    })
}

async fn resolve_coach(
    tx: &mut Transaction<'_, Postgres>,
    coach_discord_id: Option<i64>,
    fallback: Option<&str>,
) -> ScrimResult<Option<String>> {
    let Some(coach_discord_id) = coach_discord_id else {
        return Ok(fallback.map(str::to_string));
    };
    sqlx::query_scalar(
        "SELECT display_name FROM coaching.coaches \
         WHERE discord_user_id=$1 AND status='active'",
    )
    .bind(coach_discord_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ScrimError::InvalidProposal("Diesen Coach gibt es nicht.".to_string()))
    .map(Some)
}

async fn coach_role_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    coach_discord_id: i64,
) -> ScrimResult<RoleSnapshot> {
    let roles = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT discord_role_id FROM scrim.teams \
         WHERE coach_discord_id=$1 ORDER BY discord_role_id ASC NULLS LAST",
    )
    .bind(coach_discord_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(RoleSnapshot {
        subject: format!("coach-{coach_discord_id}"),
        discord_user_id: u64::try_from(coach_discord_id).ok(),
        role_ids: positive_roles(roles),
    })
}

async fn coach_sync_plans(
    tx: &mut Transaction<'_, Postgres>,
    before: Vec<RoleSnapshot>,
) -> ScrimResult<Vec<DiscordRoleSyncPlan>> {
    let mut plans = Vec::with_capacity(before.len());
    for before in before {
        let coach_id = before
            .discord_user_id
            .and_then(|id| i64::try_from(id).ok())
            .ok_or(ScrimError::InvalidActor)?;
        let after = coach_role_snapshot(tx, coach_id).await?;
        plans.push(role_diff(&before, &after));
    }
    Ok(plans)
}

async fn participant_role_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    participant_id: i32,
    reserve_role_id: Option<u64>,
    signup_role_id: Option<u64>,
) -> ScrimResult<RoleSnapshot> {
    let rows = sqlx::query(
        "SELECT p.discord_id, p.status, t.discord_role_id \
         FROM scrim.participants p \
         LEFT JOIN scrim.team_members tm ON tm.participant_id=p.id \
         LEFT JOIN scrim.teams t ON t.id=tm.team_id \
         WHERE p.id=$1 ORDER BY t.discord_role_id ASC NULLS LAST",
    )
    .bind(participant_id)
    .fetch_all(&mut **tx)
    .await?;
    let first = rows
        .first()
        .ok_or_else(|| ScrimError::NotFound("Diesen Spieler gibt es nicht.".to_string()))?;
    let discord_id: Option<i64> = first.try_get("discord_id")?;
    let status: String = first.try_get("status")?;
    let team_roles = rows
        .iter()
        .map(|row| row.try_get::<Option<i64>, _>("discord_role_id"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RoleSnapshot {
        subject: participant_id.to_string(),
        discord_user_id: discord_id.and_then(|id| u64::try_from(id).ok()),
        role_ids: managed_role_ids(&status, signup_role_id, reserve_role_id, team_roles),
    })
}

fn role_diff(before: &RoleSnapshot, after: &RoleSnapshot) -> DiscordRoleSyncPlan {
    let removes = before
        .role_ids
        .difference(&after.role_ids)
        .map(|role_id| DiscordRoleAction {
            operation: RoleOperation::Remove,
            role_id: *role_id,
        });
    let adds = after
        .role_ids
        .difference(&before.role_ids)
        .map(|role_id| DiscordRoleAction {
            operation: RoleOperation::Add,
            role_id: *role_id,
        });
    DiscordRoleSyncPlan {
        subject: after.subject.clone(),
        discord_user_id: after.discord_user_id.or(before.discord_user_id),
        actions: removes.chain(adds).collect(),
    }
}

/// Stellt den Soll-Zustand her, statt ihn nur zu ergaenzen.
///
/// Die Ist-Rollen auf Discord kennen wir nicht — wohl aber die Menge der Rollen, die wir
/// selbst verwalten (alle Team-Rollen plus Anmelde- und Reserve-Rolle). Innerhalb dieser
/// Menge koennen wir gefahrlos entfernen, was nicht ins Soll gehoert; alles andere am
/// Discord-Mitglied bleibt unberuehrt.
///
/// Ohne das Entfernen behaelt ein Spieler nach einem Teamwechsel die alte Teamrolle und
/// sieht weiter den alten Team-Kanal. Zugleich ist das der Reparaturweg, wenn ein Sync
/// nach dem DB-Commit fehlgeschlagen ist: ein erneuter Aufruf stellt den vollen Soll-Zustand
/// her, auch wenn die Datenbank laengst den Zielzustand traegt.
fn role_resync(snapshot: &RoleSnapshot, managed_role_ids: &BTreeSet<u64>) -> DiscordRoleSyncPlan {
    let removes = managed_role_ids
        .difference(&snapshot.role_ids)
        .map(|role_id| DiscordRoleAction {
            operation: RoleOperation::Remove,
            role_id: *role_id,
        });
    let adds = snapshot.role_ids.iter().map(|role_id| DiscordRoleAction {
        operation: RoleOperation::Add,
        role_id: *role_id,
    });
    DiscordRoleSyncPlan {
        subject: snapshot.subject.clone(),
        discord_user_id: snapshot.discord_user_id,
        actions: removes.chain(adds).collect(),
    }
}

/// Alle Rollen, die der Scrim-Betrieb selbst vergibt: jede Team-Rolle plus Anmelde- und
/// Reserve-Rolle. Grenzt ab, was ein Resync anfassen darf.
async fn all_managed_role_ids(
    tx: &mut Transaction<'_, Postgres>,
    signup_role_id: Option<u64>,
    reserve_role_id: Option<u64>,
) -> ScrimResult<BTreeSet<u64>> {
    let rows =
        sqlx::query("SELECT discord_role_id FROM scrim.teams WHERE discord_role_id IS NOT NULL")
            .fetch_all(&mut **tx)
            .await?;
    let team_roles = rows
        .iter()
        .map(|row| row.try_get::<Option<i64>, _>("discord_role_id"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut role_ids = positive_roles(team_roles);
    role_ids.extend(signup_role_id);
    role_ids.extend(reserve_role_id);
    Ok(role_ids)
}

fn positive_roles(values: Vec<Option<i64>>) -> BTreeSet<u64> {
    values
        .into_iter()
        .flatten()
        .filter_map(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
        .collect()
}

fn parse_positive_i64(value: &str) -> ScrimResult<i64> {
    value
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or(ScrimError::InvalidActor)
}

fn trimmed_nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn patch_option<T>(value: PatchValue<T>, current: Option<T>) -> Option<T> {
    match value {
        PatchValue::Omitted => current,
        PatchValue::Null => None,
        PatchValue::Value(value) => Some(value),
    }
}

fn patch_text_value(value: PatchValue<String>) -> Option<String> {
    match value {
        PatchValue::Value(value) => Some(value),
        PatchValue::Omitted | PatchValue::Null => None,
    }
}

fn patch_bool(value: PatchValue<bool>) -> Option<bool> {
    match value {
        PatchValue::Value(value) => Some(value),
        PatchValue::Omitted | PatchValue::Null => None,
    }
}

fn managed_role_ids(
    status: &str,
    signup_role_id: Option<u64>,
    reserve_role_id: Option<u64>,
    team_role_ids: Vec<Option<i64>>,
) -> BTreeSet<u64> {
    if status.trim().eq_ignore_ascii_case("inactive") {
        return BTreeSet::new();
    }
    let mut role_ids = BTreeSet::new();
    role_ids.extend(signup_role_id);
    if status.trim().eq_ignore_ascii_case("reserve") {
        role_ids.extend(reserve_role_id);
    }
    role_ids.extend(
        team_role_ids
            .into_iter()
            .flatten()
            .filter_map(|role_id| u64::try_from(role_id).ok())
            .filter(|role_id| *role_id > 0),
    );
    role_ids
}

fn effective_self_service_availability(
    slots: Option<Value>,
    legacy: Option<&str>,
) -> SelfServiceAvailability {
    if let Some(slots) = slots {
        return serde_json::from_value(slots).unwrap_or_default();
    }
    legacy.map(parse_legacy_availability).unwrap_or_default()
}

fn parse_legacy_availability(text: &str) -> SelfServiceAvailability {
    let text = text.trim();
    if text.is_empty() {
        return SelfServiceAvailability::default();
    }
    if let Ok(Value::Object(values)) = serde_json::from_str::<Value>(text) {
        let mut weekly = SelfServiceAvailability::default();
        for (key, slot) in [
            ("mo", &mut weekly.mon),
            ("di", &mut weekly.tue),
            ("mi", &mut weekly.wed),
            ("do", &mut weekly.thu),
            ("fr", &mut weekly.fri),
            ("sa", &mut weekly.sat),
            ("so", &mut weekly.sun),
        ] {
            if let Some(value) = values.get(key) {
                *slot = parse_legacy_slot(&legacy_value_to_string(value));
            }
        }
        return weekly;
    }
    let slot = parse_legacy_slot(text);
    SelfServiceAvailability {
        mon: slot.clone(),
        tue: slot.clone(),
        wed: slot.clone(),
        thu: slot.clone(),
        fri: slot.clone(),
        sat: slot.clone(),
        sun: slot,
    }
}

fn parse_legacy_slot(raw: &str) -> AvailabilitySlot {
    let lower = raw.trim().to_ascii_lowercase();
    let lower = lower.trim();
    if lower.is_empty() || lower == "?" {
        return AvailabilitySlot::default();
    }
    if lower.contains("geht nicht") || lower.contains("nein") || lower.contains("keine zeit") {
        return AvailabilitySlot {
            status: AvailabilityStatus::Unavailable,
            from: None,
            to: None,
        };
    }
    if matches!(
        lower,
        "flexibel" | "immer" | "immer zeit" | "jederzeit" | "optimal"
    ) {
        return available_slot(None, None);
    }
    if let Some((from, to)) = parse_hour_range(lower) {
        return available_slot(Some(from), Some(to));
    }
    if let Some(from) = parse_time(lower) {
        return available_slot(Some(from), None);
    }
    if let Some(rest) = lower.strip_prefix("ab ") {
        if let Some(from) = parse_time_or_hour(rest.trim()) {
            return available_slot(Some(from), None);
        }
    }
    if let Some(from) = parse_hour(lower) {
        return available_slot(Some(from), None);
    }
    for (needle, from) in [
        ("abend", 18 * 60),
        ("nachmittag", 14 * 60),
        ("mittag", 12 * 60),
    ] {
        if lower.contains(needle) {
            return available_slot(Some(from), None);
        }
    }
    available_slot(None, None)
}

fn available_slot(from: Option<u16>, to: Option<u16>) -> AvailabilitySlot {
    AvailabilitySlot {
        status: AvailabilityStatus::Available,
        from,
        to,
    }
}

fn parse_hour_range(value: &str) -> Option<(u16, u16)> {
    let (from, to) = value.split_once('-')?;
    let from = parse_time_or_hour(from.trim())?;
    let to = parse_time_or_hour(to.trim())?;
    (from < to && from <= 1_440 && to <= 1_440).then_some((from, to))
}

fn parse_time_or_hour(value: &str) -> Option<u16> {
    parse_time(value).or_else(|| parse_hour(value))
}

fn parse_time(value: &str) -> Option<u16> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.trim().parse::<u16>().ok()?;
    let minute = minute.trim().parse::<u16>().ok()?;
    if hour > 24 || minute > 59 || (hour == 24 && minute != 0) {
        return None;
    }
    Some(hour * 60 + minute)
}

fn parse_hour(value: &str) -> Option<u16> {
    let hour = value.parse::<u16>().ok()?;
    (hour <= 24).then_some(hour * 60)
}

fn legacy_value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        value => value.to_string(),
    }
}

async fn load_participants(pool: &Pool) -> ScrimResult<Vec<Participant>> {
    let rows = sqlx::query(
        "SELECT id, discord_id, display_name, rank, rank_source, rank_verified, roles, \
                availability, availability_slots, notes, status, source, created_at, updated_at \
           FROM scrim.participants \
          ORDER BY status ASC, display_name ASC, id ASC",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(Participant {
                id: row.try_get("id")?,
                discord_id: snowflake(row.try_get("discord_id")?),
                display_name: row.try_get("display_name")?,
                rank: row.try_get("rank")?,
                rank_source: row.try_get("rank_source")?,
                rank_verified: row.try_get("rank_verified")?,
                roles: row.try_get("roles")?,
                availability: row.try_get("availability")?,
                availability_slots: parse_weekly_availability(row.try_get("availability_slots")?),
                notes: row.try_get("notes")?,
                status: row.try_get("status")?,
                source: row.try_get("source")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

async fn load_teams(pool: &Pool) -> ScrimResult<Vec<Team>> {
    let member_rows = sqlx::query(
        "SELECT tm.team_id, tm.participant_id, p.display_name, p.rank, p.discord_id, \
                p.roles, p.availability, p.availability_slots, p.notes, tm.role, tm.is_captain, \
                tm.is_bench, tm.substitute_until \
           FROM scrim.team_members tm \
           JOIN scrim.participants p ON p.id = tm.participant_id \
          ORDER BY tm.team_id ASC, tm.is_bench ASC, tm.is_captain DESC, \
                   p.display_name ASC, tm.participant_id ASC",
    )
    .fetch_all(pool)
    .await?;
    let mut members: BTreeMap<i32, Vec<TeamMember>> = BTreeMap::new();
    for row in member_rows {
        let team_id = row.try_get("team_id")?;
        members.entry(team_id).or_default().push(TeamMember {
            team_id,
            participant_id: row.try_get("participant_id")?,
            display_name: row.try_get("display_name")?,
            rank: row.try_get("rank")?,
            discord_id: snowflake(row.try_get("discord_id")?),
            roles: row.try_get("roles")?,
            availability: row.try_get("availability")?,
            availability_slots: parse_weekly_availability(row.try_get("availability_slots")?),
            notes: row.try_get("notes")?,
            role: row.try_get("role")?,
            is_captain: row.try_get("is_captain")?,
            is_bench: row.try_get("is_bench")?,
            substitute_until: row.try_get("substitute_until")?,
        });
    }

    let rows = sqlx::query(
        "SELECT id, name, coach, coach_discord_id, discord_role_id, discord_channel_id, \
                default_from, default_to, created_at \
           FROM scrim.teams \
          ORDER BY name ASC, id ASC",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id = row.try_get("id")?;
            Ok(Team {
                id,
                name: row.try_get("name")?,
                coach: row.try_get("coach")?,
                coach_discord_id: snowflake(row.try_get("coach_discord_id")?),
                discord_role_id: snowflake(row.try_get("discord_role_id")?),
                discord_channel_id: snowflake(row.try_get("discord_channel_id")?),
                default_from: row.try_get("default_from")?,
                default_to: row.try_get("default_to")?,
                created_at: row.try_get("created_at")?,
                members: members.remove(&id).unwrap_or_default(),
            })
        })
        .collect()
}

async fn load_matches(pool: &Pool) -> ScrimResult<Vec<ScrimMatch>> {
    let rows = sqlx::query(
        "SELECT m.id, m.team_a_id, ta.name AS team_a_name, m.team_b_id, \
                tb.name AS team_b_name, m.when_text, m.scheduled_at, m.status, \
                m.lobby_state, m.party_id, m.join_code, m.lobby_code_source_user_id, \
                m.lobby_code_source_display_name, m.lobby_code_updated_at, \
                m.coach_spectator_discord_id, m.created_at, m.updated_at, \
                selected.result_ref_id, selected.steam_match_id AS selected_steam_match_id, \
                selected.winner_team_id AS selected_winner_team_id, selected.source, \
                selected.selected_at, selected.selected_by_user_id \
           FROM scrim.matches m \
           LEFT JOIN scrim.teams ta ON ta.id = m.team_a_id \
           LEFT JOIN scrim.teams tb ON tb.id = m.team_b_id \
           LEFT JOIN scrim.selected_match_results selected ON selected.match_id = m.id \
          ORDER BY m.scheduled_at DESC NULLS LAST, m.created_at DESC, m.id DESC",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id = row.try_get("id")?;
            Ok(ScrimMatch {
                id,
                team_a: team_ref(row.try_get("team_a_id")?, row.try_get("team_a_name")?),
                team_b: team_ref(row.try_get("team_b_id")?, row.try_get("team_b_name")?),
                when_text: row.try_get("when_text")?,
                scheduled_at: row.try_get("scheduled_at")?,
                status: row.try_get("status")?,
                lobby_state: row.try_get("lobby_state")?,
                party_id: row.try_get("party_id")?,
                join_code: row.try_get("join_code")?,
                lobby_code_source_user_id: row.try_get("lobby_code_source_user_id")?,
                lobby_code_source_display_name: row.try_get("lobby_code_source_display_name")?,
                lobby_code_updated_at: row.try_get("lobby_code_updated_at")?,
                coach_spectator_discord_id: snowflake(row.try_get("coach_spectator_discord_id")?),
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                selected_result: selected_result(&row)?,
            })
        })
        .collect()
}

async fn load_match_tx(tx: &mut Transaction<'_, Postgres>, id: i32) -> ScrimResult<ScrimMatch> {
    let row = sqlx::query(
        "SELECT m.id, m.team_a_id, ta.name AS team_a_name, m.team_b_id, \
                tb.name AS team_b_name, m.when_text, m.scheduled_at, m.status, \
                m.lobby_state, m.party_id, m.join_code, m.lobby_code_source_user_id, \
                m.lobby_code_source_display_name, m.lobby_code_updated_at, \
                m.coach_spectator_discord_id, m.created_at, m.updated_at, \
                selected.result_ref_id, selected.steam_match_id AS selected_steam_match_id, \
                selected.winner_team_id AS selected_winner_team_id, selected.source, \
                selected.selected_at, selected.selected_by_user_id \
           FROM scrim.matches m \
           LEFT JOIN scrim.teams ta ON ta.id = m.team_a_id \
           LEFT JOIN scrim.teams tb ON tb.id = m.team_b_id \
           LEFT JOIN scrim.selected_match_results selected ON selected.match_id = m.id \
          WHERE m.id=$1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ScrimError::NotFound("Match not found".to_string()))?;
    Ok(ScrimMatch {
        id: row.try_get("id")?,
        team_a: team_ref(row.try_get("team_a_id")?, row.try_get("team_a_name")?),
        team_b: team_ref(row.try_get("team_b_id")?, row.try_get("team_b_name")?),
        when_text: row.try_get("when_text")?,
        scheduled_at: row.try_get("scheduled_at")?,
        status: row.try_get("status")?,
        lobby_state: row.try_get("lobby_state")?,
        party_id: row.try_get("party_id")?,
        join_code: row.try_get("join_code")?,
        lobby_code_source_user_id: row.try_get("lobby_code_source_user_id")?,
        lobby_code_source_display_name: row.try_get("lobby_code_source_display_name")?,
        lobby_code_updated_at: row.try_get("lobby_code_updated_at")?,
        coach_spectator_discord_id: snowflake(row.try_get("coach_spectator_discord_id")?),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        selected_result: selected_result(&row)?,
    })
}

fn announcement_from_row(
    block_id: &str,
    row: &sqlx::postgres::PgRow,
) -> ScrimResult<AnnouncementPreview> {
    Ok(AnnouncementPreview {
        id: Some(row.try_get("id")?),
        block_id: block_id.to_string(),
        title: row.try_get("title")?,
        message: row.try_get("body")?,
        channel_id: row.try_get("channel_id")?,
        status: row.try_get("status")?,
        published_at: row.try_get("published_at")?,
    })
}

fn announcement_block_key(block_id: &str) -> String {
    if block_id.contains(':') {
        block_id.to_string()
    } else {
        format!("block:{block_id}")
    }
}

fn state_blocks_lobby_request(current: &str, requested_state: &str) -> bool {
    !(requested_state == "result_requested"
        && matches!(current, "in_progress" | "result_failed" | "finished"))
        && is_bot_owned_lobby_state(current)
}

/// Sperrt das Setzen des Lobbycodes, sobald der Bot die Lobby tatsaechlich fuehrt.
///
/// `lobby_open` ist bewusst ausgenommen: diesen Zustand setzt das Eintragen des Codes
/// selbst. Zaehlte er als bot-gesteuert, waere schon die erste Korrektur eines
/// vertippten Codes gesperrt und die mitgefuehrte Korrekturhistorie nie erreichbar.
/// Das weicht bewusst vom alten Admin-Dashboard ab, das hier zumacht.
fn blocks_lobby_code_write(state: &str) -> bool {
    state != "lobby_open" && is_bot_owned_lobby_state(state)
}

fn is_bot_owned_lobby_state(state: &str) -> bool {
    matches!(
        state,
        "start_requested"
            | "lobby_open"
            | "starting"
            | "lobby_posting"
            | "result_requested"
            | "start_failed"
            | "in_progress"
            | "finished"
            | "result_fetching"
            | "result_failed"
    )
}

async fn load_match_request_batches(pool: &Pool) -> ScrimResult<Vec<MatchRequestBatch>> {
    let batch_deadlines = sqlx::query("SELECT id, deadline_at FROM scrim.match_request_batches")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| Ok((row.try_get("id")?, row.try_get("deadline_at")?)))
        .collect::<ScrimResult<BTreeMap<i32, chrono::DateTime<chrono::Utc>>>>()?;
    let roster_members = sqlx::query(
        "SELECT tm.team_id, tm.participant_id, p.display_name, tm.is_bench \
           FROM scrim.team_members tm \
           JOIN scrim.participants p ON p.id = tm.participant_id \
          ORDER BY tm.team_id ASC, tm.is_bench ASC, p.display_name ASC, tm.participant_id ASC",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(RosterMember {
            team_id: row.try_get("team_id")?,
            participant_id: row.try_get("participant_id")?,
            display_name: row.try_get("display_name")?,
            is_bench: row.try_get("is_bench")?,
        })
    })
    .collect::<ScrimResult<Vec<_>>>()?;
    let response_rows = sqlx::query(
        "SELECT request_id, team_id, participant_id, discord_user_id, slot_index, response, \
                source, message_id, channel_id, responded_at, updated_at \
           FROM scrim.match_request_responses \
          ORDER BY request_id ASC, team_id ASC, participant_id ASC, slot_index ASC",
    )
    .fetch_all(pool)
    .await?;
    let mut responses: BTreeMap<i32, Vec<MatchRequestResponse>> = BTreeMap::new();
    for row in response_rows {
        let request_id = row.try_get("request_id")?;
        responses
            .entry(request_id)
            .or_default()
            .push(MatchRequestResponse {
                request_id,
                team_id: row.try_get("team_id")?,
                participant_id: row.try_get("participant_id")?,
                discord_user_id: row.try_get::<i64, _>("discord_user_id")?.to_string(),
                slot_index: row.try_get("slot_index")?,
                response: parse_response_choice(row.try_get("response")?)?,
                source: row.try_get("source")?,
                message_id: snowflake(row.try_get("message_id")?),
                channel_id: snowflake(row.try_get("channel_id")?),
                responded_at: row.try_get("responded_at")?,
                updated_at: row.try_get("updated_at")?,
            });
    }

    let request_rows = sqlx::query(
        "SELECT mr.id, mr.batch_id, mr.team_a_id, ta.name AS team_a_name, mr.team_b_id, \
                tb.name AS team_b_name, mr.status, mr.slot_options, mr.released_slot_index, \
                mr.released_slot, mr.released_at, mr.released_by_user_id, \
                mr.released_by_display_name, mr.override_reason, mr.created_at, mr.updated_at \
           FROM scrim.match_requests mr \
           JOIN scrim.teams ta ON ta.id = mr.team_a_id \
           LEFT JOIN scrim.teams tb ON tb.id = mr.team_b_id \
          ORDER BY mr.batch_id ASC, mr.id ASC",
    )
    .fetch_all(pool)
    .await?;
    let mut requests: BTreeMap<i32, Vec<MatchRequest>> = BTreeMap::new();
    let now = chrono::Utc::now();
    for row in request_rows {
        let id = row.try_get("id")?;
        let batch_id = row.try_get("batch_id")?;
        let team_a =
            team_ref(row.try_get("team_a_id")?, row.try_get("team_a_name")?).ok_or_else(|| {
                ScrimError::InvalidStoredData("request team_a is missing".to_string())
            })?;
        let team_b = team_ref(row.try_get("team_b_id")?, row.try_get("team_b_name")?);
        let team_ids = [Some(team_a.id), team_b.as_ref().map(|team| team.id)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let slots = parse_slots(row.try_get("slot_options")?)?;
        let request_responses = responses.remove(&id).unwrap_or_default();
        let deadline_passed = batch_deadlines
            .get(&batch_id)
            .is_some_and(|deadline| *deadline <= now);
        let released_slot_index = row
            .try_get::<Option<i32>, _>("released_slot_index")?
            .and_then(|index| usize::try_from(index).ok());
        let facts = derive_match_request_facts(
            id,
            slots.len(),
            &team_ids,
            &roster_members
                .iter()
                .filter(|member| team_ids.contains(&member.team_id))
                .cloned()
                .collect::<Vec<_>>(),
            &request_responses,
            deadline_passed,
            released_slot_index,
        )?;
        requests.entry(batch_id).or_default().push(MatchRequest {
            id,
            batch_id,
            team_a,
            team_b,
            status: row.try_get("status")?,
            slots,
            released_slot_index: released_slot_index.and_then(|index| i32::try_from(index).ok()),
            released_slot: parse_optional_slot(row.try_get("released_slot")?)?,
            released_at: row.try_get("released_at")?,
            released_by_user_id: row.try_get("released_by_user_id")?,
            released_by_display_name: row.try_get("released_by_display_name")?,
            override_reason: row.try_get("override_reason")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            responses: request_responses,
            facts,
        });
    }

    let rows = sqlx::query(
        "SELECT id, template, deadline_at, status, created_by_user_id, \
                created_by_display_name, created_at, updated_at \
           FROM scrim.match_request_batches \
          ORDER BY deadline_at DESC, id DESC",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id = row.try_get("id")?;
            Ok(MatchRequestBatch {
                id,
                template: parse_template(row.try_get("template")?)?,
                deadline_at: row.try_get("deadline_at")?,
                status: row.try_get("status")?,
                created_by_user_id: row.try_get("created_by_user_id")?,
                created_by_display_name: row.try_get("created_by_display_name")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                requests: requests.remove(&id).unwrap_or_default(),
            })
        })
        .collect()
}

async fn load_lagebild_refs(pool: &Pool) -> ScrimResult<Vec<LagebildSnapshotRef>> {
    let evidence_rows = sqlx::query(
        "SELECT id, snapshot_id, evidence_type, label, url, reference_id, occurred_at \
           FROM scrim.lagebild_evidences \
          ORDER BY snapshot_id ASC, occurred_at DESC NULLS LAST, id ASC",
    )
    .fetch_all(pool)
    .await?;
    let mut evidences: BTreeMap<i64, Vec<LagebildEvidenceRef>> = BTreeMap::new();
    for row in evidence_rows {
        evidences
            .entry(row.try_get("snapshot_id")?)
            .or_default()
            .push(LagebildEvidenceRef {
                id: row.try_get("id")?,
                evidence_type: row.try_get("evidence_type")?,
                label: row.try_get("label")?,
                url: row
                    .try_get::<Option<String>, _>("url")?
                    .filter(|url| is_allowed_lagebild_evidence_url(url)),
                reference_id: row.try_get("reference_id")?,
                occurred_at: row.try_get("occurred_at")?,
            });
    }

    let rows = sqlx::query(
        "SELECT id, team_id, generated_at, generated_for, source, status, model, error \
           FROM scrim.lagebild_snapshots \
          ORDER BY team_id ASC, generated_at DESC, id DESC",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id = row.try_get("id")?;
            Ok(LagebildSnapshotRef {
                id,
                team_id: row.try_get("team_id")?,
                generated_at: row.try_get("generated_at")?,
                generated_for: row.try_get("generated_for")?,
                source: row.try_get("source")?,
                status: row.try_get("status")?,
                model: row.try_get("model")?,
                error: row.try_get("error")?,
                evidences: evidences.remove(&id).unwrap_or_default(),
            })
        })
        .collect()
}

async fn load_runtime_control(pool: &Pool) -> ScrimResult<RuntimeControl> {
    let row = sqlx::query(
        "SELECT mode, operational_writer, epoch \
           FROM scrim.runtime_control \
          WHERE control_key = 'scrim_runtime'",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ScrimError::InvalidStoredData("runtime_control row is missing".to_string()))?;
    runtime_control_from_row(&row)
}

async fn require_turniere_runtime(
    tx: &mut Transaction<'_, Postgres>,
) -> ScrimResult<RuntimeControl> {
    let row = sqlx::query(
        "SELECT mode, operational_writer, epoch \
           FROM scrim.runtime_control \
          WHERE control_key = 'scrim_runtime'",
    )
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ScrimError::InvalidStoredData("runtime_control row is missing".to_string()))?;
    let control = runtime_control_from_row(&row)?;
    if control.operational_writer == "turniere"
        && matches!(control.mode.as_str(), "draining" | "turniere")
    {
        Ok(control)
    } else {
        Err(ScrimError::RuntimeNotWritable {
            mode: control.mode,
            operational_writer: control.operational_writer,
        })
    }
}

fn runtime_control_from_row(row: &sqlx::postgres::PgRow) -> ScrimResult<RuntimeControl> {
    Ok(RuntimeControl {
        mode: row.try_get("mode")?,
        operational_writer: row.try_get("operational_writer")?,
        epoch: row.try_get("epoch")?,
    })
}

async fn begin_command<T: DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    scope: &str,
    idempotency_key: &str,
    payload: &Value,
) -> ScrimResult<CommandStart<T>> {
    let hash = payload_hash(payload)?;
    let inserted = sqlx::query_scalar::<_, i64>(
        "INSERT INTO scrim.command_receipts(\
             command_scope, idempotency_key, payload_hash, payload, state, lease_owner, lease_until\
         ) VALUES ($1, $2, $3, $4::jsonb, 'processing', $5, now() + interval '5 minutes') \
         ON CONFLICT(command_scope, idempotency_key, idempotency_generation) DO NOTHING \
         RETURNING id",
    )
    .bind(scope)
    .bind(idempotency_key)
    .bind(&hash)
    .bind(payload)
    .bind(COMMAND_LEASE_OWNER)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(id) = inserted {
        return Ok(CommandStart::New(id));
    }

    let row = sqlx::query(
        "SELECT id, payload_hash, state, result_payload \
           FROM scrim.command_receipts \
          WHERE command_scope = $1 \
            AND idempotency_key = $2 \
            AND idempotency_generation = 0 \
          FOR UPDATE",
    )
    .bind(scope)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(ScrimError::CommandInProgress)?;
    let existing_hash = row.try_get::<Vec<u8>, _>("payload_hash")?;
    if existing_hash != hash {
        return Err(ScrimError::IdempotencyConflict);
    }
    match row.try_get::<String, _>("state")?.as_str() {
        "completed" => {
            let payload = row
                .try_get::<Option<Value>, _>("result_payload")?
                .ok_or_else(|| {
                    ScrimError::InvalidStoredData(
                        "completed command receipt has no result_payload".to_string(),
                    )
                })?;
            let receipt = serde_json::from_value(payload).map_err(|error| {
                ScrimError::InvalidStoredData(format!("invalid command receipt result: {error}"))
            })?;
            Ok(CommandStart::Replay(receipt))
        }
        _ => Err(ScrimError::CommandInProgress),
    }
}

async fn complete_command<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    receipt_id: i64,
    receipt: &T,
) -> ScrimResult<()> {
    let result_payload = serde_json::to_value(receipt).map_err(|error| {
        ScrimError::InvalidStoredData(format!("receipt serialization failed: {error}"))
    })?;
    sqlx::query(
        "UPDATE scrim.command_receipts \
            SET state = 'completed', lease_owner = NULL, lease_until = NULL, \
                result_payload = $2::jsonb, updated_at = now(), completed_at = now() \
          WHERE id = $1",
    )
    .bind(receipt_id)
    .bind(result_payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_audit_event(
    tx: &mut Transaction<'_, Postgres>,
    event_type: &str,
    entity: (&str, i64),
    actor_user_id: &str,
    request_id: &str,
    correlation_id: &str,
    after_data: Value,
) -> ScrimResult<()> {
    sqlx::query(
        "INSERT INTO scrim.audit_events(\
             event_type, entity_type, entity_id, actor_type, actor_pseudonym, actor_source, \
             request_id, correlation_id, after_data\
         ) VALUES (\
             $1, $2, $3, 'user', scrim.audit_actor_pseudonym('user', $4), 'turniere', \
             $5, $6, $7::jsonb\
         )",
    )
    .bind(event_type)
    .bind(entity.0)
    .bind(entity.1.to_string())
    .bind(actor_user_id)
    .bind(request_id)
    .bind(correlation_id)
    .bind(after_data)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn placeholder_receipt() -> ActionReceipt {
    ActionReceipt {
        accepted: true,
        message: "Wird ausgeführt.".to_string(),
    }
}

fn validated_message(message: Option<&str>) -> ScrimResult<String> {
    match message.map(str::trim).filter(|message| !message.is_empty()) {
        Some(message) if message.chars().count() <= 2_000 => Ok(message.to_string()),
        Some(_) => Err(ScrimError::InvalidProposal(
            "Die Nachricht ist zu lang — höchstens 2000 Zeichen.".to_string(),
        )),
        None => Ok("Kurze Erinnerung: eure Rückmeldung zum Scrim fehlt noch.".to_string()),
    }
}

fn team_query_message(message_ids: &Value, team_id: i32) -> ScrimResult<(i64, i64)> {
    let entry = message_ids.get(team_id.to_string()).ok_or_else(|| {
        ScrimError::InvalidStoredData("Für dieses Team ist keine Nachricht hinterlegt.".to_string())
    })?;
    let parse = |name| {
        entry
            .get(name)
            .and_then(|value| {
                value.as_i64().or_else(|| {
                    value
                        .as_str()
                        .and_then(|value| value.parse::<i64>().ok())
                        .filter(|value| *value > 0)
                })
            })
            .ok_or_else(|| {
                ScrimError::InvalidStoredData(
                    "Die hinterlegte Nachricht ist unvollständig.".to_string(),
                )
            })
    };
    Ok((parse("channel_id")?, parse("message_id")?))
}

fn validate_optional_id(provided: Option<&str>, actual: Option<i32>) -> ScrimResult<()> {
    let Some(provided) = provided else {
        return Ok(());
    };
    let provided =
        crate::model::wire_id::parse_i32(provided).map_err(ScrimError::InvalidProposal)?;
    if Some(provided) == actual {
        Ok(())
    } else {
        Err(ScrimError::InvalidProposal(
            "Die angegebene ID passt nicht zum Datensatz.".to_string(),
        ))
    }
}

fn payload_hash<T: Serialize>(payload: &T) -> ScrimResult<Vec<u8>> {
    let bytes = serde_json::to_vec(payload).map_err(|error| {
        ScrimError::InvalidStoredData(format!("payload serialization failed: {error}"))
    })?;
    Ok(Sha256::digest(bytes).to_vec())
}

async fn lock_runtime_control(tx: &mut Transaction<'_, Postgres>) -> ScrimResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(RUNTIME_LOCK_NAMESPACE)
        .bind(RUNTIME_LOCK_KEY)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn lock_team_ids(
    tx: &mut Transaction<'_, Postgres>,
    team_ids: &BTreeSet<i32>,
) -> ScrimResult<()> {
    for team_id in team_ids {
        sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
            .bind(TEAM_LOCK_NAMESPACE)
            .bind(*team_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn lock_id_generation(tx: &mut Transaction<'_, Postgres>) -> ScrimResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(ID_LOCK_NAMESPACE)
        .bind(1_i32)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn next_id(tx: &mut Transaction<'_, Postgres>, table: &str) -> ScrimResult<i32> {
    let sql = match table {
        "scrim.match_request_batches" => {
            "SELECT (COALESCE(MAX(id), 0) + 1)::int4 FROM scrim.match_request_batches"
        }
        "scrim.match_requests" => {
            "SELECT (COALESCE(MAX(id), 0) + 1)::int4 FROM scrim.match_requests"
        }
        "scrim.matches" => "SELECT (COALESCE(MAX(id), 0) + 1)::int4 FROM scrim.matches",
        _ => {
            return Err(ScrimError::InvalidStoredData(
                "unknown ID table".to_string(),
            ))
        }
    };
    Ok(sqlx::query_scalar::<_, i32>(sql)
        .fetch_one(&mut **tx)
        .await?)
}

async fn existing_team_ids_tx(
    tx: &mut Transaction<'_, Postgres>,
    team_ids: &BTreeSet<i32>,
) -> ScrimResult<BTreeSet<i32>> {
    if team_ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let team_ids = team_ids.iter().copied().collect::<Vec<_>>();
    Ok(
        sqlx::query_scalar::<_, i32>("SELECT id FROM scrim.teams WHERE id = ANY($1)")
            .bind(team_ids)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .collect(),
    )
}

async fn active_request_team_ids_tx(
    tx: &mut Transaction<'_, Postgres>,
    team_ids: &BTreeSet<i32>,
) -> ScrimResult<BTreeSet<i32>> {
    if team_ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let team_ids = team_ids.iter().copied().collect::<Vec<_>>();
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT active.team_id \
           FROM (\
                 SELECT mr.team_a_id AS team_id \
                   FROM scrim.match_requests mr \
                   JOIN scrim.match_request_batches b ON b.id = mr.batch_id \
                  WHERE b.status = ANY($2) AND b.deadline_at > now() \
                 UNION \
                 SELECT mr.team_b_id AS team_id \
                   FROM scrim.match_requests mr \
                   JOIN scrim.match_request_batches b ON b.id = mr.batch_id \
                  WHERE mr.team_b_id IS NOT NULL \
                    AND b.status = ANY($2) AND b.deadline_at > now()\
           ) active \
          WHERE active.team_id = ANY($1)",
    )
    .bind(team_ids)
    .bind(ACTIVE_REQUEST_STATUSES)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect())
}

async fn load_roster_members_tx(
    tx: &mut Transaction<'_, Postgres>,
    team_ids: &[i32],
) -> ScrimResult<Vec<RosterMember>> {
    if team_ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query(
        "SELECT tm.team_id, tm.participant_id, p.display_name, tm.is_bench \
           FROM scrim.team_members tm \
           JOIN scrim.participants p ON p.id = tm.participant_id \
          WHERE tm.team_id = ANY($1) \
          ORDER BY tm.team_id ASC, tm.is_bench ASC, p.display_name ASC, tm.participant_id ASC",
    )
    .bind(team_ids)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| {
        Ok(RosterMember {
            team_id: row.try_get("team_id")?,
            participant_id: row.try_get("participant_id")?,
            display_name: row.try_get("display_name")?,
            is_bench: row.try_get("is_bench")?,
        })
    })
    .collect()
}

async fn load_match_request_responses_tx(
    tx: &mut Transaction<'_, Postgres>,
    request_id: i32,
) -> ScrimResult<Vec<MatchRequestResponse>> {
    sqlx::query(
        "SELECT request_id, team_id, participant_id, discord_user_id, slot_index, response, \
                source, message_id, channel_id, responded_at, updated_at \
           FROM scrim.match_request_responses \
          WHERE request_id = $1 \
          ORDER BY team_id ASC, participant_id ASC, slot_index ASC",
    )
    .bind(request_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| {
        Ok(MatchRequestResponse {
            request_id: row.try_get("request_id")?,
            team_id: row.try_get("team_id")?,
            participant_id: row.try_get("participant_id")?,
            discord_user_id: row.try_get::<i64, _>("discord_user_id")?.to_string(),
            slot_index: row.try_get("slot_index")?,
            response: parse_response_choice(row.try_get("response")?)?,
            source: row.try_get("source")?,
            message_id: snowflake(row.try_get("message_id")?),
            channel_id: snowflake(row.try_get("channel_id")?),
            responded_at: row.try_get("responded_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    })
    .collect()
}

fn selected_result(row: &sqlx::postgres::PgRow) -> ScrimResult<Option<SelectedMatchResult>> {
    let Some(result_ref_id) = row.try_get::<Option<i64>, _>("result_ref_id")? else {
        return Ok(None);
    };
    Ok(Some(SelectedMatchResult {
        result_ref_id,
        steam_match_id: row
            .try_get::<Option<i64>, _>("selected_steam_match_id")?
            .ok_or_else(|| {
                ScrimError::InvalidStoredData(
                    "selected result steam_match_id is missing".to_string(),
                )
            })?
            .to_string(),
        winner_team_id: row.try_get("selected_winner_team_id")?,
        source: row.try_get::<Option<String>, _>("source")?.ok_or_else(|| {
            ScrimError::InvalidStoredData("selected result source is missing".to_string())
        })?,
        selected_at: row
            .try_get::<Option<DateTime<Utc>>, _>("selected_at")?
            .ok_or_else(|| {
                ScrimError::InvalidStoredData("selected result selected_at is missing".to_string())
            })?,
        selected_by_user_id: row
            .try_get::<Option<String>, _>("selected_by_user_id")?
            .ok_or_else(|| {
                ScrimError::InvalidStoredData(
                    "selected result selected_by_user_id is missing".to_string(),
                )
            })?,
    }))
}

fn parse_weekly_availability(value: Option<Value>) -> Option<WeeklyAvailability> {
    value.and_then(|value| serde_json::from_value(value).ok())
}

fn posted_message_matches(
    message_ids: &Value,
    team_id: i32,
    channel_id: i64,
    message_id: Option<i64>,
) -> bool {
    message_ids.get(team_id.to_string()).is_some_and(|entry| {
        entry.get("channel_id").and_then(Value::as_i64) == Some(channel_id)
            && entry.get("message_id").and_then(Value::as_i64) == message_id
    })
}

fn template_to_db(template: MatchRequestTemplate) -> &'static str {
    match template {
        MatchRequestTemplate::RegularScrim => "regular_scrim",
        MatchRequestTemplate::Testmatch => "testmatch",
        MatchRequestTemplate::Training => "training",
    }
}

fn snowflake(value: Option<i64>) -> Option<String> {
    value.map(|id| id.to_string())
}

fn team_ref(id: Option<i32>, name: Option<String>) -> Option<TeamRef> {
    id.map(|id| TeamRef {
        id,
        name: name.unwrap_or_else(|| "-".to_string()),
    })
}

fn parse_template(value: String) -> ScrimResult<MatchRequestTemplate> {
    match value.as_str() {
        "regular_scrim" => Ok(MatchRequestTemplate::RegularScrim),
        "testmatch" => Ok(MatchRequestTemplate::Testmatch),
        "training" => Ok(MatchRequestTemplate::Training),
        _ => Err(ScrimError::InvalidStoredData(format!(
            "unknown match request template: {value}"
        ))),
    }
}

fn parse_response_choice(value: String) -> ScrimResult<ResponseChoice> {
    match value.as_str() {
        "available" => Ok(ResponseChoice::Available),
        "unavailable" => Ok(ResponseChoice::Unavailable),
        _ => Err(ScrimError::InvalidStoredData(format!(
            "unknown match request response: {value}"
        ))),
    }
}

fn parse_slots(value: Value) -> ScrimResult<Vec<ScrimSlot>> {
    serde_json::from_value(value)
        .map_err(|error| ScrimError::InvalidStoredData(format!("invalid slot_options: {error}")))
}

fn parse_optional_slot(value: Option<Value>) -> ScrimResult<Option<ScrimSlot>> {
    value
        .map(|slot| {
            serde_json::from_value(slot).map_err(|error| {
                ScrimError::InvalidStoredData(format!("invalid released_slot: {error}"))
            })
        })
        .transpose()
}

fn is_allowed_lagebild_evidence_url(value: &str) -> bool {
    let Some(path) = value.strip_prefix("https://discord.com/channels/") else {
        return false;
    };
    let segments = path.split('/').collect::<Vec<_>>();
    segments.len() == 3
        && segments[0] == MAIN_GUILD_ID
        && segments[1].parse::<u64>().is_ok_and(|id| id > 0)
        && segments[2].parse::<u64>().is_ok_and(|id| id > 0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        blocks_lobby_code_write, is_allowed_lagebild_evidence_url, role_resync, DiscordDispatch,
        MutationDispatch, RoleOperation, RoleSnapshot,
    };
    use crate::dto::ActionReceipt;

    /// Der Dispatch landet im Command-Receipt und wird beim Replay wieder herausgelesen.
    /// Geht dabei die Discord-Zustellung verloren, wird eine fehlgeschlagene DM nie
    /// nachgeholt — genau das soll dieser Rundlauf verhindern.
    #[test]
    fn dispatch_survives_the_receipt_round_trip() {
        let dispatch = MutationDispatch {
            receipt: ActionReceipt {
                accepted: true,
                message: "Wird ausgeführt.".to_string(),
            },
            discord: vec![DiscordDispatch {
                record_id: 7,
                user_id: Some(123),
                channel_id: None,
                content: "Erinnerung".to_string(),
            }],
        };

        let stored = serde_json::to_value(&dispatch).expect("dispatch serialisierbar");
        let replayed: MutationDispatch =
            serde_json::from_value(stored).expect("dispatch wieder lesbar");

        assert_eq!(replayed, dispatch);
        assert!(
            !replayed.discord.is_empty(),
            "Replay muss die offene Zustellung mitbringen"
        );
    }

    /// Ein falsch eingetippter Lobbycode muss korrigierbar bleiben.
    ///
    /// Das Setzen selbst schaltet den Zustand auf `lobby_open`. Zaehlte der als
    /// bot-gesteuert, waere jede Korrektur nach dem ersten Setzen gesperrt — und die
    /// mitgefuehrte Korrekturhistorie koennte nie greifen.
    #[test]
    fn lobby_code_stays_correctable_while_the_lobby_is_only_open() {
        assert!(!blocks_lobby_code_write("lobby_open"));
        assert!(!blocks_lobby_code_write("lobby_closed"));
    }

    /// Sobald der Bot die Lobby tatsaechlich fuehrt, faesst der Operator sie nicht mehr an.
    #[test]
    fn lobby_code_is_locked_once_the_bot_runs_the_lobby() {
        for state in [
            "start_requested",
            "starting",
            "lobby_posting",
            "in_progress",
            "finished",
            "result_requested",
            "result_fetching",
            "result_failed",
            "start_failed",
        ] {
            assert!(blocks_lobby_code_write(state), "{state} muss sperren");
        }
    }

    /// Der Resync-Knopf soll den Soll-Zustand herstellen, nicht nur ergaenzen.
    /// Ohne Remove behaelt ein Spieler nach einem Teamwechsel die alte Teamrolle
    /// und damit Zugriff auf den alten Team-Kanal.
    #[test]
    fn resync_removes_managed_roles_that_are_no_longer_wanted() {
        let snapshot = RoleSnapshot {
            subject: "7".to_string(),
            discord_user_id: Some(42),
            role_ids: BTreeSet::from([100, 200]),
        };
        let managed = BTreeSet::from([100, 200, 300, 400]);

        let plan = role_resync(&snapshot, &managed);

        let mut added = plan
            .actions
            .iter()
            .filter(|action| action.operation == RoleOperation::Add)
            .map(|action| action.role_id)
            .collect::<Vec<_>>();
        let mut removed = plan
            .actions
            .iter()
            .filter(|action| action.operation == RoleOperation::Remove)
            .map(|action| action.role_id)
            .collect::<Vec<_>>();
        added.sort_unstable();
        removed.sort_unstable();

        assert_eq!(added, vec![100, 200], "Soll-Rollen werden gesetzt");
        assert_eq!(
            removed,
            vec![300, 400],
            "fremde verwaltete Rollen fliegen raus"
        );
    }

    /// Nur verwaltete Rollen anfassen: alles andere am Discord-Mitglied bleibt unberuehrt.
    #[test]
    fn resync_never_touches_roles_outside_the_managed_set() {
        let snapshot = RoleSnapshot {
            subject: "7".to_string(),
            discord_user_id: Some(42),
            role_ids: BTreeSet::from([100]),
        };
        let managed = BTreeSet::from([100, 200]);

        let plan = role_resync(&snapshot, &managed);

        assert!(plan
            .actions
            .iter()
            .all(|action| managed.contains(&action.role_id)));
    }

    #[test]
    fn lagebild_evidence_urls_are_limited_to_main_guild_message_links() {
        assert!(is_allowed_lagebild_evidence_url(
            "https://discord.com/channels/1289721245281292288/100/200"
        ));
        assert!(!is_allowed_lagebild_evidence_url(
            "https://discord.com/channels/1/100/200"
        ));
        assert!(!is_allowed_lagebild_evidence_url(
            "https://discord.com/channels/1289721245281292288/100/200?redirect=1"
        ));
        assert!(!is_allowed_lagebild_evidence_url(
            "https://example.com/channels/1289721245281292288/100/200"
        ));
    }
}
