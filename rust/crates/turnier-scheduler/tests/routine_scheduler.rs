mod common;

use chrono::{DateTime, Utc};
use common::{fake_match_manager, fake_notifier, temp_db, test_config};
use turnier_automatik::presets::{self, Category, NewPreset, PresetConfig};
use turnier_core::{BracketFormat, InviteMode, TournamentGameMode, TournamentMode};
use turnier_scheduler::Scheduler;

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid timestamp")
        .with_timezone(&Utc)
}

#[tokio::test]
async fn faelliger_check_erstellt_nur_einen_routine_vorschlag() {
    let db = temp_db().await;
    let pool = db.pool().clone();
    let preset = presets::create(
        &pool,
        &NewPreset {
            name: "Routine Cup".to_string(),
            category: Category::Fun,
            config: PresetConfig {
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
                rules: None,
                description_template: Some("description".to_string()),
            },
            active: true,
            created_by: "123456789012345700".to_string(),
        },
    )
    .await
    .expect("create preset");
    let mut config = test_config();
    config.routine_tournaments_enabled = true;
    config.routine_tournament_preset_id = preset.id;
    config.routine_tournament_weekday = "saturday".to_string();
    config.routine_tournament_time_utc = "18:00".to_string();
    config.routine_tournament_lead_days = 7;
    config.routine_tournament_checkin_lead_minutes = 30;
    config.routine_tournament_bracket_delay_minutes = 180;
    let scheduler = Scheduler::new(
        pool.clone(),
        fake_match_manager(pool.clone(), &config),
        fake_notifier(pool.clone(), &config),
        &config,
    )
    .expect("scheduler config");
    let now = utc("2026-07-11T18:01:00Z");

    scheduler.run_routine_check(now).await;
    scheduler.run_routine_check(now).await;

    let proposals: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, state::text FROM turnier.tournament_proposals \
         WHERE source = 'bot' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .expect("load proposals");
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].1, "pending_approval");
    let tournament_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnier.tournaments WHERE source = 'routine'")
            .fetch_one(&pool)
            .await
            .expect("tournament count");
    assert_eq!(tournament_count, 0);
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.audit_log \
         WHERE action = 'tournament_auto_advance' AND details->>'to' = 'registration'",
    )
    .fetch_one(&pool)
    .await
    .expect("audit count");
    assert_eq!(audit_count, 0);
    let dm_attempts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnier.discord_tasks WHERE type = 'SEND_DM'")
            .fetch_one(&pool)
            .await
            .expect("DM attempts");
    assert_eq!(dm_attempts, 0, "proposal creation must not invite by DM");
}
