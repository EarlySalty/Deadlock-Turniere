//! Phasen-Übergangs-Logik: nächster fälliger Status + geteilte Orchestrierung.
//!
//! [`advance_tournament_status`] ist die EINZIGE nach außen sichtbare Mutation
//! und wird sowohl vom Hintergrund-Loop ([`crate::loop_runner`]) als auch von den
//! Admin-Routen in turnier-api aufgerufen. Sie validiert den Übergang, generiert
//! Seiteneffekte (Gruppen/Matches/Bracket), schreibt den Status mit Optimistic-
//! Lock, auditet, berechnet bei `completed` die Punkte neu und plant Auto-Lobbys.

use chrono::{DateTime, Utc};
use sqlx::{Postgres, Row, Transaction};

use turnier_core::{now_utc, parse_discord_id, TournamentMode, TournamentStatus};
use turnier_db::Pool;
use turnier_discord::DiscordNotifier;
use turnier_engine::league::DEFAULT_SLOT_MINUTES;
use turnier_engine::{
    generate_bracket_in_tx, generate_group_matches_in_tx, generate_groups_in_tx,
    generate_league_plan_in_tx, is_valid_transition_for_mode, recalculate_player_points_in_tx,
    valid_next_statuses_for_mode,
};
use turnier_match::league::open_league_match_count;
use turnier_match::MatchManager;

use crate::error::{SchedulerError, SchedulerResult};
use crate::start_gap::{acquire_tournament_schedule_lock, ensure_start_gap};
use crate::time::is_due;

/// Die für die Übergangsentscheidung relevanten Felder einer `tournaments`-Zeile.
///
/// Die Zeitstempel kommen aus PG-`TIMESTAMPTZ`-Spalten als UTC-Instants.
#[derive(Debug, Clone)]
pub struct DueStatusRow {
    pub status: TournamentStatus,
    pub tournament_mode: TournamentMode,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub checkin_start: Option<DateTime<Utc>>,
    pub group_phase_start: Option<DateTime<Utc>>,
    pub bracket_start: Option<DateTime<Utc>>,
}

/// Ermittelt den nächsten fälligen Status anhand der Zeitstempel und des Modus.
///
/// Liefert `None`, wenn aktuell kein Übergang fällig ist. Den Check-in beendet
/// nie der Scheduler, sondern immer das Orga-Team über `finalize-checkin`, damit
/// Vorschau, Team-Bildung und Entwurfsschutz greifen.
pub fn get_due_next_status(row: &DueStatusRow, now: DateTime<Utc>) -> Option<TournamentStatus> {
    let checkin_trigger = row.checkin_start.as_ref().or(row.registration_end.as_ref());
    let league = row.tournament_mode == TournamentMode::League;
    match row.status {
        TournamentStatus::Draft if is_due(row.registration_start.as_ref(), now) => {
            Some(TournamentStatus::Registration)
        }
        TournamentStatus::Registration if is_due(checkin_trigger, now) => {
            Some(TournamentStatus::Checkin)
        }
        TournamentStatus::GroupPhase if !league && is_due(row.bracket_start.as_ref(), now) => {
            Some(TournamentStatus::Bracket)
        }
        _ => None,
    }
}

/// Wechselt den Turnierstatus und führt notwendige Seiteneffekte aus.
///
/// Geteilte Orchestrierung: wird vom Scheduler-Loop UND von turnier-api (Admin-Routen)
/// aufgerufen. Portiert `advance_tournament_status` (Z.130-195) im Verhalten 1:1:
///
/// 1. Übergang gegen die Status-Übergangstabelle validieren
///    ([`is_valid_transition_for_mode`]); ungültig → [`SchedulerError::InvalidTransition`].
/// 2. Status per Optimistic-Lock setzen
///    (`UPDATE ... WHERE id = $n AND status = current`); traf keine Zeile →
///    [`SchedulerError::StatusConflict`].
/// 3. Bei `group_phase`: Gruppen + Gruppen-Matches generieren; bei `bracket`: das
///    Bracket generieren. Generierung und Status-CAS laufen in derselben
///    Transaktion.
/// 4. Audit-Log schreiben: `tournament_auto_advance` bei `source == "scheduler"`,
///    sonst `tournament_advance`; `details` = JSON-Metadata.
/// 5. Bei `completed` und nicht `exclude_from_leaderboard`: Punkte neu berechnen.
/// 6. Bei `group_phase`/`bracket`: Auto-Lobbys best-effort planen (Fehler nur
///    geloggt).
///
/// Rückgabe: die JSON-Metadata (`tournament_id`, `from`, `to`, `source`, plus
/// optional `groups_created`/`matches_created`/`bracket_matches_created`).
///
/// Der `matchmgr` liefert `schedule_auto_lobbies_for_tournament`; der `notifier`
/// wird hier nicht direkt verwendet (Reminder/Checkin-DMs sendet der Loop), bleibt
/// aber Teil der Signatur, damit turnier-api mit denselben Handles aufrufen kann.
#[allow(clippy::too_many_arguments)]
pub async fn advance_tournament_status(
    pool: &Pool,
    matchmgr: &MatchManager,
    _notifier: &DiscordNotifier,
    tournament_id: i64,
    current_status: TournamentStatus,
    next_status: TournamentStatus,
    source: &str,
    actor_id: Option<&str>,
) -> SchedulerResult<serde_json::Value> {
    let mut tx = pool.begin().await?;
    if matches!(
        next_status,
        TournamentStatus::Registration | TournamentStatus::GroupPhase
    ) {
        acquire_tournament_schedule_lock(&mut tx).await?;
    }
    // Modus und Status bleiben während Prüfung und Generierung unverändert.
    let Some(locked) = turnier_engine::lock_tournament_row(&mut tx, tournament_id).await? else {
        return Err(SchedulerError::StatusConflict);
    };
    let stored_status: TournamentStatus = locked.try_get("status")?;
    if stored_status != current_status {
        return Err(SchedulerError::StatusConflict);
    }
    ensure_not_leaving_checkin(current_status, next_status)?;
    let mode: TournamentMode = locked
        .try_get("tournament_mode")
        .map_err(|_| SchedulerError::InvalidTransition("Unbekannter Turniermodus".to_string()))?;
    if !is_valid_transition_for_mode(current_status, next_status, mode) {
        let allowed = valid_next_statuses_for_mode(current_status, mode)
            .iter()
            .map(|status| status.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let allowed = if allowed.is_empty() {
            "keine".to_string()
        } else {
            allowed
        };
        return Err(SchedulerError::InvalidTransition(format!(
            "Ungültiger Status-Übergang: {current_status} -> {next_status}. Erlaubt: {allowed}"
        )));
    }

    if mode == TournamentMode::League && next_status == TournamentStatus::Completed {
        ensure_league_is_finished(&mut tx, tournament_id).await?;
    }
    if mode != TournamentMode::League && next_status == TournamentStatus::Completed {
        ensure_bracket_is_finished(&mut tx, tournament_id).await?;
    }

    // --- Metadata-Basis ---
    let mut metadata = serde_json::json!({
        "tournament_id": tournament_id,
        "from": current_status,
        "to": next_status,
        "source": source,
    });

    // --- 2.-5. Statuswechsel + Seiteneffekte + Audit + ggf. Punkte: EINE Transaktion ---
    if next_status == TournamentStatus::Registration {
        ensure_start_gap(&mut tx, tournament_id).await?;
    }

    let now = now_utc();
    let res = sqlx::query(
        "UPDATE turnier.tournaments SET status = $1, updated_at = $2 \
         WHERE id = $3 AND status = $4",
    )
    .bind(next_status)
    .bind(now)
    .bind(tournament_id)
    .bind(current_status)
    .execute(&mut *tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(SchedulerError::StatusConflict);
    }

    if next_status == TournamentStatus::GroupPhase && mode == TournamentMode::League {
        let plan = generate_league_plan_in_tx(&mut tx, tournament_id, DEFAULT_SLOT_MINUTES).await?;
        metadata["groups_created"] = serde_json::json!(1);
        metadata["matches_created"] = serde_json::json!(plan.matches);
    } else if next_status == TournamentStatus::GroupPhase {
        let group_ids = generate_groups_in_tx(&mut tx, tournament_id, None).await?;
        let match_count: i64 = generate_group_matches_in_tx(&mut tx, tournament_id).await?;
        metadata["groups_created"] = serde_json::json!(group_ids.len());
        metadata["matches_created"] = serde_json::json!(match_count);
    } else if next_status == TournamentStatus::Bracket {
        let match_count: i64 = generate_bracket_in_tx(&mut tx, tournament_id).await?;
        metadata["bracket_matches_created"] = serde_json::json!(match_count);
    }

    let action = if source == "scheduler" {
        "tournament_auto_advance"
    } else {
        "tournament_advance"
    };
    let actor_id = actor_id.map(parse_discord_id).transpose()?;
    sqlx::query(
        "INSERT INTO turnier.audit_log (action, user_id, details, created_at) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(action)
    .bind(actor_id)
    .bind(metadata.clone())
    .bind(now)
    .execute(&mut *tx)
    .await?;

    if next_status == TournamentStatus::Completed {
        let exclude: Option<(bool,)> = sqlx::query_as(
            "SELECT exclude_from_leaderboard FROM turnier.tournaments WHERE id = $1",
        )
        .bind(tournament_id)
        .fetch_optional(&mut *tx)
        .await?;
        if matches!(exclude, Some((false,))) {
            recalculate_player_points_in_tx(&mut tx, tournament_id).await?;
        }
    }

    tx.commit().await?;

    // --- 6. Auto-Lobbys best-effort ---
    schedule_lobbies_best_effort(matchmgr, tournament_id, next_status).await;

    Ok(metadata)
}

async fn ensure_league_is_finished(
    tx: &mut Transaction<'_, Postgres>,
    tournament_id: i64,
) -> SchedulerResult<()> {
    let open = open_league_match_count(&mut **tx, tournament_id).await?;
    if open > 0 {
        return Err(SchedulerError::InvalidTransition(format!(
            "Das Turnier kann erst abgeschlossen werden, wenn alle Spiele gewertet sind. Offen: {open}"
        )));
    }
    Ok(())
}

pub async fn ensure_bracket_is_finished(
    conn: &mut sqlx::PgConnection,
    tournament_id: i64,
) -> SchedulerResult<()> {
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM turnier.bracket_matches \
         WHERE tournament_id = $1 AND status NOT IN ('completed', 'forfeit', 'cancelled')",
    )
    .bind(tournament_id)
    .fetch_one(&mut *conn)
    .await?;
    if open > 0 {
        return Err(SchedulerError::InvalidTransition(format!(
            "Das Turnier kann erst abgeschlossen werden, wenn alle Bracket-Spiele gewertet sind. Offen: {open}"
        )));
    }
    Ok(())
}

pub const CHECKIN_EXIT_MESSAGE: &str = "Check-in bitte über „Check-in abschließen“ beenden";

pub fn ensure_not_leaving_checkin(
    current_status: TournamentStatus,
    next_status: TournamentStatus,
) -> SchedulerResult<()> {
    if current_status == TournamentStatus::Checkin && next_status != TournamentStatus::Checkin {
        return Err(SchedulerError::InvalidTransition(
            CHECKIN_EXIT_MESSAGE.to_string(),
        ));
    }
    Ok(())
}

/// Plant Auto-Lobbys nach `group_phase`/`bracket`; Fehler werden nur geloggt
/// (best-effort, wie im Original Z.185-193).
pub async fn schedule_lobbies_best_effort(
    matchmgr: &MatchManager,
    tournament_id: i64,
    next_status: TournamentStatus,
) {
    if next_status.is_match_phase() {
        if let Err(err) = matchmgr
            .schedule_auto_lobbies_for_tournament(tournament_id)
            .await
        {
            tracing::error!(
                tournament_id,
                next_status = %next_status,
                error = %err,
                "Auto-Lobby-Scheduling nach Statuswechsel fehlgeschlagen"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("{s}Z"))
            .unwrap()
            .with_timezone(&Utc)
    }

    fn now() -> DateTime<Utc> {
        utc("2026-06-14T12:00:00")
    }

    fn row(status: TournamentStatus) -> DueStatusRow {
        DueStatusRow {
            status,
            tournament_mode: TournamentMode::GroupStage,
            registration_start: None,
            registration_end: None,
            checkin_start: None,
            group_phase_start: None,
            bracket_start: None,
        }
    }

    #[test]
    fn draft_zu_registration_wenn_registration_start_faellig() {
        let mut r = row(TournamentStatus::Draft);
        r.registration_start = Some(utc("2026-06-14T11:00:00"));
        assert_eq!(
            get_due_next_status(&r, now()),
            Some(TournamentStatus::Registration)
        );
        // Noch nicht fällig.
        r.registration_start = Some(utc("2026-06-14T13:00:00"));
        assert_eq!(get_due_next_status(&r, now()), None);
    }

    #[test]
    fn registration_zu_checkin_trigger_prioritaet() {
        // checkin_start hat Vorrang vor registration_end.
        let mut r = row(TournamentStatus::Registration);
        r.checkin_start = Some(utc("2026-06-14T11:00:00"));
        r.registration_end = Some(utc("2026-06-14T13:00:00"));
        assert_eq!(
            get_due_next_status(&r, now()),
            Some(TournamentStatus::Checkin)
        );

        // Ohne checkin_start fällt es auf registration_end zurück.
        let mut r2 = row(TournamentStatus::Registration);
        r2.registration_end = Some(utc("2026-06-14T11:30:00"));
        assert_eq!(
            get_due_next_status(&r2, now()),
            Some(TournamentStatus::Checkin)
        );
    }

    #[test]
    fn checkin_never_ends_by_time_in_any_mode() {
        for mode in [
            TournamentMode::GroupStage,
            TournamentMode::BracketOnly,
            TournamentMode::League,
        ] {
            let mut r = row(TournamentStatus::Checkin);
            r.tournament_mode = mode;
            r.group_phase_start = Some(utc("2026-06-14T11:00:00"));
            r.bracket_start = Some(utc("2026-06-14T11:30:00"));
            assert_eq!(get_due_next_status(&r, now()), None);
        }
    }

    #[test]
    fn group_phase_zu_bracket() {
        let mut r = row(TournamentStatus::GroupPhase);
        r.bracket_start = Some(utc("2026-06-14T11:00:00"));
        assert_eq!(
            get_due_next_status(&r, now()),
            Some(TournamentStatus::Bracket)
        );
        r.bracket_start = Some(utc("2026-06-14T13:00:00"));
        assert_eq!(get_due_next_status(&r, now()), None);
    }

    #[test]
    fn kein_uebergang_ohne_zeitstempel() {
        assert_eq!(
            get_due_next_status(&row(TournamentStatus::Draft), now()),
            None
        );
        assert_eq!(
            get_due_next_status(&row(TournamentStatus::Registration), now()),
            None
        );
        assert_eq!(
            get_due_next_status(&row(TournamentStatus::Checkin), now()),
            None
        );
        assert_eq!(
            get_due_next_status(&row(TournamentStatus::GroupPhase), now()),
            None
        );
        // Unbekannter/Endstatus → None.
        assert_eq!(
            get_due_next_status(&row(TournamentStatus::Bracket), now()),
            None
        );
        assert_eq!(
            get_due_next_status(&row(TournamentStatus::Completed), now()),
            None
        );
    }
}
