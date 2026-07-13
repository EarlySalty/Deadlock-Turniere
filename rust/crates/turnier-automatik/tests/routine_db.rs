use chrono::{DateTime, Duration, Utc};
use turnier_automatik::presets::{self, Category, NewPreset, PresetConfig};
use turnier_automatik::routine::{
    ensure_routine_proposal, ensure_routine_tournament, load_invitation_candidate_ids,
    RoutineTournamentPlan,
};
use turnier_core::{BracketFormat, InviteMode, TournamentGameMode, TournamentMode};
use turnier_db::{test_pool, Pool};

const CREATOR: &str = "123456789012345600";

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid timestamp")
        .with_timezone(&Utc)
}

fn preset_config() -> PresetConfig {
    PresetConfig {
        team_size: 6,
        bracket_format: BracketFormat::SingleElimination,
        series_format: 1,
        final_series_format: Some(3),
        tournament_mode: TournamentMode::GroupStage,
        tournament_game_mode: TournamentGameMode::Standard,
        match_objective: "auto".to_string(),
        invite_mode: InviteMode::Always,
        reminder_offsets: Some("[1440,120,15]".to_string()),
        start_reminder_offsets: Some("[1440,60]".to_string()),
        rules: Some("rules".to_string()),
        description_template: Some("description".to_string()),
    }
}

#[tokio::test]
async fn aktives_preset_erzeugt_genau_einen_entwurf_mit_remindern() {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool();
    let preset = presets::create(
        pool,
        &NewPreset {
            name: "Routine Cup".to_string(),
            category: Category::Fun,
            config: preset_config(),
            active: true,
            created_by: CREATOR.to_string(),
        },
    )
    .await
    .expect("create preset");
    let plan = RoutineTournamentPlan {
        registration_start: utc("2026-07-11T18:01:00Z"),
        registration_end: utc("2026-07-18T17:30:00Z"),
        checkin_start: utc("2026-07-18T17:30:00Z"),
        event_start: utc("2026-07-18T18:00:00Z"),
        bracket_start: utc("2026-07-18T21:00:00Z"),
    };

    let first = ensure_routine_tournament(pool, &preset, &plan)
        .await
        .expect("first ensure");
    let second = ensure_routine_tournament(pool, &preset, &plan)
        .await
        .expect("second ensure");

    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.id, second.id);
    let row: (String, String, i64, serde_json::Value, serde_json::Value) = sqlx::query_as(
        "SELECT status, source, preset_id, reminder_offsets, start_reminder_offsets \
         FROM turnier.tournaments WHERE id = $1",
    )
    .bind(first.id)
    .fetch_one(pool)
    .await
    .expect("load tournament");
    assert_eq!(row.0, "draft");
    assert_eq!(row.1, "routine");
    assert_eq!(row.2, preset.id);
    assert_eq!(row.3, serde_json::json!([1440, 120, 15]));
    assert_eq!(row.4, serde_json::json!([1440, 60]));
}

#[tokio::test]
async fn routine_slot_erzeugt_nur_einen_offenen_vorschlag() {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool();
    let preset = presets::create(
        pool,
        &NewPreset {
            name: "Routine Cup".to_string(),
            category: Category::Fun,
            config: preset_config(),
            active: true,
            created_by: CREATOR.to_string(),
        },
    )
    .await
    .expect("create preset");
    let plan = RoutineTournamentPlan {
        registration_start: utc("2026-07-11T18:01:00Z"),
        registration_end: utc("2026-07-18T17:30:00Z"),
        checkin_start: utc("2026-07-18T17:30:00Z"),
        event_start: utc("2026-07-18T18:00:00Z"),
        bracket_start: utc("2026-07-18T21:00:00Z"),
    };

    let first = ensure_routine_proposal(pool, &preset, &plan)
        .await
        .expect("first ensure");
    let second = ensure_routine_proposal(pool, &preset, &plan)
        .await
        .expect("second ensure");

    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.id, second.id);
    assert_eq!(first.state, "pending_approval");
    let tournament_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turnier.tournaments")
        .fetch_one(pool)
        .await
        .expect("count tournaments");
    assert_eq!(tournament_count, 0);
}

async fn insert_tournament(pool: &Pool, name: &str, status: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
         (name, status, team_size, bracket_format, created_by, created_at, updated_at, \
          invite_mode, tournament_mode, series_format, exclude_from_leaderboard, \
          tournament_game_mode, auto_lobby_enabled, is_test, match_objective, \
          no_show_grace_minutes, source) \
         VALUES ($1, $2, 6, 'single_elimination', 1, now(), now(), 'always', \
                 'group_stage', 1, false, 'standard', false, false, 'auto', 10, 'test') \
         RETURNING id",
    )
    .bind(name)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("insert tournament")
}

#[tokio::test]
async fn kandidaten_sind_fruehere_teilnehmer_oder_juengst_voice_aktive() {
    let db = test_pool().await.expect("central test pool");
    let pool = db.pool();
    let current = insert_tournament(pool, "Aktuell", "registration").await;
    let previous = insert_tournament(pool, "Vorher", "completed").await;
    let now = utc("2026-07-12T12:00:00Z");

    for (tournament_id, discord_id) in [(previous, 101_i64), (current, 101), (current, 202)] {
        sqlx::query(
            "INSERT INTO turnier.tournament_signups (tournament_id, discord_id, signed_up_at) \
             VALUES ($1, $2, now())",
        )
        .bind(tournament_id)
        .bind(discord_id)
        .execute(pool)
        .await
        .expect("insert signup");
    }
    for (discord_id, occurred_at) in [
        (303_i64, now - Duration::days(2)),
        (404_i64, now - Duration::days(15)),
    ] {
        sqlx::query(
            "INSERT INTO activity.voice_metadata_events \
             (user_id, guild_id, channel_id, event_type, occurred_at) \
             VALUES ($1, 999, 888, 'join', $2)",
        )
        .bind(discord_id)
        .bind(occurred_at)
        .execute(pool)
        .await
        .expect("insert voice event");
    }

    let candidates = load_invitation_candidate_ids(pool, current, 999, now)
        .await
        .expect("load candidates");

    assert_eq!(candidates, vec!["303"]);
}
