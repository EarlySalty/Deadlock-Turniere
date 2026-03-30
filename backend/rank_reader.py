"""Combined rank reader — cached rank profiles with Steam-DB-first fallback."""
from __future__ import annotations

import time
from typing import Optional

import aiosqlite
import httpx

from config import settings
from db import get_db
from steam.reader import get_steam_link
from tournament.seeding import rank_score as calc_rank_score

DISCORD_API = "https://discord.com/api/v10"
RANK_CACHE_TTL_SECONDS = 60 * 60 * 24

MAIN_RANK_ROLE_IDS: dict[int, tuple[str, int]] = {
    1331457571118387210: ("Initiate", 1),
    1331457652877955072: ("Seeker", 2),
    1331457699992436829: ("Alchemist", 3),
    1331457724848017539: ("Arcanist", 4),
    1331457879345070110: ("Ritualist", 5),
    1331457898781474836: ("Emissary", 6),
    1331457949654319114: ("Archon", 7),
    1316966867033653338: ("Oracle", 8),
    1331458016356208680: ("Phantom", 9),
    1331458049637875785: ("Ascendant", 10),
    1331458087349129296: ("Eternus", 11),
}

RANK_NAMES_BY_TIER: dict[int, str] = {
    1: "Initiate",
    2: "Seeker",
    3: "Alchemist",
    4: "Arcanist",
    5: "Ritualist",
    6: "Emissary",
    7: "Archon",
    8: "Oracle",
    9: "Phantom",
    10: "Ascendant",
    11: "Eternus",
}

_memory_rank_cache: dict[str, tuple[float, Optional[dict]]] = {}
_discord_subrank_role_cache: tuple[float, dict[int, tuple[int, int]]] | None = None


def _normalize_profile(profile: dict | None) -> Optional[dict]:
    if not profile:
        return None
    return {
        "steam_id": profile.get("steam_id"),
        "rank": profile.get("rank"),
        "rank_tier": profile.get("rank_tier"),
        "subrank": profile.get("subrank"),
        "rank_score": profile.get("rank_score"),
        "source": profile.get("source"),
    }


def _get_memory_cache(discord_id: str) -> Optional[dict]:
    entry = _memory_rank_cache.get(discord_id)
    if not entry:
        return None

    expires_at, profile = entry
    if expires_at <= time.time():
        _memory_rank_cache.pop(discord_id, None)
        return None

    return dict(profile) if profile else None


def _set_memory_cache(discord_id: str, profile: dict | None) -> None:
    _memory_rank_cache[discord_id] = (
        time.time() + RANK_CACHE_TTL_SECONDS,
        _normalize_profile(profile),
    )


async def _get_cached_rank_profile(discord_id: str) -> Optional[dict]:
    memory_cached = _get_memory_cache(discord_id)
    if memory_cached is not None:
        return memory_cached

    try:
        async with get_db() as db:
            cursor = await db.execute(
                "SELECT source, steam_id, rank, rank_tier, subrank, rank_score, cached_at "
                "FROM rank_cache WHERE discord_id = ?",
                (discord_id,),
            )
            row = await cursor.fetchone()
    except Exception:
        return None

    if not row:
        return None

    cached_at = int(row["cached_at"] or 0)
    if cached_at + RANK_CACHE_TTL_SECONDS <= int(time.time()):
        try:
            async with get_db() as db:
                await db.execute("DELETE FROM rank_cache WHERE discord_id = ?", (discord_id,))
                await db.commit()
        except Exception:
            pass
        return None

    profile = {
        "source": row["source"],
        "steam_id": row["steam_id"],
        "rank": row["rank"],
        "rank_tier": row["rank_tier"],
        "subrank": row["subrank"],
        "rank_score": row["rank_score"],
    }
    _set_memory_cache(discord_id, profile)
    return profile


async def _store_cached_rank_profile(discord_id: str, profile: dict) -> dict:
    normalized = _normalize_profile(profile)
    if normalized is None:
        return profile

    try:
        async with get_db() as db:
            await db.execute(
                "INSERT INTO rank_cache "
                "(discord_id, source, steam_id, rank, rank_tier, subrank, rank_score, cached_at) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%s','now')) "
                "ON CONFLICT(discord_id) DO UPDATE SET "
                "source = excluded.source, "
                "steam_id = excluded.steam_id, "
                "rank = excluded.rank, "
                "rank_tier = excluded.rank_tier, "
                "subrank = excluded.subrank, "
                "rank_score = excluded.rank_score, "
                "cached_at = excluded.cached_at",
                (
                    discord_id,
                    normalized.get("source") or "unknown",
                    normalized.get("steam_id"),
                    normalized.get("rank"),
                    normalized.get("rank_tier"),
                    normalized.get("subrank"),
                    normalized.get("rank_score"),
                ),
            )
            await db.commit()
    except Exception:
        pass

    _set_memory_cache(discord_id, normalized)
    return normalized


async def _load_discord_subrank_roles() -> dict[int, tuple[int, int]]:
    """Lädt die Discord-Subrank-Rollen aus der Bridge-DB mit kurzem In-Memory-Cache."""
    global _discord_subrank_role_cache

    if _discord_subrank_role_cache and _discord_subrank_role_cache[0] > time.time():
        return _discord_subrank_role_cache[1]

    if not settings.STEAM_BRIDGE_DB_PATH:
        return {}

    try:
        async with aiosqlite.connect(
            f"file:{settings.STEAM_BRIDGE_DB_PATH}?mode=ro",
            uri=True,
        ) as db:
            db.row_factory = aiosqlite.Row
            cursor = await db.execute(
                "SELECT role_id, rank_value, subrank "
                "FROM deadlock_subrank_roles "
                "WHERE guild_id = ?",
                (int(settings.DISCORD_GUILD_ID),),
            )
            rows = await cursor.fetchall()
    except Exception:
        return {}

    role_map = {
        int(row["role_id"]): (int(row["rank_value"]), int(row["subrank"]))
        for row in rows
        if row["role_id"] is not None
    }
    _discord_subrank_role_cache = (time.time() + 300, role_map)
    return role_map


async def get_discord_role_rank(discord_id: str) -> Optional[dict]:
    """Liest den Deadlock-Rang über Discord-Guild-Rollen."""
    if not settings.DISCORD_BOT_TOKEN or not settings.DISCORD_GUILD_ID:
        return None

    headers = {"Authorization": f"Bot {settings.DISCORD_BOT_TOKEN}"}

    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            response = await client.get(
                f"{DISCORD_API}/guilds/{settings.DISCORD_GUILD_ID}/members/{discord_id}",
                headers=headers,
            )
    except Exception:
        return None

    if response.status_code != 200:
        return None

    member_data = response.json()
    role_ids = {
        int(role_id)
        for role_id in member_data.get("roles", [])
        if str(role_id).isdigit()
    }

    subrank_role_map = await _load_discord_subrank_roles()
    subrank_candidates = [subrank_role_map[role_id] for role_id in role_ids if role_id in subrank_role_map]
    if subrank_candidates:
        rank_tier, subrank = max(subrank_candidates, key=lambda item: (item[0], item[1]))
        rank_name = RANK_NAMES_BY_TIER.get(rank_tier)
        if rank_name:
            return {
                "rank": rank_name,
                "rank_tier": rank_tier,
                "subrank": subrank,
                "rank_score": calc_rank_score(rank_name, subrank),
                "source": "discord_role",
            }

    rank_candidates = [MAIN_RANK_ROLE_IDS[role_id] for role_id in role_ids if role_id in MAIN_RANK_ROLE_IDS]
    if not rank_candidates:
        return None

    rank_name, rank_tier = max(rank_candidates, key=lambda item: item[1])
    subrank = 3

    return {
        "rank": rank_name,
        "rank_tier": rank_tier,
        "subrank": subrank,
        "rank_score": calc_rank_score(rank_name, subrank),
        "source": "discord_role",
    }


async def get_player_rank_profile(discord_id: str) -> Optional[dict]:
    """Lädt Rank-Profil aus Cache, dann Steam-DB, dann Discord-Rollen."""
    cached = await _get_cached_rank_profile(discord_id)
    if cached:
        return cached

    steam_link = await get_steam_link(discord_id)
    if steam_link:
        profile = {
            "steam_id": steam_link.get("steam_id"),
            "rank": steam_link.get("rank"),
            "rank_tier": steam_link.get("rank_tier"),
            "subrank": steam_link.get("subrank"),
            "rank_score": steam_link.get("rank_score"),
            "source": "steam_bridge",
        }
        return await _store_cached_rank_profile(discord_id, profile)

    discord_rank = await get_discord_role_rank(discord_id)
    if discord_rank:
        return await _store_cached_rank_profile(discord_id, discord_rank)

    return None
