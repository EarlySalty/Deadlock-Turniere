use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};

use turnier_db::Pool;

use crate::decision::derive_match_request_facts;
use crate::dto::{
    ActionReceipt, MatchRequestAction, MatchRequestResponseRequest, ReleaseMatchRequest,
    SelfServiceParticipant, SignupRequest, WeeklyAvailability as SelfServiceAvailability,
};
use crate::model::{
    AvailabilitySlot, AvailabilityStatus, Coach, LagebildEvidenceRef, LagebildSnapshotRef,
    MatchRequest, MatchRequestBatch, MatchRequestResponse, MatchRequestTemplate, Participant,
    ResponseChoice, RosterMember, ScrimMatch, ScrimReadModel, ScrimSlot, SelectedMatchResult, Team,
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

impl PgScrimReadRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn runtime_control(&self) -> ScrimResult<RuntimeControl> {
        load_runtime_control(&self.pool).await
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
            .map_err(|_| ScrimError::InvalidProposal("Platzhalter".to_string()))?;
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
                "UPDATE scrim.participants \
                    SET display_name=$2, rank=$3, roles=$4, availability=$5, \
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
                "UPDATE scrim.participants \
                    SET discord_id=$2, rank=$3, roles=$4, availability=$5, \
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
        let availability_slots = serde_json::to_value(availability)
            .map_err(|_| ScrimError::InvalidProposal("Platzhalter".to_string()))?;
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
            return Err(ScrimError::NotFound("Platzhalter".to_string()));
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
                *entity_id,
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
            request_id,
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
            request_id,
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

enum CommandStart {
    New(i64),
    Replay(ActionReceipt),
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

async fn begin_command(
    tx: &mut Transaction<'_, Postgres>,
    scope: &str,
    idempotency_key: &str,
    payload: &Value,
) -> ScrimResult<CommandStart> {
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

async fn complete_command(
    tx: &mut Transaction<'_, Postgres>,
    receipt_id: i64,
    receipt: &ActionReceipt,
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
    entity_id: i32,
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
    .bind("match_request")
    .bind(entity_id.to_string())
    .bind(actor_user_id)
    .bind(request_id)
    .bind(correlation_id)
    .bind(after_data)
    .execute(&mut **tx)
    .await?;
    Ok(())
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
    use super::is_allowed_lagebild_evidence_url;

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
