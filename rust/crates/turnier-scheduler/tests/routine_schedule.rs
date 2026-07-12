use chrono::{DateTime, Duration, NaiveTime, Utc, Weekday};
use turnier_config::Config;
use turnier_scheduler::{RoutineDecision, RoutineSchedule, RoutineSettings};

fn utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid timestamp")
        .with_timezone(&Utc)
}

fn schedule() -> RoutineSchedule {
    RoutineSchedule {
        weekday: Weekday::Sat,
        start_time: NaiveTime::from_hms_opt(18, 0, 0).expect("valid time"),
        lead: Duration::days(7),
        checkin_lead: Duration::minutes(30),
        bracket_delay: Duration::hours(3),
    }
}

#[test]
fn woechentlicher_slot_wird_ab_vorlauf_faellig() {
    let now = utc("2026-07-11T18:01:00Z");
    let RoutineDecision::Due(plan) = schedule().decide(now) else {
        panic!("next Saturday must be due inside the seven-day lead");
    };

    assert_eq!(plan.event_start, utc("2026-07-18T18:00:00Z"));
    assert_eq!(plan.registration_start, now);
    assert_eq!(plan.registration_end, utc("2026-07-18T17:30:00Z"));
    assert_eq!(plan.checkin_start, utc("2026-07-18T17:30:00Z"));
    assert_eq!(plan.bracket_start, utc("2026-07-18T21:00:00Z"));
}

#[test]
fn slot_ist_vor_dem_vorlauf_noch_nicht_faellig() {
    let now = utc("2026-07-10T17:59:00Z");
    let schedule = RoutineSchedule {
        lead: Duration::days(1),
        ..schedule()
    };

    assert_eq!(
        schedule.decide(now),
        RoutineDecision::NotDue {
            due_at: utc("2026-07-10T18:00:00Z"),
            event_start: utc("2026-07-11T18:00:00Z"),
        }
    );
}

#[test]
fn config_parst_wochentag_und_utc_uhrzeit() {
    let mut config = Config::from_env();
    config.routine_tournaments_enabled = true;
    config.routine_tournament_preset_id = 42;
    config.routine_tournament_weekday = "friday".to_string();
    config.routine_tournament_time_utc = "20:30".to_string();
    config.routine_tournament_lead_days = 7;
    config.routine_tournament_checkin_lead_minutes = 30;
    config.routine_tournament_bracket_delay_minutes = 180;

    let settings = RoutineSettings::from_config(&config).expect("valid routine config");
    assert!(settings.enabled);
    assert_eq!(settings.preset_id, 42);
    assert_eq!(settings.schedule.weekday, Weekday::Fri);
    assert_eq!(
        settings.schedule.start_time,
        NaiveTime::from_hms_opt(20, 30, 0).expect("valid time")
    );
}

#[test]
fn deaktivierter_scheduler_ignoriert_unvollstaendige_routine_config() {
    let mut config = Config::from_env();
    config.routine_tournaments_enabled = false;
    config.routine_tournament_preset_id = 0;
    config.routine_tournament_weekday = "ungueltig".to_string();

    let settings = RoutineSettings::from_config(&config).expect("disabled is always safe");
    assert!(!settings.enabled);
}
