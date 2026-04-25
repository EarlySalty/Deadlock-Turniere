from __future__ import annotations

from draft.heroes import DEADLOCK_HEROES

HEROES: list[dict[str, str]] = [
    {"id": f"hero_{index + 1}", "name": hero_name}
    for index, hero_name in enumerate(DEADLOCK_HEROES)
]

HERO_IDS: list[str] = [hero["id"] for hero in HEROES]
HERO_NAMES: list[str] = [hero["name"] for hero in HEROES]
