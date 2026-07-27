#![cfg(feature = "testing")]

use sqlx::Row;
use turnier_scrim::dto::{
    AnnouncementPublicationRequest, CreateMatchRequest, LobbyCodeRequest, MatchIdPatchRequest,
    MatchIdsRequest, ResultFetchRequest,
};
use turnier_scrim::repository::PgScrimReadRepository;
use turnier_scrim::service::ScrimService;

#[tokio::test]
async fn match_operator_mutations_keep_the_dashboard_database_contract() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    sqlx::query(
        "INSERT INTO scrim.teams(id, name, created_at) VALUES \
         (820101, 'Operator A', now()), (820102, 'Operator B', now())",
    )
    .execute(db.pool())
    .await
    .expect("teams");
    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));

    let created = service
        .create_match(
            "match:create",
            CreateMatchRequest {
                team_a_id: Some("820101".to_string()),
                team_b_id: Some("820102".to_string()),
                match_request_id: None,
                scheduled_at: None,
                note: None,
                coach_spectator_discord_id: None,
            },
        )
        .await
        .expect("create match");
    let match_id = created.scrim_match.id;
    assert_eq!(created.scrim_match.lobby_state.as_deref(), Some("draft"));

    let lobby = service
        .set_lobby_code(
            "match:lobby",
            match_id,
            "123456789",
            "Coach",
            LobbyCodeRequest {
                lobby_code: "a1b2c".to_string(),
            },
        )
        .await
        .expect("set lobby code");
    assert_eq!(lobby.scrim_match.join_code.as_deref(), Some("A1B2C"));
    service
        .repository()
        .begin_lobby_code_delivery(&lobby)
        .await
        .expect("prepare lobby code delivery")
        .expect("current lobby code delivery")
        .finish()
        .await
        .expect("finish lobby code delivery");

    service
        .add_match_ids(
            "match:ids",
            match_id,
            "123456789",
            "Coach",
            MatchIdsRequest {
                match_ids: vec![
                    "9007199254740201".to_string(),
                    "9007199254740202".to_string(),
                ],
            },
        )
        .await
        .expect("add match IDs");
    let refs = sqlx::query(
        "UPDATE scrim.match_result_refs \
            SET fetch_status='fetched', validation_status='valid', winner_team_id=820101, \
                normalized_result_json='{\"winner\":\"team_a\"}'::jsonb, fetched_at=now(), updated_at=now() \
          WHERE match_id=$1 RETURNING id",
    )
    .bind(match_id)
    .fetch_all(db.pool())
    .await
    .expect("result refs");

    sqlx::query("UPDATE scrim.matches SET lobby_state='in_progress' WHERE id=$1")
        .bind(match_id)
        .execute(db.pool())
        .await
        .expect("result-fetch state");
    let first_ref_id = refs[0].get::<i64, _>("id");
    let fetch = service
        .request_result_fetch(
            "match:fetch",
            match_id,
            ResultFetchRequest {
                match_id_ref: Some(first_ref_id.to_string()),
                ..ResultFetchRequest::default()
            },
        )
        .await
        .expect("result fetch");
    assert_eq!(fetch.lobby_state, "result_requested");
    sqlx::query(
        "UPDATE scrim.match_result_refs \
            SET fetch_status='fetched', validation_status='valid', winner_team_id=820101, \
                normalized_result_json='{\"winner\":\"team_a\"}'::jsonb, fetched_at=now(), updated_at=now() \
          WHERE match_id=$1",
    )
    .bind(match_id)
    .execute(db.pool())
    .await
    .expect("restore fetched refs");

    for row in &refs {
        service
            .select_result_ref(
                &format!("match:select:{}", row.get::<i64, _>("id")),
                match_id,
                row.get("id"),
                "123456789",
                "Coach",
                MatchIdPatchRequest {
                    message: "wrong winner".to_string(),
                },
            )
            .await
            .expect("select result");
    }
    let selection_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM scrim.match_result_selections WHERE match_id=$1")
            .bind(match_id)
            .fetch_one(db.pool())
            .await
            .expect("selection count");
    assert_eq!(selection_count, 1);
    let selection_reason: String = sqlx::query_scalar(
        "SELECT selection_reason FROM scrim.match_result_selections WHERE match_id=$1",
    )
    .bind(match_id)
    .fetch_one(db.pool())
    .await
    .expect("selection reason");
    assert_eq!(selection_reason, "wrong_winner");
}

#[tokio::test]
async fn lobby_code_replay_remains_eligible_for_distribution() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    sqlx::query(
        "INSERT INTO scrim.teams(id, name, created_at) VALUES \
         (820101, 'Operator A', now()), (820102, 'Operator B', now())",
    )
    .execute(db.pool())
    .await
    .expect("teams");
    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));
    let created = service
        .create_match(
            "match:create",
            CreateMatchRequest {
                team_a_id: Some("820101".to_string()),
                team_b_id: Some("820102".to_string()),
                match_request_id: None,
                scheduled_at: None,
                note: None,
                coach_spectator_discord_id: None,
            },
        )
        .await
        .expect("create match");
    let request = LobbyCodeRequest {
        lobby_code: "a1b2c".to_string(),
    };

    service
        .set_lobby_code(
            "match:lobby",
            created.scrim_match.id,
            "123456789",
            "Coach",
            request.clone(),
        )
        .await
        .expect("set lobby code");
    let replay = service
        .set_lobby_code(
            "match:lobby",
            created.scrim_match.id,
            "123456789",
            "Coach",
            request,
        )
        .await
        .expect("replay lobby code");

    service
        .repository()
        .begin_lobby_code_delivery(&replay)
        .await
        .expect("prepare replay delivery")
        .expect("current replay delivery")
        .finish()
        .await
        .expect("finish replay delivery");
}

#[tokio::test]
async fn announcement_preview_is_read_only_and_actions_use_wire_ids() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scrim.announcement_drafts")
        .fetch_one(db.pool())
        .await
        .expect("draft count");
    let preview = service
        .announcement_preview("two_week_scrim_block")
        .await
        .expect("preview");
    assert!(preview.id.is_none());
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scrim.announcement_drafts")
        .fetch_one(db.pool())
        .await
        .expect("draft count");
    assert_eq!(after, before);

    let publication = service
        .create_announcement_publication(
            "two_week_scrim_block",
            "announcement:contract",
            "123456789",
            "Coach",
            AnnouncementPublicationRequest {
                title: Some("Scrim-Woche 31".to_string()),
                channel_id: Some("9007199254740302".to_string()),
                message: "Die Scrims für Woche 31 stehen fest.".to_string(),
            },
        )
        .await
        .expect("publication");
    assert_eq!(publication.status, "approved");

    let action_id: i64 = sqlx::query_scalar(
        "INSERT INTO scrim.command_receipts(\
             command_scope, idempotency_key, payload_hash, payload, state, completed_at\
         ) VALUES ('operator_contract', 'action:contract', $1, '{}'::jsonb, 'completed', now()) \
         RETURNING id",
    )
    .bind(vec![9_u8; 32])
    .fetch_one(db.pool())
    .await
    .expect("action");
    let action = service.action(action_id).await.expect("read action");
    assert_eq!(action.id, action_id);
    assert_eq!(action.state, "completed");
    assert_eq!(
        serde_json::to_value(action).expect("wire action")["id"],
        action_id.to_string()
    );
}

#[tokio::test]
async fn announcement_publication_returns_its_inserted_draft() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    enable_turniere_runtime(db.pool()).await;
    let service = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()));
    let block_id = "two_week_scrim_block";
    let newer = service
        .create_announcement_publication(
            block_id,
            "announcement:newer",
            "123456789",
            "Coach",
            AnnouncementPublicationRequest {
                title: Some("Späterer Draft".to_string()),
                channel_id: Some("9007199254740302".to_string()),
                message: "Dieser Draft bleibt der neueste im Block.".to_string(),
            },
        )
        .await
        .expect("create newer draft");
    sqlx::query(
        "UPDATE scrim.announcement_drafts \
            SET created_at = now() + interval '1 day' \
          WHERE id = $1",
    )
    .bind(newer.id.expect("newer draft id"))
    .execute(db.pool())
    .await
    .expect("move newer draft ahead");

    let inserted = service
        .create_announcement_publication(
            block_id,
            "announcement:inserted",
            "123456789",
            "Coach",
            AnnouncementPublicationRequest {
                title: Some("Gerade freigegeben".to_string()),
                channel_id: Some("9007199254740303".to_string()),
                message: "Genau dieser Draft soll jetzt veröffentlicht werden.".to_string(),
            },
        )
        .await
        .expect("create inserted draft");

    assert_ne!(inserted.id, newer.id);
    assert_eq!(
        inserted.message,
        "Genau dieser Draft soll jetzt veröffentlicht werden."
    );
}

async fn enable_turniere_runtime(pool: &sqlx::PgPool) {
    let applied: bool = sqlx::query_scalar(
        "SELECT applied FROM scrim.transition_runtime_control(\
             0, 'draining', 'turniere', '123456789', 'Coach', \
             'operator:runtime', 'operator:runtime', '{}'::jsonb\
         )",
    )
    .fetch_one(pool)
    .await
    .expect("enable turniere runtime");
    assert!(applied);
}
