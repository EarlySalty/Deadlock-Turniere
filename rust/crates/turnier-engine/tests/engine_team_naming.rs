//! Paritätstest: Team-Namensvergabe. Portiert aus
//! `backend/tests/test_engine_team_naming.py` (2 Fälle).
//!
//! Der Python-Test patcht `random.shuffle` auf einen No-op, damit die
//! Signup-Reihenfolge erhalten bleibt. Hier injizieren wir denselben Effekt über
//! [`NoShuffle`] — die Reihenfolge bleibt wie geladen, sodass die geprüften Namen
//! WÖRTLICH reproduziert werden.

mod common;

use common::{temp_pool, NullResolver};
use sqlx::Row;
use std::collections::HashSet;
use turnier_engine::{assign_random_teams, finalize_checkin, FinalizeCheckinParams, NoShuffle};

#[tokio::test]
async fn assign_random_teams_uses_captain_names_and_suffixes() {
    let pool = temp_pool().await;

    sqlx::query(
        "INSERT INTO tournaments (name, status, team_size, created_by, updated_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("Captain Names")
    .bind("registration")
    .bind(2)
    .bind("admin")
    .bind("now")
    .execute(&pool)
    .await
    .expect("insert tournament");

    let signups = [
        ("100", "Earlysalty"),
        ("101", "Mate One"),
        ("200", "Earlysalty"),
        ("201", "Mate Two"),
    ];
    for (discord_id, discord_name) in signups {
        sqlx::query(
            "INSERT INTO tournament_signups \
             (tournament_id, discord_id, discord_name, steam_id, rank, rank_score) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(1)
        .bind(discord_id)
        .bind(discord_name)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(0)
        .execute(&pool)
        .await
        .expect("insert signup");
    }

    let resolver = NullResolver;
    let mut shuffler = NoShuffle;
    let teams_created = assign_random_teams(&pool, &resolver, 1, 2, &mut shuffler)
        .await
        .expect("assign");
    assert_eq!(teams_created, 2);

    let rows = sqlx::query("SELECT name FROM teams WHERE tournament_id = 1 ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("teams");
    let team_names: Vec<String> = rows.iter().map(|r| r.get::<String, _>("name")).collect();
    assert_eq!(
        team_names,
        vec!["Earlysalty Team".to_string(), "Earlysalty Team (2)".to_string()]
    );
}

#[tokio::test]
async fn finalize_checkin_keeps_manual_name_and_uses_captain_name_for_new_team() {
    let pool = temp_pool().await;

    sqlx::query(
        "INSERT INTO tournaments (name, status, team_size, created_by, updated_at, tournament_mode) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("Finalize")
    .bind("checkin")
    .bind(2)
    .bind("admin")
    .bind("now")
    .bind("bracket_only")
    .execute(&pool)
    .await
    .expect("insert tournament");

    sqlx::query(
        "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?)",
    )
    .bind(1)
    .bind("Nova Team")
    .bind("nova team")
    .bind("111")
    .execute(&pool)
    .await
    .expect("insert team");

    for (discord_id, discord_name, role) in
        [("111", "Manual Captain", "captain"), ("112", "Manual Mate", "member")]
    {
        sqlx::query(
            "INSERT INTO team_members (team_id, discord_id, discord_name, role) VALUES (?, ?, ?, ?)",
        )
        .bind(1)
        .bind(discord_id)
        .bind(discord_name)
        .bind(role)
        .execute(&pool)
        .await
        .expect("insert member");
    }

    let existing_signups = [
        ("111", "Manual Captain", Some(1)),
        ("112", "Manual Mate", Some(1)),
        ("211", "Nova", None),
        ("212", "Pool Mate", None),
    ];
    for (discord_id, discord_name, team_id) in existing_signups {
        sqlx::query(
            "INSERT INTO tournament_signups \
             (tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(1)
        .bind(discord_id)
        .bind(discord_name)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(0)
        .bind(team_id)
        .execute(&pool)
        .await
        .expect("insert signup");
        sqlx::query("INSERT INTO tournament_checkins (tournament_id, discord_id) VALUES (?, ?)")
            .bind(1)
            .bind(discord_id)
            .execute(&pool)
            .await
            .expect("insert checkin");
    }

    let preview = finalize_checkin(&pool, 1, FinalizeCheckinParams::default())
        .await
        .expect("dry run");
    finalize_checkin(
        &pool,
        1,
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

    let rows = sqlx::query("SELECT name, captain_discord_id FROM teams WHERE tournament_id = 1 ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("teams");
    let teams: Vec<(String, String)> = rows
        .iter()
        .map(|r| {
            (
                r.get::<String, _>("name"),
                r.get::<String, _>("captain_discord_id"),
            )
        })
        .collect();

    assert_eq!(teams[0].0, "Nova Team");
    assert_eq!(teams[1].0, "Nova Team (2)");
    assert_eq!(teams[1].1, "211");
}
