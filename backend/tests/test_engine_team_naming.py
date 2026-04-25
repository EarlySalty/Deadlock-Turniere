from __future__ import annotations

import pytest

from config import settings
from db import get_db, init_db
from tournament import engine


@pytest.mark.asyncio
async def test_assign_random_teams_uses_captain_names_and_suffixes(tmp_path, monkeypatch):
    db_path = tmp_path / "assign-random.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    monkeypatch.setattr(engine.random, "shuffle", lambda values: None)
    await init_db()

    async with get_db() as db:
        await db.execute(
            "INSERT INTO tournaments (name, status, team_size, created_by, updated_at) VALUES (?, ?, ?, ?, ?)",
            ("Captain Names", "registration", 2, "admin", "now"),
        )
        signups = [
            ("100", "Earlysalty"),
            ("101", "Mate One"),
            ("200", "Earlysalty"),
            ("201", "Mate Two"),
        ]
        for discord_id, discord_name in signups:
            await db.execute(
                """
                INSERT INTO tournament_signups (
                    tournament_id, discord_id, discord_name, steam_id, rank, rank_score
                ) VALUES (?, ?, ?, ?, ?, ?)
                """,
                (1, discord_id, discord_name, None, None, 0),
            )
        await db.commit()

    teams_created = await engine.assign_random_teams(1, 2)
    assert teams_created == 2

    async with get_db() as db:
        cursor = await db.execute("SELECT name FROM teams WHERE tournament_id = 1 ORDER BY id")
        team_names = [row["name"] for row in await cursor.fetchall()]

    assert team_names == ["Earlysalty Team", "Earlysalty Team (2)"]


@pytest.mark.asyncio
async def test_finalize_checkin_keeps_manual_name_and_uses_captain_name_for_new_team(tmp_path, monkeypatch):
    db_path = tmp_path / "finalize-checkin.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            """
            INSERT INTO tournaments (
                name, status, team_size, created_by, updated_at, tournament_mode
            ) VALUES (?, ?, ?, ?, ?, ?)
            """,
            ("Finalize", "checkin", 2, "admin", "now", "bracket_only"),
        )
        await db.execute(
            "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?)",
            (1, "Nova Team", "nova team", "111"),
        )
        await db.execute(
            """
            INSERT INTO team_members (team_id, discord_id, discord_name, role)
            VALUES (?, ?, ?, ?)
            """,
            (1, "111", "Manual Captain", "captain"),
        )
        await db.execute(
            """
            INSERT INTO team_members (team_id, discord_id, discord_name, role)
            VALUES (?, ?, ?, ?)
            """,
            (1, "112", "Manual Mate", "member"),
        )
        existing_signups = [
            ("111", "Manual Captain", 1),
            ("112", "Manual Mate", 1),
            ("211", "Nova", None),
            ("212", "Pool Mate", None),
        ]
        for discord_id, discord_name, team_id in existing_signups:
            await db.execute(
                """
                INSERT INTO tournament_signups (
                    tournament_id, discord_id, discord_name, steam_id, rank, rank_score, team_id
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (1, discord_id, discord_name, None, None, 0, team_id),
            )
            await db.execute(
                "INSERT INTO tournament_checkins (tournament_id, discord_id) VALUES (?, ?)",
                (1, discord_id),
            )
        await db.commit()

    preview = await engine.finalize_checkin(1, confirm=False)
    await engine.finalize_checkin(
        1,
        confirm=True,
        expected_snapshot_token=preview["snapshot_token"],
    )

    async with get_db() as db:
        cursor = await db.execute("SELECT name, captain_discord_id FROM teams WHERE tournament_id = 1 ORDER BY id")
        teams = [dict(row) for row in await cursor.fetchall()]

    assert teams[0]["name"] == "Nova Team"
    assert teams[1]["name"] == "Nova Team (2)"
    assert teams[1]["captain_discord_id"] == "211"
