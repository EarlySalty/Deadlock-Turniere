//! Phase-1a-Tests der Turnier-Automatik gegen frische Wegwerf-PG-DBs.

use turnier_automatik::optout::{self, Scope};
use turnier_automatik::presets::{self, Category, NewPreset, PresetConfig, PresetUpdate};
use turnier_automatik::proposals::{
    self, ProposalEvent, ProposalSource, ProposalState, VoteDecision,
};
use turnier_automatik::signals::{self, SignalSnapshotInput};
use turnier_automatik::AutomatikError;
use turnier_core::{BracketFormat, InviteMode, TournamentGameMode, TournamentMode};
use turnier_db::{test_pool, Pool, TestDb};

async fn temp_db() -> TestDb {
    test_pool().await.expect("central test pool")
}

fn sample_config() -> PresetConfig {
    PresetConfig {
        team_size: 6,
        bracket_format: BracketFormat::SingleElimination,
        series_format: 1,
        final_series_format: Some(3),
        tournament_mode: TournamentMode::GroupStage,
        tournament_game_mode: TournamentGameMode::Standard,
        match_objective: "auto".to_string(),
        invite_mode: InviteMode::Always,
        reminder_offsets: Some("[1440,120,15]".to_string()),
        start_reminder_offsets: Some("[1440,60]".to_string()),
        rules: Some("rules".to_string()),
        description_template: Some("description".to_string()),
    }
}

#[tokio::test]
async fn presets_crud_roundtrip() {
    let db = temp_db().await;
    let pool = db.pool();
    let created = presets::create(
        pool,
        &NewPreset {
            name: "Fun Freitag".to_string(),
            category: Category::Fun,
            config: sample_config(),
            active: true,
            created_by: "123456789012345600".to_string(),
        },
    )
    .await
    .unwrap();

    assert_eq!(created.name, "Fun Freitag");
    assert_eq!(created.category, Category::Fun);
    assert!(created.active);
    assert_eq!(created.created_by, "123456789012345600");
    assert_eq!(created.reminder_offsets.as_deref(), Some("[1440,120,15]"));
    assert_eq!(created.start_reminder_offsets.as_deref(), Some("[1440,60]"));

    let listed = presets::list(pool).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], created);

    let active_fun = presets::list_active_by_category(pool, Category::Fun)
        .await
        .unwrap();
    assert_eq!(active_fun.len(), 1);

    let mut updated_config = sample_config();
    updated_config.team_size = 5;
    updated_config.bracket_format = BracketFormat::DoubleElimination;
    updated_config.tournament_mode = TournamentMode::BracketOnly;
    updated_config.description_template = Some("updated".to_string());
    let updated = presets::update(
        pool,
        created.id,
        &PresetUpdate {
            name: "Comp Sonntag".to_string(),
            category: Category::Comp,
            config: updated_config,
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(updated.name, "Comp Sonntag");
    assert_eq!(updated.category, Category::Comp);
    assert_eq!(updated.team_size, 5);
    assert_eq!(updated.bracket_format, BracketFormat::DoubleElimination);
    assert_eq!(updated.description_template.as_deref(), Some("updated"));

    assert!(presets::set_active(pool, created.id, false).await.unwrap());
    let inactive = presets::get(pool, created.id).await.unwrap().unwrap();
    assert!(!inactive.active);
    assert!(presets::list_active_by_category(pool, Category::Comp)
        .await
        .unwrap()
        .is_empty());

    assert!(presets::delete(pool, created.id).await.unwrap());
    assert!(presets::get(pool, created.id).await.unwrap().is_none());
}

#[test]
fn proposal_state_transitions_valid_and_invalid() {
    let valid = [
        (
            ProposalState::Draft,
            ProposalEvent::SubmitForApproval,
            ProposalState::PendingApproval,
        ),
        (
            ProposalState::PendingApproval,
            ProposalEvent::Approve,
            ProposalState::Approved,
        ),
        (
            ProposalState::PendingApproval,
            ProposalEvent::Reject,
            ProposalState::Rejected,
        ),
        (
            ProposalState::PendingApproval,
            ProposalEvent::Expire,
            ProposalState::Expired,
        ),
        (
            ProposalState::PendingApproval,
            ProposalEvent::Feedback,
            ProposalState::Draft,
        ),
    ];
    for (state, event, expected) in valid {
        assert_eq!(proposals::transition(state, event).unwrap(), expected);
    }

    let invalid = [
        (ProposalState::Draft, ProposalEvent::Approve),
        (ProposalState::Draft, ProposalEvent::Reject),
        (ProposalState::Draft, ProposalEvent::Expire),
        (ProposalState::Draft, ProposalEvent::Feedback),
        (
            ProposalState::PendingApproval,
            ProposalEvent::SubmitForApproval,
        ),
    ];
    for (state, event) in invalid {
        let err = proposals::transition(state, event).unwrap_err();
        assert!(matches!(err, AutomatikError::InvalidTransition { .. }));
    }

    let all_events = [
        ProposalEvent::SubmitForApproval,
        ProposalEvent::Approve,
        ProposalEvent::Reject,
        ProposalEvent::Expire,
        ProposalEvent::Feedback,
    ];
    for terminal in [
        ProposalState::Approved,
        ProposalState::Rejected,
        ProposalState::Expired,
    ] {
        for event in all_events {
            let err = proposals::transition(terminal, event).unwrap_err();
            assert!(matches!(err, AutomatikError::InvalidTransition { .. }));
        }
    }
}

#[tokio::test]
async fn proposals_votes_feedback_and_state_roundtrip() {
    let db = temp_db().await;
    let pool = db.pool();
    let proposal_id = proposals::create_proposal(
        pool,
        None,
        ProposalSource::Bot,
        Some("2026-07-10T18:00:00Z"),
        r#"{"name":"Auto Cup"}"#,
    )
    .await
    .unwrap();

    let proposal = proposals::get_proposal(pool, proposal_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(proposal.state, ProposalState::Draft);
    assert_eq!(proposal.source, ProposalSource::Bot);

    proposals::record_vote(
        pool,
        proposal_id,
        "123456789012345601",
        VoteDecision::Approve,
    )
    .await
    .unwrap();
    assert_eq!(
        proposals::approvals_count(pool, proposal_id).await.unwrap(),
        1
    );

    proposals::record_vote(
        pool,
        proposal_id,
        "123456789012345601",
        VoteDecision::Reject,
    )
    .await
    .unwrap();
    assert_eq!(
        proposals::approvals_count(pool, proposal_id).await.unwrap(),
        0
    );
    assert_eq!(
        proposals::list_votes(pool, proposal_id)
            .await
            .unwrap()
            .len(),
        1
    );

    proposals::record_vote(
        pool,
        proposal_id,
        "123456789012345602",
        VoteDecision::Approve,
    )
    .await
    .unwrap();
    assert_eq!(
        proposals::approvals_count(pool, proposal_id).await.unwrap(),
        1
    );
    proposals::record_vote(
        pool,
        proposal_id,
        "123456789012345603",
        VoteDecision::Approve,
    )
    .await
    .unwrap();

    let feedback_id = proposals::record_feedback(
        pool,
        proposal_id,
        "123456789012345601",
        "Bitte eine Stunde spaeter",
        Some(r#"{"proposed_start":"+1h"}"#),
    )
    .await
    .unwrap();
    assert!(feedback_id > 0);
    let feedback = proposals::list_feedback(pool, proposal_id).await.unwrap();
    assert_eq!(feedback.len(), 1);
    assert_eq!(feedback[0].raw_text, "Bitte eine Stunde spaeter");
    let learned = proposals::list_recent_feedback(pool, 20).await.unwrap();
    assert_eq!(learned.len(), 1);
    assert_eq!(learned[0].raw_text, "Bitte eine Stunde spaeter");

    assert_eq!(
        proposals::apply_event(pool, proposal_id, ProposalEvent::SubmitForApproval)
            .await
            .unwrap(),
        ProposalState::PendingApproval
    );
    let pending = proposals::get_proposal(pool, proposal_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.state, ProposalState::PendingApproval);
    assert!(pending.decided_at.is_none());

    assert_eq!(
        proposals::apply_event(pool, proposal_id, ProposalEvent::Approve)
            .await
            .unwrap(),
        ProposalState::Approved
    );
    let approved = proposals::get_proposal(pool, proposal_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(approved.state, ProposalState::Approved);
    assert!(approved.decided_at.is_some());
}

#[tokio::test]
async fn list_proposals_filters_state_and_orders_desc() {
    let db = temp_db().await;
    let pool = db.pool();
    let first_id = proposals::create_proposal(
        pool,
        None,
        ProposalSource::Bot,
        Some("2026-07-10T18:00:00Z"),
        r#"{"name":"First"}"#,
    )
    .await
    .unwrap();
    let second_id = proposals::create_proposal(
        pool,
        None,
        ProposalSource::Manual,
        Some("2026-07-11T18:00:00Z"),
        r#"{"name":"Second"}"#,
    )
    .await
    .unwrap();

    proposals::apply_event(pool, first_id, ProposalEvent::SubmitForApproval)
        .await
        .unwrap();

    let all = proposals::list_proposals(pool, None).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, second_id);
    assert_eq!(all[1].id, first_id);

    let drafts = proposals::list_proposals(pool, Some(ProposalState::Draft))
        .await
        .unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].id, second_id);
}

#[tokio::test]
async fn apply_event_rejects_invalid_transition_without_persisting() {
    let db = temp_db().await;
    let pool = db.pool();
    let proposal_id = proposals::create_proposal(
        pool,
        None,
        ProposalSource::Manual,
        None,
        r#"{"name":"Manual Cup"}"#,
    )
    .await
    .unwrap();

    let err = proposals::apply_event(pool, proposal_id, ProposalEvent::Approve)
        .await
        .unwrap_err();
    assert!(matches!(err, AutomatikError::InvalidTransition { .. }));

    let proposal = proposals::get_proposal(pool, proposal_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(proposal.state, ProposalState::Draft);
    assert!(proposal.decided_at.is_none());
}

#[tokio::test]
async fn apply_event_requires_approval_vote_before_approved() {
    let db = temp_db().await;
    let pool = db.pool();
    let proposal_id = proposals::create_proposal(
        pool,
        None,
        ProposalSource::Manual,
        None,
        r#"{"name":"Manual Cup"}"#,
    )
    .await
    .unwrap();

    proposals::apply_event(pool, proposal_id, ProposalEvent::SubmitForApproval)
        .await
        .unwrap();

    assert!(
        proposals::apply_event(pool, proposal_id, ProposalEvent::Approve)
            .await
            .is_err()
    );

    let proposal = proposals::get_proposal(pool, proposal_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(proposal.state, ProposalState::PendingApproval);
    assert!(proposal.decided_at.is_none());

    proposals::record_vote(
        pool,
        proposal_id,
        "123456789012345601",
        VoteDecision::Approve,
    )
    .await
    .unwrap();
    assert!(
        proposals::apply_event(pool, proposal_id, ProposalEvent::Approve)
            .await
            .is_err()
    );

    proposals::record_vote(
        pool,
        proposal_id,
        "123456789012345602",
        VoteDecision::Approve,
    )
    .await
    .unwrap();
    assert_eq!(
        proposals::apply_event(pool, proposal_id, ProposalEvent::Approve)
            .await
            .unwrap(),
        ProposalState::Approved
    );
}

#[tokio::test]
async fn revision_expires_old_proposal_and_starts_without_votes() {
    let db = temp_db().await;
    let pool = db.pool();
    let proposal_id = proposals::create_proposal(
        pool,
        None,
        ProposalSource::Bot,
        Some("2026-07-10T18:00:00Z"),
        r#"{"name":"Alt","revision":1}"#,
    )
    .await
    .unwrap();
    proposals::apply_event(pool, proposal_id, ProposalEvent::SubmitForApproval)
        .await
        .unwrap();
    proposals::record_vote(
        pool,
        proposal_id,
        "123456789012345601",
        VoteDecision::Approve,
    )
    .await
    .unwrap();

    let revised_id = proposals::create_revision(
        pool,
        proposal_id,
        "123456789012345601",
        "Eine Stunde spaeter",
        r#"{"name":"Neu","revision":2}"#,
    )
    .await
    .unwrap();

    assert_eq!(
        proposals::get_proposal(pool, proposal_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        ProposalState::Expired
    );
    let revised = proposals::get_proposal(pool, revised_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revised.state, ProposalState::PendingApproval);
    assert_eq!(
        revised.proposed_start.as_deref(),
        Some("2026-07-10T18:00:00+00:00")
    );
    assert_eq!(
        proposals::approvals_count(pool, revised_id).await.unwrap(),
        0
    );
    assert_eq!(
        proposals::list_feedback(pool, proposal_id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn prepared_revision_keeps_old_live_until_discord_activation() {
    let db = temp_db().await;
    let pool = db.pool();
    let proposal_id = proposals::create_proposal(
        pool,
        None,
        ProposalSource::Bot,
        Some("2026-07-10T18:00:00Z"),
        r#"{"name":"Alt","revision":1}"#,
    )
    .await
    .unwrap();
    proposals::apply_event(pool, proposal_id, ProposalEvent::SubmitForApproval)
        .await
        .unwrap();
    proposals::record_vote(
        pool,
        proposal_id,
        "123456789012345601",
        VoteDecision::Approve,
    )
    .await
    .unwrap();

    let revised_id =
        proposals::prepare_revision(pool, proposal_id, r#"{"name":"Neu","revision":2}"#)
            .await
            .unwrap();
    assert_eq!(
        proposals::get_proposal(pool, proposal_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        ProposalState::PendingApproval
    );
    assert_eq!(
        proposals::get_proposal(pool, revised_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        ProposalState::Draft
    );

    proposals::activate_prepared_revision(
        pool,
        proposal_id,
        revised_id,
        "123456789012345602",
        "Eine Stunde später",
        "1474543558793887937",
        "1474543558793887999",
    )
    .await
    .unwrap();
    assert_eq!(
        proposals::get_proposal(pool, proposal_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        ProposalState::Expired
    );
    let revised = proposals::get_proposal(pool, revised_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revised.state, ProposalState::PendingApproval);
    assert_eq!(
        revised.proposal_message_id.as_deref(),
        Some("1474543558793887999")
    );
    assert_eq!(
        proposals::approvals_count(pool, revised_id).await.unwrap(),
        0
    );
    assert_eq!(
        proposals::resolve_active_proposal_id(pool, proposal_id)
            .await
            .unwrap(),
        Some(revised_id)
    );
}

#[tokio::test]
async fn prepared_revision_rejects_parallel_draft_without_deleting_first() {
    let db = temp_db().await;
    let pool = db.pool();
    let proposal_id = proposals::create_proposal(
        pool,
        None,
        ProposalSource::Bot,
        Some("2026-07-10T18:00:00Z"),
        r#"{"name":"Alt","revision":1}"#,
    )
    .await
    .unwrap();
    proposals::apply_event(pool, proposal_id, ProposalEvent::SubmitForApproval)
        .await
        .unwrap();

    let first_revision =
        proposals::prepare_revision(pool, proposal_id, r#"{"name":"Erster Entwurf"}"#)
            .await
            .unwrap();
    let error =
        proposals::prepare_revision(pool, proposal_id, r#"{"name":"Zweiter Entwurf"}"#)
            .await
            .unwrap_err();

    assert!(matches!(error, AutomatikError::RevisionInProgress));
    let first = proposals::get_proposal(pool, first_revision)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.state, ProposalState::Draft);
}

#[test]
fn compute_recipients_filters_category_all_and_none() {
    let role_members = vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
        "e".to_string(),
    ];
    let optouts = vec![
        ("b".to_string(), Scope::Fun),
        ("c".to_string(), Scope::Comp),
        ("d".to_string(), Scope::All),
    ];

    assert_eq!(
        optout::compute_recipients(&role_members, &optouts, Category::Fun),
        vec!["a".to_string(), "c".to_string(), "e".to_string()]
    );
    assert_eq!(
        optout::compute_recipients(&role_members, &optouts, Category::Comp),
        vec!["a".to_string(), "b".to_string(), "e".to_string()]
    );
    assert_eq!(
        optout::compute_recipients(&role_members, &[], Category::Fun),
        role_members
    );
}

#[tokio::test]
async fn optout_set_clear_and_is_opted_out() {
    let db = temp_db().await;
    let pool = db.pool();

    assert!(
        !optout::is_opted_out(pool, "123456789012345603", Category::Fun)
            .await
            .unwrap()
    );
    optout::set_optout(pool, "123456789012345603", Scope::Fun)
        .await
        .unwrap();
    assert!(
        optout::is_opted_out(pool, "123456789012345603", Category::Fun)
            .await
            .unwrap()
    );
    assert!(
        !optout::is_opted_out(pool, "123456789012345603", Category::Comp)
            .await
            .unwrap()
    );

    optout::set_optout(pool, "123456789012345603", Scope::All)
        .await
        .unwrap();
    assert!(
        optout::is_opted_out(pool, "123456789012345603", Category::Comp)
            .await
            .unwrap()
    );
    assert_eq!(
        optout::list_optouts(pool, "123456789012345603")
            .await
            .unwrap()
            .len(),
        2
    );

    assert!(optout::clear_optout(pool, "123456789012345603", Scope::Fun)
        .await
        .unwrap());
    assert!(
        optout::is_opted_out(pool, "123456789012345603", Category::Fun)
            .await
            .unwrap(),
        "globaler Opt-out bleibt aktiv"
    );
    assert!(optout::clear_optout(pool, "123456789012345603", Scope::All)
        .await
        .unwrap());
    assert!(
        !optout::is_opted_out(pool, "123456789012345603", Category::Fun)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn signals_snapshot_roundtrip() {
    let db = temp_db().await;
    let pool = db.pool();
    let tournament_id = seed_tournament(pool).await;

    let input = SignalSnapshotInput {
        participants: Some(24),
        teams: Some(4),
        no_shows: Some(1),
        poll_up: Some(9),
        poll_down: Some(2),
        poll_message_id: Some("123456789012345604".to_string()),
        feedback_summary: Some("lief gut".to_string()),
        collected_at: Some("2026-07-01T12:00:00Z".to_string()),
    };
    let snapshot = signals::snapshot_signals(pool, tournament_id, &input)
        .await
        .unwrap();
    assert_eq!(snapshot.tournament_id, tournament_id);
    assert_eq!(snapshot.participants, Some(24));
    assert_eq!(snapshot.teams, Some(4));
    assert_eq!(snapshot.no_shows, Some(1));
    assert_eq!(snapshot.poll_up, Some(9));
    assert_eq!(snapshot.poll_down, Some(2));
    assert_eq!(
        snapshot.poll_message_id.as_deref(),
        Some("123456789012345604")
    );
    assert_eq!(snapshot.feedback_summary.as_deref(), Some("lief gut"));
    assert_eq!(snapshot.collected_at, "2026-07-01T12:00:00+00:00");

    let loaded = signals::get_signal(pool, snapshot.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded, snapshot);

    let all = signals::list_for_tournament(pool, tournament_id)
        .await
        .unwrap();
    assert_eq!(all, vec![snapshot]);
}

async fn seed_tournament(pool: &Pool) -> i64 {
    let now = chrono::Utc::now();
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO turnier.tournaments \
             (name, status, team_size, bracket_format, created_by, created_at, updated_at, \
              invite_mode, tournament_mode, series_format, exclude_from_leaderboard, \
              tournament_game_mode, auto_lobby_enabled, is_test, match_objective, \
              no_show_grace_minutes, source) \
         VALUES ('Signal Cup', 'completed', 6, 'single_elimination', $1, $2, $3, \
                 'always', 'bracket_only', 1, false, 'standard', false, true, \
                 'auto', 10, 'manual') \
         RETURNING id",
    )
    .bind(123456789012345605_i64)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}
