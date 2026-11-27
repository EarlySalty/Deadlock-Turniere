mod common;

use chrono::{Duration, Utc};
use common::{
    finalize_checkin_and_advance, insert_signup, insert_team, insert_team_member,
    insert_tournament, temp_db, tournament_status,
};
use turnier_core::TournamentMode;
use turnier_db::Pool;
use turnier_engine::{FinalizeCheckinParams, TournamentError};

async fn configure(pool: &Pool, tournament_id: i64, mode: &str, auto: bool, team_size: i64) {
    sqlx::query(
        "UPDATE turnier.tournaments SET tournament_mode = $1, tournament_mode_auto = $2, \
         team_size = $3 WHERE id = $4",
    )
    .bind(mode)
    .bind(auto)
    .bind(team_size)
    .bind(tournament_id)
    .execute(pool)
    .await
    .expect("configure tournament");
}

async fn insert_full_teams(pool: &Pool, tournament_id: i64, count: i64) {
    for number in 1..=count {
        let team_id = insert_team(
            pool,
            tournament_id,
            &format!("Team {number}"),
            123_456_789_012_346_000 + number,
        )
        .await;
        insert_team_member(pool, team_id, 123_456_789_012_347_000 + number).await;
        sqlx::query("UPDATE turnier.team_members SET rank_score = $1 WHERE team_id = $2")
            .bind(100 - number)
            .bind(team_id)
            .execute(pool)
            .await
            .expect("rank team member");
    }
}

async fn check_in_members(pool: &Pool, tournament_id: i64) {
    sqlx::query(
        "INSERT INTO turnier.tournament_checkins (tournament_id, discord_id, checked_in_at) \
         SELECT t.tournament_id, tm.discord_id, now() FROM turnier.team_members tm \
         JOIN turnier.teams t ON t.id = tm.team_id WHERE t.tournament_id = $1",
    )
    .bind(tournament_id)
    .execute(pool)
    .await
    .expect("check in members");
}

async fn stored_mode(pool: &Pool, tournament_id: i64) -> (String, bool) {
    sqlx::query_as(
        "SELECT tournament_mode::text, tournament_mode_auto FROM turnier.tournaments WHERE id = $1",
    )
    .bind(tournament_id)
    .fetch_one(pool)
    .await
    .expect("stored mode")
}

async fn group_count(pool: &Pool, tournament_id: i64) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM turnier.groups WHERE tournament_id = $1")
        .bind(tournament_id)
        .fetch_one(pool)
        .await
        .expect("group count")
}

async fn auto_tournament_with_teams(pool: &Pool, teams: i64) -> i64 {
    let id = insert_tournament(pool, "Auto", "checkin", false).await;
    configure(pool, id, "bracket_only", true, 1).await;
    insert_full_teams(pool, id, teams).await;
    id
}

#[tokio::test]
async fn auto_mode_with_sixteen_teams_starts_group_stage() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let id = auto_tournament_with_teams(&pool, 16).await;

    let result = finalize_checkin_and_advance(&pool, id)
        .await
        .expect("finalize");

    assert_eq!(result.final_team_count, 16);
    assert_eq!(result.tournament_mode, TournamentMode::GroupStage);
    assert!(result.tournament_mode_auto);
    assert_eq!(tournament_status(&pool, id).await, "group_phase");
    assert_eq!(stored_mode(&pool, id).await, ("group_stage".into(), true));
    assert!(group_count(&pool, id).await >= 2);
}

#[tokio::test]
async fn auto_mode_below_sixteen_teams_stays_bracket_only() {
    for teams in [5_i64, 6, 8, 15] {
        let db = temp_db().await;
        let pool = db.pool().clone();
        let id = auto_tournament_with_teams(&pool, teams).await;

        let result = finalize_checkin_and_advance(&pool, id)
            .await
            .expect("finalize");

        assert_eq!(result.final_team_count, teams as usize);
        assert_eq!(result.tournament_mode, TournamentMode::BracketOnly);
        assert_eq!(tournament_status(&pool, id).await, "bracket");
        assert_eq!(stored_mode(&pool, id).await, ("bracket_only".into(), true));
        assert_eq!(group_count(&pool, id).await, 0);
    }
}

#[tokio::test]
async fn explicit_mode_is_not_overwritten_at_sixteen_teams() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let id = insert_tournament(&pool, "Explizit", "checkin", false).await;
    configure(&pool, id, "bracket_only", false, 1).await;
    insert_full_teams(&pool, id, 16).await;

    let result = finalize_checkin_and_advance(&pool, id)
        .await
        .expect("finalize");

    assert_eq!(result.tournament_mode, TournamentMode::BracketOnly);
    assert!(!result.tournament_mode_auto);
    assert_eq!(tournament_status(&pool, id).await, "bracket");
    assert_eq!(stored_mode(&pool, id).await, ("bracket_only".into(), false));
}

#[tokio::test]
async fn preview_reports_resolved_mode_without_writing() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let id = auto_tournament_with_teams(&pool, 16).await;
    check_in_members(&pool, id).await;

    let preview = turnier_engine::finalize_checkin(&pool, id, FinalizeCheckinParams::default())
        .await
        .expect("preview");

    assert_eq!(preview.tournament_mode, TournamentMode::GroupStage);
    assert_eq!(tournament_status(&pool, id).await, "checkin");
    assert_eq!(stored_mode(&pool, id).await, ("bracket_only".into(), true));
}

#[tokio::test]
async fn auto_mode_counts_teams_formed_from_the_pool() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let id = insert_tournament(&pool, "Pool", "checkin", false).await;
    configure(&pool, id, "bracket_only", true, 2).await;
    for player in 0..32_i64 {
        insert_signup(&pool, id, 123_456_789_012_348_000 + player).await;
    }
    sqlx::query(
        "INSERT INTO turnier.tournament_checkins (tournament_id, discord_id, checked_in_at) \
         SELECT tournament_id, discord_id, now() FROM turnier.tournament_signups \
         WHERE tournament_id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .expect("check in signups");

    let result = finalize_checkin_and_advance(&pool, id)
        .await
        .expect("finalize");

    assert_eq!(result.final_team_count, 16);
    assert_eq!(result.tournament_mode, TournamentMode::GroupStage);
    assert_eq!(tournament_status(&pool, id).await, "group_phase");
}

#[tokio::test]
async fn auto_mode_in_solo_pug_uses_the_finished_teams() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let id = auto_tournament_with_teams(&pool, 12).await;
    sqlx::query("UPDATE turnier.tournaments SET invite_mode = 'solo_only' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("solo only");
    for player in 0..8_i64 {
        insert_signup(&pool, id, 123_456_789_012_349_000 + player).await;
    }
    check_in_members(&pool, id).await;
    sqlx::query(
        "INSERT INTO turnier.tournament_checkins (tournament_id, discord_id, checked_in_at) \
         SELECT tournament_id, discord_id, now() FROM turnier.tournament_signups \
         WHERE tournament_id = $1 ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .execute(&pool)
    .await
    .expect("check in pool players");

    let result = finalize_checkin_and_advance(&pool, id)
        .await
        .expect("finalize");

    assert_eq!(result.final_team_count, 12);
    assert_eq!(result.tournament_mode, TournamentMode::BracketOnly);
    assert_eq!(tournament_status(&pool, id).await, "bracket");
    assert_eq!(stored_mode(&pool, id).await, ("bracket_only".into(), true));
    assert_eq!(group_count(&pool, id).await, 0);
}

#[tokio::test]
async fn auto_mode_switch_checks_the_start_gap() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let base = Utc::now() + Duration::days(2);
    let id = auto_tournament_with_teams(&pool, 16).await;
    sqlx::query(
        "UPDATE turnier.tournaments SET group_phase_start = $1, bracket_start = $2 WHERE id = $3",
    )
    .bind(base)
    .bind(base + Duration::hours(6))
    .bind(id)
    .execute(&pool)
    .await
    .expect("set starts");
    let other = insert_tournament(&pool, "Nachbar", "registration", false).await;
    sqlx::query("UPDATE turnier.tournaments SET bracket_start = $1 WHERE id = $2")
        .bind(base + Duration::hours(1))
        .bind(other)
        .execute(&pool)
        .await
        .expect("set neighbour start");

    let err = finalize_checkin_and_advance(&pool, id)
        .await
        .expect_err("start gap");

    assert!(matches!(err, TournamentError::StartGapConflict(_)));
    assert_eq!(tournament_status(&pool, id).await, "checkin");
    assert_eq!(stored_mode(&pool, id).await, ("bracket_only".into(), true));
    assert_eq!(group_count(&pool, id).await, 0);
}
