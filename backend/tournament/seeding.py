"""Seeding-Logik — Team-Platzierung basierend auf Rank-Score."""
from __future__ import annotations

RANK_KEYS = [
    "initiate",
    "seeker",
    "alchemist",
    "arcanist",
    "ritualist",
    "emissary",
    "archon",
    "oracle",
    "phantom",
    "ascendant",
    "eternus",
]
RANK_VALUES = {rank: idx + 1 for idx, rank in enumerate(RANK_KEYS)}


def rank_score(rank: str | None, subrank: int | None) -> int:
    """Balance-Score: tier * 6 + subrank. Gleiche Formel wie Discord Bot."""
    tier = RANK_VALUES.get((rank or "").lower(), 0)
    sub = max(1, min(6, int(subrank or 3)))
    if tier == 0:
        return 3
    return tier * 6 + sub


def team_avg_score(member_scores: list[int]) -> float:
    """Durchschnittlicher Rank-Score eines Teams."""
    if not member_scores:
        return 0.0
    return sum(member_scores) / len(member_scores)
