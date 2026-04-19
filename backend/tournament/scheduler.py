"""Tournament Scheduler — automatische Phasenwechsel per Zeitplan."""
from __future__ import annotations

import asyncio
import json
import logging
from datetime import datetime, timedelta
from typing import Any

from db import get_db
from notifications.discord_notifier import notify_users
from tournament.points import recalculate_player_points
from tournament.engine import (
    VALID_STATUS_TRANSITIONS,
    generate_bracket,
    generate_group_matches,
    generate_groups,
    determine_tournament_mode,
)
from tournament.models import TournamentMode

logger = logging.getLogger(__name__)

SCHEDULER_INTERVAL_SECONDS = 60
_scheduler_lock = asyncio.Lock()


def _parse_timestamp(value: str | None) -> datetime | None:
    """Parst DB-/Frontend-Zeitstempel robust für lokale Vergleiche."""
    if not value:
        return None

    normalized = value.strip().replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(normalized)
    except ValueError:
        return None

    if parsed.tzinfo is not None:
        return parsed.astimezone().replace(tzinfo=None)
    return parsed


def _is_due(value: str | None, now: datetime) -> bool:
    parsed = _parse_timestamp(value)
    return parsed is not None and parsed <= now


def _parse_reminder_offsets(value: Any) -> list[int]:
    if isinstance(value, list):
        parsed = value
    elif isinstance(value, str) and value.strip():
        try:
            parsed = json.loads(value)
        except ValueError:
            parsed = None
    else:
        parsed = None

    if not isinstance(parsed, list):
        return [1440, 120, 15]

    offsets = sorted({int(offset) for offset in parsed if int(offset) >= 0}, reverse=True)
    return offsets or [1440, 120, 15]


def _get_due_next_status(tournament_row: Any, now: datetime) -> str | None:
    """Ermittelt den nächsten fälligen Status basierend auf den Zeitstempeln und Turnier-Modus."""
    current_status = tournament_row["status"]

    if current_status == "draft" and _is_due(tournament_row["registration_start"], now):
        return "registration"

    checkin_trigger = (
        tournament_row["checkin_start"]
        if "checkin_start" in tournament_row.keys()
        else None
    ) or tournament_row["registration_end"]
    if current_status == "registration" and _is_due(checkin_trigger, now):
        return "checkin"

    # Auto Tournament Mode: Wenn bracket_only, überspring group_phase
    tournament_mode = tournament_row.get("tournament_mode")

    if current_status == "checkin" and _is_due(tournament_row["group_phase_start"], now):
        if tournament_mode == "bracket_only":
            # Skip group_phase, gehe direkt zu bracket
            return "bracket" if _is_due(tournament_row["bracket_start"], now) else None
        return "group_phase"

    if current_status == "group_phase" and _is_due(tournament_row["bracket_start"], now):
        return "bracket"

    return None


async def _audit(db, action: str, user_id: str | None, details: str) -> None:  # noqa: ANN001
    await db.execute(
        "INSERT INTO audit_log (action, user_id, details) VALUES (?, ?, ?)",
        (action, user_id, details),
    )


async def _has_other_active_tournament(db, tournament_id: int) -> bool:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT 1 FROM tournaments "
        "WHERE id != ? AND status IN ('registration', 'checkin', 'group_phase', 'bracket') "
        "LIMIT 1",
        (tournament_id,),
    )
    return await cursor.fetchone() is not None


async def _load_tournament_participant_ids(db, tournament_id: int) -> list[str]:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT DISTINCT discord_id FROM tournament_signups WHERE tournament_id = ? "
        "UNION SELECT DISTINCT tm.discord_id FROM team_members tm "
        "JOIN teams t ON tm.team_id = t.id WHERE t.tournament_id = ?",
        (tournament_id, tournament_id),
    )
    rows = await cursor.fetchall()
    return [str(row["discord_id"]) for row in rows if row["discord_id"]]


async def _load_all_profile_ids(db) -> list[str]:  # noqa: ANN001
    cursor = await db.execute("SELECT DISTINCT discord_id FROM user_profiles")
    rows = await cursor.fetchall()
    return [str(row["discord_id"]) for row in rows if row["discord_id"]]


async def advance_tournament_status(
    tournament_id: int,
    *,
    current_status: str,
    next_status: str,
    source: str,
    actor_id: str | None = None,
) -> dict[str, Any]:
    """Wechselt den Turnierstatus und führt notwendige Seiteneffekte aus."""
    allowed = VALID_STATUS_TRANSITIONS.get(current_status, [])
    if next_status not in allowed:
        raise ValueError(
            f"Ungültiger Status-Übergang: {current_status} -> {next_status}. "
            f"Erlaubt: {', '.join(allowed) if allowed else 'keine'}"
        )

    metadata: dict[str, Any] = {
        "tournament_id": tournament_id,
        "from": current_status,
        "to": next_status,
        "source": source,
    }

    if next_status == "group_phase":
        group_ids = await generate_groups(tournament_id)
        match_count = await generate_group_matches(tournament_id)
        metadata["groups_created"] = len(group_ids)
        metadata["matches_created"] = match_count
    elif next_status == "bracket":
        match_count = await generate_bracket(tournament_id)
        metadata["bracket_matches_created"] = match_count

    async with get_db() as db:
        cursor = await db.execute(
            "UPDATE tournaments SET status = ?, updated_at = datetime('now') "
            "WHERE id = ? AND status = ?",
            (next_status, tournament_id, current_status),
        )
        if cursor.rowcount == 0:
            raise RuntimeError("Turnierstatus wurde parallel geändert")

        action = "tournament_auto_advance" if source == "scheduler" else "tournament_advance"
        await _audit(db, action, actor_id, json.dumps(metadata))

        if next_status == "completed":
            cursor = await db.execute(
                "SELECT exclude_from_leaderboard FROM tournaments WHERE id = ?",
                (tournament_id,),
            )
            row = await cursor.fetchone()
            if row and not bool(row["exclude_from_leaderboard"]):
                await recalculate_player_points(db, tournament_id)

        await db.commit()

    return metadata


async def _advance_due_tournament(tournament_row: Any, now: datetime) -> bool:
    tournament_id = int(tournament_row["id"])
    current_status = tournament_row["status"]
    next_status = _get_due_next_status(tournament_row, now)

    if next_status is None:
        return False

    async with get_db() as db:
        if next_status == "registration" and await _has_other_active_tournament(db, tournament_id):
            logger.warning(
                "Scheduler überspringt Turnier %s: anderes aktives Turnier blockiert Aktivierung",
                tournament_id,
            )
            return False

    try:
        await advance_tournament_status(
            tournament_id,
            current_status=current_status,
            next_status=next_status,
            source="scheduler",
        )
    except ValueError as exc:
        logger.warning(
            "Scheduler konnte Turnier %s nicht nach %s verschieben: %s",
            tournament_id,
            next_status,
            exc,
        )
        return False
    except RuntimeError:
        logger.info(
            "Scheduler hat Turnier %s übersprungen, weil der Status parallel geändert wurde",
            tournament_id,
        )
        return False

    logger.info(
        "Scheduler hat Turnier %s von %s nach %s verschoben",
        tournament_id,
        current_status,
        next_status,
    )

    try:
        if next_status == "checkin":
            async with get_db() as db:
                participant_ids = await _load_tournament_participant_ids(db, tournament_id)
            await notify_users(
                participant_ids,
                "checkin",
                f"Der Check-in für Turnier #{tournament_id} ist jetzt geöffnet.",
            )
        elif next_status == "registration":
            async with get_db() as db:
                profile_ids = await _load_all_profile_ids(db)
            await notify_users(
                profile_ids,
                "tournament_news",
                f"Die Registrierung für Turnier #{tournament_id} ist jetzt geöffnet.",
            )
    except Exception:
        logger.exception(
            "Scheduler notifications failed (tournament=%s next_status=%s)",
            tournament_id,
            next_status,
        )

    return True


async def _check_and_advance_tournaments() -> None:
    async with _scheduler_lock:
        while True:
            async with get_db() as db:
                cursor = await db.execute(
                    "SELECT * FROM tournaments "
                    "WHERE status IN ('draft', 'registration', 'checkin', 'group_phase') "
                    "ORDER BY id"
                )
                tournaments = await cursor.fetchall()

            now = datetime.now()
            advanced_any = False

            for tournament_row in tournaments:
                if await _advance_due_tournament(tournament_row, now):
                    advanced_any = True

            if not advanced_any:
                return


async def _check_and_send_registration_reminders() -> None:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT id, name, status, registration_end, reminder_offsets "
            "FROM tournaments "
            "WHERE status IN ('draft', 'registration') AND registration_end IS NOT NULL "
            "ORDER BY id"
        )
        tournaments = await cursor.fetchall()

        now = datetime.now()
        tolerance_end = now + timedelta(minutes=5)

        for tournament in tournaments:
            registration_end = _parse_timestamp(tournament["registration_end"])
            if registration_end is None:
                continue

            for offset_minutes in _parse_reminder_offsets(tournament["reminder_offsets"]):
                reminder_at = registration_end - timedelta(minutes=offset_minutes)
                if not (reminder_at <= now <= reminder_at + timedelta(minutes=5)):
                    continue

                dedupe_cur = await db.execute(
                    "SELECT 1 FROM sent_tournament_reminders WHERE tournament_id = ? AND offset_minutes = ?",
                    (tournament["id"], offset_minutes),
                )
                if await dedupe_cur.fetchone():
                    continue

                profile_cur = await db.execute(
                    "SELECT discord_id FROM user_profiles WHERE notify_registration_reminder = 1"
                )
                profile_rows = await profile_cur.fetchall()
                profile_ids = [str(row["discord_id"]) for row in profile_rows if row["discord_id"]]
                if not profile_ids:
                    continue

                hours = offset_minutes // 60
                minutes = offset_minutes % 60
                if hours and minutes:
                    offset_label = f"{hours}h {minutes}min"
                elif hours:
                    offset_label = f"{hours}h"
                else:
                    offset_label = f"{minutes}min"

                try:
                    await notify_users(
                        profile_ids,
                        "registration_reminder",
                        f"Anmeldeschluss für Turnier '{tournament['name']}' in {offset_label}!",
                    )
                except Exception:
                    logger.exception(
                        "Registration reminder failed (tournament=%s offset=%s)",
                        tournament["id"],
                        offset_minutes,
                    )
                    continue

                await db.execute(
                    "INSERT OR IGNORE INTO sent_tournament_reminders (tournament_id, offset_minutes, sent_at) "
                    "VALUES (?, ?, datetime('now'))",
                    (tournament["id"], offset_minutes),
                )

        await db.commit()


async def start_scheduler(app: Any | None = None) -> None:
    """Startet den Hintergrund-Loop für automatische Turnier-Übergänge."""
    logger.info("Tournament-Scheduler gestartet")

    try:
        await _check_and_advance_tournaments()
        await _check_and_send_registration_reminders()
        while True:
            await asyncio.sleep(SCHEDULER_INTERVAL_SECONDS)
            await _check_and_advance_tournaments()
            await _check_and_send_registration_reminders()
    except asyncio.CancelledError:
        logger.info("Tournament-Scheduler gestoppt")
        raise
