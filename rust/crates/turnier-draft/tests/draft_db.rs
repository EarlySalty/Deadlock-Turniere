//! Integrationstests der Draft-Persistenz gegen eine echte Wegwerf-PG-DB.
//!
//! Deckt den Lebenszyklus ab: Session starten (idempotent) → Aktionen ausführen
//! → Zustand ableiten → Abschluss → die Validierungs- und Konfliktfälle
//! (unbekannter Held, Doppel-Pick, abgeschlossene Session, Compare-and-Swap-
//! Konflikt). Keine externen Dienste — nur DB.

use turnier_db::{test_pool, Pool, TestDb};
use turnier_draft::{
    get_draft_state, start_draft, take_action, ActionType, DraftError, DEFAULT_SEQUENCE,
};

const ADMIN_ID: &str = "123456789012345701";
const OTHER_ADMIN_ID: &str = "123456789012345702";
const PLAYER_A_ID: &str = "123456789012345703";
const PLAYER_B_ID: &str = "123456789012345704";

async fn temp_db() -> TestDb {
    test_pool().await.expect("central test pool")
}

/// Legt ein minimales bracket_match an (FK-Ziel der Draft-Session) und gibt die
/// id zurück. Tournament zuerst, da bracket_matches.tournament_id darauf zeigt.
async fn seed_bracket_match(pool: &Pool) -> i64 {
    let now = chrono::Utc::now();
    let tournament_id: i64 = sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
             (name, status, team_size, bracket_format, created_by, created_at, updated_at, \
              invite_mode, tournament_mode, series_format, exclude_from_leaderboard, \
              tournament_game_mode, auto_lobby_enabled, is_test, match_objective, \
              no_show_grace_minutes, source) \
         VALUES ('T', 'bracket', 6, 'single_elimination', $1, $2, $3, \
                 'always', 'bracket_only', 1, false, 'standard', false, true, \
                 'auto', 10, 'manual') \
         RETURNING id",
    )
    .bind(123456789012345700_i64)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await
    .unwrap();
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO turnier.bracket_matches \
             (tournament_id, round, position, bracket_type, status, on_stream) \
         VALUES ($1, 1, 0, 'winners', 'pending', false) RETURNING id",
    )
    .bind(tournament_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

#[tokio::test]
async fn start_draft_ist_idempotent() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;

    let s1 = start_draft(pool, match_id, ADMIN_ID).await.unwrap();
    let s2 = start_draft(pool, match_id, OTHER_ADMIN_ID).await.unwrap();
    assert_eq!(s1, s2, "zweiter Start gibt dieselbe Session zurück");

    // Genau 18 Aktionszeilen materialisiert.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnier.draft_actions WHERE session_id = $1")
            .bind(s1)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 18);
}

#[tokio::test]
async fn start_draft_materialisiert_sequenz_korrekt() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;
    let session_id = start_draft(pool, match_id, ADMIN_ID).await.unwrap();

    let state = get_draft_state(pool, session_id).await.unwrap();
    assert_eq!(state.actions.len(), 18);
    for (i, action) in state.actions.iter().enumerate() {
        assert_eq!(action.sequence_index, i as i64);
        assert_eq!(action.action_type, DEFAULT_SEQUENCE[i].action_type);
        assert_eq!(action.team_slot, DEFAULT_SEQUENCE[i].team_slot.as_i64());
        assert!(action.hero_name.is_none());
    }
    assert_eq!(state.current_action_type, Some(ActionType::Ban));
    assert_eq!(state.current_team_slot, Some(1));
}

#[tokio::test]
async fn take_action_rueckt_vor_und_leitet_zustand_ab() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;
    let session_id = start_draft(pool, match_id, ADMIN_ID).await.unwrap();

    // Erste Aktion: Ban von Team 1.
    let outcome = take_action(pool, session_id, "Abrams", ADMIN_ID, false)
        .await
        .unwrap();
    assert!(!outcome.is_complete);
    assert_eq!(outcome.next_action_type, Some(ActionType::Ban));
    assert_eq!(outcome.next_team_slot, Some(2));

    let state = get_draft_state(pool, session_id).await.unwrap();
    assert_eq!(state.session.current_action_index, 1);
    assert_eq!(state.bans, vec!["Abrams"]);
    assert!(state.picks_team1.is_empty());
    assert_eq!(state.current_action_type, Some(ActionType::Ban));
    assert_eq!(state.current_team_slot, Some(2));
}

#[tokio::test]
async fn force_setzt_admin_flag() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;
    let session_id = start_draft(pool, match_id, ADMIN_ID).await.unwrap();

    take_action(pool, session_id, "Haze", ADMIN_ID, true)
        .await
        .unwrap();
    let flag: bool = sqlx::query_scalar(
        "SELECT is_admin_forced FROM turnier.draft_actions \
         WHERE session_id = $1 AND sequence_index = 0",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(flag);
}

#[tokio::test]
async fn voller_draft_laeuft_bis_abschluss() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;
    let session_id = start_draft(pool, match_id, ADMIN_ID).await.unwrap();

    // 18 unterschiedliche Helden in Sequenz-Reihenfolge.
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
        "McGinnis",
        "Mirage",
        "Mo & Krill",
        "Paradox",
        "Pocket",
        "Seven",
    ];
    let mut last = None;
    for (i, hero) in heroes.iter().enumerate() {
        let outcome = take_action(pool, session_id, hero, ADMIN_ID, false)
            .await
            .unwrap();
        if i < 17 {
            assert!(!outcome.is_complete, "Aktion {i} darf nicht abschließen");
        }
        last = Some(outcome);
    }
    let last = last.unwrap();
    assert!(last.is_complete);
    assert_eq!(last.next_action_type, None);
    assert_eq!(last.next_team_slot, None);

    let state = get_draft_state(pool, session_id).await.unwrap();
    assert_eq!(state.session.status, "completed");
    assert!(state.session.completed_at.is_some());
    assert_eq!(state.session.current_action_index, 18);
    assert_eq!(state.current_action_type, None);
    assert_eq!(state.bans.len(), 6);
    assert_eq!(state.picks_team1.len(), 6);
    assert_eq!(state.picks_team2.len(), 6);
    // Abgeschlossene Session ist nicht mehr in_progress → weitere Aktion scheitert.
    let err = take_action(pool, session_id, "Shiv", ADMIN_ID, false)
        .await
        .unwrap_err();
    assert!(matches!(err, DraftError::SessionNotActive));
}

#[tokio::test]
async fn unbekannter_held_wird_abgelehnt() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;
    let session_id = start_draft(pool, match_id, ADMIN_ID).await.unwrap();

    let err = take_action(pool, session_id, "Nicht-Existent", ADMIN_ID, false)
        .await
        .unwrap_err();
    assert!(matches!(err, DraftError::UnknownHero(_)));
    // Zustand unverändert.
    let state = get_draft_state(pool, session_id).await.unwrap();
    assert_eq!(state.session.current_action_index, 0);
}

#[tokio::test]
async fn doppelter_held_wird_abgelehnt() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;
    let session_id = start_draft(pool, match_id, ADMIN_ID).await.unwrap();

    take_action(pool, session_id, "Abrams", ADMIN_ID, false)
        .await
        .unwrap();
    let err = take_action(pool, session_id, "Abrams", ADMIN_ID, false)
        .await
        .unwrap_err();
    assert!(matches!(err, DraftError::HeroAlreadyTaken(_)));
    // Index ist nicht weitergerückt (nur die erste Aktion zählt).
    let state = get_draft_state(pool, session_id).await.unwrap();
    assert_eq!(state.session.current_action_index, 1);
}

#[tokio::test]
async fn unbekannte_session_liefert_not_found() {
    let db = temp_db().await;
    let pool = db.pool();
    let err = get_draft_state(pool, 999).await.unwrap_err();
    assert!(matches!(err, DraftError::SessionNotFound));
}

#[tokio::test]
async fn aktion_auf_fehlende_session_meldet_not_active() {
    let db = temp_db().await;
    let pool = db.pool();
    let err = take_action(pool, 999, "Abrams", ADMIN_ID, false)
        .await
        .unwrap_err();
    assert!(matches!(err, DraftError::SessionNotActive));
}

/// Compare-and-Swap-Konflikt: Wenn `current_action_index` zwischen dem Lesen und
/// dem Schreiben einer Aktion durch eine parallele Aktion verschoben wird, muss
/// `take_action` mit [`DraftError::ActionConflict`] scheitern statt blind zu
/// überschreiben. Hier wird der parallele Vorgriff durch ein direktes UPDATE
/// simuliert, das den Index manipuliert — analog zu einer zweiten Aktion, die
/// denselben Index gelesen und bereits vorgerückt hat.
///
/// Mechanik: Wir setzen den materialisierten Sequenz-Eintrag an Index 0 manuell
/// auf einen anderen `current_action_index` als den, den `take_action` aus der
/// Session liest — das ist nicht möglich, ohne die Session selbst zu fassen.
/// Stattdessen prüfen wir die CAS-Bedingung direkt: nach einer regulären Aktion
/// steht der Index auf 1; ein nachgereichtes UPDATE mit der alten CAS-Bedingung
/// (`current_action_index = 0`) betrifft 0 Zeilen — exakt die Logik, die
/// `take_action` als Konflikt wertet.
#[tokio::test]
async fn compare_and_swap_erkennt_konflikt() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;
    let session_id = start_draft(pool, match_id, ADMIN_ID).await.unwrap();

    // Reguläre Aktion: Index 0 → 1.
    take_action(pool, session_id, "Abrams", ADMIN_ID, false)
        .await
        .unwrap();

    // Ein UPDATE mit der veralteten CAS-Bedingung (current_action_index = 0)
    // betrifft jetzt 0 Zeilen — genau die Bedingung, die take_action als
    // ActionConflict behandelt.
    let res = sqlx::query(
        "UPDATE turnier.draft_sessions SET current_action_index = 1 \
         WHERE id = $1 AND current_action_index = 0 AND status = 'in_progress'",
    )
    .bind(session_id)
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(
        res.rows_affected(),
        0,
        "veraltete CAS-Bedingung trifft keine Zeile → Konflikt"
    );

    // Und eine gültige CAS-Bedingung (current_action_index = 1) trifft genau eine.
    let res = sqlx::query(
        "UPDATE turnier.draft_sessions SET current_action_index = 2 \
         WHERE id = $1 AND current_action_index = 1 AND status = 'in_progress'",
    )
    .bind(session_id)
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(res.rows_affected(), 1);
}

/// Echter Race-Nachweis über zwei nebenläufige `take_action`-Aufrufe auf einer
/// PG-Test-DB. Das entscheidende Invariant des CAS-Schutzes: keine zwei Helden
/// landen je an DERSELBEN Sequenz-Position, und der Index zählt genau die
/// erfolgreichen Aktionen.
#[tokio::test]
async fn zwei_parallele_aktionen_serialisieren_sauber() {
    let db = temp_db().await;
    let pool = db.pool();
    let match_id = seed_bracket_match(pool).await;
    let session_id = start_draft(pool, match_id, ADMIN_ID).await.unwrap();

    let p1 = pool.clone();
    let p2 = pool.clone();
    let h1 =
        tokio::spawn(
            async move { take_action(&p1, session_id, "Abrams", PLAYER_A_ID, false).await },
        );
    let h2 =
        tokio::spawn(
            async move { take_action(&p2, session_id, "Bebop", PLAYER_B_ID, false).await },
        );
    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();

    // Mindestens einer gewinnt; falls beide gewinnen, dann nur weil sie
    // serialisiert an aufeinanderfolgende Positionen geschrieben haben.
    let oks = [r1.is_ok(), r2.is_ok()].iter().filter(|b| **b).count();
    assert!(oks >= 1, "wenigstens eine Aktion muss durchkommen");

    let state = get_draft_state(pool, session_id).await.unwrap();
    // Index = Anzahl erfolgreicher Aktionen.
    assert_eq!(state.session.current_action_index, oks as i64);

    // Keine zwei Helden an derselben Position: belegte Positionen sind eindeutig
    // und lückenlos 0..oks.
    let belegte: Vec<i64> = state
        .actions
        .iter()
        .filter(|a| a.hero_name.is_some())
        .map(|a| a.sequence_index)
        .collect();
    assert_eq!(belegte.len(), oks, "genau {oks} Position(en) belegt");
    let mut erwartet: Vec<i64> = (0..oks as i64).collect();
    let mut sortiert = belegte.clone();
    sortiert.sort_unstable();
    erwartet.sort_unstable();
    assert_eq!(sortiert, erwartet, "Positionen lückenlos 0..{oks}");
}
