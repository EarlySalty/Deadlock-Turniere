"""Series Manager — Bo3/Bo5 Serien-Logik für Bracket-Matches."""
from __future__ import annotations

import json
from datetime import datetime, timezone
from typing import Any

from db import get_db


async def ensure_game_exists(bracket_match_id: int, game_number: int) -> int:
    """Stellt sicher dass Spiel N in der Serie existiert, gibt game.id zurück."""
    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT id FROM match_games WHERE bracket_match_id = ? AND game_number = ?",
            (bracket_match_id, game_number),
        )
        row = await cursor.fetchone()
        if row:
            return int(row["id"])

        cursor = await db.execute(
            """
            INSERT INTO match_games (bracket_match_id, game_number, status, created_at)
            VALUES (?, ?, 'pending', ?)
            """,
            (bracket_match_id, game_number, now),
        )
        await db.commit()
        return int(cursor.lastrowid)


async def record_game_result(
    bracket_match_id: int,
    game_number: int,
    *,
    winner_team: int,
    steam_party_id: str | None = None,
    deadlock_match_id: str | None = None,
    duration_s: int | None = None,
    match_stats: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """
    Trägt Ergebnis für Spiel N ein und prüft ob die Serie entschieden ist.
    Gibt zurück: {series_done, series_winner_team, wins_team1, wins_team2, next_game_number}
    """
    if winner_team not in (1, 2):
        raise ValueError("winner_team muss 1 oder 2 sein")

    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT id
            FROM match_games
            WHERE bracket_match_id = ? AND game_number = ?
            """,
            (bracket_match_id, game_number),
        )
        game = await cursor.fetchone()
        if not game:
            cursor = await db.execute(
                """
                INSERT INTO match_games (bracket_match_id, game_number, status, created_at)
                VALUES (?, ?, 'pending', ?)
                """,
                (bracket_match_id, game_number, now),
            )
            game_id = int(cursor.lastrowid)
        else:
            game_id = int(game["id"])

        await db.execute(
            """
            UPDATE match_games
            SET winner_team = ?,
                steam_party_id = COALESCE(?, steam_party_id),
                deadlock_match_id = COALESCE(?, deadlock_match_id),
                duration_s = COALESCE(?, duration_s),
                match_stats = COALESCE(?, match_stats),
                status = 'completed',
                completed_at = ?
            WHERE id = ?
            """,
            (
                winner_team,
                steam_party_id,
                deadlock_match_id,
                duration_s,
                json.dumps(match_stats) if match_stats is not None else None,
                now,
                game_id,
            ),
        )
        await db.commit()

        cursor = await db.execute(
            """
            SELECT winner_team
            FROM match_games
            WHERE bracket_match_id = ? AND status = 'completed'
            """,
            (bracket_match_id,),
        )
        rows = await cursor.fetchall()

        cursor = await db.execute(
            """
            SELECT t.series_format
            FROM tournaments t
            JOIN bracket_matches bm ON bm.tournament_id = t.id
            WHERE bm.id = ?
            """,
            (bracket_match_id,),
        )
        fmt_row = await cursor.fetchone()

    series_format = int(fmt_row["series_format"]) if fmt_row else 1
    wins_needed = (series_format // 2) + 1
    wins1 = sum(1 for row in rows if row["winner_team"] == 1)
    wins2 = sum(1 for row in rows if row["winner_team"] == 2)

    series_done = wins1 >= wins_needed or wins2 >= wins_needed
    series_winner = 1 if wins1 >= wins_needed else 2 if wins2 >= wins_needed else None

    return {
        "series_done": series_done,
        "series_winner_team": series_winner,
        "wins_team1": wins1,
        "wins_team2": wins2,
        "next_game_number": game_number + 1 if not series_done else None,
    }


async def get_series_games(bracket_match_id: int) -> list[dict[str, Any]]:
    """Gibt alle Spiele einer Serie zurück."""
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM match_games WHERE bracket_match_id = ? ORDER BY game_number",
            (bracket_match_id,),
        )
        rows = await cursor.fetchall()
    return [dict(row) for row in rows]
