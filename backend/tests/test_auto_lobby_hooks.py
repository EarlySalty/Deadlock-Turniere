from __future__ import annotations

import pytest

from config import settings
from db import get_db, init_db
from match import manager as match_manager
from match import result_processor


@pytest.mark.asyncio
async def test_apply_bracket_match_result_creates_lobby_for_ready_next_round(tmp_path, monkeypatch):
    db_path = tmp_path / "auto-lobby.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            """
            INSERT INTO tournaments (
                name, status, created_by, updated_at, auto_lobby_enabled
            ) VALUES (?, ?, ?, ?, ?)
            """,
            ("Auto Lobby", "bracket", "admin", "now", 1),
        )
        for team_id in range(1, 5):
            await db.execute(
                "INSERT INTO teams (id, tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?, ?)",
                (team_id, 1, f"Team {team_id}", f"team-{team_id}", f"{team_id:03d}"),
            )
        await db.execute(
            """
            INSERT INTO bracket_matches (
                id, tournament_id, round, position, team1_id, team2_id, status
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (1, 1, 1, 0, 1, 4, "in_progress"),
        )
        await db.execute(
            """
            INSERT INTO bracket_matches (
                id, tournament_id, round, position, team1_id, team2_id, status
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (2, 1, 1, 1, 2, 3, "in_progress"),
        )
        await db.execute(
            """
            INSERT INTO bracket_matches (
                id, tournament_id, round, position, source_match1_id, source_match2_id, status
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (3, 1, 2, 0, 1, 2, "pending"),
        )
        await db.commit()

    created_lobbies: list[tuple[int, int]] = []

    async def fake_create_lobby(tournament_id: int, match_id: int, **_: object) -> dict[str, object]:
        created_lobbies.append((tournament_id, match_id))
        return {"success": True, "match_id": match_id}

    monkeypatch.setattr(match_manager, "create_lobby", fake_create_lobby)

    await result_processor.apply_bracket_match_result(1, 1, winner_id=1, source="manual")
    assert created_lobbies == []

    await result_processor.apply_bracket_match_result(1, 2, winner_id=2, source="manual")
    assert created_lobbies == [(1, 3)]
