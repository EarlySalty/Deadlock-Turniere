//! Phasen-Übergangs-Logik: nächster fälliger Status + geteilte Orchestrierung.
//!
//! [`advance_tournament_status`] ist die EINZIGE nach außen sichtbare Mutation
//! und wird sowohl vom Hintergrund-Loop ([`crate::loop_runner`]) als auch von den
//! Admin-Routen in turnier-api aufgerufen. Sie validiert den Übergang, generiert
//! Seiteneffekte (Gruppen/Matches/Bracket), schreibt den Status mit Optimistic-
//! Lock, auditet, berechnet bei `completed` die Punkte neu und plant Auto-Lobbys.

use chrono::{DateTime, Utc};

use turnier_core::{now_utc, parse_discord_id, TournamentStatus};
use turnier_db::Pool;
use turnier_discord::DiscordNotifier;
use turnier_engine::{
    generate_bracket, generate_group_matches, generate_groups, is_valid_transition,
    recalculate_player_points_in_tx, valid_next_statuses,
};
use turnier_match::MatchManager;

use crate::error::{SchedulerError, SchedulerResult};
use crate::time::is_due;

/// Die für die Übergangsentscheidung relevanten Felder einer `tournaments`-Zeile.
///
/// Die Zeitstempel kommen aus PG-`TIMESTAMPTZ`-Spalten als UTC-Instants. Das
/// Feld `status`/`tournament_mode` kommt weiter roh, damit die Entscheidung 1:1
/// die Original-String-Vergleiche abbildet.
#[derive(Debug, Clone, Default)]
pub struct DueStatusRow {
    pub status: String,
    pub tournament_mode: Option<String>,
    pub registration_start: Option<DateTime<Utc>>,
    pub registration_end: Option<DateTime<Utc>>,
    pub checkin_start: Option<DateTime<Utc>>,
    pub group_phase_start: Option<DateTime<Utc>>,
    pub bracket_start: Option<DateTime<Utc>>,
}

/// Ermittelt den nächsten fälligen Status anhand der Zeitstempel und des Modus.
///
/// Portiert `_get_due_next_status` (Z.68-92). Liefert `None`, wenn aktuell kein
/// Übergang fällig ist. Bei `tournament_mode == "bracket_only"` wird die
/// Gruppenphase übersprungen (checkin → bracket), aber NUR wenn `bracket_start`
/// fällig ist (Logikfalle aus dem Original, bewusst 1:1 erhalten — `bugs_preserved`).
pub fn get_due_next_status(row: &DueStatusRow, now: DateTime<Utc>) -> Option<&'static str> {
    let s = row.status.as_str();

    if s == "draft" && is_due(row.registration_start.as_ref(), now) {
        return Some("registration");
    }

    // checkin_start ODER registration_end als Auslöser (wie im Original).
    let checkin_trigger = row.checkin_start.as_ref().or(row.registration_end.as_ref());
    if s == "registration" && is_due(checkin_trigger, now) {
        return Some("checkin");
    }

    let bracket_only = row.tournament_mode.as_deref() == Some("bracket_only");

    if s == "checkin" && is_due(row.group_phase_start.as_ref(), now) {
        if bracket_only {
            // Gruppenphase überspringen — aber erst springen, wenn bracket_start
            // fällig ist. group_phase_start dient hier nur als Auslöse-Gate.
            // (Logikfalle 1:1 erhalten, siehe Modul-Doku.)
            return if is_due(row.bracket_start.as_ref(), now) {
                Some("bracket")
            } else {
                None
            };
        }
        return Some("group_phase");
    }

    if s == "group_phase" && is_due(row.bracket_start.as_ref(), now) {
        return Some("bracket");
    }

    None
}

/// Übersetzt einen Status-String in das typsichere [`TournamentStatus`]-Enum.
fn parse_status(value: &str) -> Option<TournamentStatus> {
    serde_json::from_value(serde_json::Value::String(value.to_string())).ok()
}

/// Wechselt den Turnierstatus und führt notwendige Seiteneffekte aus.
///
/// Geteilte Orchestrierung — wird vom Scheduler-Loop UND von turnier-api (Admin-Routen)
/// aufgerufen. Portiert `advance_tournament_status` (Z.130-195) im Verhalten 1:1:
///
/// 1. Übergang gegen die Status-Übergangstabelle validieren
///    ([`is_valid_transition`]); ungültig → [`SchedulerError::InvalidTransition`].
/// 2. Bei `group_phase`: Gruppen + Gruppen-Matches generieren; bei `bracket`: das
///    Bracket generieren. (Generierung läuft VOR dem UPDATE — wie im Original;
///    siehe `bugs_preserved`.)
/// 3. Status per Optimistic-Lock setzen
///    (`UPDATE ... WHERE id = $n AND status = current`); traf keine Zeile →
///    [`SchedulerError::StatusConflict`].
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
    current_status: &str,
    next_status: &str,
    source: &str,
    actor_id: Option<&str>,
) -> SchedulerResult<serde_json::Value> {
    // --- 1. Übergang validieren ---
    let from = parse_status(current_status);
    let to = parse_status(next_status);
    let transition_ok = matches!((from, to), (Some(f), Some(t)) if is_valid_transition(f, t));
    if !transition_ok {
        let allowed = from
            .map(|f| {
                valid_next_statuses(f)
                    .iter()
                    .map(status_as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "keine".to_string());
        return Err(SchedulerError::InvalidTransition(format!(
            "Ungültiger Status-Übergang: {current_status} -> {next_status}. Erlaubt: {allowed}"
        )));
    }

    // --- Metadata-Basis ---
    let mut metadata = serde_json::json!({
        "tournament_id": tournament_id,
        "from": current_status,
        "to": next_status,
        "source": source,
    });

    // --- 2. Seiteneffekt-Generierung (VOR dem UPDATE, wie im Original) ---
    if next_status == "group_phase" {
        let group_ids = generate_groups(pool, tournament_id, None).await?;
        let match_count = generate_group_matches(pool, tournament_id).await?;
        metadata["groups_created"] = serde_json::json!(group_ids.len());
        metadata["matches_created"] = serde_json::json!(match_count);
    } else if next_status == "bracket" {
        let match_count = generate_bracket(pool, tournament_id).await?;
        metadata["bracket_matches_created"] = serde_json::json!(match_count);
    }

    // --- 3.-5. Statuswechsel + Audit + ggf. Punkte: EINE Transaktion ---
    let mut tx = pool.begin().await?;

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

    if next_status == "completed" {
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

/// Plant Auto-Lobbys nach `group_phase`/`bracket` — Fehler werden nur geloggt
/// (best-effort, wie im Original Z.185-193).
async fn schedule_lobbies_best_effort(
    matchmgr: &MatchManager,
    tournament_id: i64,
    next_status: &str,
) {
    if next_status == "group_phase" || next_status == "bracket" {
        if let Err(err) = matchmgr
            .schedule_auto_lobbies_for_tournament(tournament_id)
            .await
        {
            tracing::error!(
                tournament_id,
                next_status,
                error = %err,
                "Auto-Lobby-Scheduling nach Statuswechsel fehlgeschlagen"
            );
        }
    }
}

/// Der kleingeschriebene Wire-String eines [`TournamentStatus`] (für die
/// „Erlaubt: …"-Fehlermeldung).
fn status_as_str(s: &TournamentStatus) -> &'static str {
    use TournamentStatus::*;
    match s {
        Draft => "draft",
        Registration => "registration",
        Checkin => "checkin",
        GroupPhase => "group_phase",
        Bracket => "bracket",
        Completed => "completed",
        Archived => "archived",
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

    fn row(status: &str) -> DueStatusRow {
        DueStatusRow {
            status: status.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn draft_zu_registration_wenn_registration_start_faellig() {
        let mut r = row("draft");
        r.registration_start = Some(utc("2026-06-14T11:00:00"));
        assert_eq!(get_due_next_status(&r, now()), Some("registration"));
        // Noch nicht fällig.
        r.registration_start = Some(utc("2026-06-14T13:00:00"));
        assert_eq!(get_due_next_status(&r, now()), None);
    }

    #[test]
    fn registration_zu_checkin_trigger_prioritaet() {
        // checkin_start hat Vorrang vor registration_end.
        let mut r = row("registration");
        r.checkin_start = Some(utc("2026-06-14T11:00:00"));
        r.registration_end = Some(utc("2026-06-14T13:00:00"));
        assert_eq!(get_due_next_status(&r, now()), Some("checkin"));

        // Ohne checkin_start fällt es auf registration_end zurück.
        let mut r2 = row("registration");
        r2.registration_end = Some(utc("2026-06-14T11:30:00"));
        assert_eq!(get_due_next_status(&r2, now()), Some("checkin"));
    }

    #[test]
    fn checkin_zu_group_phase_standardmodus() {
        let mut r = row("checkin");
        r.group_phase_start = Some(utc("2026-06-14T11:00:00"));
        assert_eq!(get_due_next_status(&r, now()), Some("group_phase"));
    }

    #[test]
    fn checkin_bracket_only_ueberspringt_gruppenphase() {
        let mut r = row("checkin");
        r.tournament_mode = Some("bracket_only".into());
        r.group_phase_start = Some(utc("2026-06-14T11:00:00"));
        // bracket_start noch nicht fällig → bleibt hängen (Logikfalle 1:1).
        r.bracket_start = Some(utc("2026-06-14T13:00:00"));
        assert_eq!(get_due_next_status(&r, now()), None);
        // bracket_start fällig → direkt nach bracket.
        r.bracket_start = Some(utc("2026-06-14T11:30:00"));
        assert_eq!(get_due_next_status(&r, now()), Some("bracket"));
    }

    #[test]
    fn group_phase_zu_bracket() {
        let mut r = row("group_phase");
        r.bracket_start = Some(utc("2026-06-14T11:00:00"));
        assert_eq!(get_due_next_status(&r, now()), Some("bracket"));
        r.bracket_start = Some(utc("2026-06-14T13:00:00"));
        assert_eq!(get_due_next_status(&r, now()), None);
    }

    #[test]
    fn kein_uebergang_ohne_zeitstempel() {
        assert_eq!(get_due_next_status(&row("draft"), now()), None);
        assert_eq!(get_due_next_status(&row("registration"), now()), None);
        assert_eq!(get_due_next_status(&row("checkin"), now()), None);
        assert_eq!(get_due_next_status(&row("group_phase"), now()), None);
        // Unbekannter/Endstatus → None.
        assert_eq!(get_due_next_status(&row("bracket"), now()), None);
        assert_eq!(get_due_next_status(&row("completed"), now()), None);
    }
}
