"""Tournament Engine — Team-Zuweisung, Status-Management und Business-Logik."""
from __future__ import annotations

import json
import random

from db import get_db
from steam.reader import get_steam_link
from tournament.seeding import rank_score as calc_rank_score

TEAM_NAMES = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel",
    "India", "Juliet", "Kilo", "Lima", "Mike", "November", "Oscar", "Papa",
]

VALID_STATUS_TRANSITIONS: dict[str, list[str]] = {
    "draft": ["registration"],
    "registration": ["group_phase"],
    "group_phase": ["bracket"],
    "bracket": ["completed"],
    "completed": ["archived"],
}


def get_valid_status_transitions() -> dict[str, list[str]]:
    """Gibt die erlaubten Status-Uebergaenge zurueck."""
    return VALID_STATUS_TRANSITIONS


async def assign_random_teams(tournament_id: int, team_size: int) -> int:
    """Verteilt Solo-Anmeldungen (ohne team_id) zufaellig auf neue Teams.

    Erstellt Teams mit generierten Namen (Team Alpha, Team Bravo, etc.).
    Returns: Anzahl erstellter Teams.
    """
    async with get_db() as db:
        # Solo-Anmeldungen laden (noch keinem Team zugewiesen)
        cursor = await db.execute(
            "SELECT id, discord_id, steam_id, rank, rank_score "
            "FROM tournament_signups "
            "WHERE tournament_id = ? AND team_id IS NULL",
            (tournament_id,),
        )
        signups = await cursor.fetchall()

        if not signups:
            return 0

        # Zufaellig mischen
        signup_list = list(signups)
        random.shuffle(signup_list)

        # Bereits existierende Team-Namen ermitteln
        cursor = await db.execute(
            "SELECT name_key FROM teams WHERE tournament_id = ?",
            (tournament_id,),
        )
        existing_keys = {row["name_key"] for row in await cursor.fetchall()}

        # Verfuegbare Team-Namen filtern
        available_names = [
            n for n in TEAM_NAMES if f"team {n.lower()}" not in existing_keys
        ]

        # Teams erstellen und Spieler zuweisen
        teams_created = 0
        for chunk_idx in range(0, len(signup_list), team_size):
            chunk = signup_list[chunk_idx : chunk_idx + team_size]

            # Team-Name generieren
            if chunk_idx // team_size < len(available_names):
                team_name = f"Team {available_names[chunk_idx // team_size]}"
            else:
                team_name = f"Team {teams_created + len(existing_keys) + 1}"

            name_key = team_name.casefold()
            captain_discord_id = chunk[0]["discord_id"]

            # Team erstellen
            cursor = await db.execute(
                "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) "
                "VALUES (?, ?, ?, ?)",
                (tournament_id, team_name, name_key, captain_discord_id),
            )
            team_id = cursor.lastrowid

            # Mitglieder hinzufuegen
            for i, signup in enumerate(chunk):
                role = "captain" if i == 0 else "member"

                # Steam-Link laden fuer Rank-Score
                steam_data = await get_steam_link(signup["discord_id"])
                steam_id = steam_data["steam_id"] if steam_data else signup["steam_id"]
                rank = steam_data["rank"] if steam_data else signup["rank"]
                score = steam_data["rank_score"] if steam_data else (signup["rank_score"] or 0)

                await db.execute(
                    "INSERT INTO team_members (team_id, discord_id, steam_id, rank, rank_score, role) "
                    "VALUES (?, ?, ?, ?, ?, ?)",
                    (team_id, signup["discord_id"], steam_id, rank, score, role),
                )

                # Signup mit Team verknuepfen
                await db.execute(
                    "UPDATE tournament_signups SET team_id = ? WHERE id = ?",
                    (team_id, signup["id"]),
                )

            teams_created += 1

        await db.commit()

        # Audit-Log
        await _audit(
            db,
            "assign_random_teams",
            None,
            json.dumps({
                "tournament_id": tournament_id,
                "teams_created": teams_created,
                "signups_assigned": len(signup_list),
            }),
        )

    return teams_created


async def _audit(db, action: str, user_id: str | None, details: str) -> None:  # noqa: ANN001
    """Schreibt einen Eintrag in den Audit-Log."""
    await db.execute(
        "INSERT INTO audit_log (action, user_id, details) VALUES (?, ?, ?)",
        (action, user_id, details),
    )
    await db.commit()
