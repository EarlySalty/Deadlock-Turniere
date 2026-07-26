use serde_json::json;
use turnier_scrim::dto::{
    ActionReceipt, AnnouncementPublicationRequest, CreateMatchRequest, LobbyCodeRequest,
    MatchIdPatchRequest, MatchIdsRequest, MatchRequestAction, MatchRequestResponseRequest,
    PatchValue, PlanningCreateRequest, ResultFetchRequest, TeamPatchRequest,
    MATCH_REQUEST_RESPONSE_SCHEMA_VERSION,
};

#[test]
fn planning_contract_accepts_the_exact_bff_fixture() {
    let fixture_source = include_str!("fixtures/planning_create.json").trim_end();
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_source).expect("planning fixture JSON");

    let request: PlanningCreateRequest =
        serde_json::from_value(fixture.clone()).expect("canonical BFF planning DTO");
    assert_eq!(request.technical_template_key.as_deref(), Some("training"));
    assert_eq!(
        request.deadline_at.to_rfc3339(),
        "2026-08-01T18:00:00+00:00"
    );
    assert_eq!(request.slots.as_ref().expect("global slots").len(), 2);
    assert_eq!(request.pairings.len(), 2);
    assert_eq!(request.pairings[0].team_a_id, "1");
    assert!(request.pairings[0].team_b_id.is_none());
    assert!(request.pairings[0].slots.is_none());
    assert_eq!(request.pairings[1].slots.as_ref().expect("slots").len(), 2);
    assert_eq!(serde_json::to_value(request).unwrap(), fixture);
    assert_eq!(serde_json::to_string(&fixture).unwrap(), fixture_source);
}

#[test]
fn match_operator_contract_accepts_the_exact_bff_payloads() {
    let create: CreateMatchRequest = serde_json::from_value(json!({
        "team_a_id": "1",
        "team_b_id": "2",
        "match_request_id": null,
        "scheduled_at": "2026-08-01T18:00:00Z",
        "note": null
    }))
    .expect("canonical match create DTO");
    assert_eq!(create.team_a_id.as_deref(), Some("1"));
    assert_eq!(create.team_b_id.as_deref(), Some("2"));

    let lobby: LobbyCodeRequest =
        serde_json::from_value(json!({"lobby_code":"a1b2c"})).expect("canonical lobby DTO");
    assert_eq!(lobby.lobby_code, "a1b2c");

    let match_ids: MatchIdsRequest =
        serde_json::from_value(json!({"match_ids":["9007199254740991"]}))
            .expect("canonical match IDs DTO");
    assert_eq!(match_ids.match_ids, ["9007199254740991"]);

    let fetch: ResultFetchRequest =
        serde_json::from_value(json!({})).expect("empty result-fetch capability DTO");
    assert!(fetch.match_id_ref.is_none());

    let selection: MatchIdPatchRequest = serde_json::from_value(json!({"message":"wrong_winner"}))
        .expect("canonical result-ref patch DTO");
    assert_eq!(selection.message, "wrong_winner");

    let publication: AnnouncementPublicationRequest =
        serde_json::from_value(json!({"message":"announcement"}))
            .expect("canonical announcement publication DTO");
    assert_eq!(publication.message, "announcement");
}

#[test]
fn planning_contract_rejects_legacy_deadline_and_arbitrary_weekdays() {
    let legacy_deadline = json!({
        "technical_template_key": "training",
        "deadline": "2026-08-01T18:00:00Z",
        "slots": [
            {"day":"fri", "from_minute":1170, "to_minute":1290},
            {"day":"sat", "from_minute":960, "to_minute":1080}
        ],
        "pairings": [{"team_a_id":"1", "team_b_id":null, "slots":null}]
    });
    assert!(serde_json::from_value::<PlanningCreateRequest>(legacy_deadline).is_err());

    let arbitrary_weekday = json!({
        "deadline_at": "2026-08-01T18:00:00Z",
        "slots": [
            {"day":"friday", "from_minute":1170, "to_minute":1290},
            {"day":"sat", "from_minute":960, "to_minute":1080}
        ],
        "pairings": [{"team_a_id":"1", "team_b_id":null, "slots":null}]
    });
    assert!(serde_json::from_value::<PlanningCreateRequest>(arbitrary_weekday).is_err());
}

#[test]
fn planning_contract_allows_missing_template_and_global_slots_with_pairing_overrides() {
    let request: PlanningCreateRequest = serde_json::from_value(json!({
        "deadline_at": "2026-08-01T18:00:00Z",
        "pairings": [{
            "team_a_id":"1",
            "team_b_id":null,
            "slots":[
                {"day":"fri", "from_minute":1170, "to_minute":1290},
                {"day":"sat", "from_minute":960, "to_minute":1080}
            ]
        }]
    }))
    .expect("canonical planning DTO with defaults at the server boundary");

    assert!(request.technical_template_key.is_none());
    assert!(request.slots.is_none());
    assert_eq!(
        request.pairings[0].slots.as_ref().expect("override").len(),
        2
    );
}

#[test]
fn dl_interaction_contract_matches_the_relay_and_keeps_snowflakes_as_strings() {
    let fixture_source = include_str!("fixtures/match_request_response.json").trim_end();
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_source).expect("interaction fixture JSON");
    let request: MatchRequestResponseRequest =
        serde_json::from_value(fixture.clone()).expect("canonical interaction DTO");

    assert_eq!(
        request.schema_version,
        MATCH_REQUEST_RESPONSE_SCHEMA_VERSION
    );
    assert_eq!(request.action, MatchRequestAction::Slot);
    assert_eq!(request.interaction, "900719925474099344");
    assert_eq!(request.slot, Some(0));

    let encoded = serde_json::to_value(request).expect("serialize interaction DTO");
    assert_eq!(encoded, fixture);
    assert_eq!(serde_json::to_string(&encoded).unwrap(), fixture_source);
    assert!(encoded["interaction"].is_string());
    assert!(encoded["guild"].is_string());
    assert!(encoded["channel"].is_string());
    assert!(encoded["message"].is_string());
    assert!(encoded["actor"].is_string());
}

#[test]
fn none_action_has_no_slot_and_receipt_matches_the_dl_adapter() {
    let request: MatchRequestResponseRequest = serde_json::from_value(json!({
        "schema_version": "turnier-scrim-match-request-response:v1",
        "event": "scrimreq:v1:interaction:45",
        "idempotency": "scrimreq:v1:interaction:45",
        "action": "none",
        "request": "31",
        "team": "2",
        "slot": null,
        "interaction": "45",
        "guild": "55",
        "channel": "66",
        "message": null,
        "actor": "88",
        "actor_role_ids": []
    }))
    .expect("none interaction DTO");
    assert_eq!(request.action, MatchRequestAction::None);
    assert!(request.slot.is_none());

    let receipt = ActionReceipt {
        accepted: false,
        message: "Scrim-Mutationen sind noch deaktiviert.".to_string(),
    };
    assert_eq!(
        serde_json::to_value(receipt).unwrap(),
        json!({
            "accepted": false,
            "message": "Scrim-Mutationen sind noch deaktiviert."
        })
    );
}

#[test]
fn nullable_patch_fields_distinguish_omitted_null_and_value() {
    let patch: TeamPatchRequest = serde_json::from_value(json!({
        "coach": null,
        "coach_discord_id": "123",
    }))
    .expect("patch DTO");
    assert!(matches!(patch.name, PatchValue::Omitted));
    assert!(matches!(patch.coach, PatchValue::Null));
    assert!(matches!(patch.coach_discord_id, PatchValue::Value(value) if value == "123"));
}
