"""Tournament Engine — Team-Zuweisung, Status-Management und Business-Logik."""
from __future__ import annotations

import json
import math
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


# ---------------------------------------------------------------------------
# Gruppenphase
# ---------------------------------------------------------------------------


async def generate_groups(tournament_id: int, num_groups: int = 4) -> list[int]:
    """Generiert Gruppen fuer ein Turnier mit Seed-basierter Verteilung.

    Snake-Draft Seeding: Teams nach Rank-Score sortiert, dann im Schlangen-Muster verteilt.
    z.B. bei 4 Gruppen und 16 Teams:
    Runde 1: A1, B2, C3, D4
    Runde 2: D5, C6, B7, A8
    Runde 3: A9, B10, C11, D12
    etc.

    Returns: Liste der erstellten Group IDs.
    """
    async with get_db() as db:
        # Turnier pruefen
        cursor = await db.execute("SELECT * FROM tournaments WHERE id = ?", (tournament_id,))
        tournament = await cursor.fetchone()
        if not tournament:
            raise ValueError("Turnier nicht gefunden")

        # Alle Teams mit ihrem Durchschnitts-Score laden
        cursor = await db.execute("SELECT * FROM teams WHERE tournament_id = ?", (tournament_id,))
        teams = await cursor.fetchall()

        if len(teams) < 2:
            raise ValueError("Mindestens 2 Teams benoetigt")

        # Teams nach Average Rank-Score sortieren (hoechster zuerst)
        team_scores = []
        for t in teams:
            cursor = await db.execute("SELECT rank_score FROM team_members WHERE team_id = ?", (t["id"],))
            members = await cursor.fetchall()
            avg = sum(m["rank_score"] for m in members) / max(1, len(members))
            team_scores.append({"id": t["id"], "avg_score": avg})

        team_scores.sort(key=lambda x: x["avg_score"], reverse=True)

        # Gruppen-Anzahl anpassen (max = Teams/2, min = 2)
        actual_groups = min(num_groups, len(team_scores) // 2)
        actual_groups = max(2, actual_groups)

        # Bestehende Gruppen loeschen (falls regeneriert)
        cursor = await db.execute("SELECT id FROM groups WHERE tournament_id = ?", (tournament_id,))
        old_groups = await cursor.fetchall()
        for g in old_groups:
            await db.execute("DELETE FROM group_matches WHERE group_id = ?", (g["id"],))
            await db.execute("DELETE FROM group_teams WHERE group_id = ?", (g["id"],))
        await db.execute("DELETE FROM groups WHERE tournament_id = ?", (tournament_id,))

        # Gruppen erstellen (A, B, C, D...)
        group_names = [chr(65 + i) for i in range(actual_groups)]  # A, B, C, D
        group_ids = []
        for idx, name in enumerate(group_names):
            cursor = await db.execute(
                "INSERT INTO groups (tournament_id, name, seeding_order) VALUES (?, ?, ?)",
                (tournament_id, f"Gruppe {name}", idx),
            )
            group_ids.append(cursor.lastrowid)

        # Snake-Draft Verteilung
        for i, ts in enumerate(team_scores):
            round_num = i // actual_groups
            if round_num % 2 == 0:
                group_idx = i % actual_groups
            else:
                group_idx = actual_groups - 1 - (i % actual_groups)

            await db.execute(
                "INSERT INTO group_teams (group_id, team_id) VALUES (?, ?)",
                (group_ids[group_idx], ts["id"]),
            )

        await db.commit()

    return group_ids


async def generate_group_matches(tournament_id: int) -> int:
    """Generiert Round-Robin Matches fuer alle Gruppen eines Turniers.

    Jedes Team spielt einmal gegen jedes andere Team in seiner Gruppe.
    Returns: Anzahl generierter Matches.
    """
    match_count = 0
    async with get_db() as db:
        cursor = await db.execute("SELECT id FROM groups WHERE tournament_id = ?", (tournament_id,))
        groups = await cursor.fetchall()

        for g in groups:
            group_id = g["id"]

            # Bestehende Matches loeschen
            await db.execute("DELETE FROM group_matches WHERE group_id = ?", (group_id,))

            # Teams in dieser Gruppe
            cursor = await db.execute("SELECT team_id FROM group_teams WHERE group_id = ?", (group_id,))
            team_ids = [r["team_id"] for r in await cursor.fetchall()]

            # Round-Robin: jedes Paar einmal
            for i in range(len(team_ids)):
                for j in range(i + 1, len(team_ids)):
                    await db.execute(
                        "INSERT INTO group_matches (group_id, team1_id, team2_id) VALUES (?, ?, ?)",
                        (group_id, team_ids[i], team_ids[j]),
                    )
                    match_count += 1

        await db.commit()

    return match_count


# ---------------------------------------------------------------------------
# Bracket-Generierung
# ---------------------------------------------------------------------------


async def generate_bracket(tournament_id: int, bracket_format: str = "single_elimination") -> int:
    """Generiert einen Elimination-Bracket aus den Gruppen-Ergebnissen.

    Seeding: Gruppensieger + Zweitplatzierte, sortiert nach Punkten.
    Bracket: Macht-2 Aufstockung mit BYEs. Hoechster Seed trifft niedrigsten.

    Returns: Anzahl generierter Bracket-Matches.
    """
    async with get_db() as db:
        # Bestehende Bracket-Matches loeschen
        await db.execute("DELETE FROM bracket_matches WHERE tournament_id = ?", (tournament_id,))

        # Gruppen-Standings laden (Top 2 pro Gruppe)
        cursor = await db.execute(
            "SELECT id FROM groups WHERE tournament_id = ? ORDER BY seeding_order",
            (tournament_id,),
        )
        groups = await cursor.fetchall()

        qualified_teams: list[dict] = []
        for g in groups:
            cursor = await db.execute(
                "SELECT team_id, wins, losses, points FROM group_teams "
                "WHERE group_id = ? ORDER BY points DESC, wins DESC",
                (g["id"],),
            )
            standings = await cursor.fetchall()
            # Top 2 pro Gruppe (oder weniger wenn Gruppe kleiner)
            for rank_pos, s in enumerate(standings[:2]):
                qualified_teams.append({
                    "team_id": s["team_id"],
                    "points": s["points"],
                    "wins": s["wins"],
                    "seed": rank_pos,  # 0 = Gruppensieger, 1 = Zweiter
                })

        if not qualified_teams:
            # Fallback: Alle Teams direkt ins Bracket (wenn keine Gruppenphase)
            cursor = await db.execute(
                "SELECT id FROM teams WHERE tournament_id = ?",
                (tournament_id,),
            )
            all_teams = await cursor.fetchall()
            qualified_teams = [
                {"team_id": t["id"], "points": 0, "wins": 0, "seed": i}
                for i, t in enumerate(all_teams)
            ]

        if len(qualified_teams) < 2:
            raise ValueError("Mindestens 2 Teams fuer Bracket benoetigt")

        # Sortieren: Gruppensieger zuerst, dann nach Punkten
        qualified_teams.sort(key=lambda x: (-x["points"], -x["wins"], x["seed"]))

        # Bracket-Groesse auf naechste Zweierpotenz aufstocken
        num_teams = len(qualified_teams)
        bracket_size = 1
        while bracket_size < num_teams:
            bracket_size *= 2

        num_rounds = int(math.log2(bracket_size))

        # Erste Runde generieren
        match_count = 0
        first_round_matches = bracket_size // 2

        for pos in range(first_round_matches):
            # Standard Seeding: Position 0 = Seed 1 vs Seed N, etc.
            seed_a = pos
            seed_b = bracket_size - 1 - pos

            team1_id = qualified_teams[seed_a]["team_id"] if seed_a < num_teams else None
            team2_id = qualified_teams[seed_b]["team_id"] if seed_b < num_teams else None

            # Wenn einer None ist: BYE (Gewinner steht fest)
            winner_id = None
            match_status = "pending"
            if team1_id is None and team2_id is not None:
                winner_id = team2_id
                match_status = "completed"
            elif team2_id is None and team1_id is not None:
                winner_id = team1_id
                match_status = "completed"

            await db.execute(
                "INSERT INTO bracket_matches "
                "(tournament_id, round, position, bracket_type, team1_id, team2_id, winner_id, status) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (tournament_id, 1, pos, "winners", team1_id, team2_id, winner_id, match_status),
            )
            match_count += 1

        # Weitere Runden generieren (noch ohne Teams — werden durch Ergebnisse gefuellt)
        matches_in_round = first_round_matches // 2
        for round_num in range(2, num_rounds + 1):
            for pos in range(matches_in_round):
                await db.execute(
                    "INSERT INTO bracket_matches "
                    "(tournament_id, round, position, bracket_type, status) "
                    "VALUES (?, ?, ?, ?, 'pending')",
                    (tournament_id, round_num, pos, "winners"),
                )
                match_count += 1
            matches_in_round = max(1, matches_in_round // 2)

        # BYE-Gewinner in naechste Runde propagieren
        await _propagate_byes(db, tournament_id)

        await db.commit()

    return match_count


async def _propagate_byes(db, tournament_id: int) -> None:  # noqa: ANN001
    """Propagiert BYE-Gewinner automatisch in die naechste Runde."""
    cursor = await db.execute(
        "SELECT * FROM bracket_matches "
        "WHERE tournament_id = ? AND status = 'completed' AND winner_id IS NOT NULL "
        "ORDER BY round, position",
        (tournament_id,),
    )
    completed = await cursor.fetchall()

    for match in completed:
        round_num = match["round"]
        position = match["position"]
        winner_id = match["winner_id"]

        # Naechste Runde: position // 2
        next_round = round_num + 1
        next_position = position // 2

        # Pruefen ob naechste Runde existiert
        cursor = await db.execute(
            "SELECT * FROM bracket_matches WHERE tournament_id = ? AND round = ? AND position = ?",
            (tournament_id, next_round, next_position),
        )
        next_match = await cursor.fetchone()
        if not next_match:
            continue

        # In team1 oder team2 einsetzen (gerade Position = team1, ungerade = team2)
        if position % 2 == 0:
            await db.execute(
                "UPDATE bracket_matches SET team1_id = ? WHERE id = ?",
                (winner_id, next_match["id"]),
            )
        else:
            await db.execute(
                "UPDATE bracket_matches SET team2_id = ? WHERE id = ?",
                (winner_id, next_match["id"]),
            )


async def advance_bracket_winner(tournament_id: int, match_id: int, winner_id: int) -> None:
    """Nach einem Bracket-Match: Gewinner in die naechste Runde setzen."""
    async with get_db() as db:
        cursor = await db.execute("SELECT * FROM bracket_matches WHERE id = ?", (match_id,))
        match = await cursor.fetchone()
        if not match:
            return

        round_num = match["round"]
        position = match["position"]
        next_round = round_num + 1
        next_position = position // 2

        cursor = await db.execute(
            "SELECT * FROM bracket_matches WHERE tournament_id = ? AND round = ? AND position = ?",
            (tournament_id, next_round, next_position),
        )
        next_match = await cursor.fetchone()
        if not next_match:
            return  # Finale gewonnen

        if position % 2 == 0:
            await db.execute(
                "UPDATE bracket_matches SET team1_id = ? WHERE id = ?",
                (winner_id, next_match["id"]),
            )
        else:
            await db.execute(
                "UPDATE bracket_matches SET team2_id = ? WHERE id = ?",
                (winner_id, next_match["id"]),
            )

        await db.commit()
