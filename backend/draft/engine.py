"""Draft Engine — Zustandsmaschine für Pick/Ban-Drafts (6v6)."""
from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from db import get_db
from draft.heroes import is_valid_hero

# Standard-Sequenz: 6 Bans + 12 Picks (6 pro Team)
DEFAULT_SEQUENCE: list[tuple[str, int]] = [
    ("ban", 1),
    ("ban", 2),
    ("ban", 1),
    ("ban", 2),
    ("ban", 1),
    ("ban", 2),
    ("pick", 1),
    ("pick", 2),
    ("pick", 2),
    ("pick", 1),
    ("pick", 1),
    ("pick", 2),
    ("pick", 2),
    ("pick", 1),
    ("pick", 1),
    ("pick", 2),
    ("pick", 2),
    ("pick", 1),
]


async def start_draft(bracket_match_id: int, started_by: str) -> int:
    """Erstellt neue Draft-Session oder gibt bestehende zurück. Gibt session_id zurück."""
    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT id
            FROM draft_sessions
            WHERE bracket_match_id = ? AND status IN ('pending', 'in_progress')
            """,
            (bracket_match_id,),
        )
        existing = await cursor.fetchone()
        if existing:
            return int(existing["id"])

        cursor = await db.execute(
            """
            INSERT INTO draft_sessions
                (bracket_match_id, status, current_action_index, started_by, started_at, created_at)
            VALUES (?, 'in_progress', 0, ?, ?, ?)
            """,
            (bracket_match_id, started_by, now, now),
        )
        session_id = int(cursor.lastrowid)

        for idx, (action_type, team_slot) in enumerate(DEFAULT_SEQUENCE):
            await db.execute(
                """
                INSERT INTO draft_actions (session_id, sequence_index, action_type, team_slot)
                VALUES (?, ?, ?, ?)
                """,
                (session_id, idx, action_type, team_slot),
            )
        await db.commit()

    return session_id


async def take_action(
    session_id: int,
    hero_name: str,
    taken_by: str,
    *,
    force: bool = False,
) -> dict[str, Any]:
    """
    Führt die aktuelle Draft-Aktion aus.
    Gibt zurück: {is_complete, next_action_type, next_team_slot}
    """
    if not is_valid_hero(hero_name):
        raise ValueError(f"Unbekannter Held: {hero_name}")

    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT current_action_index
            FROM draft_sessions
            WHERE id = ? AND status = 'in_progress'
            """,
            (session_id,),
        )
        session = await cursor.fetchone()
        if not session:
            raise ValueError("Draft-Session nicht gefunden oder bereits abgeschlossen")
        idx = int(session["current_action_index"])

        cursor = await db.execute(
            "SELECT id FROM draft_actions WHERE session_id = ? AND hero_name = ?",
            (session_id, hero_name),
        )
        if await cursor.fetchone():
            raise ValueError(f"{hero_name} wurde bereits gebannt oder gepickt")

        await db.execute(
            """
            UPDATE draft_actions
            SET hero_name = ?, taken_by = ?, taken_at = ?, is_admin_forced = ?
            WHERE session_id = ? AND sequence_index = ?
            """,
            (hero_name, taken_by, now, 1 if force else 0, session_id, idx),
        )

        next_idx = idx + 1
        is_complete = next_idx >= len(DEFAULT_SEQUENCE)
        await db.execute(
            """
            UPDATE draft_sessions
            SET current_action_index = ?,
                status = ?,
                completed_at = ?
            WHERE id = ?
            """,
            (next_idx, "completed" if is_complete else "in_progress", now if is_complete else None, session_id),
        )
        await db.commit()

    next_action = DEFAULT_SEQUENCE[next_idx] if not is_complete else None
    return {
        "is_complete": is_complete,
        "next_action_type": next_action[0] if next_action else None,
        "next_team_slot": next_action[1] if next_action else None,
    }


async def get_draft_state(session_id: int) -> dict[str, Any]:
    """Gibt den vollständigen Draft-Zustand zurück."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM draft_sessions WHERE id = ?",
            (session_id,),
        )
        session = await cursor.fetchone()
        if not session:
            raise ValueError("Session nicht gefunden")

        cursor = await db.execute(
            "SELECT * FROM draft_actions WHERE session_id = ? ORDER BY sequence_index",
            (session_id,),
        )
        actions = await cursor.fetchall()

    session_dict = dict(session)
    actions_list = [dict(action) for action in actions]
    idx = int(session_dict["current_action_index"])
    current = DEFAULT_SEQUENCE[idx] if idx < len(DEFAULT_SEQUENCE) else None

    return {
        **session_dict,
        "actions": actions_list,
        "current_action_type": current[0] if current else None,
        "current_team_slot": current[1] if current else None,
        "bans": [action["hero_name"] for action in actions_list if action["action_type"] == "ban" and action["hero_name"]],
        "picks_team1": [
            action["hero_name"]
            for action in actions_list
            if action["action_type"] == "pick" and action["team_slot"] == 1 and action["hero_name"]
        ],
        "picks_team2": [
            action["hero_name"]
            for action in actions_list
            if action["action_type"] == "pick" and action["team_slot"] == 2 and action["hero_name"]
        ],
    }
