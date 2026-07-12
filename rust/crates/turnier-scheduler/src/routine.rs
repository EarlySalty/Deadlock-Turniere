//! Reine Zeitentscheidung fuer den woechentlichen Routine-Slot.

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, NaiveTime, Utc, Weekday};

use crate::{SchedulerError, SchedulerResult};

/// Konfigurierbarer Wochenrhythmus. Zeitwerte sind UTC, damit der Dienst ohne
/// systemabhaengige lokale Zeitzone deterministisch bleibt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineSchedule {
    pub weekday: Weekday,
    pub start_time: NaiveTime,
    pub lead: Duration,
    pub checkin_lead: Duration,
    pub bracket_delay: Duration,
}

/// Aus dem naechsten Wochen-Slot abgeleitete Phasen-Zeitpunkte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutinePlan {
    pub registration_start: DateTime<Utc>,
    pub registration_end: DateTime<Utc>,
    pub checkin_start: DateTime<Utc>,
    pub event_start: DateTime<Utc>,
    pub bracket_start: DateTime<Utc>,
}

/// Vollstaendige Entscheidung eines Scheduler-Ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutineDecision {
    Due(RoutinePlan),
    NotDue {
        due_at: DateTime<Utc>,
        event_start: DateTime<Utc>,
    },
}

/// Aufgeloeste Laufzeit-Konfiguration des Routine-Checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineSettings {
    pub enabled: bool,
    pub preset_id: i64,
    pub announcement_channel_id: i64,
    pub schedule: RoutineSchedule,
}

impl RoutineSettings {
    pub fn from_config(config: &turnier_config::Config) -> SchedulerResult<Self> {
        if !config.routine_tournaments_enabled {
            return Ok(Self {
                enabled: false,
                preset_id: 0,
                announcement_channel_id: config.discord_tournament_lobby_channel_id,
                schedule: RoutineSchedule {
                    weekday: Weekday::Sat,
                    start_time: NaiveTime::MIN,
                    lead: Duration::zero(),
                    checkin_lead: Duration::zero(),
                    bracket_delay: Duration::zero(),
                },
            });
        }
        if config.routine_tournament_preset_id <= 0 {
            return Err(SchedulerError::InvalidRoutineConfig(
                "ROUTINE_TOURNAMENT_PRESET_ID muss positiv sein".to_string(),
            ));
        }
        let weekday = parse_weekday(&config.routine_tournament_weekday).ok_or_else(|| {
            SchedulerError::InvalidRoutineConfig(
                "ROUTINE_TOURNAMENT_WEEKDAY muss monday..sunday sein".to_string(),
            )
        })?;
        let start_time = NaiveTime::parse_from_str(&config.routine_tournament_time_utc, "%H:%M")
            .map_err(|_| {
                SchedulerError::InvalidRoutineConfig(
                    "ROUTINE_TOURNAMENT_TIME_UTC muss HH:MM sein".to_string(),
                )
            })?;
        for (name, value) in [
            (
                "ROUTINE_TOURNAMENT_LEAD_DAYS",
                config.routine_tournament_lead_days,
            ),
            (
                "ROUTINE_TOURNAMENT_CHECKIN_LEAD_MINUTES",
                config.routine_tournament_checkin_lead_minutes,
            ),
            (
                "ROUTINE_TOURNAMENT_BRACKET_DELAY_MINUTES",
                config.routine_tournament_bracket_delay_minutes,
            ),
        ] {
            if value < 0 {
                return Err(SchedulerError::InvalidRoutineConfig(format!(
                    "{name} darf nicht negativ sein"
                )));
            }
        }
        Ok(Self {
            enabled: true,
            preset_id: config.routine_tournament_preset_id,
            announcement_channel_id: config.discord_tournament_lobby_channel_id,
            schedule: RoutineSchedule {
                weekday,
                start_time,
                lead: Duration::days(config.routine_tournament_lead_days),
                checkin_lead: Duration::minutes(config.routine_tournament_checkin_lead_minutes),
                bracket_delay: Duration::minutes(config.routine_tournament_bracket_delay_minutes),
            },
        })
    }
}

fn parse_weekday(value: &str) -> Option<Weekday> {
    match value.trim().to_ascii_lowercase().as_str() {
        "monday" => Some(Weekday::Mon),
        "tuesday" => Some(Weekday::Tue),
        "wednesday" => Some(Weekday::Wed),
        "thursday" => Some(Weekday::Thu),
        "friday" => Some(Weekday::Fri),
        "saturday" => Some(Weekday::Sat),
        "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

impl RoutineSchedule {
    pub fn decide(&self, now: DateTime<Utc>) -> RoutineDecision {
        let today = now.date_naive();
        let days = (7 + self.weekday.num_days_from_monday() as i64
            - today.weekday().num_days_from_monday() as i64)
            % 7;
        let mut event_start =
            NaiveDateTime::new(today + Duration::days(days), self.start_time).and_utc();
        if event_start <= now {
            event_start += Duration::weeks(1);
        }
        let due_at = event_start - self.lead;
        if now < due_at {
            return RoutineDecision::NotDue {
                due_at,
                event_start,
            };
        }
        let checkin_start = event_start - self.checkin_lead;
        RoutineDecision::Due(RoutinePlan {
            registration_start: now,
            registration_end: checkin_start,
            checkin_start,
            event_start,
            bracket_start: event_start + self.bracket_delay,
        })
    }
}
