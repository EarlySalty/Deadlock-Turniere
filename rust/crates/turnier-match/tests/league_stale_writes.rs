#![cfg(feature = "testing")]

mod common;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use common::{insert_group, insert_group_team, insert_team, insert_tournament, temp_db};
use serde_json::{json, Value};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use turnier_config::Config;
use turnier_db::Pool;
use turnier_match::league::{
    league_is_running, reset_league_match_in_tx, write_league_manual_result_in_tx,
};
use turnier_match::repo;
use turnier_match::{ApplyGroupParams, MatchError, MatchKind, MatchManager, SteamBridge};

const OLD_PARTY: &str = "party-old";
const NEW_PARTY: &str = "party-new";

struct Fixture {
    pool: Pool,
    steam: SqlitePool,
    manager: Arc<MatchManager>,
    tournament_id: i64,
    group_id: i64,
    a: i64,
    b: i64,
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct MatchState {
    status: String,
    winner_id: Option<i64>,
    steam_party_id: Option<String>,
    deadlock_match_id: Option<String>,
}

async fn steam_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect("sqlite::memory:")
        .await
        .expect("steam bridge");
    sqlx::query(
        "CREATE TABLE steam_tasks (\
             id INTEGER PRIMARY KEY AUTOINCREMENT, type TEXT, payload TEXT, status TEXT, \
             result TEXT, error TEXT, created_at INTEGER, updated_at INTEGER, \
             started_at INTEGER, finished_at INTEGER, attempts INTEGER DEFAULT 0)",
    )
    .execute(&pool)
    .await
    .expect("steam task schema");
    pool
}

async fn fixture(pool: &Pool, mode: &str) -> Fixture {
    let tournament_id = insert_tournament(pool, "Liga", false, false).await;
    sqlx::query(
        "UPDATE turnier.tournaments SET tournament_mode = $1, status = 'group_phase' \
         WHERE id = $2",
    )
    .bind(mode)
    .bind(tournament_id)
    .execute(pool)
    .await
    .expect("mode");
    let group_id = insert_group(pool, tournament_id, "Tabelle").await;
    let a = insert_team(pool, tournament_id, "Team A").await;
    let b = insert_team(pool, tournament_id, "Team B").await;
    insert_group_team(pool, group_id, a).await;
    insert_group_team(pool, group_id, b).await;
    let steam = steam_pool().await;
    let manager = Arc::new(MatchManager::new(
        pool.clone(),
        None,
        Some(SteamBridge::from_pool(steam.clone())),
        &Config::default(),
    ));
    Fixture {
        pool: pool.clone(),
        steam,
        manager,
        tournament_id,
        group_id,
        a,
        b,
    }
}

impl Fixture {
    async fn insert_match(&self, status: &str, party: Option<&str>) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO turnier.group_matches (group_id, team1_id, team2_id, status, steam_party_id) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(self.group_id)
        .bind(self.a)
        .bind(self.b)
        .bind(status)
        .bind(party)
        .fetch_one(&self.pool)
        .await
        .expect("insert match")
    }

    async fn correct(&self, match_id: i64, winner_id: i64) {
        let slot = if winner_id == self.a { 1 } else { 2 };
        let mut tx = self.pool.begin().await.expect("begin");
        write_league_manual_result_in_tx(&mut tx, match_id, self.group_id, winner_id, slot)
            .await
            .expect("correction");
        tx.commit().await.expect("commit");
    }

    async fn reset(&self, match_id: i64) {
        let mut tx = self.pool.begin().await.expect("begin");
        reset_league_match_in_tx(&mut tx, match_id, self.group_id)
            .await
            .expect("reset");
        tx.commit().await.expect("commit");
    }

    async fn state(&self, match_id: i64) -> MatchState {
        sqlx::query_as(
            "SELECT status, winner_id, steam_party_id, deadlock_match_id \
             FROM turnier.group_matches WHERE id = $1",
        )
        .bind(match_id)
        .fetch_one(&self.pool)
        .await
        .expect("match state")
    }

    async fn standing(&self, team_id: i64) -> (i64, i64, i64) {
        sqlx::query_as(
            "SELECT wins, losses, points FROM turnier.group_teams \
             WHERE group_id = $1 AND team_id = $2",
        )
        .bind(self.group_id)
        .bind(team_id)
        .fetch_one(&self.pool)
        .await
        .expect("standing")
    }

    async fn result_sources(&self, match_id: i64) -> Vec<String> {
        sqlx::query_scalar("SELECT source FROM turnier.match_results WHERE group_match_id = $1")
            .bind(match_id)
            .fetch_all(&self.pool)
            .await
            .expect("match results")
    }

    fn automatic_win(&self, winner_id: i64) -> ApplyGroupParams {
        ApplyGroupParams {
            winner_id: Some(winner_id),
            source: "automatic".to_string(),
            ..Default::default()
        }
    }
}

async fn wait_for_pending_task(steam: &SqlitePool, task_type: &str) -> i64 {
    for _ in 0..3000 {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM steam_tasks WHERE type = ? AND status = 'PENDING' ORDER BY id LIMIT 1",
        )
        .bind(task_type)
        .fetch_optional(steam)
        .await
        .expect("task lookup");
        if let Some(id) = id {
            return id;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("kein Steam-Task {task_type}");
}

async fn answer_after<F: Future<Output = ()>>(
    steam: &SqlitePool,
    task_type: &str,
    reply: Value,
    before_reply: F,
) {
    let task_id = wait_for_pending_task(steam, task_type).await;
    before_reply.await;
    sqlx::query("UPDATE steam_tasks SET status = 'DONE', result = ? WHERE id = ?")
        .bind(reply.to_string())
        .bind(task_id)
        .execute(steam)
        .await
        .expect("task reply");
}

async fn answer_start_controls(steam: &SqlitePool) {
    answer_after(steam, "GC_LOBBY_SET_SPECTATOR", json!({}), async {}).await;
    answer_after(steam, "GC_LOBBY_READY", json!({}), async {}).await;
}

async fn answer_lobby_leaves(steam: SqlitePool) {
    loop {
        answer_after(&steam, "GC_LOBBY_LEAVE", json!({}), async {}).await;
    }
}

fn lobby_reply() -> Value {
    json!({ "party_id": NEW_PARTY, "party_code": "CODE1" })
}

async fn wait_for_lock_waiter(pool: &Pool) {
    for _ in 0..1000 {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pg_stat_activity \
             WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(pool)
        .await
        .expect("lock waiters");
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("kein wartender Schreiber");
}

#[tokio::test]
async fn result_queued_behind_a_running_correction_is_refused() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("in_progress", Some(OLD_PARTY)).await;
    let mut correction = f.pool.begin().await.expect("begin");
    assert!(league_is_running(&mut correction, f.tournament_id)
        .await
        .expect("running"));
    write_league_manual_result_in_tx(&mut correction, m, f.group_id, f.b, 2)
        .await
        .expect("correction");

    let manager = f.manager.clone();
    let tournament_id = f.tournament_id;
    let params = f.automatic_win(f.a);
    let apply = tokio::spawn(async move {
        manager
            .apply_group_match_result(tournament_id, m, params)
            .await
    });
    wait_for_lock_waiter(&f.pool).await;
    correction.commit().await.expect("correction commit");

    let applied = apply.await.expect("apply task");
    assert!(applied.is_err(), "{applied:?}");
    assert_eq!(f.state(m).await.winner_id, Some(f.b));
    assert_eq!(f.standing(f.b).await, (1, 0, 2));
    assert_eq!(f.standing(f.a).await, (0, 1, -1));
    assert_eq!(f.result_sources(m).await, vec!["manual".to_string()]);
}

#[tokio::test]
async fn channel_id_is_only_stored_on_the_lobby_it_belongs_to() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("lobby_created", Some(OLD_PARTY)).await;
    let store = |party: &'static str| {
        repo::set_discord_channel_id(
            &f.pool,
            MatchKind::Group,
            m,
            f.group_id,
            party,
            "123456789012345678",
        )
    };

    assert!(!store(NEW_PARTY).await.expect("other lobby"));
    assert!(store(OLD_PARTY).await.expect("own lobby"));
    f.correct(m, f.a).await;
    assert!(!store(OLD_PARTY).await.expect("completed match"));
    f.reset(m).await;
    assert!(!store(OLD_PARTY).await.expect("reset match"));
}

#[tokio::test]
async fn stale_result_reply_does_not_overwrite_a_correction() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("in_progress", Some(OLD_PARTY)).await;

    let (reply, ()) = tokio::join!(
        f.manager.fetch_group_match_result(f.tournament_id, m),
        answer_after(
            &f.steam,
            "GC_GET_MATCH_RESULT",
            json!({ "winner_id": f.a }),
            f.correct(m, f.b),
        ),
    );

    assert!(reply.is_err(), "{reply:?}");
    assert_eq!(f.state(m).await.winner_id, Some(f.b));
    assert_eq!(f.standing(f.b).await, (1, 0, 2));
    assert_eq!(f.standing(f.a).await, (0, 1, -1));
    assert_eq!(f.result_sources(m).await, vec!["manual".to_string()]);
}

#[tokio::test]
async fn stale_result_reply_does_not_undo_a_reset() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("in_progress", Some(OLD_PARTY)).await;

    let (reply, ()) = tokio::join!(
        f.manager.fetch_group_match_result(f.tournament_id, m),
        answer_after(
            &f.steam,
            "GC_GET_MATCH_RESULT",
            json!({ "winner_id": f.a }),
            f.reset(m),
        ),
    );

    assert!(reply.is_err(), "{reply:?}");
    let state = f.state(m).await;
    assert_eq!(state.status, "pending");
    assert_eq!(state.winner_id, None);
    assert_eq!(f.standing(f.a).await, (0, 0, 0));
    assert_eq!(f.standing(f.b).await, (0, 0, 0));
    assert!(f.result_sources(m).await.is_empty());
}

#[tokio::test]
async fn stale_result_reply_does_not_land_on_the_lobby_after_a_reset() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("in_progress", Some(OLD_PARTY)).await;
    let reset_and_replay = async {
        f.reset(m).await;
        sqlx::query(
            "UPDATE turnier.group_matches SET status = 'in_progress', steam_party_id = $1 \
             WHERE id = $2",
        )
        .bind(NEW_PARTY)
        .bind(m)
        .execute(&f.pool)
        .await
        .expect("new lobby");
    };

    let (reply, ()) = tokio::join!(
        f.manager.fetch_group_match_result(f.tournament_id, m),
        answer_after(
            &f.steam,
            "GC_GET_MATCH_RESULT",
            json!({ "winner_id": f.a }),
            reset_and_replay,
        ),
    );

    assert!(reply.is_err(), "{reply:?}");
    let state = f.state(m).await;
    assert_eq!(state.status, "in_progress");
    assert_eq!(state.winner_id, None);
    assert_eq!(f.standing(f.a).await, (0, 0, 0));
}

#[tokio::test]
async fn result_is_refused_once_the_tournament_stopped_running() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("in_progress", Some(OLD_PARTY)).await;
    sqlx::query("UPDATE turnier.tournaments SET status = 'completed' WHERE id = $1")
        .bind(f.tournament_id)
        .execute(&f.pool)
        .await
        .expect("complete");

    let applied = f
        .manager
        .apply_group_match_result(f.tournament_id, m, f.automatic_win(f.a))
        .await;

    assert!(applied.is_err());
    assert_eq!(f.state(m).await.status, "in_progress");
    assert_eq!(f.standing(f.a).await, (0, 0, 0));
    assert!(f.result_sources(m).await.is_empty());
}

#[tokio::test]
async fn result_waits_for_a_running_finish_and_is_then_refused() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("in_progress", Some(OLD_PARTY)).await;
    let mut finish = f.pool.begin().await.expect("begin");
    sqlx::query("SELECT id FROM turnier.tournaments WHERE id = $1 FOR UPDATE")
        .bind(f.tournament_id)
        .execute(&mut *finish)
        .await
        .expect("finish lock");
    sqlx::query("UPDATE turnier.tournaments SET status = 'completed' WHERE id = $1")
        .bind(f.tournament_id)
        .execute(&mut *finish)
        .await
        .expect("finish write");

    let manager = f.manager.clone();
    let tournament_id = f.tournament_id;
    let params = f.automatic_win(f.a);
    let apply = tokio::spawn(async move {
        manager
            .apply_group_match_result(tournament_id, m, params)
            .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!apply.is_finished());
    finish.commit().await.expect("finish commit");

    let applied = apply.await.expect("apply task");
    assert!(applied.is_err());
    assert_eq!(f.state(m).await.status, "in_progress");
}

#[tokio::test]
async fn racing_results_and_corrections_keep_the_table_consistent() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let mut matches = Vec::new();
    for _ in 0..6 {
        matches.push(f.insert_match("in_progress", Some(OLD_PARTY)).await);
    }

    let mut racers = tokio::task::JoinSet::new();
    for m in matches.iter().copied() {
        let pool = f.pool.clone();
        let manager = f.manager.clone();
        let (tournament_id, group_id, a, b) = (f.tournament_id, f.group_id, f.a, f.b);
        let params = f.automatic_win(a);
        racers.spawn(async move {
            let correction = async {
                let mut tx = pool.begin().await.expect("begin");
                write_league_manual_result_in_tx(&mut tx, m, group_id, b, 2)
                    .await
                    .expect("correction");
                tx.commit().await.expect("commit");
            };
            let (applied, ()) = tokio::join!(
                manager.apply_group_match_result(tournament_id, m, params),
                correction,
            );
            if let Err(err) = applied {
                assert!(matches!(err, MatchError::State(_)), "{err}");
            }
        });
    }
    while let Some(finished) = racers.join_next().await {
        finished.expect("racer");
    }

    let mut a_wins = 0;
    for m in &matches {
        let state = f.state(*m).await;
        assert_eq!(state.status, "completed");
        assert_eq!(f.result_sources(*m).await.len(), 1);
        if state.winner_id == Some(f.a) {
            a_wins += 1;
        }
    }
    let b_wins = 6 - a_wins;
    assert_eq!(f.standing(f.a).await, (a_wins, b_wins, 2 * a_wins - b_wins));
    assert_eq!(f.standing(f.b).await, (b_wins, a_wins, 2 * b_wins - a_wins));
}

#[tokio::test]
async fn late_lobby_reply_leaves_a_completed_match_alone_and_closes_the_lobby() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("pending", None).await;
    let leaves = tokio::spawn(answer_lobby_leaves(f.steam.clone()));

    let (reply, ()) = tokio::join!(
        f.manager.create_group_lobby(f.tournament_id, m),
        answer_after(
            &f.steam,
            "GC_CREATE_CUSTOM_LOBBY",
            lobby_reply(),
            f.correct(m, f.b),
        ),
    );
    leaves.abort();

    assert!(reply.is_err(), "{reply:?}");
    let state = f.state(m).await;
    assert_eq!(state.status, "completed");
    assert_eq!(state.steam_party_id, None);
    assert_eq!(state.winner_id, Some(f.b));
    let leave_payload: String =
        sqlx::query_scalar("SELECT payload FROM steam_tasks WHERE type = 'GC_LOBBY_LEAVE'")
            .fetch_one(&f.steam)
            .await
            .expect("lobby wird wieder verlassen");
    assert!(leave_payload.contains(NEW_PARTY), "{leave_payload}");
}

#[tokio::test]
async fn late_start_reply_leaves_a_completed_match_alone() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("lobby_created", Some(OLD_PARTY)).await;
    let answers = async {
        answer_start_controls(&f.steam).await;
        answer_after(
            &f.steam,
            "GC_LOBBY_START_MATCH",
            json!({ "match_id": 777 }),
            f.correct(m, f.a),
        )
        .await;
    };

    let (reply, ()) = tokio::join!(f.manager.start_group_match(f.tournament_id, m), answers);

    assert!(reply.is_err(), "{reply:?}");
    let state = f.state(m).await;
    assert_eq!(state.status, "completed");
    assert_eq!(state.deadlock_match_id, None);
    assert_eq!(f.standing(f.a).await, (1, 0, 2));
}

#[tokio::test]
async fn late_start_reply_does_not_undo_a_reset() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("lobby_created", Some(OLD_PARTY)).await;
    let answers = async {
        answer_start_controls(&f.steam).await;
        answer_after(
            &f.steam,
            "GC_LOBBY_START_MATCH",
            json!({ "match_id": 777 }),
            f.reset(m),
        )
        .await;
    };

    let (reply, ()) = tokio::join!(f.manager.start_group_match(f.tournament_id, m), answers);

    assert!(reply.is_err(), "{reply:?}");
    let state = f.state(m).await;
    assert_eq!(state.status, "pending");
    assert_eq!(state.deadlock_match_id, None);
}

async fn play_full_match(f: &Fixture, m: i64) {
    let (created, ()) = tokio::join!(
        f.manager.create_group_lobby(f.tournament_id, m),
        answer_after(&f.steam, "GC_CREATE_CUSTOM_LOBBY", lobby_reply(), async {}),
    );
    created.expect("lobby");
    assert_eq!(f.state(m).await.status, "lobby_created");

    let answers = async {
        answer_start_controls(&f.steam).await;
        answer_after(
            &f.steam,
            "GC_LOBBY_START_MATCH",
            json!({ "match_id": 777 }),
            async {},
        )
        .await;
    };
    let (started, ()) = tokio::join!(f.manager.start_group_match(f.tournament_id, m), answers);
    started.expect("start");
    let state = f.state(m).await;
    assert_eq!(state.status, "in_progress");
    assert_eq!(state.deadlock_match_id.as_deref(), Some("777"));

    let (fetched, ()) = tokio::join!(
        f.manager.fetch_group_match_result(f.tournament_id, m),
        answer_after(
            &f.steam,
            "GC_GET_MATCH_RESULT",
            json!({ "winner_id": f.a }),
            async {},
        ),
    );
    fetched.expect("result");
    assert_eq!(f.state(m).await.status, "completed");
}

#[tokio::test]
async fn undisturbed_flow_still_works_in_a_league() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "league").await;
    let m = f.insert_match("pending", None).await;

    play_full_match(&f, m).await;

    assert_eq!(f.standing(f.a).await, (1, 0, 2));
    assert_eq!(f.standing(f.b).await, (0, 1, -1));
}

#[tokio::test]
async fn undisturbed_flow_still_works_in_group_stage() {
    let db = temp_db().await;
    let f = fixture(db.pool(), "group_stage").await;
    let m = f.insert_match("pending", None).await;

    play_full_match(&f, m).await;

    assert_eq!(f.standing(f.a).await, (1, 0, 3));
    assert_eq!(f.standing(f.b).await, (0, 1, 0));
}
