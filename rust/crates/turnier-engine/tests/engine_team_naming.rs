//! Paritätstest: Team-Namensvergabe. Portiert aus
//! `backend/tests/test_engine_team_naming.py` (2 Fälle).
//!
//! Der Python-Test patcht `random.shuffle` auf einen No-op, damit die
//! Signup-Reihenfolge erhalten bleibt. Hier injizieren wir denselben Effekt über
//! [`NoShuffle`] — die Reihenfolge bleibt wie geladen, sodass die geprüften Namen
//! WÖRTLICH reproduziert werden.

#![cfg(feature = "testing")]

mod common;

use common::{
    insert_checkin, insert_signup, insert_team, insert_team_member, insert_tournament, temp_pool,
    NullResolver,
};
use sqlx::Row;
use std::collections::HashSet;
use turnier_engine::{assign_random_teams, finalize_checkin, FinalizeCheckinParams, NoShuffle};

#[tokio::test]
async fn assign_random_teams_uses_captain_names_and_suffixes() {
    let db = temp_pool().await;
    let pool = db.pool();

    let tournament_id = insert_tournament(
        pool,
        "Captain Names",
        "registration",
        2,
        "single_elimination",
        "bracket_only",
    )
    .await;

    let signups = [
        (100, "Earlysalty"),
        (101, "Mate One"),
        (200, "Earlysalty"),
        (201, "Mate Two"),
    ];
    for (discord_id, discord_name) in signups {
        insert_signup(pool, tournament_id, discord_id, discord_name, None, 0).await;
    }

    let resolver = NullResolver;
    let mut shuffler = NoShuffle;
    let teams_created = assign_random_teams(pool, &resolver, tournament_id, 2, &mut shuffler)
        .await
        .expect("assign");
    assert_eq!(teams_created, 2);

    let rows = sqlx::query("SELECT name FROM turnier.teams WHERE tournament_id = $1 ORDER BY id")
        .bind(tournament_id)
        .fetch_all(pool)
        .await
        .expect("teams");
    let team_names: Vec<String> = rows.iter().map(|r| r.get::<String, _>("name")).collect();
    assert_eq!(
        team_names,
        vec![
            "Earlysalty Team".to_string(),
            "Earlysalty Team (2)".to_string()
        ]
    );
}

#[tokio::test]
async fn finalize_checkin_keeps_manual_name_and_uses_captain_name_for_new_team() {
    let db = temp_pool().await;
    let pool = db.pool();

    let tournament_id = insert_tournament(
        pool,
        "Finalize",
        "checkin",
        2,
        "single_elimination",
        "bracket_only",
    )
    .await;

    let manual_team_id = insert_team(pool, tournament_id, "Nova Team", 111).await;

    for (discord_id, discord_name, role) in [
        (111, "Manual Captain", "captain"),
        (112, "Manual Mate", "member"),
    ] {
        insert_team_member(pool, manual_team_id, discord_id, discord_name, role, 0).await;
    }

    let existing_signups = [
        (111, "Manual Captain", Some(manual_team_id)),
        (112, "Manual Mate", Some(manual_team_id)),
        (211, "Nova", None),
        (212, "Pool Mate", None),
    ];
    for (discord_id, discord_name, team_id) in existing_signups {
        insert_signup(pool, tournament_id, discord_id, discord_name, team_id, 0).await;
        insert_checkin(pool, tournament_id, discord_id).await;
    }

    let preview = finalize_checkin(pool, tournament_id, FinalizeCheckinParams::default())
        .await
        .expect("dry run");
    finalize_checkin(
        pool,
        tournament_id,
        FinalizeCheckinParams {
            confirm: true,
            allowed_team_ids: HashSet::new(),
            actor_id: None,
            expected_snapshot_token: Some(&preview.snapshot_token),
            advance_to_group_phase: false,
        },
    )
    .await
    .expect("confirm");

    let rows = sqlx::query(
        "SELECT name, captain_discord_id FROM turnier.teams \
         WHERE tournament_id = $1 ORDER BY id",
    )
    .bind(tournament_id)
    .fetch_all(pool)
    .await
    .expect("teams");
    let teams: Vec<(String, i64)> = rows
        .iter()
        .map(|r| {
            (
                r.get::<String, _>("name"),
                r.get::<i64, _>("captain_discord_id"),
            )
        })
        .collect();

    assert_eq!(teams[0].0, "Nova Team");
    assert_eq!(teams[1].0, "Nova Team (2)");
    assert_eq!(teams[1].1, 211);
}

#[tokio::test]
async fn finalize_checkin_rolls_back_team_creation_when_audit_fails() {
    let db = temp_pool().await;
    let pool = db.pool();

    let tournament_id = insert_tournament(
        pool,
        "Finalize Rollback",
        "checkin",
        2,
        "single_elimination",
        "bracket_only",
    )
    .await;

    for (discord_id, discord_name) in [(501, "Rollback Captain"), (502, "Rollback Mate")] {
        insert_signup(pool, tournament_id, discord_id, discord_name, None, 0).await;
        insert_checkin(pool, tournament_id, discord_id).await;
    }

    let preview = finalize_checkin(pool, tournament_id, FinalizeCheckinParams::default())
        .await
        .expect("dry run");

    let err = finalize_checkin(
        pool,
        tournament_id,
        FinalizeCheckinParams {
            confirm: true,
            allowed_team_ids: HashSet::new(),
            actor_id: Some("not-a-discord-id"),
            expected_snapshot_token: Some(&preview.snapshot_token),
            advance_to_group_phase: false,
        },
    )
    .await
    .expect_err("invalid audit actor rolls back");
    assert!(err.to_string().contains("Discord-ID"));

    let team_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnier.teams WHERE tournament_id = $1")
            .bind(tournament_id)
            .fetch_one(pool)
            .await
            .expect("team count");
    assert_eq!(team_count, 0);

    let assigned_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.tournament_signups \
         WHERE tournament_id = $1 AND team_id IS NOT NULL",
    )
    .bind(tournament_id)
    .fetch_one(pool)
    .await
    .expect("assigned signups");
    assert_eq!(assigned_count, 0);
}
