use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::decision::{is_scrim_history_entry, validate_match_request_batch};
use crate::model::{
    MatchRequestBatchInput, ScrimMatch, ScrimReadModel, ValidatedMatchRequestBatch,
};
use crate::repository::ScrimReadRepository;
use crate::{ScrimError, ScrimResult};

pub struct ScrimService<R> {
    repository: R,
}

impl<R> ScrimService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &R {
        &self.repository
    }
}

impl<R: ScrimReadRepository> ScrimService<R> {
    pub async fn read_model(&self) -> ScrimResult<ScrimReadModel> {
        self.repository.read_model().await
    }

    pub async fn history(&self) -> ScrimResult<Vec<ScrimMatch>> {
        Ok(self
            .read_model()
            .await?
            .matches
            .into_iter()
            .filter(|scrim_match| {
                is_scrim_history_entry(
                    &scrim_match.status,
                    scrim_match.lobby_state.as_deref(),
                    scrim_match.selected_result.is_some(),
                )
            })
            .collect())
    }

    pub async fn authorize_operator(&self, actor_discord_id: &str) -> ScrimResult<()> {
        let actor_discord_id = parse_discord_id(actor_discord_id)?;
        if self.repository.is_active_coach(actor_discord_id).await? {
            Ok(())
        } else {
            Err(ScrimError::CoachUnauthorized)
        }
    }

    pub async fn validate_batch(
        &self,
        input: &MatchRequestBatchInput,
        now: DateTime<Utc>,
    ) -> ScrimResult<ValidatedMatchRequestBatch> {
        let initially_validated = validate_match_request_batch(input, now, &BTreeSet::new())?;
        let team_ids = initially_validated
            .matches
            .iter()
            .flat_map(|pairing| [Some(pairing.team_a_id), pairing.team_b_id])
            .flatten()
            .collect::<BTreeSet<_>>();
        if self.repository.existing_team_ids(&team_ids).await? != team_ids {
            return Err(ScrimError::InvalidProposal(
                "At least one team was not found".to_string(),
            ));
        }
        let active_team_ids = self.repository.active_request_team_ids(&team_ids).await?;
        if active_team_ids.is_empty() {
            Ok(initially_validated)
        } else {
            validate_match_request_batch(input, now, &active_team_ids)
        }
    }
}

fn parse_discord_id(value: &str) -> ScrimResult<i64> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ScrimError::InvalidActor);
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or(ScrimError::InvalidActor)
}
