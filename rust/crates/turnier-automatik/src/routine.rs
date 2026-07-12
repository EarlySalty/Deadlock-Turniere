//! Persistenz fuer automatisch wiederkehrende Turniere und ihre spaetere
//! Einladungs-Kohorte. Versand gehoert bewusst nicht in dieses Modul.

use chrono::{DateTime, Duration, Utc};
use turnier_core::{discord_id_to_string, json::wire_string_to_jsonb, parse_discord_id};
use turnier_db::Pool;

use crate::{presets::Preset, AutomatikResult};

const ROUTINE_CREATE_LOCK: i64 = i64::from_be_bytes(*b"trnrout1");

/// Vollstaendige Zeitplanung eines Routine-Turniers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineTournamentPlan {
    pub registration_start: DateTime<Utc>,
    pub registration_end: DateTime<Utc>,
    pub checkin_start: DateTime<Utc>,
    pub event_start: DateTime<Utc>,
    pub bracket_start: DateTime<Utc>,
}

/// Ergebnis der idempotenten Turnier-Erzeugung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsuredRoutineTournament {
    pub id: i64,
    pub status: String,
    pub created: bool,
}

/// Legt fuer Preset und Startslot hoechstens einen Entwurf an. Der Aufrufer
/// oeffnet ihn anschliessend ueber die bestehende Statusmaschine.
pub async fn ensure_routine_tournament(
    pool: &Pool,
    preset: &Preset,
    plan: &RoutineTournamentPlan,
) -> AutomatikResult<EnsuredRoutineTournament> {
    let mut tx = pool.begin().await?;
    let created_by = parse_discord_id(&preset.created_by)
        .map_err(|_| crate::AutomatikError::InvalidNumericId(preset.created_by.clone()))?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ROUTINE_CREATE_LOCK)
        .execute(&mut *tx)
        .await?;

    if let Some((id, status)) = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, status FROM turnier.tournaments \
         WHERE source = 'routine' AND preset_id = $1 AND group_phase_start = $2 \
         ORDER BY id LIMIT 1",
    )
    .bind(preset.id)
    .bind(plan.event_start)
    .fetch_optional(&mut *tx)
    .await?
    {
        tx.commit().await?;
        return Ok(EnsuredRoutineTournament {
            id,
            status,
            created: false,
        });
    }

    let id = sqlx::query_scalar(
        "INSERT INTO turnier.tournaments \
         (name, status, description, team_size, registration_start, registration_end, \
          checkin_start, group_phase_start, bracket_start, bracket_format, created_by, \
          created_at, updated_at, invite_mode, tournament_mode, series_format, \
          exclude_from_leaderboard, reminder_offsets, tournament_game_mode, \
          auto_lobby_enabled, is_test, rules, final_series_format, match_objective, \
          no_show_grace_minutes, start_reminder_offsets, source, preset_id) \
         VALUES ($1, 'draft', $2, $3, $4, $5, $6, $7, $8, $9, $10, now(), now(), \
                 $11, $12, $13, false, $14, $15, true, false, $16, $17, $18, 10, $19, \
                 'routine', $20) RETURNING id",
    )
    .bind(&preset.name)
    .bind(&preset.description_template)
    .bind(preset.team_size)
    .bind(plan.registration_start)
    .bind(plan.registration_end)
    .bind(plan.checkin_start)
    .bind(plan.event_start)
    .bind(plan.bracket_start)
    .bind(preset.bracket_format)
    .bind(created_by)
    .bind(preset.invite_mode)
    .bind(preset.tournament_mode)
    .bind(preset.series_format)
    .bind(wire_string_to_jsonb(preset.reminder_offsets.as_deref()))
    .bind(preset.tournament_game_mode)
    .bind(&preset.rules)
    .bind(preset.final_series_format)
    .bind(&preset.match_objective)
    .bind(wire_string_to_jsonb(
        preset.start_reminder_offsets.as_deref(),
    ))
    .bind(preset.id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(EnsuredRoutineTournament {
        id,
        status: "draft".to_string(),
        created: true,
    })
}

/// Liefert die spaetere Outbox-Kohorte: Teilnehmer abgeschlossener Turniere
/// oder Voice-Aktivitaet der letzten 14 Tage, ohne bereits Angemeldete des
/// Zielturniers. Kein Versand und kein Opt-out-Handling an dieser Stelle.
pub async fn load_invitation_candidate_ids(
    pool: &Pool,
    tournament_id: i64,
    guild_id: i64,
    now: DateTime<Utc>,
) -> AutomatikResult<Vec<String>> {
    let voice_since = now - Duration::days(14);
    let rows: Vec<(i64,)> = sqlx::query_as(
        "WITH candidates AS ( \
             SELECT s.discord_id FROM turnier.tournament_signups s \
             JOIN turnier.tournaments t ON t.id = s.tournament_id \
             WHERE t.status IN ('completed', 'archived') \
             UNION \
             SELECT tm.discord_id FROM turnier.team_members tm \
             JOIN turnier.teams team ON team.id = tm.team_id \
             JOIN turnier.tournaments t ON t.id = team.tournament_id \
             WHERE t.status IN ('completed', 'archived') \
             UNION \
             SELECT user_id FROM activity.voice_metadata_events \
             WHERE guild_id = $2 AND occurred_at >= $3 \
             UNION \
             SELECT user_id FROM activity.voice_open_sessions \
             WHERE guild_id = $2 AND (joined_at >= $3 OR updated_at >= $3) \
         ), current_participants AS ( \
             SELECT discord_id FROM turnier.tournament_signups WHERE tournament_id = $1 \
             UNION \
             SELECT tm.discord_id FROM turnier.team_members tm \
             JOIN turnier.teams team ON team.id = tm.team_id \
             WHERE team.tournament_id = $1 \
         ) \
         SELECT discord_id FROM candidates \
         WHERE discord_id NOT IN (SELECT discord_id FROM current_participants) \
         ORDER BY discord_id",
    )
    .bind(tournament_id)
    .bind(guild_id)
    .bind(voice_since)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id,)| discord_id_to_string(id))
        .collect())
}
