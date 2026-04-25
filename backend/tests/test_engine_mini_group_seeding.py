from __future__ import annotations

import pytest

from config import settings
from db import get_db, init_db
from tournament import engine


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("team_count", "expected_match_count", "expected_mini_group_count"),
    [
        (5, 5, 1),
        (6, 6, 1),
        (7, 8, 2),
        (9, 9, 1),
        (11, 12, 2),
        (13, 14, 2),
    ],
)
async def test_generate_bracket_uses_mini_groups_without_byes(
    tmp_path,
    monkeypatch,
    team_count: int,
    expected_match_count: int,
    expected_mini_group_count: int,
):
    db_path = tmp_path / f"mini-groups-{team_count}.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            "INSERT INTO tournaments (name, status, created_by, updated_at) VALUES (?, ?, ?, ?)",
            (f"Mini RR {team_count}", "bracket", "admin", "now"),
        )
        for team_number in range(team_count):
            await db.execute(
                "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) VALUES (?, ?, ?, ?)",
                (1, f"Team {team_number + 1}", f"team-{team_number + 1}", f"{team_number + 1:03d}"),
            )
        await db.commit()

    match_count = await engine.generate_bracket(1)
    assert match_count == expected_match_count

    async with get_db() as db:
        cursor = await db.execute("SELECT COUNT(*) AS cnt FROM bracket_matches WHERE tournament_id = 1")
        assert int((await cursor.fetchone())["cnt"]) == expected_match_count

        cursor = await db.execute("SELECT * FROM bracket_mini_groups WHERE tournament_id = 1 ORDER BY round, position, id")
        mini_groups = [dict(row) for row in await cursor.fetchall()]
        assert len(mini_groups) == expected_mini_group_count

        cursor = await db.execute(
            """
            SELECT id, team1_id, team2_id, source_match1_id, source_match2_id, source_mini_group1_id, source_mini_group2_id
            FROM bracket_matches
            WHERE tournament_id = 1
            ORDER BY round, position, id
            """
        )
        matches = [dict(row) for row in await cursor.fetchall()]

        for match in matches:
            assert (
                match["team1_id"] is not None
                or match["source_match1_id"] is not None
                or match["source_mini_group1_id"] is not None
            )
            assert (
                match["team2_id"] is not None
                or match["source_match2_id"] is not None
                or match["source_mini_group2_id"] is not None
            )

        for mini_group in mini_groups:
            cursor = await db.execute(
                "SELECT COUNT(*) AS cnt FROM bracket_mini_group_teams WHERE mini_group_id = ?",
                (mini_group["id"],),
            )
            participant_count = int((await cursor.fetchone())["cnt"])
            cursor = await db.execute(
                "SELECT COUNT(*) AS cnt FROM bracket_matches WHERE mini_group_id = ?",
                (mini_group["id"],),
            )
            rr_match_count = int((await cursor.fetchone())["cnt"])
            assert rr_match_count == participant_count * (participant_count - 1) // 2

            if mini_group["advances_to_match_id"] is None:
                cursor = await db.execute(
                    """
                    SELECT COUNT(*) AS cnt
                    FROM bracket_matches
                    WHERE source_mini_group1_id = ? OR source_mini_group2_id = ?
                    """,
                    (mini_group["id"], mini_group["id"]),
                )
                downstream_count = int((await cursor.fetchone())["cnt"])
                if mini_group["round"] < max(group["round"] for group in mini_groups):
                    assert downstream_count > 0
            else:
                assert mini_group["advances_to_slot"] in (1, 2)
