from __future__ import annotations

import pytest

from config import settings
from db import get_db, init_db
from tournament import engine


async def _load_matches(
    tournament_id: int,
    bracket_type: str,
    round_num: int,
) -> list[dict]:
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT *
            FROM bracket_matches
            WHERE tournament_id = ? AND bracket_type = ? AND round = ?
            ORDER BY position, id
            """,
            (tournament_id, bracket_type, round_num),
        )
        return [dict(row) for row in await cursor.fetchall()]


@pytest.mark.asyncio
async def test_generate_bracket_builds_double_elimination_and_grand_final_reset(tmp_path, monkeypatch):
    db_path = tmp_path / "double-elim.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            """
            INSERT INTO tournaments (name, status, created_by, updated_at, bracket_format)
            VALUES (?, ?, ?, ?, ?)
            """,
            ("Double Elim", "bracket", "admin", "now", "double_elimination"),
        )
        for team_number in range(8):
            await db.execute(
                """
                INSERT INTO teams (tournament_id, name, name_key, captain_discord_id)
                VALUES (?, ?, ?, ?)
                """,
                (1, f"Team {team_number + 1}", f"team-{team_number + 1}", f"{team_number + 1:03d}"),
            )
        await db.commit()

    match_count = await engine.generate_bracket(1)
    assert match_count == 15

    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT bracket_type, COUNT(*) AS cnt
            FROM bracket_matches
            WHERE tournament_id = ?
            GROUP BY bracket_type
            """,
            (1,),
        )
        type_counts = {row["bracket_type"]: int(row["cnt"]) for row in await cursor.fetchall()}

    assert type_counts == {"winners": 7, "losers": 6, "grand_final": 2}

    winners_round_one = await _load_matches(1, "winners", 1)
    winners_round_two = await _load_matches(1, "winners", 2)
    winners_final = await _load_matches(1, "winners", 3)
    losers_round_one = await _load_matches(1, "losers", 1)
    losers_round_two = await _load_matches(1, "losers", 2)
    losers_round_three = await _load_matches(1, "losers", 3)
    losers_final = await _load_matches(1, "losers", 4)
    grand_finals = await _load_matches(1, "grand_final", 4)
    grand_final_reset = await _load_matches(1, "grand_final", 5)

    assert len(winners_round_one) == 4
    assert len(winners_round_two) == 2
    assert len(winners_final) == 1
    assert len(losers_round_one) == 2
    assert len(losers_round_two) == 2
    assert len(losers_round_three) == 1
    assert len(losers_final) == 1
    assert len(grand_finals) == 1
    assert len(grand_final_reset) == 1

    assert winners_round_one[0]["loser_to_match_id"] == losers_round_one[0]["id"]
    assert winners_round_one[0]["loser_to_slot"] == 1
    assert winners_round_one[1]["loser_to_match_id"] == losers_round_one[0]["id"]
    assert winners_round_one[1]["loser_to_slot"] == 2
    assert winners_round_one[2]["loser_to_match_id"] == losers_round_one[1]["id"]
    assert winners_round_one[2]["loser_to_slot"] == 1
    assert winners_round_one[3]["loser_to_match_id"] == losers_round_one[1]["id"]
    assert winners_round_one[3]["loser_to_slot"] == 2

    assert winners_round_two[0]["loser_to_match_id"] == losers_round_two[1]["id"]
    assert winners_round_two[0]["loser_to_slot"] == 2
    assert winners_round_two[1]["loser_to_match_id"] == losers_round_two[0]["id"]
    assert winners_round_two[1]["loser_to_slot"] == 2
    assert winners_final[0]["loser_to_match_id"] == losers_final[0]["id"]
    assert winners_final[0]["loser_to_slot"] == 2

    for match in winners_round_one:
        await engine.advance_bracket_winner(1, int(match["id"]), int(match["team1_id"]))

    losers_round_one = await _load_matches(1, "losers", 1)
    winners_round_two = await _load_matches(1, "winners", 2)
    assert [(match["team1_id"], match["team2_id"]) for match in losers_round_one] == [(8, 5), (7, 6)]
    assert [(match["team1_id"], match["team2_id"]) for match in winners_round_two] == [(1, 4), (2, 3)]

    await engine.advance_bracket_winner(1, int(losers_round_one[0]["id"]), int(losers_round_one[0]["team2_id"]))
    await engine.advance_bracket_winner(1, int(losers_round_one[1]["id"]), int(losers_round_one[1]["team2_id"]))

    for match in winners_round_two:
        await engine.advance_bracket_winner(1, int(match["id"]), int(match["team1_id"]))

    losers_round_two = await _load_matches(1, "losers", 2)
    winners_final = await _load_matches(1, "winners", 3)
    assert [(match["team1_id"], match["team2_id"]) for match in losers_round_two] == [(5, 3), (6, 4)]
    assert [(match["team1_id"], match["team2_id"]) for match in winners_final] == [(1, 2)]

    await engine.advance_bracket_winner(1, int(losers_round_two[0]["id"]), int(losers_round_two[0]["team2_id"]))
    await engine.advance_bracket_winner(1, int(losers_round_two[1]["id"]), int(losers_round_two[1]["team2_id"]))

    losers_round_three = await _load_matches(1, "losers", 3)
    assert [(match["team1_id"], match["team2_id"]) for match in losers_round_three] == [(3, 4)]
    await engine.advance_bracket_winner(
        1,
        int(losers_round_three[0]["id"]),
        int(losers_round_three[0]["team1_id"]),
    )

    await engine.advance_bracket_winner(
        1,
        int(winners_final[0]["id"]),
        int(winners_final[0]["team1_id"]),
    )

    losers_final = await _load_matches(1, "losers", 4)
    assert [(match["team1_id"], match["team2_id"]) for match in losers_final] == [(3, 2)]
    await engine.advance_bracket_winner(
        1,
        int(losers_final[0]["id"]),
        int(losers_final[0]["team1_id"]),
    )

    grand_finals = await _load_matches(1, "grand_final", 4)
    assert [(match["team1_id"], match["team2_id"]) for match in grand_finals] == [(1, 3)]
    await engine.advance_bracket_winner(
        1,
        int(grand_finals[0]["id"]),
        int(grand_finals[0]["team2_id"]),
    )

    grand_final_reset = await _load_matches(1, "grand_final", 5)
    assert grand_final_reset[0]["status"] == "pending"
    assert (grand_final_reset[0]["team1_id"], grand_final_reset[0]["team2_id"]) == (1, 3)


@pytest.mark.asyncio
async def test_double_elimination_sets_stream_heuristic(tmp_path, monkeypatch):
    db_path = tmp_path / "double-elim-stream.db"
    avatars_dir = tmp_path / "avatars"
    monkeypatch.setattr(settings, "DATABASE_PATH", str(db_path))
    monkeypatch.setattr(settings, "AVATAR_DIR", str(avatars_dir))
    await init_db()

    async with get_db() as db:
        await db.execute(
            """
            INSERT INTO tournaments (name, status, created_by, updated_at, bracket_format)
            VALUES (?, ?, ?, ?, ?)
            """,
            ("Stream Heuristic", "bracket", "admin", "now", "double_elimination"),
        )
        for team_number in range(8):
            await db.execute(
                """
                INSERT INTO teams (tournament_id, name, name_key, captain_discord_id)
                VALUES (?, ?, ?, ?)
                """,
                (1, f"Team {team_number + 1}", f"team-{team_number + 1}", f"{team_number + 1:03d}"),
            )
        await db.commit()

    await engine.generate_bracket(1)

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT bracket_type, round, on_stream FROM bracket_matches WHERE tournament_id = 1"
        )
        rows = [dict(row) for row in await cursor.fetchall()]

    # Winner-Bracket und Grand Final laufen auf Stream
    assert all(r["on_stream"] == 1 for r in rows if r["bracket_type"] == "winners")
    assert all(r["on_stream"] == 1 for r in rows if r["bracket_type"] == "grand_final")

    losers = [r for r in rows if r["bracket_type"] == "losers"]
    last_losers_round = max(r["round"] for r in losers)
    # LB-Finale auf Stream, frühere Loser-Runden parallel/off-stream
    assert all(r["on_stream"] == 1 for r in losers if r["round"] == last_losers_round)
    assert all(r["on_stream"] == 0 for r in losers if r["round"] != last_losers_round)
