from __future__ import annotations

import json

import pytest

from config import settings
from db import get_db, init_db
from notifications import discord_notifier


@pytest.mark.asyncio
async def test_notify_users_filters_by_profile_flags(tmp_path, monkeypatch):
    db_path = tmp_path / "tournament.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            """
            INSERT INTO user_profiles (
                discord_id, bio, invite_auto_accept, notify_discord_dm,
                notify_browser, notify_match_start, notify_checkin,
                notify_team_invite, notify_tournament_news, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            ("111", None, 0, 1, 0, 1, 1, 0, 0, "now"),
        )
        await db.execute(
            """
            INSERT INTO user_profiles (
                discord_id, bio, invite_auto_accept, notify_discord_dm,
                notify_browser, notify_match_start, notify_checkin,
                notify_team_invite, notify_tournament_news, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            ("222", None, 0, 0, 0, 1, 1, 1, 1, "now"),
        )
        await db.commit()

    sent_payloads: list[dict[str, object]] = []

    async def fake_post_internal_api(path: str, payload: dict[str, object]) -> dict[str, object]:
        sent_payloads.append({"path": path, "payload": payload})
        return {"ok": True, "path": path, "payload": payload}

    monkeypatch.setattr(discord_notifier, "_post_internal_api", fake_post_internal_api)

    summary = await discord_notifier.notify_users(
        ["111", "222", "333"],
        "match_start",
        "Match startet jetzt",
    )

    assert summary["sent"] == ["111", "333"]
    assert summary["skipped"] == ["222"]
    assert summary["failed"] == []
    assert [entry["payload"]["user_id"] for entry in sent_payloads] == [111, 333]

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT type, status, payload, result_payload FROM discord_tasks ORDER BY id"
        )
        rows = await cursor.fetchall()

    assert [row["type"] for row in rows] == ["SEND_DM", "SEND_DM"]
    assert all(row["status"] == "DONE" for row in rows)
    assert json.loads(rows[0]["payload"])["event_type"] == "match_start"


@pytest.mark.asyncio
async def test_create_match_channel_and_lobby_message_use_tasks(tmp_path, monkeypatch):
    db_path = tmp_path / "tournament.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    monkeypatch.setattr(settings, "DISCORD_MATCH_CHANNEL_CATEGORY_ID", 123456789)
    await init_db()

    calls: list[tuple[str, dict[str, object]]] = []

    async def fake_post_internal_api(path: str, payload: dict[str, object]) -> dict[str, object]:
        calls.append((path, payload))
        if path.endswith("/create-channel"):
            return {"channel_id": "987654321"}
        return {"ok": True}

    monkeypatch.setattr(discord_notifier, "_post_internal_api", fake_post_internal_api)

    channel_id = await discord_notifier.create_match_channel(12, "Mäx Team", "Other Team")
    assert channel_id == "987654321"

    result = await discord_notifier.send_match_lobby_info(channel_id, "ABCD-1234", ["111", "222"])
    assert result == {"ok": True}

    assert calls[0][0].endswith("/create-channel")
    assert calls[0][1]["name"] == "match-max-team-vs-other-team"
    assert calls[1][0].endswith("/send-rich-message")
    assert calls[1][1]["allowed_user_ids"] == ["111", "222"]
    assert "<@111>" in str(calls[1][1]["content"])

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT type, status, result_payload FROM discord_tasks ORDER BY id"
        )
        rows = await cursor.fetchall()

    assert [row["type"] for row in rows] == ["CREATE_CHANNEL", "SEND_MATCH_INFO"]
    assert all(row["status"] == "DONE" for row in rows)
