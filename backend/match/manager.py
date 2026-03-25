"""Match Manager — Orchestriert Steam-Lobby-Workflow für Bracket-Matches."""
from __future__ import annotations

from typing import Any

from db import get_db
from match import steam_bridge
from match.result_processor import (
    MatchNotFoundError,
    MatchResultError,
    MatchStateError,
    SteamTaskError,
    apply_bracket_match_result,
)

DEFAULT_GAME_MODE = 1
DEFAULT_REGION_MODE = 1
VALID_LOBBY_STATUSES = {"pending", "checkin"}
VALID_START_STATUSES = {"lobby_created"}
VALID_RESULT_STATUSES = {"in_progress"}
VALID_LEAVE_STATUSES = {"lobby_created", "in_progress"}


async def create_lobby(
    tournament_id: int,
    match_id: int,
    *,
    game_mode: int = DEFAULT_GAME_MODE,
    region_mode: int = DEFAULT_REGION_MODE,
) -> dict[str, Any]:
    """Erstellt eine Steam-Custom-Lobby für ein Bracket-Match."""
    match = await _get_bracket_match(tournament_id, match_id)
    _require_match_ready_for_lobby(match)
    await _ensure_no_duplicate_lobby_request(match_id, match)

    result = await _run_steam_task(
        action="Lobby-Erstellung",
        task_type="GC_CREATE_CUSTOM_LOBBY",
        payload={
            "tournament_id": tournament_id,
            "match_id": match_id,
            "game_mode": game_mode,
            "region_mode": region_mode,
        },
        timeout_s=45,
    )

    party_id = _coerce_required_str(result.get("party_id"), "party_id")
    party_code = result.get("party_code") or result.get("party_code_display") or result.get("join_code")
    join_code = result.get("join_code") or result.get("party_code") or result.get("party_code_display")
    if party_code is None and join_code is None:
        raise SteamTaskError("Lobby-Erstellung lieferte keinen Party-Code")
    if party_code is None:
        party_code = join_code
    if join_code is None:
        join_code = party_code

    async with get_db() as db:
        await db.execute(
            """
            UPDATE bracket_matches
            SET steam_party_id = ?, party_code = ?, status = 'lobby_created'
            WHERE id = ? AND tournament_id = ?
            """,
            (party_id, str(party_code), match_id, tournament_id),
        )
        await db.commit()

    normalized_result = dict(result)
    normalized_result.update(
        {
            "success": True,
            "party_id": party_id,
            "party_code": str(party_code),
            "join_code": str(join_code),
        }
    )
    return normalized_result


async def set_bot_spectator(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Setzt den Bot auf den Spectator-Slot."""
    party_id = await _get_party_id(tournament_id, match_id)
    return await _run_steam_task(
        action="Spectator-Slot setzen",
        task_type="GC_LOBBY_SET_SPECTATOR",
        payload={"party_id": party_id},
        timeout_s=20,
    )


async def set_bot_ready(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Setzt den Bot in der Lobby auf ready."""
    party_id = await _get_party_id(tournament_id, match_id)
    return await _run_steam_task(
        action="Ready-Status setzen",
        task_type="GC_LOBBY_READY",
        payload={"party_id": party_id},
        timeout_s=20,
    )


async def start_match(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Startet ein Match über den Steam-Bot."""
    match = await _get_bracket_match(tournament_id, match_id)
    _require_match_ready_for_start(match)

    await set_bot_spectator(tournament_id, match_id)
    await set_bot_ready(tournament_id, match_id)

    result = await _run_steam_task(
        action="Match-Start",
        task_type="GC_LOBBY_START_MATCH",
        payload={"party_id": match["steam_party_id"]},
        timeout_s=45,
    )

    match_id_value = _coerce_optional_int(
        result.get("match_id") or result.get("deadlock_match_id"),
        "match_id",
    )

    async with get_db() as db:
        await db.execute(
            """
            UPDATE bracket_matches
            SET status = 'in_progress',
                deadlock_match_id = COALESCE(?, deadlock_match_id)
            WHERE id = ? AND tournament_id = ?
            """,
            (
                str(match_id_value) if match_id_value is not None else None,
                match_id,
                tournament_id,
            ),
        )
        await db.commit()

    normalized_result = dict(result)
    normalized_result.update({"success": True, "match_id": match_id_value})
    return normalized_result


async def fetch_match_result(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Holt das Match-Ergebnis über die Steam-Bridge und übernimmt es ins Bracket."""
    match = await _get_bracket_match(tournament_id, match_id)
    _require_match_ready_for_result_fetch(match)

    result = await _run_steam_task(
        action="Match-Ergebnis abrufen",
        task_type="GC_GET_MATCH_RESULT",
        payload=_build_match_result_payload(match),
        timeout_s=45,
    )

    winning_team_raw = result.get("winning_team")
    winner_id_raw = result.get("winner_id")

    try:
        applied = await apply_bracket_match_result(
            tournament_id,
            match_id,
            winning_team=_coerce_optional_int(winning_team_raw, "winning_team"),
            winner_id=_coerce_optional_int(winner_id_raw, "winner_id"),
            duration_s=_coerce_optional_int(result.get("duration_s"), "duration_s"),
            players=result.get("players"),
            source="automatic",
        )
    except (MatchNotFoundError, MatchStateError):
        raise
    except MatchResultError as exc:
        raise SteamTaskError(f"Ungültige Steam-Ergebnisdaten: {exc}") from exc

    normalized_result = dict(result)
    normalized_result.update(applied)
    normalized_result["success"] = True
    return normalized_result


async def leave_lobby(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Lässt den Bot die Lobby verlassen."""
    match = await _get_bracket_match(tournament_id, match_id)
    _require_match_ready_for_leave(match)
    result = await _run_steam_task(
        action="Lobby verlassen",
        task_type="GC_LOBBY_LEAVE",
        payload={"party_id": match["steam_party_id"]},
        timeout_s=20,
    )
    normalized_result = dict(result)
    normalized_result["success"] = True
    return normalized_result


async def _get_bracket_match(tournament_id: int, match_id: int) -> dict[str, Any]:
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT id, tournament_id, round, position, team1_id, team2_id, winner_id,
                   status, steam_party_id, party_code, deadlock_match_id,
                   match_duration_s, match_stats, scheduled_at, played_at
            FROM bracket_matches
            WHERE id = ? AND tournament_id = ?
            """,
            (match_id, tournament_id),
        )
        row = await cursor.fetchone()

    if not row:
        raise MatchNotFoundError(f"Bracket-Match {match_id} im Turnier {tournament_id} nicht gefunden")
    return dict(row)


async def _get_party_id(tournament_id: int, match_id: int) -> str:
    match = await _get_bracket_match(tournament_id, match_id)
    party_id = match.get("steam_party_id")
    if not party_id:
        raise MatchStateError(f"Für Match {match_id} ist keine Party-ID gespeichert")
    return str(party_id)


def _require_match_ready_for_lobby(match: dict[str, Any]) -> None:
    if match["winner_id"] is not None or match["status"] not in VALID_LOBBY_STATUSES:
        raise MatchStateError("Für dieses Match kann keine Lobby erstellt werden")
    if match["team1_id"] is None or match["team2_id"] is None:
        raise MatchStateError("Beide Teams müssen gesetzt sein, bevor eine Lobby erstellt wird")
    if match.get("steam_party_id"):
        raise MatchStateError("Für dieses Match existiert bereits eine Lobby")


def _require_match_ready_for_start(match: dict[str, Any]) -> None:
    if match["status"] not in VALID_START_STATUSES:
        raise MatchStateError("Ein Match kann nur aus dem Status 'lobby_created' gestartet werden")
    if not match.get("steam_party_id"):
        raise MatchStateError("Für dieses Match existiert noch keine Lobby")
    if match.get("deadlock_match_id"):
        raise MatchStateError("Für dieses Match wurde bereits eine Deadlock-Match-ID gespeichert")


def _require_match_ready_for_result_fetch(match: dict[str, Any]) -> None:
    if match["status"] not in VALID_RESULT_STATUSES:
        raise MatchStateError("Match-Ergebnisse können nur aus laufenden Matches abgerufen werden")
    if not match.get("steam_party_id") and not match.get("deadlock_match_id"):
        raise MatchStateError(
            "Für dieses Match ist weder eine Party-ID noch eine Deadlock-Match-ID gespeichert"
        )


def _require_match_ready_for_leave(match: dict[str, Any]) -> None:
    if match["status"] not in VALID_LEAVE_STATUSES:
        raise MatchStateError("Die Lobby kann nur im Status 'lobby_created' oder 'in_progress' verlassen werden")
    if not match.get("steam_party_id"):
        raise MatchStateError("Für dieses Match ist keine Party-ID gespeichert")


async def _ensure_no_duplicate_lobby_request(match_id: int, match: dict[str, Any]) -> None:
    if match.get("steam_party_id"):
        raise MatchStateError("Für dieses Match existiert bereits eine Lobby")
    if await steam_bridge.has_active_task("GC_CREATE_CUSTOM_LOBBY", match_id=match_id):
        raise MatchStateError("Für dieses Match läuft bereits eine Lobby-Erstellung")


async def _run_steam_task(
    *,
    action: str,
    task_type: str,
    payload: dict[str, Any],
    timeout_s: float,
) -> dict[str, Any]:
    try:
        task_id = await steam_bridge.create_task(task_type, payload)
        result = await steam_bridge.poll_task_result(task_id, timeout_s=timeout_s)
    except TimeoutError:
        raise
    except RuntimeError as exc:
        raise SteamTaskError(f"{action} fehlgeschlagen: {exc}") from exc

    if not isinstance(result, dict):
        raise SteamTaskError(f"{action} lieferte kein gültiges Ergebnis")
    if result.get("success") is False:
        raise SteamTaskError(f"{action} fehlgeschlagen: {result.get('error', 'unbekannter Fehler')}")
    return result


def _coerce_optional_int(value: Any, field_name: str) -> int | None:
    if value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError) as exc:
        raise SteamTaskError(f"{field_name} muss eine ganze Zahl sein") from exc


def _coerce_required_int(value: Any, field_name: str) -> int:
    coerced = _coerce_optional_int(value, field_name)
    if coerced is None:
        raise SteamTaskError(f"{field_name} fehlt im Steam-Ergebnis")
    return coerced


def _coerce_required_str(value: Any, field_name: str) -> str:
    if value is None:
        raise SteamTaskError(f"{field_name} fehlt im Steam-Ergebnis")
    coerced = str(value).strip()
    if not coerced:
        raise SteamTaskError(f"{field_name} fehlt im Steam-Ergebnis")
    return coerced


def _build_match_result_payload(match: dict[str, Any]) -> dict[str, Any]:
    payload: dict[str, Any] = {}
    if match.get("deadlock_match_id"):
        payload["match_id"] = match["deadlock_match_id"]
    if match.get("steam_party_id"):
        payload["party_id"] = match["steam_party_id"]
    return payload
