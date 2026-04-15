from __future__ import annotations

import asyncio

import pytest

from config import settings
from db import get_db, init_db
from match import result_processor


@pytest.mark.asyncio
async def test_apply_bracket_match_result_schedules_channel_cleanup(tmp_path, monkeypatch):
    db_path = tmp_path / "tournament.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            "INSERT INTO tournaments (name, status, created_by, updated_at) VALUES (?, ?, ?, ?)",
            ("Test Cup", "bracket", "admin", "now"),
        )
        await db.execute(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?)",
            (1, "Alpha", "alpha", "111"),
        )
        await db.execute(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?)",
            (1, "Bravo", "bravo", "222"),
        )
        await db.execute(
            """
            INSERT INTO bracket_matches (
                tournament_id, round, position, team1_id, team2_id, status, discord_channel_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (1, 1, 1, 1, 2, "in_progress", "9001"),
        )
        await db.commit()

    scheduled: list[asyncio.Future | asyncio.Task | object] = []

    def fake_create_task(coro):
        scheduled.append(coro)
        future = asyncio.get_running_loop().create_future()
        future.set_result(None)
        return future

    async def fake_advance_bracket_winner(*_: object, **__: object) -> None:
        return None

    monkeypatch.setattr(result_processor.asyncio, "create_task", fake_create_task)
    monkeypatch.setattr(result_processor, "advance_bracket_winner", fake_advance_bracket_winner)

    await result_processor.apply_bracket_match_result(
        1,
        1,
        winner_id=1,
        duration_s=42,
        players=[],
        source="manual",
    )

    assert scheduled
    assert getattr(scheduled[0], "cr_code", None).co_name == "delete_match_channel_later"
    scheduled[0].close()
