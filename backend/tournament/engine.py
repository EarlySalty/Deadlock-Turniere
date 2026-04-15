"""Tournament Engine — Team-Zuweisung, Status-Management und Business-Logik."""
from __future__ import annotations

from collections import deque
import hashlib
import json
import math
import random

from db import get_db
from rank_reader import get_player_rank_profile
from tournament.seeding import rank_score as calc_rank_score
from tournament.models import TournamentMode

TEAM_NAMES = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel",
    "India", "Juliet", "Kilo", "Lima", "Mike", "November", "Oscar", "Papa",
]

VALID_STATUS_TRANSITIONS: dict[str, list[str]] = {
    "draft": ["registration"],
    "registration": ["checkin"],
    "checkin": ["group_phase"],
    "group_phase": ["bracket"],
    "bracket": ["completed"],
    "completed": ["archived"],
}

# Auto Tournament Mode: >= 12 Teams = Group Stage, < 12 Teams = Bracket Only
AUTO_GROUP_STAGE_THRESHOLD = 12


def determine_tournament_mode(
    team_count: int,
    force_mode: TournamentMode | None = None,
) -> TournamentMode:
    """Bestimmt automatisch den Turnier-Modus basierend auf Team-Anzahl.

    Args:
        team_count: Anzahl der Teams im Turnier
        force_mode: Admin-Override (ignoriert team_count)

    Returns:
        TournamentMode: group_stage oder bracket_only
    """
    if force_mode is not None:
        return force_mode

    if team_count >= AUTO_GROUP_STAGE_THRESHOLD:
        return TournamentMode.group_stage
    return TournamentMode.bracket_only


def get_valid_status_transitions() -> dict[str, list[str]]:
    """Gibt die erlaubten Status-Übergänge zurück."""
    return VALID_STATUS_TRANSITIONS


class CheckinSnapshotMismatchError(RuntimeError):
    """Der bestätigte Check-in basiert auf einem veralteten Dry-Run."""


async def _existing_team_name_keys(db, tournament_id: int) -> set[str]:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT name_key FROM teams WHERE tournament_id = ?",
        (tournament_id,),
    )
    return {row["name_key"] for row in await cursor.fetchall()}


def _available_team_names(existing_keys: set[str]) -> list[str]:
    return [name for name in TEAM_NAMES if f"team {name.lower()}" not in existing_keys]


def _next_team_name(existing_keys: set[str], ordinal_hint: int = 1) -> str:
    available_names = _available_team_names(existing_keys)
    if available_names:
        team_name = f"Team {available_names[0]}"
    else:
        suffix = max(ordinal_hint, len(existing_keys) + 1)
        team_name = f"Team {suffix}"
        while team_name.casefold() in existing_keys:
            suffix += 1
            team_name = f"Team {suffix}"

    existing_keys.add(team_name.casefold())
    return team_name


async def assign_random_teams(tournament_id: int, team_size: int) -> int:
    """Verteilt Solo-Anmeldungen (ohne team_id) zufällig auf neue Teams.

    Erstellt Teams mit generierten Namen (Team Alpha, Team Bravo, etc.).
    Returns: Anzahl erstellter Teams.
    """
    async with get_db() as db:
        # Solo-Anmeldungen laden (noch keinem Team zugewiesen)
        cursor = await db.execute(
            "SELECT id, discord_id, discord_name, steam_id, rank, rank_score "
            "FROM tournament_signups "
            "WHERE tournament_id = ? AND team_id IS NULL",
            (tournament_id,),
        )
        signups = await cursor.fetchall()

        if not signups:
            return 0

        # Zufällig mischen
        signup_list = list(signups)
        random.shuffle(signup_list)

        # Bereits existierende Team-Namen ermitteln
        cursor = await db.execute(
            "SELECT name_key FROM teams WHERE tournament_id = ?",
            (tournament_id,),
        )
        existing_keys = {row["name_key"] for row in await cursor.fetchall()}

        # Verfügbare Team-Namen filtern
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

            # Mitglieder hinzufügen
            for i, signup in enumerate(chunk):
                role = "captain" if i == 0 else "member"

                # Steam-Link laden für Rank-Score
                rank_data = await get_player_rank_profile(signup["discord_id"])
                steam_id = rank_data.get("steam_id") if rank_data else signup["steam_id"]
                rank = rank_data.get("rank") if rank_data else signup["rank"]
                score = rank_data.get("rank_score", signup["rank_score"] or 0) if rank_data else (signup["rank_score"] or 0)

                await db.execute(
                    "INSERT INTO team_members (team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (
                        team_id,
                        signup["discord_id"],
                        signup["discord_name"],
                        steam_id,
                        rank,
                        score,
                        role,
                    ),
                )

                # Signup mit Team verknüpfen
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


async def _sync_team_captain(db, team_id: int) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT discord_id FROM team_members WHERE team_id = ? ORDER BY joined_at, id",
        (team_id,),
    )
    members = await cursor.fetchall()
    captain_id = members[0]["discord_id"] if members else ""

    await db.execute(
        "UPDATE team_members SET role = 'member' WHERE team_id = ?",
        (team_id,),
    )
    if captain_id:
        await db.execute(
            "UPDATE team_members SET role = 'captain' WHERE team_id = ? AND discord_id = ?",
            (team_id, captain_id),
        )
    await db.execute(
        "UPDATE teams SET captain_discord_id = ? WHERE id = ?",
        (captain_id, team_id),
    )


async def _build_checkin_snapshot_token(
    db,
    tournament_id: int,
    tournament_status: str,
) -> str:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT discord_id, checked_in_at FROM tournament_checkins "
        "WHERE tournament_id = ? ORDER BY checked_in_at, id",
        (tournament_id,),
    )
    checkins = [dict(row) for row in await cursor.fetchall()]

    cursor = await db.execute(
        "SELECT tm.team_id, tm.discord_id, tm.joined_at, tm.role, t.name, t.created_at "
        "FROM team_members tm "
        "JOIN teams t ON t.id = tm.team_id "
        "WHERE t.tournament_id = ? "
        "ORDER BY t.created_at, t.id, tm.joined_at, tm.id",
        (tournament_id,),
    )
    members = [dict(row) for row in await cursor.fetchall()]

    cursor = await db.execute(
        "SELECT id, discord_id, team_id, signed_up_at "
        "FROM tournament_signups WHERE tournament_id = ? "
        "ORDER BY signed_up_at, id",
        (tournament_id,),
    )
    signups = [dict(row) for row in await cursor.fetchall()]

    payload = {
        "status": tournament_status,
        "checkins": checkins,
        "members": members,
        "signups": signups,
    }
    serialized = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(serialized.encode("utf-8")).hexdigest()


async def finalize_checkin(
    tournament_id: int,
    *,
    confirm: bool = False,
    allowed_team_ids: set[int] | None = None,
    actor_id: str | None = None,
    expected_snapshot_token: str | None = None,
    advance_to_group_phase: bool = False,
) -> dict:
    """Bereinigt Teams auf Basis der Turnier-Check-ins."""
    allowed_team_ids = allowed_team_ids or set()

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        tournament = await cursor.fetchone()
        if not tournament:
            raise ValueError("Turnier nicht gefunden")
        if tournament["status"] != "checkin":
            raise ValueError("Check-in kann nur in der Check-in-Phase abgeschlossen werden")
        snapshot_token = await _build_checkin_snapshot_token(
            db,
            tournament_id,
            tournament["status"],
        )
        if confirm:
            if not expected_snapshot_token:
                raise ValueError("snapshot_token ist für confirm erforderlich")
            if expected_snapshot_token != snapshot_token:
                raise CheckinSnapshotMismatchError(
                    "Check-in-Daten haben sich seit der Vorschau geändert. Bitte Dry-Run erneut ausführen."
                )

        team_size = int(tournament["team_size"])

        cursor = await db.execute(
            "SELECT discord_id, checked_in_at FROM tournament_checkins "
            "WHERE tournament_id = ? ORDER BY checked_in_at, id",
            (tournament_id,),
        )
        checkin_rows = await cursor.fetchall()
        checked_in_ids = {row["discord_id"] for row in checkin_rows}
        checkin_order = {
            row["discord_id"]: index
            for index, row in enumerate(checkin_rows)
        }

        cursor = await db.execute(
            "SELECT id, name, name_key, captain_discord_id, created_at "
            "FROM teams WHERE tournament_id = ? ORDER BY created_at, id",
            (tournament_id,),
        )
        team_rows = await cursor.fetchall()
        teams_by_id = {row["id"]: dict(row) for row in team_rows}

        cursor = await db.execute(
            "SELECT tm.id, tm.team_id, tm.discord_id, tm.discord_name, tm.steam_id, "
            "tm.rank, tm.rank_score, tm.role, tm.joined_at, t.name AS team_name "
            "FROM team_members tm "
            "JOIN teams t ON t.id = tm.team_id "
            "WHERE t.tournament_id = ? "
            "ORDER BY t.created_at, t.id, tm.joined_at, tm.id",
            (tournament_id,),
        )
        member_rows = await cursor.fetchall()

        members_by_team: dict[int, list[dict]] = {}
        for row in member_rows:
            members_by_team.setdefault(row["team_id"], []).append(dict(row))

        cursor = await db.execute(
            "SELECT id, tournament_id, discord_id, discord_name, steam_id, rank, "
            "rank_score, team_id, signed_up_at "
            "FROM tournament_signups WHERE tournament_id = ? "
            "ORDER BY signed_up_at, id",
            (tournament_id,),
        )
        signup_rows = await cursor.fetchall()
        signups_by_discord = {row["discord_id"]: dict(row) for row in signup_rows}

        team_states: dict[int, list[dict]] = {
            team_id: list(members_by_team.get(team_id, []))
            for team_id in teams_by_id
        }
        removed_members: list[dict] = []
        for team_id, members in list(team_states.items()):
            kept_members: list[dict] = []
            for member in members:
                if member["discord_id"] in checked_in_ids:
                    kept_members.append(member)
                    continue
                removed_members.append(
                    {
                        "team_id": team_id,
                        "team_name": teams_by_id[team_id]["name"],
                        "discord_id": member["discord_id"],
                        "discord_name": member["discord_name"],
                    }
                )
            team_states[team_id] = kept_members

        pool_entries = []
        for signup in signups_by_discord.values():
            if signup["team_id"] is not None:
                continue
            if signup["discord_id"] not in checked_in_ids:
                continue
            pool_entries.append(signup)
        pool_entries.sort(
            key=lambda signup: (
                checkin_order.get(signup["discord_id"], math.inf),
                signup["signed_up_at"],
                signup["id"],
            )
        )
        pool = deque(pool_entries)

        added_players: list[dict] = []
        created_teams: list[dict] = []
        affected_team_ids: set[int] = set()

        for team_id in teams_by_id:
            members = team_states[team_id]
            while len(members) < team_size and pool:
                signup = pool.popleft()
                member_data = {
                    "team_id": team_id,
                    "discord_id": signup["discord_id"],
                    "discord_name": signup["discord_name"],
                    "steam_id": signup["steam_id"],
                    "rank": signup["rank"],
                    "rank_score": signup["rank_score"] or 0,
                    "role": "member",
                }
                members.append(member_data)
                affected_team_ids.add(team_id)
                added_players.append(
                    {
                        "team_id": team_id,
                        "team_name": teams_by_id[team_id]["name"],
                        "discord_id": signup["discord_id"],
                        "discord_name": signup["discord_name"],
                        "source": "solo_pool",
                    }
                )

        existing_keys = await _existing_team_name_keys(db, tournament_id)
        ordinal_hint = len(existing_keys) + 1
        while len(pool) >= team_size:
            team_name = _next_team_name(existing_keys, ordinal_hint)
            ordinal_hint += 1
            chunk = [pool.popleft() for _ in range(team_size)]
            created_teams.append(
                {
                    "name": team_name,
                    "name_key": team_name.casefold(),
                    "captain_discord_id": chunk[0]["discord_id"],
                    "members": chunk,
                }
            )
            for signup in chunk:
                added_players.append(
                    {
                        "team_id": None,
                        "team_name": team_name,
                        "discord_id": signup["discord_id"],
                        "discord_name": signup["discord_name"],
                        "source": "new_team",
                    }
                )

        warnings = []
        deleted_team_ids: list[int] = []
        for team_id, members in team_states.items():
            member_count = len(members)
            if member_count == 0:
                deleted_team_ids.append(team_id)
                continue
            if member_count < team_size:
                warnings.append(
                    {
                        "team_id": team_id,
                        "team_name": teams_by_id[team_id]["name"],
                        "current": member_count,
                        "required": team_size,
                    }
                )

        missing_allowed = [
            warning
            for warning in warnings
            if warning["team_id"] not in allowed_team_ids
        ]
        if confirm and missing_allowed:
            team_labels = ", ".join(
                f"{warning['team_name']} ({warning['current']}/{warning['required']})"
                for warning in missing_allowed
            )
            raise ValueError(
                f"Unvollständige Teams müssen bestätigt werden: {team_labels}"
            )

        if confirm:
            for removed in removed_members:
                await db.execute(
                    "DELETE FROM team_members WHERE team_id = ? AND discord_id = ?",
                    (removed["team_id"], removed["discord_id"]),
                )
                await db.execute(
                    "UPDATE tournament_signups SET team_id = NULL "
                    "WHERE tournament_id = ? AND discord_id = ?",
                    (tournament_id, removed["discord_id"]),
                )
                affected_team_ids.add(removed["team_id"])

            for team_id in deleted_team_ids:
                await db.execute(
                    "UPDATE tournament_signups SET team_id = NULL "
                    "WHERE tournament_id = ? AND team_id = ?",
                    (tournament_id, team_id),
                )
                await db.execute("DELETE FROM team_members WHERE team_id = ?", (team_id,))
                await db.execute("DELETE FROM teams WHERE id = ?", (team_id,))
                affected_team_ids.discard(team_id)

            for added in added_players:
                if added["source"] != "solo_pool":
                    continue
                team_id = added["team_id"]
                if team_id is None:
                    continue
                signup = signups_by_discord.get(added["discord_id"])
                if not signup:
                    continue
                await db.execute(
                    "INSERT INTO team_members "
                    "(team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
                    "VALUES (?, ?, ?, ?, ?, ?, 'member')",
                    (
                        team_id,
                        signup["discord_id"],
                        signup["discord_name"],
                        signup["steam_id"],
                        signup["rank"],
                        signup["rank_score"] or 0,
                    ),
                )
                await db.execute(
                    "UPDATE tournament_signups SET team_id = ? WHERE id = ?",
                    (team_id, signup["id"]),
                )

            for created_team in created_teams:
                cursor = await db.execute(
                    "INSERT INTO teams (tournament_id, name, name_key, captain_discord_id) "
                    "VALUES (?, ?, ?, ?)",
                    (
                        tournament_id,
                        created_team["name"],
                        created_team["name_key"],
                        created_team["captain_discord_id"],
                    ),
                )
                new_team_id = int(cursor.lastrowid)
                created_team["team_id"] = new_team_id
                for index, signup in enumerate(created_team["members"]):
                    role = "captain" if index == 0 else "member"
                    await db.execute(
                        "INSERT INTO team_members "
                        "(team_id, discord_id, discord_name, steam_id, rank, rank_score, role) "
                        "VALUES (?, ?, ?, ?, ?, ?, ?)",
                        (
                            new_team_id,
                            signup["discord_id"],
                            signup["discord_name"],
                            signup["steam_id"],
                            signup["rank"],
                            signup["rank_score"] or 0,
                            role,
                        ),
                    )
                    await db.execute(
                        "UPDATE tournament_signups SET team_id = ? WHERE id = ?",
                        (new_team_id, signup["id"]),
                    )

                for added in added_players:
                    if (
                        added["source"] == "new_team"
                        and added["team_name"] == created_team["name"]
                    ):
                        added["team_id"] = new_team_id

            for team_id in sorted(affected_team_ids):
                await _sync_team_captain(db, team_id)

            groups_created = 0
            matches_created = 0
            if advance_to_group_phase:
                group_ids = await _generate_groups_in_db(db, tournament_id)
                groups_created = len(group_ids)
                matches_created = await _generate_group_matches_in_db(db, tournament_id)
                cursor = await db.execute(
                    "UPDATE tournaments SET status = 'group_phase', updated_at = datetime('now') "
                    "WHERE id = ? AND status = 'checkin'",
                    (tournament_id,),
                )
                if cursor.rowcount == 0:
                    raise RuntimeError("Turnierstatus wurde parallel geändert")

            await _audit(
                db,
                "finalize_checkin",
                actor_id,
                json.dumps(
                    {
                        "tournament_id": tournament_id,
                        "removed_players": len(removed_members),
                        "added_players": len(added_players),
                        "warnings": warnings,
                        "deleted_team_ids": deleted_team_ids,
                        "created_teams": [team["name"] for team in created_teams],
                        "advanced_to_group_phase": advance_to_group_phase,
                        "groups_created": groups_created,
                        "matches_created": matches_created,
                    }
                ),
            )

        result = {
            "warnings": warnings,
            "removed_players": removed_members,
            "added_players": added_players,
            "created_teams": [
                {"team_id": team.get("team_id"), "team_name": team["name"]}
                if "team_id" in team
                else {"team_id": None, "team_name": team["name"]}
                for team in created_teams
            ],
            "deleted_team_ids": deleted_team_ids,
            "remaining_solo_players": [
                {
                    "discord_id": signup["discord_id"],
                    "discord_name": signup["discord_name"],
                }
                for signup in pool
            ],
            "dry_run": not confirm,
            "snapshot_token": snapshot_token,
        }
        if confirm and advance_to_group_phase:
            result["groups_created"] = groups_created
            result["matches_created"] = matches_created
            result["advanced_to_group_phase"] = True
        return result


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


async def _generate_groups_in_db(
    db,
    tournament_id: int,
    num_groups: int = 4,
) -> list[int]:  # noqa: ANN001
    # Turnier prüfen
    cursor = await db.execute("SELECT * FROM tournaments WHERE id = ?", (tournament_id,))
    tournament = await cursor.fetchone()
    if not tournament:
        raise ValueError("Turnier nicht gefunden")

    # Alle Teams mit ihrem Durchschnitts-Score laden
    cursor = await db.execute("SELECT * FROM teams WHERE tournament_id = ?", (tournament_id,))
    teams = await cursor.fetchall()

    if len(teams) < 2:
        raise ValueError("Mindestens 2 Teams benötigt")

    # Teams nach Average Rank-Score sortieren (höchster zuerst)
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

    # Bestehende Gruppen löschen (falls regeneriert)
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

    return group_ids


async def generate_groups(tournament_id: int, num_groups: int = 4) -> list[int]:
    """Generiert Gruppen für ein Turnier mit Seed-basierter Verteilung.

    Snake-Draft Seeding: Teams nach Rank-Score sortiert, dann im Schlangen-Muster verteilt.
    z.B. bei 4 Gruppen und 16 Teams:
    Runde 1: A1, B2, C3, D4
    Runde 2: D5, C6, B7, A8
    Runde 3: A9, B10, C11, D12
    etc.

    Returns: Liste der erstellten Group IDs.
    """
    async with get_db() as db:
        group_ids = await _generate_groups_in_db(db, tournament_id, num_groups)
        await db.commit()

    return group_ids


async def _generate_group_matches_in_db(db, tournament_id: int) -> int:  # noqa: ANN001
    match_count = 0
    cursor = await db.execute("SELECT id FROM groups WHERE tournament_id = ?", (tournament_id,))
    groups = await cursor.fetchall()

    for g in groups:
        group_id = g["id"]

        # Bestehende Matches löschen
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

    return match_count


async def generate_group_matches(tournament_id: int) -> int:
    """Generiert Round-Robin Matches für alle Gruppen eines Turniers.

    Jedes Team spielt einmal gegen jedes andere Team in seiner Gruppe.
    Returns: Anzahl generierter Matches.
    """
    async with get_db() as db:
        match_count = await _generate_group_matches_in_db(db, tournament_id)
        await db.commit()

    return match_count


# ---------------------------------------------------------------------------
# Bracket-Generierung
# ---------------------------------------------------------------------------


async def generate_bracket(tournament_id: int, bracket_format: str = "single_elimination") -> int:
    """Generiert einen Elimination-Bracket aus den Gruppen-Ergebnissen.

    Seeding: Gruppensieger + Zweitplatzierte, sortiert nach Punkten.
    Nicht-Potenzen-von-zwei werden über eine Play-in-Runde der niedrigeren Seeds
    auf die nächste faire Hauptgröße reduziert. Dadurch fällt kein Team aus dem Raster.

    Returns: Anzahl generierter Bracket-Matches.
    """
    async with get_db() as db:
        # Bestehende Bracket-Matches löschen
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
            raise ValueError("Mindestens 2 Teams für Bracket benötigt")

        # Sortieren: Gruppensieger zuerst, dann nach Punkten
        qualified_teams.sort(key=lambda x: (-x["points"], -x["wins"], x["seed"]))
        seeded_entries = [{"team_id": team["team_id"]} for team in qualified_teams]
        match_count = await _build_seeded_bracket(db, tournament_id, seeded_entries)

        await db.commit()

    return match_count


async def _build_seeded_bracket(
    db,
    tournament_id: int,
    seeded_entries: list[dict],
) -> int:  # noqa: ANN001
    num_teams = len(seeded_entries)
    if num_teams < 2:
        raise ValueError("Mindestens 2 Teams für Bracket benötigt")

    match_count = 0
    current_round = 1

    if _is_power_of_two(num_teams):
        current_entries = seeded_entries
    else:
        main_size = _highest_power_of_two_below(num_teams)
        play_in_matches = num_teams - main_size
        bye_entries = seeded_entries[: 2 * main_size - num_teams]
        play_in_pool = seeded_entries[len(bye_entries) :]

        play_in_entries: list[dict] = []
        for idx in range(play_in_matches):
            left_entry = play_in_pool[idx]
            right_entry = play_in_pool[-1 - idx]
            match_id = await _insert_bracket_match(
                db,
                tournament_id,
                round_num=current_round,
                position=idx,
                team1_id=left_entry["team_id"],
                team2_id=right_entry["team_id"],
            )
            play_in_entries.append({"source_match_id": match_id})
            match_count += 1

        current_entries = bye_entries + play_in_entries
        current_round += 1

    while len(current_entries) >= 2:
        seed_order = _seed_slot_order(len(current_entries))
        slot_entries = [current_entries[seed_num - 1] for seed_num in seed_order]

        next_round_entries: list[dict] = []
        for position, slot_idx in enumerate(range(0, len(slot_entries), 2)):
            left_entry = slot_entries[slot_idx]
            right_entry = slot_entries[slot_idx + 1]

            match_id = await _insert_bracket_match(
                db,
                tournament_id,
                round_num=current_round,
                position=position,
                team1_id=left_entry.get("team_id"),
                team2_id=right_entry.get("team_id"),
                source_match1_id=left_entry.get("source_match_id"),
                source_match2_id=right_entry.get("source_match_id"),
            )
            next_round_entries.append({"source_match_id": match_id})
            match_count += 1

        current_entries = next_round_entries
        current_round += 1

    return match_count


def _is_power_of_two(value: int) -> bool:
    return value > 0 and (value & (value - 1)) == 0


def _highest_power_of_two_below(value: int) -> int:
    power = 1
    while power * 2 < value:
        power *= 2
    return power


def _seed_slot_order(size: int) -> list[int]:
    order = [1]
    current_size = 1
    while current_size < size:
        current_size *= 2
        next_order: list[int] = []
        for seed in order:
            next_order.append(seed)
            next_order.append(current_size + 1 - seed)
        order = next_order
    return order


async def _insert_bracket_match(
    db,
    tournament_id: int,
    *,
    round_num: int,
    position: int,
    team1_id: int | None = None,
    team2_id: int | None = None,
    source_match1_id: int | None = None,
    source_match2_id: int | None = None,
) -> int:  # noqa: ANN001
    cursor = await db.execute(
        "INSERT INTO bracket_matches "
        "(tournament_id, round, position, bracket_type, team1_id, team2_id, source_match1_id, source_match2_id, status) "
        "VALUES (?, ?, ?, 'winners', ?, ?, ?, ?, 'pending')",
        (tournament_id, round_num, position, team1_id, team2_id, source_match1_id, source_match2_id),
    )
    return int(cursor.lastrowid)


async def _propagate_byes(db, tournament_id: int) -> None:  # noqa: ANN001
    """Propagiert historische BYE-Gewinner automatisch in die nächste Runde."""
    cursor = await db.execute(
        "SELECT * FROM bracket_matches "
        "WHERE tournament_id = ? AND status = 'completed' AND winner_id IS NOT NULL "
        "ORDER BY round, position",
        (tournament_id,),
    )
    completed = await cursor.fetchall()

    for match in completed:
        await _advance_bracket_winner_in_db(db, tournament_id, dict(match), int(match["winner_id"]))


async def advance_bracket_winner(tournament_id: int, match_id: int, winner_id: int) -> None:
    """Nach einem Bracket-Match: Gewinner in die nächste Runde setzen."""
    async with get_db() as db:
        cursor = await db.execute("SELECT * FROM bracket_matches WHERE id = ?", (match_id,))
        match = await cursor.fetchone()
        if not match:
            return
        await _advance_bracket_winner_in_db(db, tournament_id, dict(match), winner_id)

        await db.commit()


async def _advance_bracket_winner_in_db(
    db,
    tournament_id: int,
    match: dict,
    winner_id: int,
) -> None:  # noqa: ANN001
    cursor = await db.execute(
        "SELECT id, source_match1_id, source_match2_id "
        "FROM bracket_matches "
        "WHERE tournament_id = ? AND (source_match1_id = ? OR source_match2_id = ?) "
        "LIMIT 1",
        (tournament_id, match["id"], match["id"]),
    )
    next_match = await cursor.fetchone()
    if next_match:
        if next_match["source_match1_id"] == match["id"]:
            await db.execute(
                "UPDATE bracket_matches SET team1_id = ? WHERE id = ?",
                (winner_id, next_match["id"]),
            )
        elif next_match["source_match2_id"] == match["id"]:
            await db.execute(
                "UPDATE bracket_matches SET team2_id = ? WHERE id = ?",
                (winner_id, next_match["id"]),
            )
        return

    # Fallback für ältere Brackets ohne Source-Mapping.
    round_num = match["round"]
    position = match["position"]
    next_round = round_num + 1
    next_position = position // 2

    cursor = await db.execute(
        "SELECT id FROM bracket_matches WHERE tournament_id = ? AND round = ? AND position = ?",
        (tournament_id, next_round, next_position),
    )
    legacy_next_match = await cursor.fetchone()
    if not legacy_next_match:
        return

    if position % 2 == 0:
        await db.execute(
            "UPDATE bracket_matches SET team1_id = ? WHERE id = ?",
            (winner_id, legacy_next_match["id"]),
        )
    else:
        await db.execute(
            "UPDATE bracket_matches SET team2_id = ? WHERE id = ?",
            (winner_id, legacy_next_match["id"]),
        )
