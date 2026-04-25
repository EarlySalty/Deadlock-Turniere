from __future__ import annotations

import pytest

from config import settings
from db import get_db, init_db
from match.game_modes import prepare_match_assignments


@pytest.mark.asyncio
async def test_prepare_match_assignments_covers_all_modes(tmp_path, monkeypatch):
    db_path = tmp_path / "game-modes.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            """
            INSERT INTO tournaments (
                name, status, created_by, updated_at, tournament_game_mode
            ) VALUES (?, ?, ?, ?, ?)
            """,
            ("Modes", "bracket", "admin", "now", "standard"),
        )
        await db.execute(
            "INSERT INTO teams (id, tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?, ?)",
            (1, 1, "Alpha", "alpha", "100"),
        )
        await db.execute(
            "INSERT INTO teams (id, tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?, ?)",
            (2, 1, "Bravo", "bravo", "200"),
        )
        for offset in range(3):
            await db.execute(
                """
                INSERT INTO team_members (team_id, discord_id, discord_name, role)
                VALUES (?, ?, ?, ?)
                """,
                (1, f"10{offset}", f"Alpha {offset}", "member"),
            )
            await db.execute(
                """
                INSERT INTO team_members (team_id, discord_id, discord_name, role)
                VALUES (?, ?, ?, ?)
                """,
                (2, f"20{offset}", f"Bravo {offset}", "member"),
            )
        await db.execute(
            """
            INSERT INTO bracket_matches (
                id, tournament_id, round, position, team1_id, team2_id, status
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (1, 1, 1, 0, 1, 2, "pending"),
        )
        await db.commit()

    async with get_db() as db:
        await db.execute(
            "UPDATE tournaments SET tournament_game_mode = ? WHERE id = 1",
            ("standard",),
        )
        await db.commit()
    standard = await prepare_match_assignments(1, "bracket", 1)
    assert standard["convars"] == {}
    assert standard["hero_assignments"] == {}
    assert standard["announcement_lines"] == []

    async with get_db() as db:
        await db.execute(
            "UPDATE tournaments SET tournament_game_mode = ? WHERE id = 1",
            ("mirror",),
        )
        await db.commit()
    mirror = await prepare_match_assignments(1, "bracket", 1)
    assert mirror["convars"]["citadel_allow_duplicate_heroes"] == 1
    assert len(mirror["hero_assignments"]["teams"]) == 2
    assert len(set(mirror["hero_assignments"]["teams"].values())) == 2

    async with get_db() as db:
        await db.execute(
            "UPDATE tournaments SET tournament_game_mode = ? WHERE id = 1",
            ("all_same",),
        )
        await db.commit()
    all_same = await prepare_match_assignments(1, "bracket", 1)
    assert all_same["convars"]["citadel_allow_duplicate_heroes"] == 1
    assert len(set(all_same["hero_assignments"]["players"].values())) == 1
    assert len(all_same["announcement_lines"]) == 1

    async with get_db() as db:
        await db.execute(
            "UPDATE tournaments SET tournament_game_mode = ? WHERE id = 1",
            ("random_heroes",),
        )
        await db.commit()
    random_heroes = await prepare_match_assignments(1, "bracket", 1)
    player_assignments = random_heroes["hero_assignments"]["players"]
    assert len(player_assignments) == 6
    assert len(set(player_assignments.values())) == 6
    assert random_heroes["convars"] == {}

    async with get_db() as db:
        await db.execute(
            "UPDATE tournaments SET tournament_game_mode = ? WHERE id = 1",
            ("single_lane",),
        )
        await db.commit()
    single_lane = await prepare_match_assignments(1, "bracket", 1)
    assert single_lane["convars"] == {}
    assert any("Vorbereitung" in line for line in single_lane["announcement_lines"])
