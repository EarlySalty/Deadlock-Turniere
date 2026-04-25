from __future__ import annotations

import random
from typing import Any

from db import get_db
from match.heroes import HERO_NAMES
from tournament.models import TournamentGameMode


def _pick_unique_heroes(count: int) -> list[str]:
    if count <= 0:
        return []
    if count >= len(HERO_NAMES):
        return random.sample(HERO_NAMES, len(HERO_NAMES))
    return random.sample(HERO_NAMES, count)


def _display_name(participant: dict[str, Any]) -> str:
    return str(
        participant.get("discord_name")
        or participant.get("team_name")
        or participant.get("discord_id")
        or "Unbekannt"
    )


async def _load_match_mode_context(
    tournament_id: int,
    match_type: str,
    match_id: int,
) -> dict[str, Any]:
    async with get_db() as db:
        if match_type == "group":
            cursor = await db.execute(
                """
                SELECT t.id AS tournament_id,
                       t.tournament_game_mode,
                       gm.team1_id,
                       gm.team2_id,
                       team1.name AS team1_name,
                       team2.name AS team2_name
                FROM tournaments t
                JOIN groups g ON g.tournament_id = t.id
                JOIN group_matches gm ON gm.group_id = g.id
                LEFT JOIN teams team1 ON team1.id = gm.team1_id
                LEFT JOIN teams team2 ON team2.id = gm.team2_id
                WHERE t.id = ? AND gm.id = ?
                """,
                (tournament_id, match_id),
            )
        else:
            cursor = await db.execute(
                """
                SELECT t.id AS tournament_id,
                       t.tournament_game_mode,
                       bm.team1_id,
                       bm.team2_id,
                       team1.name AS team1_name,
                       team2.name AS team2_name
                FROM tournaments t
                JOIN bracket_matches bm ON bm.tournament_id = t.id
                LEFT JOIN teams team1 ON team1.id = bm.team1_id
                LEFT JOIN teams team2 ON team2.id = bm.team2_id
                WHERE t.id = ? AND bm.id = ?
                """,
                (tournament_id, match_id),
            )
        match_row = await cursor.fetchone()
        if not match_row:
            raise ValueError("Match oder Turnier nicht gefunden")

        team_ids = [
            int(team_id)
            for team_id in (match_row["team1_id"], match_row["team2_id"])
            if team_id is not None
        ]
        if not team_ids:
            return {
                "mode": TournamentGameMode(match_row["tournament_game_mode"]),
                "team1_id": match_row["team1_id"],
                "team2_id": match_row["team2_id"],
                "team1_name": match_row["team1_name"],
                "team2_name": match_row["team2_name"],
                "participants": [],
            }

        placeholders = ", ".join("?" for _ in team_ids)
        cursor = await db.execute(
            f"""
            SELECT tm.team_id, tm.discord_id, tm.discord_name, t.name AS team_name
            FROM team_members tm
            JOIN teams t ON t.id = tm.team_id
            WHERE tm.team_id IN ({placeholders})
            ORDER BY tm.team_id, tm.joined_at, tm.id
            """,
            team_ids,
        )
        participants = [dict(row) for row in await cursor.fetchall()]

    return {
        "mode": TournamentGameMode(match_row["tournament_game_mode"]),
        "team1_id": match_row["team1_id"],
        "team2_id": match_row["team2_id"],
        "team1_name": match_row["team1_name"] or f"Team {match_row['team1_id']}",
        "team2_name": match_row["team2_name"] or f"Team {match_row['team2_id']}",
        "participants": participants,
    }


async def prepare_match_assignments(
    tournament_id: int,
    match_type: str,
    match_id: int,
) -> dict[str, Any]:
    context = await _load_match_mode_context(tournament_id, match_type, match_id)
    mode = context["mode"]
    participants: list[dict[str, Any]] = context["participants"]
    team1_id = context["team1_id"]
    team2_id = context["team2_id"]
    team1_name = context["team1_name"]
    team2_name = context["team2_name"]

    if mode == TournamentGameMode.standard:
        return {"convars": {}, "hero_assignments": {}, "announcement_lines": []}

    if mode == TournamentGameMode.mirror:
        heroes = _pick_unique_heroes(2)
        hero_assignments = {
            "mode": mode.value,
            "teams": {
                str(team1_id): heroes[0],
                str(team2_id): heroes[1] if len(heroes) > 1 else heroes[0],
            },
        }
        return {
            "convars": {"citadel_allow_duplicate_heroes": 1},
            "hero_assignments": hero_assignments,
            "announcement_lines": [
                f"{team1_name}: {hero_assignments['teams'][str(team1_id)]}",
                f"{team2_name}: {hero_assignments['teams'][str(team2_id)]}",
            ],
        }

    if mode == TournamentGameMode.all_same:
        hero_name = random.choice(HERO_NAMES)
        player_assignments = {
            str(participant["discord_id"]): hero_name
            for participant in participants
            if participant.get("discord_id")
        }
        return {
            "convars": {"citadel_allow_duplicate_heroes": 1},
            "hero_assignments": {
                "mode": mode.value,
                "all": hero_name,
                "players": player_assignments,
            },
            "announcement_lines": [f"Alle Spieler: {hero_name}"],
        }

    if mode == TournamentGameMode.random_heroes:
        unique_player_ids = [
            str(participant["discord_id"])
            for participant in participants
            if participant.get("discord_id")
        ]
        allow_duplicates = len(unique_player_ids) > len(HERO_NAMES)
        if allow_duplicates:
            selected_heroes = [random.choice(HERO_NAMES) for _ in unique_player_ids]
        else:
            selected_heroes = _pick_unique_heroes(len(unique_player_ids))

        player_assignments = {
            discord_id: selected_heroes[index]
            for index, discord_id in enumerate(unique_player_ids)
        }
        announcement_lines = [
            f"{_display_name(participant)}: {player_assignments[str(participant['discord_id'])]}"
            for participant in participants
            if participant.get("discord_id")
        ]
        convars: dict[str, Any] = {}
        if allow_duplicates:
            convars["citadel_allow_duplicate_heroes"] = 1
        return {
            "convars": convars,
            "hero_assignments": {"mode": mode.value, "players": player_assignments},
            "announcement_lines": announcement_lines,
        }

    # TODO: Kein belastbar dokumentierter/sicherer Single-Lane-ConVar-Pfad für den Bot-Flow.
    # Community-Hinweise nennen u.a. `citadel_active_lane`, der Modus wirkt aktuell aber nicht robust genug.
    return {
        "convars": {},
        "hero_assignments": {},
        "announcement_lines": [
            "Single-Lane-Battle ist aktuell in Vorbereitung; es werden noch keine sicheren Citadel-ConVars gesetzt."
        ],
    }
