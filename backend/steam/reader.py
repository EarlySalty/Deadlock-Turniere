"""Steam Bridge Reader — Read-only Zugriff auf die Discord Bot SQLite (steam_links)."""
from __future__ import annotations

from typing import Optional

import aiosqlite

from config import settings


def _rank_score(rank_tier: int | None, subrank: int | None) -> int:
    """Balance-Score: Initiate 1 = 7, Eternus 6 = 72, Obscurus = 3.

    Gleiche Formel wie im Discord Bot (turnier.py).
    """
    tier = int(rank_tier or 0)
    sub = max(1, min(6, int(subrank or 3)))
    if tier == 0:
        return 3
    return tier * 6 + sub


async def get_steam_link(discord_id: str) -> Optional[dict]:
    """Liest steam_id, Rank-Infos und berechnet rank_score für einen User.

    Bevorzugt den als primary markierten Account.
    Fällt andernfalls auf den bestverfügbaren Account mit Rangdaten zurück.
    Gibt None zurück wenn kein Link gefunden oder DB-Pfad nicht konfiguriert.
    """
    if not settings.STEAM_BRIDGE_DB_PATH:
        return None

    try:
        async with aiosqlite.connect(
            f"file:{settings.STEAM_BRIDGE_DB_PATH}?mode=ro",
            uri=True,
        ) as db:
            db.row_factory = aiosqlite.Row
            cursor = await db.execute(
                "SELECT steam_id, deadlock_rank, deadlock_rank_name, deadlock_subrank, primary_account "
                "FROM steam_links "
                "WHERE user_id = ? "
                "ORDER BY "
                "primary_account DESC, "
                "CASE WHEN deadlock_rank IS NULL THEN 1 ELSE 0 END ASC, "
                "deadlock_rank DESC, "
                "deadlock_subrank DESC "
                "LIMIT 1",
                (discord_id,),
            )
            row = await cursor.fetchone()

        if not row:
            return None

        rank_tier = row["deadlock_rank"]
        subrank = row["deadlock_subrank"]

        return {
            "steam_id": row["steam_id"],
            "rank": row["deadlock_rank_name"],
            "rank_tier": rank_tier,
            "subrank": subrank,
            "rank_score": _rank_score(rank_tier, subrank),
        }
    except Exception:
        return None
