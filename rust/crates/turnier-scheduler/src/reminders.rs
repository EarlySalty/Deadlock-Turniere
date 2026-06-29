//! Reminder-Versand: Registrierungs-, Start- und Match-Reminder.
//!
//! Drei Tasks mit jeweils 5-Minuten-Toleranzfenster
//! (`reminder_at <= now <= reminder_at + 5min`), Dedupe über die
//! `sent_*_reminders`-Tabellen (`INSERT OR IGNORE`) und Offset-Labels „Xh Ymin".
//! Die Event-Keys an [`DiscordNotifier::notify_users`] sind exakt:
//! `registration_reminder`, `match_start`, `checkin`, `tournament_news`.
//!
//! ## Zeit-Parität & Fenster-Befund (1:1 erhalten)
//! Das feste 5-Minuten-Fenster ist zur 60-s-Loop-Kadenz überdimensioniert und
//! holt verpasste Fenster nach einem Ausfall NICHT nach (kein Catch-up). Das ist
//! ein dokumentierter `needs-decision`-Befund und bleibt unverändert, damit der
//! Port nicht ändert, WANN/OB Reminder feuern.

use chrono::{Duration, NaiveDateTime};

use turnier_db::Pool;
use turnier_discord::{DiscordNotifier, NotificationEvent};

use crate::time::{is_within_window, offset_label, parse_reminder_offsets, parse_timestamp};

/// Lädt alle Teilnehmer-Discord-IDs eines Turniers: Solo-Signups UNION
/// Teammitglieder. Portiert `_load_tournament_participant_ids` (Z.113-121).
pub async fn load_tournament_participant_ids(
    pool: &Pool,
    tournament_id: i64,
) -> sqlx::Result<Vec<String>> {
    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT DISTINCT discord_id FROM tournament_signups WHERE tournament_id = ? \
         UNION SELECT DISTINCT tm.discord_id FROM team_members tm \
         JOIN teams t ON tm.team_id = t.id WHERE t.tournament_id = ?",
    )
    .bind(tournament_id)
    .bind(tournament_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(|(id,)| non_empty(id)).collect())
}

/// Lädt alle Profil-Discord-IDs. Portiert `_load_all_profile_ids` (Z.124-127).
pub async fn load_all_profile_ids(pool: &Pool) -> sqlx::Result<Vec<String>> {
    let rows: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT DISTINCT discord_id FROM user_profiles")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().filter_map(|(id,)| non_empty(id)).collect())
}

fn non_empty(id: Option<String>) -> Option<String> {
    id.filter(|s| !s.is_empty())
}

/// Felder eines Turniers für die Registrierungs-Reminder.
#[derive(sqlx::FromRow)]
struct RegistrationReminderRow {
    id: i64,
    name: String,
    registration_end: Option<String>,
    reminder_offsets: Option<String>,
    is_test: i64,
}

/// Felder eines Turniers für die Start-Reminder. Bildet `_tournament_start_value`
/// (Z.376-384) ab.
#[derive(sqlx::FromRow)]
struct StartReminderRow {
    id: i64,
    name: String,
    tournament_mode: Option<String>,
    group_phase_start: Option<String>,
    bracket_start: Option<String>,
    start_reminder_offsets: Option<String>,
    is_test: i64,
}

/// Felder einer offenen Bracket-Paarung für die Match-Reminder.
#[derive(sqlx::FromRow)]
struct MatchReminderRow {
    id: i64,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
    tournament_name: String,
}

/// Effektiver Turnier-Start: bei `bracket_only` zählt `bracket_start`, sonst
/// `group_phase_start` (Fallback `bracket_start`). Portiert
/// `_tournament_start_value` (Z.376-384).
fn tournament_start_value(row: &StartReminderRow) -> Option<&str> {
    if row.tournament_mode.as_deref() == Some("bracket_only") {
        return row.bracket_start.as_deref();
    }
    row.group_phase_start
        .as_deref()
        .or(row.bracket_start.as_deref())
}

/// Registrierungs-Reminder an Profile mit `notify_registration_reminder = 1`, vor
/// `registration_end` je konfiguriertem Offset. Portiert
/// `_check_and_send_registration_reminders` (Z.294-363).
pub async fn check_and_send_registration_reminders(
    pool: &Pool,
    notifier: &DiscordNotifier,
    now: NaiveDateTime,
) -> sqlx::Result<()> {
    let tournaments: Vec<RegistrationReminderRow> = sqlx::query_as(
        "SELECT id, name, registration_end, reminder_offsets, is_test FROM tournaments \
         WHERE status IN ('draft', 'registration') AND registration_end IS NOT NULL ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    for t in tournaments {
        let RegistrationReminderRow {
            id,
            name,
            registration_end,
            reminder_offsets,
            is_test,
        } = t;
        if is_test != 0 {
            continue;
        }
        let Some(end_at) = parse_timestamp(registration_end.as_deref()) else {
            continue;
        };

        for offset in parse_reminder_offsets(reminder_offsets.as_deref()) {
            let reminder_at = end_at - Duration::minutes(offset);
            if !is_within_window(reminder_at, now) {
                continue;
            }
            if reminder_already_sent(pool, "sent_tournament_reminders", id, offset).await? {
                continue;
            }

            let profile_ids = load_registration_reminder_profiles(pool).await?;
            if profile_ids.is_empty() {
                continue;
            }

            let message = format!(
                "Turnier '{name}' startet in {} — letzte Chance zur Anmeldung!",
                offset_label(offset)
            );
            if !send_reminder(
                notifier,
                &profile_ids,
                NotificationEvent::RegistrationReminder,
                &message,
            )
            .await
            {
                tracing::error!(tournament_id = id, offset, "Registration reminder failed");
                continue;
            }

            mark_reminder_sent(pool, "sent_tournament_reminders", id, offset).await?;
        }
    }
    Ok(())
}

/// Start-Reminder an Turnierteilnehmer vor dem effektiven Turnierstart. Portiert
/// `_check_and_send_start_reminders` (Z.387-444).
pub async fn check_and_send_start_reminders(
    pool: &Pool,
    notifier: &DiscordNotifier,
    now: NaiveDateTime,
) -> sqlx::Result<()> {
    let rows: Vec<StartReminderRow> = sqlx::query_as(
        "SELECT id, name, tournament_mode, group_phase_start, bracket_start, \
                start_reminder_offsets, is_test FROM tournaments \
         WHERE status IN ('registration', 'checkin') ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        if row.is_test != 0 {
            continue;
        }
        let Some(start_at) = parse_timestamp(tournament_start_value(&row)) else {
            continue;
        };

        for offset in parse_reminder_offsets(row.start_reminder_offsets.as_deref()) {
            let reminder_at = start_at - Duration::minutes(offset);
            if !is_within_window(reminder_at, now) {
                continue;
            }
            if reminder_already_sent(pool, "sent_start_reminders", row.id, offset).await? {
                continue;
            }

            let participant_ids = load_tournament_participant_ids(pool, row.id).await?;
            if participant_ids.is_empty() {
                continue;
            }

            let message = format!(
                "Turnier '{}' startet in {} — sei rechtzeitig da und mach dich ready!",
                row.name,
                offset_label(offset)
            );
            if !send_reminder(
                notifier,
                &participant_ids,
                NotificationEvent::MatchStart,
                &message,
            )
            .await
            {
                tracing::error!(tournament_id = row.id, offset, "Start reminder failed");
                continue;
            }

            mark_reminder_sent(pool, "sent_start_reminders", row.id, offset).await?;
        }
    }
    Ok(())
}

/// „Gleich dran"-Reminder an Teammitglieder offener Bracket-Matches (pending,
/// beide Teams gesetzt, keine Steam-Party). Portiert
/// `_check_and_send_match_reminders` (Z.447-500). Diese Funktion hat keinen
/// `now`-Parameter, weil das Original (wie hier) rein über den Match-Zustand und
/// die Dedupe-Tabelle gated — Konsistenz zur injizierbaren Signatur bleibt durch
/// den ungenutzten Aufruf-Kontext gewahrt.
pub async fn check_and_send_match_reminders(
    pool: &Pool,
    notifier: &DiscordNotifier,
) -> sqlx::Result<()> {
    let matches: Vec<MatchReminderRow> = sqlx::query_as(
        "SELECT bm.id, bm.team1_id, bm.team2_id, t.name AS tournament_name \
         FROM bracket_matches bm \
         JOIN tournaments t ON t.id = bm.tournament_id \
         WHERE t.status IN ('group_phase', 'bracket') \
           AND t.is_test = 0 \
           AND bm.status = 'pending' \
           AND bm.team1_id IS NOT NULL \
           AND bm.team2_id IS NOT NULL \
           AND bm.steam_party_id IS NULL \
         ORDER BY bm.id",
    )
    .fetch_all(pool)
    .await?;

    for m in matches {
        if match_reminder_already_sent(pool, m.id).await? {
            continue;
        }

        let member_ids = load_match_member_ids(pool, m.team1_id, m.team2_id).await?;
        if member_ids.is_empty() {
            continue;
        }

        let message = format!(
            "Hey! Euer Match im Turnier '{}' ist als Nächstes dran — macht euch ready.",
            m.tournament_name
        );
        if !send_reminder(notifier, &member_ids, NotificationEvent::MatchStart, &message).await {
            tracing::error!(match_id = m.id, "Match reminder failed");
            continue;
        }

        sqlx::query(
            "INSERT OR IGNORE INTO sent_match_reminders (match_type, match_id, kind, sent_at) \
             VALUES ('bracket', ?, 'next_up', datetime('now'))",
        )
        .bind(m.id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

// --- gemeinsame Helfer ---------------------------------------------------

/// Sendet einen Reminder und meldet Erfolg (`true`) / Fehler (`false`). Der
/// Discord-Versand ist best-effort: ein Broker-Fehler wird hier in `false`
/// übersetzt, der Aufrufer überspringt dann den Dedupe-Insert (wie das Original,
/// das bei einer Exception `continue` macht).
async fn send_reminder(
    notifier: &DiscordNotifier,
    ids: &[String],
    event: NotificationEvent,
    message: &str,
) -> bool {
    notifier.notify_users(ids, event, message).await.is_ok()
}

/// `true`, wenn für `(tournament_id, offset_minutes)` schon ein Reminder in der
/// Tabelle steht. `table` ist whitelisted (fester Aufrufer-String, keine
/// User-Eingabe).
async fn reminder_already_sent(
    pool: &Pool,
    table: &str,
    tournament_id: i64,
    offset_minutes: i64,
) -> sqlx::Result<bool> {
    let sql =
        format!("SELECT 1 FROM {table} WHERE tournament_id = ? AND offset_minutes = ? LIMIT 1");
    let row: Option<(i64,)> = sqlx::query_as(&sql)
        .bind(tournament_id)
        .bind(offset_minutes)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

/// Schreibt den Dedupe-Eintrag (`INSERT OR IGNORE`). `table` ist whitelisted.
async fn mark_reminder_sent(
    pool: &Pool,
    table: &str,
    tournament_id: i64,
    offset_minutes: i64,
) -> sqlx::Result<()> {
    let sql = format!(
        "INSERT OR IGNORE INTO {table} (tournament_id, offset_minutes, sent_at) \
         VALUES (?, ?, datetime('now'))"
    );
    sqlx::query(&sql)
        .bind(tournament_id)
        .bind(offset_minutes)
        .execute(pool)
        .await?;
    Ok(())
}

async fn match_reminder_already_sent(pool: &Pool, match_id: i64) -> sqlx::Result<bool> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM sent_match_reminders \
         WHERE match_type = 'bracket' AND match_id = ? AND kind = 'next_up' LIMIT 1",
    )
    .bind(match_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

async fn load_registration_reminder_profiles(pool: &Pool) -> sqlx::Result<Vec<String>> {
    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT discord_id FROM user_profiles WHERE notify_registration_reminder = 1",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(|(id,)| non_empty(id)).collect())
}

async fn load_match_member_ids(
    pool: &Pool,
    team1_id: Option<i64>,
    team2_id: Option<i64>,
) -> sqlx::Result<Vec<String>> {
    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT DISTINCT discord_id FROM team_members WHERE team_id IN (?, ?)",
    )
    .bind(team1_id)
    .bind(team2_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(|(id,)| non_empty(id)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srow(mode: Option<&str>, group: Option<&str>, bracket: Option<&str>) -> StartReminderRow {
        StartReminderRow {
            id: 1,
            name: "T".into(),
            tournament_mode: mode.map(str::to_string),
            group_phase_start: group.map(str::to_string),
            bracket_start: bracket.map(str::to_string),
            start_reminder_offsets: None,
            is_test: 0,
        }
    }

    #[test]
    fn start_value_standardmodus_nimmt_group_phase() {
        let r = srow(None, Some("2026-06-14T10:00:00"), Some("2026-06-14T12:00:00"));
        assert_eq!(tournament_start_value(&r), Some("2026-06-14T10:00:00"));
        // group_stage explizit.
        let r2 = srow(
            Some("group_stage"),
            Some("2026-06-14T10:00:00"),
            Some("2026-06-14T12:00:00"),
        );
        assert_eq!(tournament_start_value(&r2), Some("2026-06-14T10:00:00"));
    }

    #[test]
    fn start_value_fallback_auf_bracket_wenn_kein_group() {
        let r = srow(None, None, Some("2026-06-14T12:00:00"));
        assert_eq!(tournament_start_value(&r), Some("2026-06-14T12:00:00"));
    }

    #[test]
    fn start_value_bracket_only_nimmt_bracket() {
        let r = srow(
            Some("bracket_only"),
            Some("2026-06-14T10:00:00"),
            Some("2026-06-14T12:00:00"),
        );
        assert_eq!(tournament_start_value(&r), Some("2026-06-14T12:00:00"));
        // bracket_only ohne bracket_start → None (auch wenn group_phase_start da ist).
        let r2 = srow(Some("bracket_only"), Some("2026-06-14T10:00:00"), None);
        assert_eq!(tournament_start_value(&r2), None);
    }
}
