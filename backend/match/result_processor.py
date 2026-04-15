"""Result Processor — Vereinheitlicht Bracket-Ergebnisse für manuell und automatisch."""
from __future__ import annotations

import asyncio
import json
from typing import Any

from config import settings
from db import get_db
from notifications.discord_notifier import delete_match_channel_later
from tournament.engine import advance_bracket_winner


class MatchResultError(RuntimeError):
    """Ein übergebenes Bracket-Ergebnis ist fachlich ungültig."""


class MatchNotFoundError(MatchResultError):
    """Das betroffene Match existiert nicht."""


class MatchStateError(MatchResultError):
    """Das Match befindet sich nicht in einem erlaubten Status."""


class SteamTaskError(RuntimeError):
    """Die Steam-/GC-Ergebnisdaten sind ungültig oder unvollständig."""


async def apply_bracket_match_result(
    tournament_id: int,
    match_id: int,
    *,
    winning_team: int | None = None,
    winner_id: int | None = None,
    duration_s: int | None,
    players: list[dict[str, Any]] | None,
    source: str = "automatic",
    force: bool = False,
) -> dict[str, Any]:
    """Persistiert ein Bracket-Ergebnis und propagiert den Gewinner."""
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT id, round, position, team1_id, team2_id, winner_id, status,
                   source_match1_id, source_match2_id, match_duration_s, match_stats,
                   discord_channel_id
            FROM bracket_matches
            WHERE id = ? AND tournament_id = ?
            """,
            (match_id, tournament_id),
        )
        match = await cursor.fetchone()
        if not match:
            raise MatchNotFoundError(f"Bracket-Match {match_id} nicht gefunden")

        if match["status"] in {"completed", "cancelled", "forfeit"} and not force:
            raise MatchStateError(
                f"Bracket-Match {match_id} kann aus Status {match['status']} nicht verarbeitet werden"
            )

        team1_id = match["team1_id"]
        team2_id = match["team2_id"]
        if team1_id is None or team2_id is None:
            raise MatchStateError(
                f"Bracket-Match {match_id} hat noch nicht beide Teams gesetzt"
            )

        winning_team_value = _coerce_optional_int(winning_team, "winning_team")
        winner_id_value = _coerce_optional_int(winner_id, "winner_id")

        if winning_team_value is None and winner_id_value is None:
            raise MatchResultError("winner_id oder winning_team ist erforderlich")

        if winner_id_value is None:
            if winning_team_value == 0:
                winner_id_value = team1_id
            elif winning_team_value == 1:
                winner_id_value = team2_id
            else:
                raise MatchResultError(f"Ungültiger winning_team-Wert: {winning_team_value}")
        elif winning_team_value is None:
            winning_team_value = _resolve_winning_team(team1_id, team2_id, winner_id_value)
        else:
            if winning_team_value not in (0, 1):
                raise MatchResultError(f"Ungültiger winning_team-Wert: {winning_team_value}")
            expected_winning_team = _resolve_winning_team(team1_id, team2_id, winner_id_value)
            if winning_team_value != expected_winning_team:
                raise MatchResultError(
                    "winning_team passt nicht zum übergebenen winner_id"
                )

        if winner_id_value not in (team1_id, team2_id):
            raise MatchResultError(
                f"winner_id {winner_id_value} gehört nicht zu Match {match_id}"
            )

        if (
            force
            and match["winner_id"] is not None
            and int(match["winner_id"]) != winner_id_value
        ):
            await _reset_bracket_downstream(db, tournament_id, dict(match))

        duration_value = _coerce_optional_int(duration_s, "duration_s")
        if duration_value is None:
            duration_value = match["match_duration_s"]

        player_stats_json, return_players = _resolve_player_stats(
            players,
            match["match_stats"],
        )

        await db.execute(
            "DELETE FROM match_results WHERE bracket_match_id = ?",
            (match_id,),
        )
        await db.execute(
            """
            UPDATE bracket_matches
            SET winner_id = ?,
                status = 'completed',
                match_duration_s = ?,
                match_stats = ?,
                played_at = datetime('now')
            WHERE id = ? AND tournament_id = ?
            """,
            (winner_id_value, duration_value, player_stats_json, match_id, tournament_id),
        )
        await db.execute(
            """
            INSERT INTO match_results (
                bracket_match_id,
                winning_team,
                duration_s,
                player_stats,
                source
            )
            VALUES (?, ?, ?, ?, ?)
            """,
            (match_id, winner_id_value, duration_value, player_stats_json, source),
        )
        await db.commit()

    discord_channel_id = match.get("discord_channel_id")
    if discord_channel_id:
        asyncio.create_task(
            delete_match_channel_later(
                str(discord_channel_id),
                delay_seconds=float(settings.DISCORD_MATCH_CHANNEL_DELETE_DELAY_SECONDS),
            )
        )

    await advance_bracket_winner(tournament_id, match_id, winner_id_value)

    return {
        "match_id": match_id,
        "winner_id": winner_id_value,
        "winning_team": winning_team_value,
        "duration_s": duration_value,
        "players": return_players,
    }


async def _reset_bracket_downstream(
    db,
    tournament_id: int,
    match: dict[str, Any],
) -> None:  # noqa: ANN001
    next_match = await _load_next_bracket_match(db, tournament_id, match)
    if not next_match:
        return

    await _reset_bracket_downstream(db, tournament_id, dict(next_match))

    slot_column = _resolve_next_slot(match, dict(next_match))
    await db.execute(
        f"""
        UPDATE bracket_matches
        SET {slot_column} = NULL,
            winner_id = NULL,
            status = 'pending',
            steam_party_id = NULL,
            party_code = NULL,
            deadlock_match_id = NULL,
            match_duration_s = NULL,
            match_stats = NULL,
            played_at = NULL
        WHERE id = ?
        """,
        (next_match["id"],),
    )
    await db.execute(
        "DELETE FROM match_results WHERE bracket_match_id = ?",
        (next_match["id"],),
    )


async def _load_next_bracket_match(
    db,
    tournament_id: int,
    match: dict[str, Any],
):  # noqa: ANN001
    cursor = await db.execute(
        """
        SELECT id, round, position, source_match1_id, source_match2_id
        FROM bracket_matches
        WHERE tournament_id = ? AND (source_match1_id = ? OR source_match2_id = ?)
        LIMIT 1
        """,
        (tournament_id, match["id"], match["id"]),
    )
    next_match = await cursor.fetchone()
    if next_match:
        return next_match

    cursor = await db.execute(
        """
        SELECT id, round, position, source_match1_id, source_match2_id
        FROM bracket_matches
        WHERE tournament_id = ? AND round = ? AND position = ?
        LIMIT 1
        """,
        (tournament_id, int(match["round"]) + 1, int(match["position"]) // 2),
    )
    return await cursor.fetchone()


def _resolve_next_slot(match: dict[str, Any], next_match: dict[str, Any]) -> str:
    if next_match.get("source_match1_id") == match["id"]:
        return "team1_id"
    if next_match.get("source_match2_id") == match["id"]:
        return "team2_id"
    return "team1_id" if int(match["position"]) % 2 == 0 else "team2_id"


def _coerce_optional_int(value: int | None, field_name: str) -> int | None:
    if value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError) as exc:
        raise MatchResultError(f"{field_name} muss eine ganze Zahl sein") from exc


def _resolve_winning_team(team1_id: int, team2_id: int, winner_id: int) -> int:
    if winner_id == team1_id:
        return 0
    if winner_id == team2_id:
        return 1
    raise MatchResultError(f"winner_id {winner_id} gehört nicht zu diesem Match")


def _resolve_player_stats(
    players: list[dict[str, Any]] | None,
    existing_match_stats: str | None,
) -> tuple[str | None, list[dict[str, Any]]]:
    if players is not None:
        if not isinstance(players, list):
            raise MatchResultError("players muss eine Liste sein")
        try:
            return json.dumps(players), players
        except TypeError as exc:
            raise MatchResultError("players enthält nicht serialisierbare Daten") from exc

    if not existing_match_stats:
        return None, []

    try:
        decoded = json.loads(existing_match_stats)
    except json.JSONDecodeError as exc:
        raise MatchResultError("match_stats enthält ungültiges JSON") from exc

    if not isinstance(decoded, list):
        raise MatchResultError("match_stats enthält keine gültige Spielerliste")

    return existing_match_stats, decoded
