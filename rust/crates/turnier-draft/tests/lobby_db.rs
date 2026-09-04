//! Integrationstests fuer freie Draft-Lobbys gegen eine echte Wegwerf-PG-DB.

use chrono::{Duration, Utc};
use turnier_db::{test_pool, Pool, TestDb};
use turnier_draft::{
    create_lobby, get_state_by_code, take_lobby_action, CreateLobbyOptions, DraftError,
    QUICK_NO_BAN,
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
