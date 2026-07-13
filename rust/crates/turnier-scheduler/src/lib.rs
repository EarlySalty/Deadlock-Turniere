//! `turnier-scheduler` — Phasenübergänge + Reminder-Hintergrund-Loop des
//! Turnier-Backends.
//!
//! Portiert `backend/tournament/scheduler.py` 1:1 im Verhalten. Zwei
//! Verantwortungen:
//!
//! 1. **Geteilte Orchestrierung** [`advance_tournament_status`]: wechselt den
//!    Turnierstatus, generiert Seiteneffekte (Gruppen/Matches/Bracket), schreibt
//!    den Status mit Optimistic-Lock, auditet, berechnet bei `completed` die
//!    Punkte neu und plant Auto-Lobbys. Wird vom Loop UND von turnier-api (Admin-Routen)
//!    aufgerufen.
//! 2. **Hintergrund-Loop** [`start_scheduler`]: führt im 60-s-Takt vier Checks aus
//!    — Phasen-Kaskade + drei Reminder-Klassen. Jeder Check ist fehlertolerant;
//!    der Loop stirbt nie (behebt den Tot-Loop des Originals).
//!
//! ## Bewusst erhaltene Befunde (`bugs_preserved`)
//! - **`completed`-Zweig** (needs-decision): der Scheduler erzeugt nie `completed`
//!   als next_status; der Zweig greift nur bei externen Aufrufern (turnier-api).
//! - **5-Minuten-Reminder-Fenster ohne Catch-up** (needs-decision): verpasste
//!   Fenster nach einem Ausfall werden nicht nachgeholt.
//! - **bracket_only-Logikfalle** (needs-decision): bei `bracket_only` bleibt das
//!   Turnier in `checkin`, bis `bracket_start` fällig ist (`group_phase_start`
//!   dient nur als Auslöse-Gate).

pub mod error;
pub mod loop_runner;
pub mod reminders;
pub mod routine;
pub mod time;
pub mod transition;

pub use error::{SchedulerError, SchedulerResult};
pub use loop_runner::{
    start_scheduler, Scheduler, ROUTINE_PROPOSAL_INTERVAL_SECONDS, SCHEDULER_INTERVAL_SECONDS,
};
pub use reminders::{
    check_and_send_match_reminders, check_and_send_registration_reminders,
    check_and_send_start_reminders, load_all_profile_ids, load_tournament_participant_ids,
};
pub use routine::{RoutineDecision, RoutinePlan, RoutineSchedule, RoutineSettings};
pub use time::{
    is_due, is_within_window, offset_label, parse_reminder_offsets, DEFAULT_REMINDER_OFFSETS,
    REMINDER_WINDOW_MINUTES,
};
pub use transition::{
    acquire_single_active_tournament_lock, advance_tournament_status, get_due_next_status,
    DueStatusRow, SINGLE_ACTIVE_TOURNAMENT_ADVISORY_LOCK_KEY,
};
