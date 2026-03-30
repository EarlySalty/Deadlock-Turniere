"""Tournament Scheduler — automatische Phasenwechsel per Zeitplan."""
from __future__ import annotations

import asyncio
import json
import logging
from datetime import datetime
from typing import Any

from db import get_db
from tournament.engine import (
    VALID_STATUS_TRANSITIONS,
    generate_bracket,
    generate_group_matches,
    generate_groups,
)

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


def _get_due_next_status(tournament_row: Any, now: datetime) -> str | None:
    """Ermittelt den nächsten fälligen Status basierend auf den Zeitstempeln."""
    current_status = tournament_row["status"]

    if current_status == "draft" and _is_due(tournament_row["registration_start"], now):
        return "registration"

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


async def start_scheduler(app: Any | None = None) -> None:
    """Startet den Hintergrund-Loop für automatische Turnier-Übergänge."""
    logger.info("Tournament-Scheduler gestartet")

    try:
        await _check_and_advance_tournaments()
        while True:
            await asyncio.sleep(SCHEDULER_INTERVAL_SECONDS)
            await _check_and_advance_tournaments()
    except asyncio.CancelledError:
        logger.info("Tournament-Scheduler gestoppt")
        raise
