from __future__ import annotations

import logging

from db import get_db

logger = logging.getLogger(__name__)


async def schedule_auto_lobbies_for_tournament(tournament_id: int) -> None:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT auto_lobby_enabled, is_test FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament or bool(tournament["is_test"]) or not bool(tournament["auto_lobby_enabled"]):
            return

        cursor = await db.execute(
            """
            SELECT id
            FROM bracket_matches
            WHERE tournament_id = ?
              AND team1_id IS NOT NULL
              AND team2_id IS NOT NULL
              AND status IN ('pending', 'checkin')
              AND steam_party_id IS NULL
            ORDER BY round, position, id
            """,
            (tournament_id,),
        )
        bracket_match_ids = [int(row["id"]) for row in await cursor.fetchall()]

        cursor = await db.execute(
            """
            SELECT gm.id
            FROM group_matches gm
            JOIN groups g ON g.id = gm.group_id
            WHERE g.tournament_id = ?
              AND gm.team1_id IS NOT NULL
              AND gm.team2_id IS NOT NULL
              AND gm.status IN ('pending', 'checkin')
              AND gm.steam_party_id IS NULL
            ORDER BY gm.id
            """,
            (tournament_id,),
        )
        group_match_ids = [int(row["id"]) for row in await cursor.fetchall()]

    for match_id in bracket_match_ids:
        try:
            from match import manager as match_manager

            await match_manager.create_lobby(tournament_id, match_id)
        except Exception:
            logger.exception(
                "Auto-Lobby für Bracket-Match fehlgeschlagen (tournament=%s match=%s)",
                tournament_id,
                match_id,
            )

    for match_id in group_match_ids:
        try:
            from match import manager as match_manager

            await match_manager.create_group_lobby(tournament_id, match_id)
        except Exception:
            logger.exception(
                "Auto-Lobby für Group-Match fehlgeschlagen (tournament=%s match=%s)",
                tournament_id,
                match_id,
            )


async def schedule_auto_lobby_for_next_round(
    tournament_id: int,
    completed_match_id: int,
) -> None:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT auto_lobby_enabled, is_test FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament or bool(tournament["is_test"]) or not bool(tournament["auto_lobby_enabled"]):
            return

        cursor = await db.execute(
            """
            SELECT id
            FROM bracket_matches
            WHERE tournament_id = ?
              AND (source_match1_id = ? OR source_match2_id = ?)
              AND team1_id IS NOT NULL
              AND team2_id IS NOT NULL
              AND status IN ('pending', 'checkin')
              AND steam_party_id IS NULL
            ORDER BY round, position, id
            """,
            (tournament_id, completed_match_id, completed_match_id),
        )
        next_match_ids = [int(row["id"]) for row in await cursor.fetchall()]

    for match_id in next_match_ids:
        try:
            from match import manager as match_manager

            await match_manager.create_lobby(tournament_id, match_id)
        except Exception:
            logger.exception(
                "Auto-Lobby für Folge-Match fehlgeschlagen (tournament=%s completed_match=%s next_match=%s)",
                tournament_id,
                completed_match_id,
                match_id,
            )
