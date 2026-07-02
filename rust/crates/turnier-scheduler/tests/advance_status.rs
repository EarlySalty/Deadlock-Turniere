//! Integrationstests für [`advance_tournament_status`] mit zentraler
//! Wegwerf-PG-DB.
//!
//! Getestet wird der Übergang OHNE Generierungs-Seiteneffekte (registration →
//! checkin) — die Generierung selbst (group_phase/bracket) ist in turnier-engine
//! abgedeckt. Discord/Match sind No-op-Fakes.

mod common;

use std::sync::Arc;

use common::{
    audit_count, fake_match_manager, fake_notifier, insert_team, insert_team_member,
    insert_tournament, temp_db, test_config, tournament_status,
};
use serde_json::Value;
use tokio::sync::Barrier;
use turnier_core::now_utc;
use turnier_discord::DiscordNotifier;
use turnier_match::MatchManager;
use turnier_scheduler::{advance_tournament_status, SchedulerError, SchedulerResult};

struct ConcurrentAdvance {
    pool: turnier_db::Pool,
    matchmgr: Arc<MatchManager>,
    notifier: DiscordNotifier,
    barrier: Arc<Barrier>,
    tournament_id: i64,
    current_status: &'static str,
    next_status: &'static str,
    source: &'static str,
    actor_id: Option<&'static str>,
}

async fn concurrent_advance(call: ConcurrentAdvance) -> SchedulerResult<Value> {
    let ConcurrentAdvance {
        pool,
        matchmgr,
        notifier,
        barrier,
        tournament_id,
        current_status,
        next_status,
        source,
        actor_id,
    } = call;
    barrier.wait().await;
    advance_tournament_status(
        &pool,
        matchmgr.as_ref(),
        &notifier,
        tournament_id,
        current_status,
        next_status,
        source,
        actor_id,
    )
    .await
}

#[tokio::test]
async fn scheduler_quelle_advanciert_und_auditet_auto_advance() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "registration", false).await;

    let meta = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "registration",
        "checkin",
        "scheduler",
        None,
    )
    .await
    .expect("advance ok");

    assert_eq!(tournament_status(&pool, id).await, "checkin");
    assert_eq!(meta["from"], "registration");
    assert_eq!(meta["to"], "checkin");
    assert_eq!(meta["source"], "scheduler");
    assert_eq!(meta["tournament_id"], id);
    // scheduler-Quelle → tournament_auto_advance.
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 1);
    assert_eq!(audit_count(&pool, "tournament_advance").await, 0);
}

#[tokio::test]
async fn admin_quelle_auditet_tournament_advance_mit_actor() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "registration", false).await;

    advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "registration",
        "checkin",
        "admin",
        Some("123456789012345710"),
    )
    .await
    .expect("advance ok");

    assert_eq!(audit_count(&pool, "tournament_advance").await, 1);
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 0);
    let (user_id,): (Option<i64>,) =
        sqlx::query_as("SELECT user_id FROM turnier.audit_log WHERE action = 'tournament_advance'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user_id, Some(123456789012345710));
}

#[tokio::test]
async fn ungueltiger_uebergang_wirft_und_aendert_nichts() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "registration", false).await;

    // registration → bracket ist nicht erlaubt (nur registration → checkin).
    let err = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "registration",
        "bracket",
        "scheduler",
        None,
    )
    .await
    .expect_err("muss fehlschlagen");

    assert!(matches!(err, SchedulerError::InvalidTransition(_)));
    assert!(err.to_string().contains("Ungültiger Status-Übergang"));
    // Status unverändert, kein Audit.
    assert_eq!(tournament_status(&pool, id).await, "registration");
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 0);
}

#[tokio::test]
async fn optimistic_lock_konflikt_bei_falschem_current_status() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    // Turnier ist tatsächlich in 'checkin'.
    let id = insert_tournament(&pool, "T", "checkin", false).await;

    // Aufrufer glaubt, es sei noch 'registration' → der Übergang
    // registration→checkin ist gültig, aber das WHERE status='registration'
    // trifft keine Zeile → StatusConflict.
    let err = advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "registration",
        "checkin",
        "scheduler",
        None,
    )
    .await
    .expect_err("muss konfligieren");

    assert!(matches!(err, SchedulerError::StatusConflict));
    // Status unverändert.
    assert_eq!(tournament_status(&pool, id).await, "checkin");
}

async fn insert_ranked_team(pool: &turnier_db::Pool, tournament_id: i64, team_number: i64) -> i64 {
    let team_id = insert_team(
        pool,
        tournament_id,
        &format!("Team {team_number}"),
        123456789012346000_i64 + team_number,
    )
    .await;
    insert_team_member(pool, team_id, 123456789012347000_i64 + team_number).await;
    sqlx::query("UPDATE turnier.team_members SET rank_score = $1 WHERE team_id = $2")
        .bind(100_i64 - team_number)
        .bind(team_id)
        .execute(pool)
        .await
        .expect("rank team member");
    team_id
}

#[tokio::test]
async fn paralleler_group_phase_uebergang_erzeugt_gruppen_nur_einmal() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "checkin", false).await;
    for team_number in 1..=4 {
        insert_ranked_team(&pool, id, team_number).await;
    }

    let barrier = Arc::new(Barrier::new(2));
    let first = tokio::spawn(concurrent_advance(ConcurrentAdvance {
        pool: pool.clone(),
        matchmgr: matchmgr.clone(),
        notifier: notifier.clone(),
        barrier: barrier.clone(),
        tournament_id: id,
        current_status: "checkin",
        next_status: "group_phase",
        source: "scheduler",
        actor_id: None,
    }));
    let second = tokio::spawn(concurrent_advance(ConcurrentAdvance {
        pool: pool.clone(),
        matchmgr: matchmgr.clone(),
        notifier: notifier.clone(),
        barrier,
        tournament_id: id,
        current_status: "checkin",
        next_status: "group_phase",
        source: "scheduler",
        actor_id: None,
    }));

    let results = [
        first.await.expect("first task"),
        second.await.expect("second task"),
    ];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let status_conflicts = results
        .iter()
        .map(|result| match result {
            Err(SchedulerError::StatusConflict) => 1,
            _ => 0,
        })
        .sum::<usize>();
    assert_eq!(status_conflicts, 1);

    assert_eq!(tournament_status(&pool, id).await, "group_phase");
    let (groups, group_teams, group_matches): (i64, i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM turnier.groups WHERE tournament_id = $1), \
            (SELECT COUNT(*) FROM turnier.group_teams gt \
                JOIN turnier.groups g ON g.id = gt.group_id WHERE g.tournament_id = $1), \
            (SELECT COUNT(*) FROM turnier.group_matches gm \
                JOIN turnier.groups g ON g.id = gm.group_id WHERE g.tournament_id = $1)",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .expect("group counts");
    assert_eq!((groups, group_teams, group_matches), (2, 4, 2));
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 1);
}

#[tokio::test]
async fn parallele_registration_aktivierung_erlaubt_nur_ein_aktives_turnier() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let first_id = insert_tournament(&pool, "T1", "draft", false).await;
    let second_id = insert_tournament(&pool, "T2", "draft", false).await;

    let barrier = Arc::new(Barrier::new(2));
    let first = tokio::spawn(concurrent_advance(ConcurrentAdvance {
        pool: pool.clone(),
        matchmgr: matchmgr.clone(),
        notifier: notifier.clone(),
        barrier: barrier.clone(),
        tournament_id: first_id,
        current_status: "draft",
        next_status: "registration",
        source: "scheduler",
        actor_id: None,
    }));
    let second = tokio::spawn(concurrent_advance(ConcurrentAdvance {
        pool: pool.clone(),
        matchmgr: matchmgr.clone(),
        notifier: notifier.clone(),
        barrier,
        tournament_id: second_id,
        current_status: "draft",
        next_status: "registration",
        source: "manual",
        actor_id: Some("123456789012345710"),
    }));

    let results = [
        first.await.expect("first task"),
        second.await.expect("second task"),
    ];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let active_conflicts = results
        .iter()
        .map(|result| match result {
            Err(SchedulerError::ActiveTournamentConflict(_)) => 1,
            _ => 0,
        })
        .sum::<usize>();
    assert_eq!(active_conflicts, 1);

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.tournaments \
         WHERE status IN ('registration', 'checkin', 'group_phase', 'bracket') \
           AND is_test = false",
    )
    .fetch_one(&pool)
    .await
    .expect("active count");
    assert_eq!(active_count, 1);
    let draft_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.tournaments WHERE status = 'draft' AND is_test = false",
    )
    .fetch_one(&pool)
    .await
    .expect("draft count");
    assert_eq!(draft_count, 1);
}

async fn seed_two_team_completed_bracket(pool: &turnier_db::Pool, tournament_id: i64) {
    let mut team_ids = Vec::new();
    for team_number in 1..=2 {
        let team_id: i64 = sqlx::query_scalar(
            "INSERT INTO turnier.teams \
                 (tournament_id, name, name_key, captain_discord_id, created_at, recruitment_status) \
             VALUES ($1, $2, $3, $4, $5, 'open') RETURNING id",
        )
            .bind(tournament_id)
            .bind(format!("Team {team_number}"))
            .bind(format!("team-{team_number}"))
            .bind(123456789012345800_i64 + team_number)
            .bind(now_utc())
            .fetch_one(pool)
            .await
            .expect("insert team");
        team_ids.push(team_id);

        sqlx::query(
            "INSERT INTO turnier.team_members (team_id, discord_id, role, joined_at) \
             VALUES ($1, $2, 'member', $3)",
        )
        .bind(team_id)
        .bind(123456789012345900_i64 + team_number)
        .bind(now_utc())
        .execute(pool)
        .await
        .expect("insert member");
    }

    sqlx::query(
        "INSERT INTO turnier.bracket_matches \
         (tournament_id, round, position, bracket_type, team1_id, team2_id, winner_id, status, on_stream) \
         VALUES ($1, 1, 0, 'winners', $2, $3, $2, 'completed', false)",
    )
    .bind(tournament_id)
    .bind(team_ids[0])
    .bind(team_ids[1])
    .execute(pool)
    .await
    .expect("insert completed match");
}

#[tokio::test]
async fn completed_transition_setzt_status_und_rechnet_punkte() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "bracket", false).await;
    seed_two_team_completed_bracket(&pool, id).await;

    advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "bracket",
        "completed",
        "scheduler",
        None,
    )
    .await
    .expect("advance ok");

    assert_eq!(tournament_status(&pool, id).await, "completed");
    let points: Vec<(i64, i64, i64, i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT discord_id, total_points, tournaments_played, matches_played, matches_won, best_placement \
         FROM turnier.player_points ORDER BY discord_id",
    )
    .fetch_all(&pool)
    .await
    .expect("points");
    assert_eq!(
        points,
        vec![
            (123456789012345901, 11, 1, 1, 1, Some(1)),
            (123456789012345902, 7, 1, 1, 0, Some(2)),
        ]
    );
}

#[tokio::test]
async fn completed_transition_rollt_status_und_audit_bei_recompute_fehler_zurueck() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let config = test_config();
    let notifier = fake_notifier(pool.clone(), &config);
    let matchmgr = fake_match_manager(pool.clone(), &config);
    let id = insert_tournament(&pool, "T", "bracket", false).await;
    seed_two_team_completed_bracket(&pool, id).await;

    sqlx::query(
        "INSERT INTO turnier.player_points \
         (discord_id, total_points, tournaments_played, matches_played, matches_won, updated_at) \
         VALUES ($1, 1, 1, 1, 1, $2)",
    )
    .bind(123456789012345999_i64)
    .bind(now_utc())
    .execute(&pool)
    .await
    .expect("insert sentinel points");
    sqlx::query(
        "CREATE OR REPLACE FUNCTION turnier.fail_player_points_delete() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forced recompute failure'; END; $$",
    )
    .execute(&pool)
    .await
    .expect("create failing function");
    sqlx::query(
        "CREATE TRIGGER fail_player_points_delete BEFORE DELETE ON turnier.player_points \
         FOR EACH STATEMENT EXECUTE FUNCTION turnier.fail_player_points_delete()",
    )
    .execute(&pool)
    .await
    .expect("create failing trigger");

    advance_tournament_status(
        &pool,
        &matchmgr,
        &notifier,
        id,
        "bracket",
        "completed",
        "scheduler",
        None,
    )
    .await
    .expect_err("recompute must fail");

    assert_eq!(tournament_status(&pool, id).await, "bracket");
    assert_eq!(audit_count(&pool, "tournament_auto_advance").await, 0);

    let sentinel = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT total_points, tournaments_played, matches_played, matches_won \
         FROM turnier.player_points WHERE discord_id = $1",
    )
    .bind(123456789012345999_i64)
    .fetch_one(&pool)
    .await
    .expect("sentinel points still exist after rollback");
    assert_eq!(sentinel, (1, 1, 1, 1));
}
