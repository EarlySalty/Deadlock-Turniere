use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use turnier_scrim::model::ScrimReadModel;
use turnier_scrim::repository::ScrimReadRepository;
use turnier_scrim::service::ScrimService;
use turnier_scrim::ScrimResult;

struct FakeReadRepository {
    read_calls: AtomicUsize,
    coach_calls: AtomicUsize,
    active_coach: bool,
}

impl FakeReadRepository {
    fn new(active_coach: bool) -> Self {
        Self {
            read_calls: AtomicUsize::new(0),
            coach_calls: AtomicUsize::new(0),
            active_coach,
        }
    }
}

#[async_trait]
impl ScrimReadRepository for FakeReadRepository {
    async fn read_model(&self) -> ScrimResult<ScrimReadModel> {
        self.read_calls.fetch_add(1, Ordering::SeqCst);
        Ok(ScrimReadModel {
            participants: Vec::new(),
            teams: Vec::new(),
            matches: Vec::new(),
            match_request_batches: Vec::new(),
            lagebild_refs: Vec::new(),
        })
    }

    async fn coaches(&self) -> ScrimResult<Vec<turnier_scrim::model::Coach>> {
        Ok(Vec::new())
    }

    async fn is_active_coach(&self, _discord_id: i64) -> ScrimResult<bool> {
        self.coach_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.active_coach)
    }

    async fn existing_team_ids(&self, team_ids: &BTreeSet<i32>) -> ScrimResult<BTreeSet<i32>> {
        Ok(team_ids.clone())
    }

    async fn active_request_team_ids(
        &self,
        _team_ids: &BTreeSet<i32>,
    ) -> ScrimResult<BTreeSet<i32>> {
        Ok(BTreeSet::new())
    }
}

#[tokio::test]
async fn reads_use_repository_without_runtime_gate() {
    let service = ScrimService::new(FakeReadRepository::new(true));
    assert!(service.read_model().await.is_ok());
    assert_eq!(service.repository().read_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn operator_authorization_uses_the_repository_and_rejects_bad_ids() {
    let active = ScrimService::new(FakeReadRepository::new(true));
    assert!(active.authorize_operator("123456789").await.is_ok());
    assert_eq!(active.repository().coach_calls.load(Ordering::SeqCst), 1);
    assert!(active.authorize_operator("not-a-snowflake").await.is_err());
    assert_eq!(active.repository().coach_calls.load(Ordering::SeqCst), 1);

    let inactive = ScrimService::new(FakeReadRepository::new(false));
    assert!(inactive.authorize_operator("123456789").await.is_err());
}
