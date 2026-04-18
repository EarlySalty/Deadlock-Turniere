"""Statische Deadlock-Heldenliste."""
from __future__ import annotations

DEADLOCK_HEROES: list[str] = [
    "Abrams",
    "Bebop",
    "Calico",
    "Dynamo",
    "Grey Talon",
    "Haze",
    "Holliday",
    "Infernus",
    "Ivy",
    "Kelvin",
    "Lady Geist",
    "Lash",
    "McGinnis",
    "Mirage",
    "Mo & Krill",
    "Paradox",
    "Pocket",
    "Seven",
    "Shiv",
    "Sinclair",
    "Vindicta",
    "Viscous",
    "Vyper",
    "Warden",
    "Wraith",
    "Yamato",
]

HERO_SET: frozenset[str] = frozenset(DEADLOCK_HEROES)


def is_valid_hero(name: str) -> bool:
    return name in HERO_SET
