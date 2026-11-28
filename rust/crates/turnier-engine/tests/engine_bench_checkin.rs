#![cfg(feature = "testing")]

mod common;

use common::{
    insert_checkin, insert_signup, insert_team, insert_team_member, insert_tournament, temp_pool,
};
use std::collections::HashSet;
use turnier_db::Pool;
use turnier_engine::{finalize_checkin, FinalizeCheckinParams, RosterChange, TeamOutcome};

async fn enable_bench(pool: &Pool, tournament_id: i64, bench_slots: i64) {
    sqlx::query("UPDATE turnier.tournaments SET bench_slots = $2 WHERE id = $1")
        .bind(tournament_id)
        .bind(bench_slots)
        .execute(pool)
        .await
        .expect("enable bench");
}

async fn seat_team(
    pool: &Pool,
    tournament_id: i64,
    name: &str,
    first_id: i64,
    starters: i64,
    bench_checked_in: bool,
) -> (i64, i64) {
    let team_id = insert_team(pool, tournament_id, name, first_id).await;
    for offset in 0..starters {
        let discord_id = first_id + offset;
        let role = if offset == 0 { "captain" } else { "member" };
        insert_team_member(pool, team_id, discord_id, "Starter", role, 0).await;
        insert_signup(pool, tournament_id, discord_id, "Starter", Some(team_id), 0).await;
        insert_checkin(pool, tournament_id, discord_id).await;
    }
    let bench_id = first_id + starters;
    insert_team_member(pool, team_id, bench_id, "Ersatz", "member", 0).await;
    sqlx::query("UPDATE turnier.team_members SET is_bench = true WHERE discord_id = $1")
        .bind(bench_id)
        .execute(pool)
        .await
        .expect("bench flag");
    insert_signup(pool, tournament_id, bench_id, "Ersatz", Some(team_id), 0).await;
    if bench_checked_in {
        insert_checkin(pool, tournament_id, bench_id).await;
    }
    (team_id, bench_id)
}

async fn checkin_tournament(pool: &Pool, team_size: i64) -> i64 {
    let tournament_id = insert_tournament(
        pool,
        "Bench",
        "checkin",
        team_size,
        "single_elimination",
        "bracket_only",
    )
    .await;
    enable_bench(pool, tournament_id, 1).await;
    tournament_id
}

#[tokio::test]
async fn preview_counts_only_starters_against_team_size() {
    for team_size in [6, 8] {
        let db = temp_pool().await;
        let pool = db.pool();
        let tournament_id = checkin_tournament(pool, team_size).await;
        let (full_id, _) = seat_team(pool, tournament_id, "Voll", 1_000, team_size, false).await;
        let (short_id, _) =
            seat_team(pool, tournament_id, "Knapp", 2_000, team_size - 1, true).await;

        for fill_from_solos in [false, true] {
            let preview = finalize_checkin(
                pool,
                tournament_id,
                FinalizeCheckinParams {
                    fill_from_solos,
                    ..Default::default()
                },
            )
            .await
            .expect("preview");

            let full = preview
                .team_plans
                .iter()
                .find(|p| p.team_id == full_id)
                .expect("full plan");
            assert_eq!(full.outcome, TeamOutcome::Complete, "size {team_size}");
            assert_eq!(full.checked_in, team_size);
            assert_eq!(full.required, team_size);
            assert_eq!(full.roster.iter().filter(|r| r.is_bench).count(), 1);

            let short = preview
                .team_plans
                .iter()
                .find(|p| p.team_id == short_id)
                .expect("short plan");
            assert_eq!(short.outcome, TeamOutcome::Incomplete, "size {team_size}");
            assert_eq!(short.checked_in, team_size - 1);

            let warned: Vec<(i64, i64)> = preview
                .warnings
                .iter()
                .map(|w| (w.team_id, w.current))
                .collect();
            assert_eq!(warned, vec![(short_id, team_size - 1)]);
        }
    }
}

#[tokio::test]
async fn confirm_keeps_bench_members_without_checkin() {
    for team_size in [6, 8] {
        let db = temp_pool().await;
        let pool = db.pool();
        let tournament_id = checkin_tournament(pool, team_size).await;
        let (first_id, first_bench) =
            seat_team(pool, tournament_id, "Erste", 1_000, team_size, false).await;
        let (second_id, second_bench) =
            seat_team(pool, tournament_id, "Zweite", 2_000, team_size, false).await;

        let preview = finalize_checkin(
            pool,
            tournament_id,
            FinalizeCheckinParams {
                fill_from_solos: true,
                ..Default::default()
            },
        )
        .await
        .expect("preview");
        assert!(preview.warnings.is_empty(), "size {team_size}");
        assert!(preview.removed_players.is_empty(), "size {team_size}");

        finalize_checkin(
            pool,
            tournament_id,
            FinalizeCheckinParams {
                confirm: true,
                allowed_team_ids: HashSet::new(),
                actor_id: None,
                expected_snapshot_token: Some(&preview.snapshot_token),
                advance_to_group_phase: false,
                fill_from_solos: true,
            },
        )
        .await
        .expect("confirm");

        for (team_id, bench_id) in [(first_id, first_bench), (second_id, second_bench)] {
            let (starters, bench): (i64, Vec<i64>) = sqlx::query_as(
                "SELECT COUNT(*) FILTER (WHERE NOT is_bench), \
                        COALESCE(array_agg(discord_id) FILTER (WHERE is_bench), '{}') \
                 FROM turnier.team_members WHERE team_id = $1",
            )
            .bind(team_id)
            .fetch_one(pool)
            .await
            .expect("lineup");
            assert_eq!(starters, team_size);
            assert_eq!(bench, vec![bench_id]);
        }
    }
}

#[tokio::test]
async fn lineup_move_invalidates_the_preview_token() {
    let db = temp_pool().await;
    let pool = db.pool();
    let tournament_id = checkin_tournament(pool, 6).await;
    let (team_id, bench_id) = seat_team(pool, tournament_id, "Wechsel", 1_000, 6, true).await;
    seat_team(pool, tournament_id, "Andere", 2_000, 6, false).await;

    let preview = finalize_checkin(pool, tournament_id, FinalizeCheckinParams::default())
        .await
        .expect("preview");

    sqlx::query(
        "UPDATE turnier.team_members SET is_bench = (discord_id = $2) \
         WHERE team_id = $1 AND discord_id IN ($2, $3)",
    )
    .bind(team_id)
    .bind(1_005_i64)
    .bind(bench_id)
    .execute(pool)
    .await
    .expect("swap");

    let after = finalize_checkin(pool, tournament_id, FinalizeCheckinParams::default())
        .await
        .expect("second preview");
    assert_ne!(preview.snapshot_token, after.snapshot_token);
}

#[tokio::test]
async fn fill_mode_keeps_a_team_whose_checked_in_bench_remains() {
    let db = temp_pool().await;
    let pool = db.pool();
    let tournament_id = checkin_tournament(pool, 6).await;
    let (kept_id, kept_bench) = seat_team(pool, tournament_id, "Bank da", 1_000, 0, true).await;
    let (gone_id, gone_bench) = seat_team(pool, tournament_id, "Bank weg", 2_000, 0, false).await;

    let preview = finalize_checkin(
        pool,
        tournament_id,
        FinalizeCheckinParams {
            fill_from_solos: true,
            ..Default::default()
        },
    )
    .await
    .expect("preview");

    let kept = preview
        .team_plans
        .iter()
        .find(|p| p.team_id == kept_id)
        .expect("kept plan");
    assert_eq!(kept.outcome, TeamOutcome::Incomplete);
    assert!(kept
        .roster
        .iter()
        .any(|r| r.discord_id == kept_bench.to_string() && r.change == RosterChange::Stays));

    let gone = preview
        .team_plans
        .iter()
        .find(|p| p.team_id == gone_id)
        .expect("gone plan");
    assert_eq!(gone.outcome, TeamOutcome::Deleted);
    assert!(gone
        .roster
        .iter()
        .all(|r| r.change == RosterChange::Removed));
    let removed: Vec<String> = preview
        .removed_players
        .iter()
        .map(|p| p.discord_id.clone())
        .collect();
    assert_eq!(removed, vec![gone_bench.to_string()]);
}

#[tokio::test]
async fn kept_bench_only_team_gets_a_starting_captain() {
    let db = temp_pool().await;
    let pool = db.pool();
    let tournament_id = checkin_tournament(pool, 6).await;
    let (team_id, bench_id) = seat_team(pool, tournament_id, "Nur Bank", 1_000, 0, true).await;
    let absent = 900;
    insert_team_member(pool, team_id, absent, "Abwesend", "captain", 0).await;
    insert_signup(pool, tournament_id, absent, "Abwesend", Some(team_id), 0).await;
    sqlx::query("UPDATE turnier.teams SET captain_discord_id = $2 WHERE id = $1")
        .bind(team_id)
        .bind(absent)
        .execute(pool)
        .await
        .expect("absent captain");

    let preview = finalize_checkin(
        pool,
        tournament_id,
        FinalizeCheckinParams {
            fill_from_solos: true,
            ..Default::default()
        },
    )
    .await
    .expect("preview");

    finalize_checkin(
        pool,
        tournament_id,
        FinalizeCheckinParams {
            confirm: true,
            allowed_team_ids: HashSet::from([team_id]),
            actor_id: None,
            expected_snapshot_token: Some(&preview.snapshot_token),
            advance_to_group_phase: false,
            fill_from_solos: true,
        },
    )
    .await
    .expect("confirm");

    let members: Vec<(i64, String, bool)> = sqlx::query_as(
        "SELECT discord_id, role, is_bench FROM turnier.team_members WHERE team_id = $1",
    )
    .bind(team_id)
    .fetch_all(pool)
    .await
    .expect("members");
    assert_eq!(members, vec![(bench_id, "captain".to_string(), false)]);
    let captain: i64 =
        sqlx::query_scalar("SELECT captain_discord_id FROM turnier.teams WHERE id = $1")
            .bind(team_id)
            .fetch_one(pool)
            .await
            .expect("captain");
    assert_eq!(captain, bench_id);
}
