from __future__ import annotations

import pytest

from config import settings
from db import get_db, init_db
from tournament import engine


@pytest.mark.asyncio
async def test_generate_bracket_uses_cross_seed_pairs_from_groups(tmp_path, monkeypatch):
    db_path = tmp_path / "cross-seeding.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            """
            INSERT INTO tournaments (name, status, created_by, updated_at)
            VALUES (?, ?, ?, ?)
            """,
            ("Cross Seed", "bracket", "admin", "now"),
        )

        team_ids: dict[str, int] = {}
        for team_name in ["A1", "A2", "B1", "B2", "C1", "C2", "D1", "D2"]:
            cursor = await db.execute(
                """
                INSERT INTO teams (tournament_id, name, name_key, captain_discord_id)
                VALUES (?, ?, ?, ?)
                """,
                (1, team_name, team_name.casefold(), f"captain-{team_name}"),
            )
            team_ids[team_name] = int(cursor.lastrowid)

        group_ids: dict[str, int] = {}
        for seeding_order, group_name in enumerate(["A", "B", "C", "D"]):
            cursor = await db.execute(
                "INSERT INTO groups (tournament_id, name, seeding_order) VALUES (?, ?, ?)",
                (1, f"Gruppe {group_name}", seeding_order),
            )
            group_ids[group_name] = int(cursor.lastrowid)

        group_rows = [
            ("A", "A1", 9, 3, 0),
            ("A", "A2", 6, 2, 1),
            ("B", "B1", 9, 3, 0),
            ("B", "B2", 6, 2, 1),
            ("C", "C1", 9, 3, 0),
            ("C", "C2", 6, 2, 1),
            ("D", "D1", 9, 3, 0),
            ("D", "D2", 6, 2, 1),
        ]
        for group_name, team_name, points, wins, losses in group_rows:
            await db.execute(
                """
                INSERT INTO group_teams (group_id, team_id, points, wins, losses)
                VALUES (?, ?, ?, ?, ?)
                """,
                (group_ids[group_name], team_ids[team_name], points, wins, losses),
            )

        await db.commit()

    match_count = await engine.generate_bracket(1)
    assert match_count == 7

    expected_pairs = [
        (team_ids["A1"], team_ids["B2"]),
        (team_ids["B1"], team_ids["C2"]),
        (team_ids["C1"], team_ids["D2"]),
        (team_ids["D1"], team_ids["A2"]),
    ]

    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT team1_id, team2_id
            FROM bracket_matches
            WHERE tournament_id = ? AND round = ? AND bracket_type = 'winners'
            ORDER BY position
            """,
            (1, 1),
        )
        round_one_pairs = [
            (int(row["team1_id"]), int(row["team2_id"]))
            for row in await cursor.fetchall()
        ]

    assert round_one_pairs == expected_pairs
    assert all(team1_id != team_ids["A2"] or team2_id != team_ids["A1"] for team1_id, team2_id in round_one_pairs)
    assert all(team1_id != team_ids["B2"] or team2_id != team_ids["B1"] for team1_id, team2_id in round_one_pairs)
    assert all(team1_id != team_ids["C2"] or team2_id != team_ids["C1"] for team1_id, team2_id in round_one_pairs)
    assert all(team1_id != team_ids["D2"] or team2_id != team_ids["D1"] for team1_id, team2_id in round_one_pairs)
