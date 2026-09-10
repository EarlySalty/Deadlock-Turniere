//! Integrationstests fuer freie Draft-Lobbys gegen eine echte Wegwerf-PG-DB.

use chrono::{Duration, Utc};
use turnier_db::{test_pool, Pool, TestDb};
use turnier_draft::{
    claim_room, create_lobby, create_room, get_state_by_code, leave_room, rematch_room, room_ready,
    take_lobby_action, CreateLobbyOptions, CreateRoomOptions, DraftError, QUICK_NO_BAN,
};

async fn temp_db() -> TestDb {
    test_pool().await.expect("central test pool")
}

fn options(round_seconds: Option<i32>, reserve_seconds: Option<i32>) -> CreateLobbyOptions {
    CreateLobbyOptions {
        team1_name: "Team Eins".to_string(),
        team2_name: "Team Zwei".to_string(),
        sequence: QUICK_NO_BAN.to_vec(),
        round_seconds,
        reserve_seconds,
    }
}

async fn slot_tokens(pool: &Pool, code: &str) -> (String, String) {
    sqlx::query_as("SELECT team1_token, team2_token FROM turnier.draft_sessions WHERE code = $1")
        .bind(code)
        .fetch_one(pool)
        .await
        .expect("Slot-Tokens laden")
}

fn room_options(bans_per_team: i32, round_seconds: Option<i32>) -> CreateRoomOptions {
    CreateRoomOptions {
        team1_name: "Team Eins".to_string(),
        team2_name: "Team Zwei".to_string(),
        sequence: turnier_draft::sequence_for_bans(bans_per_team),
        bans_per_team,
        round_seconds,
    }
}

async fn raum_anlegen(bans_per_team: i32, round_seconds: Option<i32>) -> (TestDb, String) {
    let db = test_pool().await.expect("central test pool");
    let code = create_room(db.pool(), room_options(bans_per_team, round_seconds))
        .await
        .expect("Raum anlegen");
    (db, code)
}

async fn raum_zustand(pool: &Pool, code: &str) -> turnier_draft::DraftState {
    get_state_by_code(pool, code).await.expect("Zustand laden")
}

async fn spalten(
    pool: &Pool,
    code: &str,
) -> (
    String,
    Option<chrono::DateTime<Utc>>,
    Option<chrono::DateTime<Utc>>,
    bool,
    bool,
    String,
    i32,
) {
    sqlx::query_as(
        "SELECT status, team1_claimed_at, team2_claimed_at, team1_ready, team2_ready, \
                lobby_status, bans_per_team \
         FROM turnier.draft_sessions WHERE code = $1",
    )
    .bind(code)
    .fetch_one(pool)
    .await
    .expect("Raum-Zeile laden")
}

async fn beide_captains_bereit(pool: &Pool, code: &str) -> bool {
    let erster = claim_room(pool, code, 1).await.expect("Claim Team 1");
    room_ready(pool, code, &erster.token)
        .await
        .expect("Ready Team 1");
    let zweiter = claim_room(pool, code, 2).await.expect("Claim Team 2");
    room_ready(pool, code, &zweiter.token)
        .await
        .expect("Ready Team 2")
        .started
}

async fn spiele_raum_zu_ende(pool: &Pool, code: &str) {
    let (slot1_token, slot2_token) = slot_tokens(pool, code).await;
    for runde in 0..24 {
        let state = raum_zustand(pool, code).await;
        if state.session.status == "completed" {
            return;
        }
        let token = match state.current_team_slot {
            Some(1) => &slot1_token,
            Some(2) => &slot2_token,
            other => panic!("unerwarteter Team-Slot nach {runde} Zügen: {other:?}"),
        };
        let held = HELDEN[runde % HELDEN.len()];
        take_lobby_action(pool, code, token, held)
            .await
            .expect("gültiger Zug");
    }
    panic!("Draft nach 24 Zügen nicht abgeschlossen");
}

const HELDEN: [&str; 12] = [
    "Abrams",
    "Bebop",
    "Calico",
    "Dynamo",
    "Grey Talon",
    "Haze",
    "Holliday",
    "Infernus",
    "Ivy",
    "Kelvin",
    "Lady Geist",
    "Lash",
];

async fn expire_at(pool: &Pool, code: &str, deadline: chrono::DateTime<Utc>) {
    sqlx::query("UPDATE turnier.draft_sessions SET deadline_at = $1 WHERE code = $2")
        .bind(deadline)
        .bind(code)
        .execute(pool)
        .await
        .expect("Deadline setzen");
}

#[tokio::test]
async fn lobby_anlegen_liefert_code_und_zwei_tokens() {
    let db = temp_db().await;
    let lobby = create_lobby(db.pool(), options(Some(60), Some(30)))
        .await
        .unwrap();

    assert_eq!(lobby.code.len(), 8);
    assert!(!lobby.team1_token.is_empty());
    assert!(!lobby.team2_token.is_empty());
    assert_ne!(lobby.team1_token, lobby.team2_token);

    let (name1, token1, name2, token2): (String, String, String, String) = sqlx::query_as(
        "SELECT team1_name, team1_token, team2_name, team2_token \
         FROM turnier.draft_sessions WHERE code = $1",
    )
    .bind(&lobby.code)
    .fetch_one(db.pool())
    .await
    .expect("Slot-Zeile laden");
    let paare = [
        (name1.as_str(), token1.as_str()),
        (name2.as_str(), token2.as_str()),
    ];
    assert!(paare.contains(&("Team Eins", lobby.team1_token.as_str())));
    assert!(paare.contains(&("Team Zwei", lobby.team2_token.as_str())));

    let state = get_state_by_code(db.pool(), &lobby.code).await.unwrap();
    assert!(state.session.bracket_match_id.is_none());
    assert_eq!(state.actions.len(), QUICK_NO_BAN.len());
    assert!(state.session.deadline_at.is_some());
}

#[tokio::test]
async fn lobby_tokens_unterscheiden_ungueltig_von_falschem_team() {
    let db = temp_db().await;
    let lobby = create_lobby(db.pool(), options(None, None)).await.unwrap();

    let invalid = take_lobby_action(db.pool(), &lobby.code, "falsch", "Abrams")
        .await
        .unwrap_err();
    assert!(matches!(invalid, DraftError::InvalidToken));

    let (_slot1_token, slot2_token) = slot_tokens(db.pool(), &lobby.code).await;
    let wrong_team = take_lobby_action(db.pool(), &lobby.code, &slot2_token, "Abrams")
        .await
        .unwrap_err();
    assert!(matches!(wrong_team, DraftError::NotYourTurn));
}

#[tokio::test]
async fn quick_lobby_laeuft_mit_captain_tokens_bis_completed() {
    let db = temp_db().await;
    let lobby = create_lobby(db.pool(), options(None, None)).await.unwrap();
    let (slot1_token, slot2_token) = slot_tokens(db.pool(), &lobby.code).await;
    let heroes = [
        "Abrams",
        "Bebop",
        "Calico",
        "Dynamo",
        "Grey Talon",
        "Haze",
        "Holliday",
        "Infernus",
        "Ivy",
        "Kelvin",
        "Lady Geist",
        "Lash",
    ];

    for hero in heroes {
        let state = get_state_by_code(db.pool(), &lobby.code).await.unwrap();
        let token = match state.current_team_slot {
            Some(1) => &slot1_token,
            Some(2) => &slot2_token,
            other => panic!("unerwarteter Team-Slot: {other:?}"),
        };
        take_lobby_action(db.pool(), &lobby.code, token, hero)
            .await
            .unwrap();
    }

    let state = get_state_by_code(db.pool(), &lobby.code).await.unwrap();
    assert_eq!(state.session.status, "completed");
    assert_eq!(
        state.session.current_action_index,
        QUICK_NO_BAN.len() as i64
    );
    assert_eq!(state.picks_team1.len(), 6);
    assert_eq!(state.picks_team2.len(), 6);
}

#[tokio::test]
async fn eine_abgelaufene_deadline_setzt_genau_einen_auto_pick() {
    let db = temp_db().await;
    let lobby = create_lobby(db.pool(), options(Some(60), Some(30)))
        .await
        .unwrap();
    expire_at(db.pool(), &lobby.code, Utc::now() - Duration::seconds(1)).await;

    let state = get_state_by_code(db.pool(), &lobby.code).await.unwrap();

    assert_eq!(state.session.current_action_index, 1);
    assert_eq!(
        state.actions.iter().filter(|action| action.is_auto).count(),
        1
    );
    assert!(state.actions[0].hero_name.is_some());
}

#[tokio::test]
async fn lange_abgelaufene_deadline_setzt_mehrere_auto_picks() {
    let db = temp_db().await;
    let lobby = create_lobby(db.pool(), options(Some(60), Some(0)))
        .await
        .unwrap();
    expire_at(db.pool(), &lobby.code, Utc::now() - Duration::seconds(125)).await;

    let state = get_state_by_code(db.pool(), &lobby.code).await.unwrap();

    assert_eq!(state.session.current_action_index, 3);
    assert_eq!(
        state.actions.iter().filter(|action| action.is_auto).count(),
        3
    );
}

#[tokio::test]
async fn ohne_round_seconds_bleibt_abgelaufene_deadline_unveraendert() {
    let db = temp_db().await;
    let lobby = create_lobby(db.pool(), options(None, Some(30)))
        .await
        .unwrap();
    let deadline = Utc::now() - Duration::seconds(300);
    expire_at(db.pool(), &lobby.code, deadline).await;

    let state = get_state_by_code(db.pool(), &lobby.code).await.unwrap();

    assert_eq!(state.session.current_action_index, 0);
    assert!(state.actions.iter().all(|action| !action.is_auto));
}

#[tokio::test]
async fn raum_startet_im_warteraum_ohne_deadline() {
    let (db, code) = raum_anlegen(2, Some(30)).await;

    let (status, claim1, claim2, ready1, ready2, lobby_status, bans_per_team) =
        spalten(db.pool(), &code).await;
    assert_eq!(status, "warteraum");
    assert!(claim1.is_none());
    assert!(claim2.is_none());
    assert!(!ready1);
    assert!(!ready2);
    assert_eq!(lobby_status, "keine");
    assert_eq!(bans_per_team, 2);

    let state = raum_zustand(db.pool(), &code).await;
    assert!(state.session.deadline_at.is_none());
    assert!(state.session.started_at.is_none());
    assert_eq!(state.session.current_action_index, 0);
    assert!(state
        .actions
        .iter()
        .all(|action| action.hero_name.is_none()));
}

#[tokio::test]
async fn raum_startet_erst_nach_beidseitigem_bereit() {
    let (db, code) = raum_anlegen(1, Some(45)).await;

    let erster = claim_room(db.pool(), &code, 1).await.expect("Claim Team 1");
    assert_eq!(erster.team, 1);
    assert!(!erster.token.is_empty());

    let (_status, claim1, claim2, _r1, _r2, _ls, _bans) = spalten(db.pool(), &code).await;
    assert!(claim1.is_some());
    assert!(claim2.is_none());

    let zweiter_vorzeitig = claim_room(db.pool(), &code, 1).await.unwrap_err();
    assert!(matches!(zweiter_vorzeitig, DraftError::SlotTaken));

    let ready_ohne_claim = room_ready(db.pool(), &code, "falsch").await.unwrap_err();
    assert!(matches!(ready_ohne_claim, DraftError::InvalidToken));

    let nach_bereit_eins = room_ready(db.pool(), &code, &erster.token)
        .await
        .expect("Ready Team 1");
    assert!(!nach_bereit_eins.started);
    let (status, _c1, _c2, ready1, ready2, _ls, _bans) = spalten(db.pool(), &code).await;
    assert_eq!(status, "warteraum");
    assert!(ready1);
    assert!(!ready2);

    let zweiter = claim_room(db.pool(), &code, 2).await.expect("Claim Team 2");
    assert_ne!(erster.token, zweiter.token);
    let start = room_ready(db.pool(), &code, &zweiter.token)
        .await
        .expect("Ready Team 2");
    assert!(start.started);

    let (status, _c1, _c2, _r1, _r2, _ls, _bans) = spalten(db.pool(), &code).await;
    assert_eq!(status, "in_progress");
    let state = raum_zustand(db.pool(), &code).await;
    assert!(state.session.started_at.is_some());
    assert!(state.session.deadline_at.is_some());
    assert_eq!(state.current_team_slot, Some(1));
}

#[tokio::test]
async fn leave_ohne_start_gibt_den_platz_wieder_frei() {
    let (db, code) = raum_anlegen(0, None).await;
    let claim = claim_room(db.pool(), &code, 2).await.expect("Claim Team 2");

    let doppelt = claim_room(db.pool(), &code, 2).await.unwrap_err();
    assert!(matches!(doppelt, DraftError::SlotTaken));

    let zug = take_lobby_action(db.pool(), &code, &claim.token, "Abrams")
        .await
        .unwrap_err();
    assert!(matches!(zug, DraftError::SessionNotActive));

    leave_room(db.pool(), &code, &claim.token)
        .await
        .expect("Verlassen");
    let (_status, _c1, c2, _r1, r2, _ls, _bans) = spalten(db.pool(), &code).await;
    assert!(c2.is_none());
    assert!(!r2);

    let wieder = claim_room(db.pool(), &code, 2).await.expect("Re-Claim");
    assert!(!wieder.token.is_empty());

    let erster = claim_room(db.pool(), &code, 1).await.expect("Claim Team 1");
    room_ready(db.pool(), &code, &erster.token)
        .await
        .expect("Ready Team 1");
    let start = room_ready(db.pool(), &code, &wieder.token)
        .await
        .expect("Ready Team 2");
    assert!(start.started);

    let leave_nach_start = leave_room(db.pool(), &code, &wieder.token)
        .await
        .unwrap_err();
    assert!(matches!(leave_nach_start, DraftError::RoomNotOpen));
}

#[tokio::test]
async fn raum_mit_runde_0_laeuft_ohne_deadline_und_ohne_auto_pick() {
    let (db, code) = raum_anlegen(1, Some(0)).await;
    assert!(beide_captains_bereit(db.pool(), &code).await);

    let state = raum_zustand(db.pool(), &code).await;
    assert!(state.session.deadline_at.is_none());
    assert_eq!(state.session.round_seconds, Some(0));
    assert_eq!(state.session.current_action_index, 0);
    assert!(state.actions.iter().all(|action| !action.is_auto));

    sqlx::query("UPDATE turnier.draft_sessions SET deadline_at = now() - interval '300 seconds' WHERE code = $1")
        .bind(&code)
        .execute(db.pool())
        .await
        .expect("Deadline künstlich ablaufen lassen");

    let state = raum_zustand(db.pool(), &code).await;
    assert_eq!(state.session.current_action_index, 0);
    assert!(state.actions.iter().all(|action| !action.is_auto));
    assert!(state.actions[0].hero_name.is_none());

    let (_slot1, slot2) = slot_tokens(db.pool(), &code).await;
    let falsches_team = take_lobby_action(db.pool(), &code, &slot2, "Abrams")
        .await
        .unwrap_err();
    assert!(matches!(falsches_team, DraftError::NotYourTurn));
}

#[tokio::test]
async fn rematch_nach_abschluss_tauscht_die_seiten() {
    let (db, code) = raum_anlegen(0, None).await;
    let (slot1_token, _slot2) = slot_tokens(db.pool(), &code).await;
    let zu_frueh = rematch_room(db.pool(), &code, &slot1_token)
        .await
        .unwrap_err();
    assert!(matches!(zu_frueh, DraftError::RematchUnavailable));

    assert!(beide_captains_bereit(db.pool(), &code).await);
    spiele_raum_zu_ende(db.pool(), &code).await;
    let (status, _c1, _c2, _r1, _r2, lobby_status, _bans) = spalten(db.pool(), &code).await;
    assert_eq!(status, "completed");
    assert_eq!(lobby_status, "angefordert");

    let neues_raum = rematch_room(db.pool(), &code, &slot1_token)
        .await
        .expect("Rematch");
    assert_ne!(neues_raum, code);

    let alt: (String, String) =
        sqlx::query_as("SELECT team1_name, team2_name FROM turnier.draft_sessions WHERE code = $1")
            .bind(&code)
            .fetch_one(db.pool())
            .await
            .expect("Original-Zeile laden");
    let zeile: (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT team1_name, team2_name, status, rematch_of_code \
         FROM turnier.draft_sessions WHERE code = $1",
    )
    .bind(&neues_raum)
    .fetch_one(db.pool())
    .await
    .expect("Rematch-Zeile laden");
    assert_eq!(zeile.0, alt.1, "Slot 1 im Rematch ist der alte Slot 2");
    assert_eq!(zeile.1, alt.0, "Slot 2 im Rematch ist der alte Slot 1");
    assert_eq!(zeile.2, "warteraum");
    assert_eq!(zeile.3.as_deref(), Some(code.as_str()));

    let fremd = rematch_room(db.pool(), &code, "falsch").await.unwrap_err();
    assert!(matches!(fremd, DraftError::InvalidToken));
}
