//! Hintergrund-Loop: vier fehlertolerante Checks im 60-s-Takt.
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

use chrono::{Local, NaiveDateTime};
use tokio::sync::watch;

use tb_db::Pool;
use tb_discord::DiscordNotifier;
use tb_match::MatchManager;

use crate::reminders::{
    check_and_send_match_reminders, check_and_send_registration_reminders,
    check_and_send_start_reminders, load_all_profile_ids, load_tournament_participant_ids,
};
use crate::transition::{advance_tournament_status, get_due_next_status, DueStatusRow};

/// Loop-Intervall in Sekunden (wie im Original `SCHEDULER_INTERVAL_SECONDS`).
pub const SCHEDULER_INTERVAL_SECONDS: u64 = 60;

/// Gebündelte Handles des Schedulers. Werden vom App-Bootstrap (tb-app/tb-web)
/// gebaut und an [`start_scheduler`] übergeben.
#[derive(Clone)]
pub struct Scheduler {
    pool: Pool,
    matchmgr: Arc<MatchManager>,
    notifier: DiscordNotifier,
}

impl Scheduler {
    /// Baut den Scheduler aus seinen Abhängigkeiten.
    pub fn new(pool: Pool, matchmgr: Arc<MatchManager>, notifier: DiscordNotifier) -> Self {
        Self {
            pool,
            matchmgr,
            notifier,
        }
    }

    /// Führt alle vier Checks EINMAL aus — jeder fehlertolerant. Ein Fehler in
    /// einem Check beendet weder die anderen noch den Loop.
    pub async fn run_all_checks(&self, now: NaiveDateTime) {
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

    /// Phasen-Kaskade: loopt über alle Turniere in den aktiven Phasen und schiebt
    /// fällige weiter, bis in einem ganzen Durchlauf keines mehr advanciert.
    /// Portiert `_check_and_advance_tournaments` (Z.272-291).
    async fn check_and_advance_tournaments(&self, now: NaiveDateTime) -> sqlx::Result<()> {
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
             FROM tournaments \
             WHERE status IN ('draft', 'registration', 'checkin', 'group_phase') \
             ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Schiebt ein einzelnes fälliges Turnier weiter und verschickt die
    /// Checkin-/Registration-Notifications. Portiert `_advance_due_tournament`
    /// (Z.198-269). Rückgabe `true`, wenn das Turnier advanciert wurde.
    async fn advance_due_tournament(&self, row: &AdvanceRow, now: NaiveDateTime) -> bool {
        let due_row = DueStatusRow {
            status: row.status.clone(),
            tournament_mode: row.tournament_mode.clone(),
            registration_start: row.registration_start.clone(),
            registration_end: row.registration_end.clone(),
            checkin_start: row.checkin_start.clone(),
            group_phase_start: row.group_phase_start.clone(),
            bracket_start: row.bracket_start.clone(),
        };
        let Some(next_status) = get_due_next_status(&due_row, now) else {
            return false;
        };

        // Single-Active-Invariante: vor der Aktivierung (→ registration) darf kein
        // anderes Nicht-Test-Turnier aktiv sein.
        if next_status == "registration" {
            match self.has_other_active_tournament(row.id).await {
                Ok(true) => {
                    tracing::warn!(
                        tournament_id = row.id,
                        "Scheduler überspringt Turnier: anderes aktives Turnier blockiert Aktivierung"
                    );
                    return false;
                }
                Ok(false) => {}
                Err(err) => {
                    tracing::error!(tournament_id = row.id, error = %err, "Aktiv-Check fehlgeschlagen");
                    return false;
                }
            }
        }

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
        if row.is_test != 0 {
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
    async fn notify_after_advance(&self, tournament_id: i64, next_status: &str) -> sqlx::Result<()> {
        match next_status {
            "checkin" => {
                let ids = load_tournament_participant_ids(&self.pool, tournament_id).await?;
                let _ = self
                    .notifier
                    .notify_users(
                        &ids,
                        tb_discord::NotificationEvent::Checkin,
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
                        tb_discord::NotificationEvent::TournamentNews,
                        &format!("Die Registrierung für Turnier #{tournament_id} ist jetzt geöffnet."),
                    )
                    .await;
            }
            _ => {}
        }
        Ok(())
    }

    /// `true`, wenn ein ANDERES Nicht-Test-Turnier aktiv ist
    /// (registration/checkin/group_phase/bracket). Portiert
    /// `_has_other_active_tournament` (Z.102-110).
    async fn has_other_active_tournament(&self, tournament_id: i64) -> sqlx::Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM tournaments \
             WHERE id != ? \
               AND status IN ('registration', 'checkin', 'group_phase', 'bracket') \
               AND is_test = 0 LIMIT 1",
        )
        .bind(tournament_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }
}

/// Eine `tournaments`-Zeile für die Phasen-Kaskade.
#[derive(sqlx::FromRow)]
struct AdvanceRow {
    id: i64,
    status: String,
    tournament_mode: Option<String>,
    registration_start: Option<String>,
    registration_end: Option<String>,
    checkin_start: Option<String>,
    group_phase_start: Option<String>,
    bracket_start: Option<String>,
    is_test: i64,
}

/// Startet den Hintergrund-Loop: alle vier Checks EINMAL sofort, danach im
/// 60-s-Takt — jeder Tick fehlertolerant. Der Loop endet erst, wenn über
/// `shutdown` ein `true` gesendet (oder der Sender fallengelassen) wird; das
/// entspricht dem `CancelledError`-Pfad des Originals.
pub async fn start_scheduler(scheduler: Scheduler, mut shutdown: watch::Receiver<bool>) {
    tracing::info!("Tournament-Scheduler gestartet");

    // Erster Lauf SOFORT (wie im Original vor der Schleife).
    scheduler.run_all_checks(Local::now().naive_local()).await;

    let mut ticker =
        tokio::time::interval(std::time::Duration::from_secs(SCHEDULER_INTERVAL_SECONDS));
    // Den sofort feuernden ersten Tick verwerfen — der erste Lauf ist schon erfolgt.
    ticker.tick().await;

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                scheduler.run_all_checks(Local::now().naive_local()).await;
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
