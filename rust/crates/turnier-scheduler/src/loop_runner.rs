//! Hintergrund-Loop: Routine-Erzeugung, Phasen und Reminder im 60-s-Takt.
//!
//! Portiert `start_scheduler`/`_run_all_checks` (Z.503-521) und die
//! Phasen-Kaskade `_check_and_advance_tournaments`/`_advance_due_tournament`
//! (Z.198-291).
//!
//! ## Loop-Robustheit (safe-Fix, behebt den Tot-Loop)
//! Im Original propagiert eine Exception aus `_run_all_checks` aus
//! `start_scheduler` heraus und TÖTET den Task dauerhaft. Hier ist jeder der vier
//! Checks einzeln fehlertolerant (Result loggen, weitermachen); der Loop stirbt
//! nie — er endet nur über das `shutdown`-Signal (entspricht `CancelledError`).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::watch;

use turnier_automatik::{presets, routine as routine_store};
use turnier_core::TournamentMode;
use turnier_db::Pool;
use turnier_discord::DiscordNotifier;
use turnier_match::MatchManager;

use crate::reminders::{
    check_and_send_match_reminders, check_and_send_registration_reminders,
    check_and_send_start_reminders, load_all_profile_ids, load_tournament_participant_ids,
};
use crate::transition::{advance_tournament_status, get_due_next_status, DueStatusRow};
use crate::{RoutineDecision, RoutineSettings, SchedulerResult};

/// Loop-Intervall in Sekunden (wie im Original `SCHEDULER_INTERVAL_SECONDS`).
pub const SCHEDULER_INTERVAL_SECONDS: u64 = 60;
pub const ROUTINE_PROPOSAL_INTERVAL_SECONDS: u64 = 60 * 60;

/// Gebündelte Handles des Schedulers. Werden vom App-Bootstrap (turnier-bot/turnier-api)
/// gebaut und an [`start_scheduler`] übergeben.
#[derive(Clone)]
pub struct Scheduler {
    pool: Pool,
    matchmgr: Arc<MatchManager>,
    notifier: DiscordNotifier,
    routine: RoutineSettings,
    settings: turnier_config::SchedulerConfig,
}

impl Scheduler {
    /// Baut den Scheduler aus seinen Abhängigkeiten.
    pub fn new(
        pool: Pool,
        matchmgr: Arc<MatchManager>,
        notifier: DiscordNotifier,
        config: &turnier_config::Config,
    ) -> SchedulerResult<Self> {
        Ok(Self {
            pool,
            matchmgr,
            notifier,
            routine: RoutineSettings::from_config(config)?,
            settings: config.scheduler.clone(),
        })
    }

    /// Führt alle Checks EINMAL aus — jeder fehlertolerant. Ein Fehler in
    /// einem Check beendet weder die anderen noch den Loop.
    pub async fn run_all_checks(&self, now: DateTime<Utc>) {
        if let Err(err) = self.check_and_advance_tournaments(now).await {
            tracing::error!(error = %err, "Phasen-Kaskade fehlgeschlagen");
        }
        if let Err(err) =
            check_and_send_registration_reminders(&self.pool, &self.notifier, now).await
        {
            tracing::error!(error = %err, "Registrierungs-Reminder-Check fehlgeschlagen");
        }
        if let Err(err) = check_and_send_start_reminders(&self.pool, &self.notifier, now).await {
            tracing::error!(error = %err, "Start-Reminder-Check fehlgeschlagen");
        }
        if let Err(err) = check_and_send_match_reminders(&self.pool, &self.notifier).await {
            tracing::error!(error = %err, "Match-Reminder-Check fehlgeschlagen");
        }
    }

    pub async fn run_routine_check(&self, now: DateTime<Utc>) {
        self.check_routine_tournament(now).await;
    }

    async fn check_routine_tournament(&self, now: DateTime<Utc>) {
        if !self.routine.enabled {
            tracing::info!(
                decision = "skipped",
                reason = "feature_disabled",
                "Routine-Scheduler-Entscheidung"
            );
            return;
        }
        let plan = match self.routine.schedule.decide(now) {
            RoutineDecision::NotDue {
                due_at,
                event_start,
            } => {
                tracing::info!(decision = "skipped", reason = "not_due", %due_at, %event_start, "Routine-Scheduler-Entscheidung");
                return;
            }
            RoutineDecision::Due(plan) => plan,
        };
        let preset = match presets::get(&self.pool, self.routine.preset_id).await {
            Ok(Some(preset)) if preset.active => preset,
            Ok(Some(_)) => {
                tracing::warn!(
                    decision = "skipped",
                    reason = "preset_inactive",
                    preset_id = self.routine.preset_id,
                    "Routine-Scheduler-Entscheidung"
                );
                return;
            }
            Ok(None) => {
                tracing::warn!(
                    decision = "skipped",
                    reason = "preset_missing",
                    preset_id = self.routine.preset_id,
                    "Routine-Scheduler-Entscheidung"
                );
                return;
            }
            Err(err) => {
                tracing::error!(decision = "error", reason = "preset_load_failed", preset_id = self.routine.preset_id, error = %err, "Routine-Scheduler-Entscheidung");
                return;
            }
        };
        let db_plan = routine_store::RoutineTournamentPlan {
            registration_start: plan.registration_start,
            registration_end: plan.registration_end,
            checkin_start: plan.checkin_start,
            event_start: plan.event_start,
            bracket_start: if preset.tournament_mode == TournamentMode::BracketOnly {
                plan.event_start
            } else {
                plan.bracket_start
            },
        };
        let ensured = match routine_store::ensure_routine_proposal(&self.pool, &preset, &db_plan)
            .await
        {
            Ok(value) => value,
            Err(err) => {
                tracing::error!(decision = "error", reason = "proposal_create_failed", preset_id = preset.id, event_start = %plan.event_start, error = %err, "Routine-Scheduler-Entscheidung");
                return;
            }
        };
        if ensured.created {
            tracing::info!(decision = "created", proposal_id = ensured.id, preset_id = preset.id, event_start = %plan.event_start, "Routine-Scheduler-Entscheidung");
        } else {
            tracing::info!(decision = "skipped", reason = "proposal_exists", proposal_id = ensured.id, state = ensured.state, event_start = %plan.event_start, "Routine-Scheduler-Entscheidung");
        }

        match self
            .notifier
            .request_proposal_publish(ensured.id, self.routine.proposal_channel_id)
            .await
        {
            Ok(_) => tracing::info!(
                decision = "publish_requested",
                proposal_id = ensured.id,
                "Routine-Scheduler-Entscheidung"
            ),
            Err(err) => {
                tracing::error!(decision = "error", reason = "proposal_publish_failed", proposal_id = ensured.id, error = %err, "Routine-Scheduler-Entscheidung")
            }
        }
    }

    /// Phasen-Kaskade: loopt über alle Turniere in den aktiven Phasen und schiebt
    /// fällige weiter, bis in einem ganzen Durchlauf keines mehr advanciert.
    /// Portiert `_check_and_advance_tournaments` (Z.272-291).
    async fn check_and_advance_tournaments(&self, now: DateTime<Utc>) -> sqlx::Result<()> {
        loop {
            let tournaments = self.load_advanceable_tournaments().await?;
            let mut advanced_any = false;
            for row in tournaments {
                if self.advance_due_tournament(&row, now).await {
                    advanced_any = true;
                }
            }
            if !advanced_any {
                return Ok(());
            }
        }
    }

    /// Lädt die für die Kaskade relevanten Felder aller Turniere in den aktiven
    /// Phasen. Entspricht dem `SELECT *` des Originals, beschränkt auf die für
    /// [`get_due_next_status`] und die Benachrichtigung benötigten Spalten.
    async fn load_advanceable_tournaments(&self) -> sqlx::Result<Vec<AdvanceRow>> {
        sqlx::query_as::<_, AdvanceRow>(
            "SELECT id, status, tournament_mode, registration_start, registration_end, \
                    checkin_start, group_phase_start, bracket_start, is_test \
             FROM turnier.tournaments \
             WHERE status IN ('registration', 'checkin', 'group_phase') \
                OR (status = 'draft' AND source <> 'routine') \
             ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Schiebt ein einzelnes fälliges Turnier weiter und verschickt die
    /// Checkin-/Registration-Notifications. Portiert `_advance_due_tournament`
    /// (Z.198-269). Rückgabe `true`, wenn das Turnier advanciert wurde.
    async fn advance_due_tournament(&self, row: &AdvanceRow, now: DateTime<Utc>) -> bool {
        let due_row = DueStatusRow {
            status: row.status.clone(),
            tournament_mode: row.tournament_mode.clone(),
            registration_start: row.registration_start,
            registration_end: row.registration_end,
            checkin_start: row.checkin_start,
            group_phase_start: row.group_phase_start,
            bracket_start: row.bracket_start,
        };
        let Some(next_status) = get_due_next_status(&due_row, now) else {
            return false;
        };

        match advance_tournament_status(
            &self.pool,
            &self.matchmgr,
            &self.notifier,
            row.id,
            &row.status,
            next_status,
            "scheduler",
            None,
        )
        .await
        {
            Ok(_) => {}
            Err(crate::SchedulerError::InvalidTransition(msg)) => {
                tracing::warn!(
                    tournament_id = row.id,
                    next_status,
                    "Scheduler konnte Turnier nicht verschieben: {msg}"
                );
                return false;
            }
            Err(crate::SchedulerError::ActiveTournamentConflict(msg)) => {
                tracing::warn!(
                    tournament_id = row.id,
                    next_status,
                    "Scheduler überspringt Turnier: {msg}"
                );
                return false;
            }
            Err(crate::SchedulerError::StatusConflict) => {
                tracing::info!(
                    tournament_id = row.id,
                    "Scheduler hat Turnier übersprungen, weil der Status parallel geändert wurde"
                );
                return false;
            }
            Err(err) => {
                tracing::error!(tournament_id = row.id, next_status, error = %err, "Statuswechsel fehlgeschlagen");
                return false;
            }
        }

        tracing::info!(
            tournament_id = row.id,
            from = %row.status,
            to = next_status,
            "Scheduler hat Turnier verschoben"
        );

        // Test-Turniere lösen keine Benachrichtigungen aus.
        if row.is_test {
            return true;
        }

        // Checkin-/Registration-Notifications — best-effort (Fehler nur geloggt).
        if let Err(err) = self.notify_after_advance(row.id, next_status).await {
            tracing::error!(
                tournament_id = row.id,
                next_status,
                error = %err,
                "Scheduler notifications failed"
            );
        }
        true
    }

    /// Sendet nach dem Statuswechsel die passende DM:
    /// `checkin` → an Teilnehmer; `registration` → an alle Profile.
    async fn notify_after_advance(
        &self,
        tournament_id: i64,
        next_status: &str,
    ) -> sqlx::Result<()> {
        match next_status {
            "checkin" => {
                let ids = load_tournament_participant_ids(&self.pool, tournament_id).await?;
                let _ = self
                    .notifier
                    .notify_users(
                        &ids,
                        turnier_discord::NotificationEvent::Checkin,
                        &format!("Der Check-in für Turnier #{tournament_id} ist jetzt geöffnet."),
                    )
                    .await;
            }
            "registration" => {
                let ids = load_all_profile_ids(&self.pool).await?;
                let _ = self
                    .notifier
                    .notify_users(
                        &ids,
                        turnier_discord::NotificationEvent::TournamentNews,
                        &format!(
                            "Die Registrierung für Turnier #{tournament_id} ist jetzt geöffnet."
                        ),
                    )
                    .await;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Eine `tournaments`-Zeile für die Phasen-Kaskade.
#[derive(sqlx::FromRow)]
struct AdvanceRow {
    id: i64,
    status: String,
    tournament_mode: Option<String>,
    registration_start: Option<DateTime<Utc>>,
    registration_end: Option<DateTime<Utc>>,
    checkin_start: Option<DateTime<Utc>>,
    group_phase_start: Option<DateTime<Utc>>,
    bracket_start: Option<DateTime<Utc>>,
    is_test: bool,
}

/// Startet den Hintergrund-Loop: Match-/Reminder-Checks minuetlich,
/// Routine-Vorschlaege beim Start und danach stuendlich. Der Loop endet erst, wenn über
/// `shutdown` ein `true` gesendet (oder der Sender fallengelassen) wird; das
/// entspricht dem `CancelledError`-Pfad des Originals.
pub async fn start_scheduler(scheduler: Scheduler, mut shutdown: watch::Receiver<bool>) {
    tracing::info!("Tournament-Scheduler gestartet");

    // Erster Lauf SOFORT (wie im Original vor der Schleife).
    let now = Utc::now();
    scheduler.run_routine_check(now).await;
    scheduler.run_all_checks(now).await;
    tracing::info!(
        anchor = "turnier-scheduler-toml-heartbeat",
        "Scheduler-Prüflauf beendet"
    );

    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(
        scheduler.settings.tick_seconds,
    ));
    // Den sofort feuernden ersten Tick verwerfen — der erste Lauf ist schon erfolgt.
    ticker.tick().await;
    let mut routine_ticker = tokio::time::interval(std::time::Duration::from_secs(
        scheduler.settings.routine_seconds,
    ));
    routine_ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                scheduler.run_all_checks(Utc::now()).await;
                tracing::info!(anchor = "turnier-scheduler-toml-heartbeat", "Scheduler-Prüflauf beendet");
            }
            _ = routine_ticker.tick() => {
                scheduler.run_routine_check(Utc::now()).await;
            }
            res = shutdown.changed() => {
                // Sender meldet Shutdown ODER wurde fallengelassen → beenden.
                if res.is_err() || *shutdown.borrow() {
                    tracing::info!("Tournament-Scheduler gestoppt");
                    return;
                }
            }
        }
    }
}
