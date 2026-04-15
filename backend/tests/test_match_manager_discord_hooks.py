from __future__ import annotations

import pytest

from config import settings
from db import get_db, init_db
from match import manager as match_manager


@pytest.mark.asyncio
async def test_create_lobby_persists_discord_channel_and_sends_info(tmp_path, monkeypatch):
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
                tournament_id, round, position, team1_id, team2_id, status
            ) VALUES (?, ?, ?, ?, ?, ?)
            """,
            (1, 1, 1, 1, 2, "pending"),
        )
        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id) VALUES (?, ?, ?, ?)",
            (1, "111", "Player 1", "steam-111"),
        )
        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id) VALUES (?, ?, ?, ?)",
            (1, "112", "Player 2", "steam-112"),
        )
        await db.execute(
            "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id) VALUES (?, ?, ?, ?)",
            (2, "222", "Player 3", "steam-222"),
        )
        await db.commit()

    async def fake_run_steam_task(**_: object) -> dict[str, object]:
        return {
            "success": True,
            "party_id": "party-123",
            "party_code": "ABCD-1234",
            "join_code": "ABCD-1234",
        }

    created_channels: list[tuple[int, str, str]] = []
    sent_messages: list[tuple[str, str, list[str]]] = []

    async def fake_create_match_channel(match_id: int, team1_name: str, team2_name: str) -> str:
        created_channels.append((match_id, team1_name, team2_name))
        return "9001"

    async def fake_send_match_lobby_info(
        channel_id: str | int,
        party_code: str,
        participant_discord_ids: list[str],
    ) -> dict[str, object]:
        sent_messages.append((str(channel_id), party_code, participant_discord_ids))
        return {"ok": True}

    async def fake_invite_players_to_lobby(party_id: str, steam_ids: list[str]) -> dict[str, object]:
        return {
            "success": True,
            "party_id": party_id,
            "steam_ids": steam_ids,
            "invited": steam_ids,
            "failed": [],
            "skipped": [],
        }

    monkeypatch.setattr(match_manager, "_run_steam_task", fake_run_steam_task)
    monkeypatch.setattr(match_manager, "create_match_channel", fake_create_match_channel)
    monkeypatch.setattr(match_manager, "send_match_lobby_info", fake_send_match_lobby_info)
    monkeypatch.setattr(match_manager.steam_bridge, "invite_players_to_lobby", fake_invite_players_to_lobby)

    result = await match_manager.create_lobby(1, 1)

    assert result["discord_channel_id"] == "9001"
    assert created_channels == [(1, "Alpha", "Bravo")]
    assert sent_messages == [("9001", "ABCD-1234", ["111", "112", "222"])]
    assert result["invite_result"]["invited"] == ["steam-111", "steam-112", "steam-222"]

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT steam_party_id, party_code, discord_channel_id, status FROM bracket_matches WHERE id = 1"
        )
        row = await cursor.fetchone()

    assert row["steam_party_id"] == "party-123"
    assert row["party_code"] == "ABCD-1234"
    assert row["discord_channel_id"] == "9001"
    assert row["status"] == "lobby_created"
