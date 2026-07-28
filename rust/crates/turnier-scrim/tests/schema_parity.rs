#![cfg(feature = "testing")]

use std::collections::BTreeSet;
use turnier_scrim::model::{MatchRequestTemplate, ResponseChoice, ScrimDay};

use turnier_scrim::repository::{PgScrimReadRepository, ScrimReadRepository};
use turnier_scrim::service::ScrimService;

#[tokio::test]
async fn typed_read_model_matches_the_current_scrim_schema() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.participants(
            id, discord_id, display_name, rank, rank_source, rank_verified, roles,
            availability, availability_slots, notes, status, source, created_at, updated_at
        ) VALUES (
            810001, 9007199254740991, 'Alpha', 'Phantom', 'self', true, 'flex',
            'Sa/So 20-22', '{"sat":{"status":"available","from":1200,"to":1320}}'::jsonb,
            'note', 'assigned', 'web_form', now(), now()
        ), (
            810003, 9007199254740989, 'No Slots', 'Oracle', 'self', true, 'duelist',
            'Ask in Discord', NULL,
            'text-only note', 'assigned', 'web_form', now(), now()
        );
        INSERT INTO scrim.teams(
            id, name, coach, coach_discord_id, discord_role_id, discord_channel_id,
            default_from, default_to, created_at
        ) VALUES (
            810010, 'Team Alpha', 'Coach', 9007199254740992, 9007199254740993,
            9007199254740994, 1200, 1320, now()
        );
        INSERT INTO scrim.team_members(
            team_id, participant_id, role, is_captain, is_bench, substitute_until
        ) VALUES
            (810010, 810001, 'player', true, false, NULL),
            (810010, 810003, 'player', false, false, NULL);
        INSERT INTO scrim.matches(
            id, team_a_id, team_b_id, status, lobby_state, steam_match_id,
            coach_spectator_discord_id, created_at, updated_at
        ) VALUES (
            810020, 810010, NULL, 'played', 'finished', 9007199254740995,
            9007199254740996, now(), now()
        );
        INSERT INTO scrim.match_result_refs(
            match_id, steam_match_id, source_user_id, source_display_name, fetch_status,
            winner_team_id, normalized_result_json, validation_status, entered_at, updated_at
        ) VALUES (
            810020, 9007199254740997, 'coach', 'Coach', 'fetched', 810010,
            '{"winner":"team_a"}'::jsonb, 'valid', now(), now()
        );
        INSERT INTO scrim.match_result_selections(
            match_id, result_ref_id, selected_by_user_id, selected_by_display_name, selection_reason
        )
        SELECT 810020, id, '9007199254740992', 'Coach', 'contract_test'
          FROM scrim.match_result_refs
         WHERE match_id = 810020 AND steam_match_id = 9007199254740997;
        INSERT INTO scrim.match_request_batches(
            id, template, deadline_at, status, created_by_user_id,
            created_by_display_name, created_at, updated_at
        ) VALUES (
            810030, 'regular_scrim', now() + interval '2 days', 'open', 'coach', 'Coach', now(), now()
        );
        INSERT INTO scrim.match_requests(
            id, batch_id, team_a_id, team_b_id, status, slot_options, created_at, updated_at
        ) VALUES (
            810031, 810030, 810010, NULL, 'open',
            '[{"day":"sat","from":1200,"to":1320},{"day":"sun","from":1200,"to":1320}]'::jsonb,
            now(), now()
        );
        INSERT INTO scrim.match_request_responses(
            request_id, team_id, participant_id, discord_user_id, slot_index, response,
            source, message_id, channel_id, responded_at, updated_at
        ) VALUES (
            810031, 810010, 810001, 9007199254740991, 0, 'available', 'button',
            9007199254740998, 9007199254740994, now(), now()
        );
        INSERT INTO scrim.participants(
            id, display_name, rank_source, rank_verified, status, source, created_at, updated_at
        ) VALUES (810002, 'Former Alpha', 'self', false, 'inactive', 'web_form', now(), now());
        INSERT INTO scrim.match_request_responses(
            request_id, team_id, participant_id, discord_user_id, slot_index, response,
            source, responded_at, updated_at
        ) VALUES (
            810031, 810010, 810002, 9007199254740999, 0, 'available', 'button', now(), now()
        );
        INSERT INTO scrim.lagebild_snapshots(
            id, team_id, generated_for, source, status, lagebild_text, model, error, generated_at, created_at
        ) OVERRIDING SYSTEM VALUE VALUES (
            810039, 810010, 'weekly', 'ai', 'error', '', NULL, 'LLM provider error: HTTP 400',
            now() - interval '2 days', now() - interval '2 days'
        ), (
            810040, 810010, 'weekly', 'ai', 'ok', 'internal text', 'model', NULL, now(), now()
        );
        INSERT INTO scrim.lagebild_evidences(
            snapshot_id, evidence_type, label, reference_id, occurred_at
        ) VALUES (810040, 'match', 'Match 810020', '810020', now());
        "#,
    )
    .execute(db.pool())
    .await
    .expect("seed current scrim schema");

    let model = PgScrimReadRepository::new(db.pool().clone())
        .read_model()
        .await
        .expect("typed read model");

    let participant = model
        .participants
        .iter()
        .find(|participant| participant.id == 810001)
        .expect("participant");
    assert_eq!(participant.discord_id.as_deref(), Some("9007199254740991"));

    let team = model
        .teams
        .iter()
        .find(|team| team.id == 810010)
        .expect("team");
    assert_eq!(team.members[0].participant_id, 810001);
    assert_eq!(team.members[0].rank.as_deref(), Some("Phantom"));
    assert_eq!(
        team.members[0].discord_id.as_deref(),
        Some("9007199254740991")
    );
    assert_eq!(team.members[0].roles.as_deref(), Some("flex"));
    assert_eq!(team.members[0].availability.as_deref(), Some("Sa/So 20-22"));
    assert_eq!(team.members[0].notes.as_deref(), Some("note"));
    assert_eq!(
        team.members[0]
            .availability_slots
            .as_ref()
            .and_then(|slots| slots.sat.as_ref())
            .and_then(|slot| slot.from),
        Some(1200)
    );
    let text_only_member = team
        .members
        .iter()
        .find(|member| member.participant_id == 810003)
        .expect("text-only availability member");
    assert_eq!(
        text_only_member.availability.as_deref(),
        Some("Ask in Discord")
    );
    assert!(text_only_member.availability_slots.is_none());
    assert_eq!(team.coach_discord_id.as_deref(), Some("9007199254740992"));

    let scrim_match = model
        .matches
        .iter()
        .find(|scrim_match| scrim_match.id == 810020)
        .expect("match");
    assert!(scrim_match.team_b.is_none());
    assert_eq!(scrim_match.status, "played");
    assert_eq!(scrim_match.lobby_state.as_deref(), Some("finished"));
    let selected = scrim_match
        .selected_result
        .as_ref()
        .expect("selected result");
    assert_eq!(selected.steam_match_id, "9007199254740997");
    assert_eq!(selected.winner_team_id, Some(810010));

    let encoded = serde_json::to_value(&model).expect("wire model");
    assert!(encoded["participants"][0]["id"].is_string());
    let encoded_match = encoded["matches"]
        .as_array()
        .expect("matches array")
        .iter()
        .find(|value| value["id"] == "810020")
        .expect("encoded match");
    assert!(encoded_match["id"].is_string());
    assert_eq!(encoded_match["selected_result"]["winner_team_id"], "810010");
    assert!(encoded_match.get("result_refs").is_none());
    assert!(encoded_match.get("result_json").is_none());
    assert!(encoded_match.get("lobby_code_message_ids").is_none());

    let batch = model
        .match_request_batches
        .iter()
        .find(|batch| batch.id == 810030)
        .expect("batch");
    assert_eq!(batch.template, MatchRequestTemplate::RegularScrim);
    assert!(batch.requests[0].team_b.is_none());
    assert_eq!(batch.requests[0].slots[0].day, ScrimDay::Saturday);
    assert_eq!(
        batch.requests[0].responses[0].response,
        ResponseChoice::Available
    );
    assert_eq!(
        batch.requests[0].responses[0].message_id.as_deref(),
        Some("9007199254740998")
    );
    assert_eq!(batch.requests[0].responses.len(), 2);
    assert_eq!(batch.requests[0].facts.slots[0].available_count, 1);
    assert_eq!(batch.requests[0].facts.slots[0].starter_available_count, 1);
    assert!(batch.requests[0].facts.recommended_slot_index.is_none());

    let lagebild = model
        .lagebild_refs
        .iter()
        .find(|snapshot| snapshot.id == 810040)
        .expect("lagebild ref");
    assert_eq!(
        lagebild.evidences[0].reference_id.as_deref(),
        Some("810020")
    );
    // Ohne den Text kann die Uebersicht nur Modellnamen und Fehler anzeigen.
    assert_eq!(lagebild.lagebild_text, "internal text");
    // Pro Team zaehlt der aktuelle Stand. Alte Fehlversuche wuerden die Uebersicht
    // sonst dauerhaft zumuellen, ihre Historie bleibt in der Detailansicht.
    assert!(
        !model
            .lagebild_refs
            .iter()
            .any(|snapshot| snapshot.id == 810039),
        "aeltere Snapshots desselben Teams gehoeren nicht ins Read-Model"
    );
    assert_eq!(
        model
            .lagebild_refs
            .iter()
            .filter(|snapshot| snapshot.team_id == 810010)
            .count(),
        1
    );

    // Die Zeitleiste eines Teams braucht die vollstaendige Historie, auch die
    // fehlgeschlagenen Laeufe. Sonst waere der Verlauf mit der Uebersicht weg.
    let history = PgScrimReadRepository::new(db.pool().clone())
        .lagebild_history(810010)
        .await
        .expect("lagebild history");
    assert_eq!(
        history.iter().map(|snapshot| snapshot.id).collect::<Vec<_>>(),
        vec![810040, 810039]
    );
    assert_eq!(history[1].error.as_deref(), Some("LLM provider error: HTTP 400"));
}

#[tokio::test]
async fn repository_checks_team_existence_active_conflicts_and_operator_roles() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.teams(id, name, created_at)
        VALUES (820010, 'One', now()), (820020, 'Two', now());
        INSERT INTO scrim.match_request_batches(
            id, template, deadline_at, status, created_by_user_id, created_by_display_name
        ) VALUES (820030, 'regular_scrim', now() + interval '2 days', 'open', 'actor', 'Actor');
        INSERT INTO scrim.match_requests(id, batch_id, team_a_id, team_b_id, status, slot_options)
        VALUES (
            820031, 820030, 820010, NULL, 'open',
            '[{"day":"sat","from":1200,"to":1320},{"day":"sun","from":1200,"to":1320}]'::jsonb
        );
        INSERT INTO coaching.coaches(id, discord_user_id, display_name, status)
        VALUES ('scrim-repository-coach', 820001, 'Coach', 'active');
        INSERT INTO core.meta_users(id, username, display_name, role) VALUES
            (820002, 'admin', 'Admin', 'admin'),
            (820003, 'mod', 'Mod', 'mod'),
            (820004, 'user', 'User', 'user');
        "#,
    )
    .execute(db.pool())
    .await
    .expect("seed repository contracts");
    let repository = PgScrimReadRepository::new(db.pool().clone());

    let coaches = repository.coaches().await.expect("active coaches");
    assert!(coaches
        .iter()
        .any(|coach| { coach.discord_user_id == "820001" && coach.display_name == "Coach" }));

    assert_eq!(
        repository
            .existing_team_ids(&BTreeSet::from([820010, 820020, 820099]))
            .await
            .expect("existing teams"),
        BTreeSet::from([820010, 820020])
    );
    assert_eq!(
        repository
            .active_request_team_ids(&BTreeSet::from([820010, 820020]))
            .await
            .expect("active teams"),
        BTreeSet::from([820010])
    );
    assert!(repository
        .is_active_coach(820001)
        .await
        .expect("coach lookup"));
    for actor in [820002, 820003] {
        assert!(!repository
            .is_active_coach(actor)
            .await
            .expect("admin/mod lookup"));
    }
    assert!(!repository
        .is_active_coach(820004)
        .await
        .expect("user lookup"));
}

#[tokio::test]
async fn history_uses_only_selected_result_and_excludes_cancelled() {
    let db = turnier_db::test_pool().await.expect("central test pool");
    sqlx::raw_sql(
        r#"
        INSERT INTO scrim.matches(id, status, lobby_state, result_json, created_at) VALUES
            (830001, 'scheduled', 'finished', NULL, now()),
            (830002, 'scheduled', NULL, '{"winner":"team_a"}'::jsonb, now()),
            (830003, 'completed', NULL, NULL, now()),
            (830004, 'cancelled', 'finished', '{"winner":"team_a"}'::jsonb, now()),
            (830005, 'scheduled', NULL, NULL, now()),
            (830006, 'scheduled', NULL, NULL, now());
        INSERT INTO scrim.match_result_refs(
            match_id, steam_match_id, source_user_id, source_display_name, fetch_status,
            winner_team_id, normalized_result_json, validation_status, entered_at, updated_at
        ) VALUES
            (830005, 830005001, 'actor', 'Actor', 'fetched', 1, '{}'::jsonb, 'valid', now(), now()),
            (830006, 830006001, 'actor', 'Actor', 'fetched', 1, '{}'::jsonb, 'valid', now(), now());
        INSERT INTO scrim.match_result_selections(
            match_id, result_ref_id, selected_by_user_id, selected_by_display_name, selection_reason
        )
        SELECT 830006, id, '42', 'Actor', 'contract_test'
          FROM scrim.match_result_refs
         WHERE match_id = 830006;
        "#,
    )
    .execute(db.pool())
    .await
    .expect("seed history contracts");

    let history = ScrimService::new(PgScrimReadRepository::new(db.pool().clone()))
        .history()
        .await
        .expect("history");
    let ids = history
        .into_iter()
        .map(|scrim_match| scrim_match.id)
        .collect::<BTreeSet<_>>();
    assert!(ids.is_superset(&BTreeSet::from([830006])));
    assert!(!ids.contains(&830001));
    assert!(!ids.contains(&830002));
    assert!(!ids.contains(&830003));
    assert!(!ids.contains(&830004));
    assert!(!ids.contains(&830005));
}
