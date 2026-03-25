"""Steam Bridge — Task-Queue Zugriff fuer Deadlock Custom Matches."""
from __future__ import annotations

import asyncio
import json
import time
from typing import Any

import aiosqlite

from config import settings

STALE_RUNNING_TASK_TIMEOUT_MS = 120_000


def _now_ms() -> int:
    return int(time.time() * 1000)


async def create_task(task_type: str, payload: dict[str, Any]) -> int:
    """Legt einen Steam-Task in der Deadlock-Service-DB an."""
    now = _now_ms()
    async with aiosqlite.connect(settings.STEAM_BRIDGE_DB_PATH) as db:
        await _fail_stale_running_tasks(db)
        cursor = await db.execute(
            """
            INSERT INTO steam_tasks(type, payload, status, created_at, updated_at)
            VALUES (?, ?, 'PENDING', ?, ?)
            """,
            (task_type, json.dumps(payload), now, now),
        )
        await db.commit()
        return int(cursor.lastrowid)


async def get_task(task_id: int) -> dict[str, Any] | None:
    """Laedt einen Task aus der Steam-Queue."""
    async with aiosqlite.connect(settings.STEAM_BRIDGE_DB_PATH) as db:
        await _fail_stale_running_tasks(db)
        db.row_factory = aiosqlite.Row
        cursor = await db.execute(
            """
            SELECT id, type, payload, status, result, error,
                   created_at, updated_at, started_at, finished_at, attempts
            FROM steam_tasks
            WHERE id = ?
            """,
            (task_id,),
        )
        row = await cursor.fetchone()
    return dict(row) if row else None


async def poll_task_result(
    task_id: int,
    *,
    timeout_s: float = 30,
    poll_interval_s: float = 0.5,
) -> dict[str, Any]:
    """Wartet auf DONE/FAILED und liefert das decodierte Ergebnis zurueck."""
    deadline = time.monotonic() + timeout_s

    while time.monotonic() < deadline:
        task = await get_task(task_id)
        if not task:
            raise RuntimeError(f"Steam task {task_id} nicht gefunden")

        status = str(task["status"]).upper()
        if status == "DONE":
            result_payload = task["result"]
            if not result_payload:
                return {"success": True}
            try:
                return json.loads(result_payload)
            except json.JSONDecodeError as exc:
                raise RuntimeError(
                    f"Steam task {task_id} hat ungueltiges Result-JSON"
                ) from exc

        if status == "FAILED":
            return {
                "success": False,
                "error": task["error"] or "Steam-Task fehlgeschlagen",
                "task_id": task_id,
            }

        await asyncio.sleep(poll_interval_s)

    raise TimeoutError(
        f"Steam task {task_id} hat innerhalb von {timeout_s}s nicht geantwortet"
    )


async def has_active_task(
    task_type: str,
    *,
    match_id: int | None = None,
    party_id: str | None = None,
) -> bool:
    """Prueft, ob bereits ein passender PENDING/RUNNING Task existiert."""
    clauses = ["type = ?", "status IN ('PENDING', 'RUNNING')"]
    params: list[Any] = [task_type]

    if match_id is not None:
        clauses.append("json_extract(payload, '$.match_id') = ?")
        params.append(match_id)

    if party_id is not None:
        clauses.append("json_extract(payload, '$.party_id') = ?")
        params.append(str(party_id))

    async with aiosqlite.connect(settings.STEAM_BRIDGE_DB_PATH) as db:
        await _fail_stale_running_tasks(db)
        cursor = await db.execute(
            f"""
            SELECT 1
            FROM steam_tasks
            WHERE {' AND '.join(clauses)}
            LIMIT 1
            """,
            params,
        )
        return await cursor.fetchone() is not None


async def _fail_stale_running_tasks(db: aiosqlite.Connection) -> None:
    """Markiert haengende RUNNING-Tasks als FAILED, damit sie nicht ewig blockieren."""
    now = _now_ms()
    stale_before = now - STALE_RUNNING_TASK_TIMEOUT_MS
    await db.execute(
        """
        UPDATE steam_tasks
        SET status = 'FAILED',
            error = COALESCE(error, 'Steam worker stale or crashed while task was RUNNING'),
            updated_at = ?,
            finished_at = ?
        WHERE status = 'RUNNING'
          AND started_at IS NOT NULL
          AND started_at < ?
        """,
        (now, now, stale_before),
    )
    await db.commit()
