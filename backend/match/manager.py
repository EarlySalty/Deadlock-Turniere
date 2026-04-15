"""Match Manager — Orchestriert Steam-Lobby-Workflow für Bracket-Matches."""
from __future__ import annotations

import logging
import json
from typing import Any

from db import get_db
from match import steam_bridge
from notifications.discord_notifier import (
    create_match_channel,
    notify_users,
    send_match_lobby_info,
)
from match.result_processor import (
    MatchNotFoundError,
    MatchResultError,
    MatchStateError,
    SteamTaskError,
    apply_bracket_match_result,
)

logger = logging.getLogger(__name__)

DEFAULT_GAME_MODE = 1
DEFAULT_REGION_MODE = 1
VALID_LOBBY_STATUSES = {"pending", "checkin"}
VALID_START_STATUSES = {"lobby_created"}
VALID_RESULT_STATUSES = {"in_progress"}
VALID_LEAVE_STATUSES = {"lobby_created", "in_progress"}

MATCH_EVENT_PRESETS: dict[str, dict[str, Any]] = {
    "duplicate_heroes": {
        "label": "Duplicate Heroes",
        "description": "Alle Teams duerfen denselben Hero mehrfach spielen.",
        "requires_cheats": False,
        "convars": {
            "citadel_allow_duplicate_heroes": 1,
        },
        "reset_convars": {
            "citadel_allow_duplicate_heroes": 0,
        },
    },
    "slowmo": {
        "label": "Slow Motion",
        "description": "Verlangsamt das Match fuer Clutch- oder Showmomente.",
        "requires_cheats": True,
        "convars": {
            "host_timescale": 0.7,
        },
        "reset_convars": {
            "host_timescale": 1,
        },
    },
    "melee_mayhem": {
        "label": "Melee Mayhem",
        "description": "Nahkampf wird deutlich staerker als gewohnt.",
        "requires_cheats": True,
        "convars": {
            "citadel_melee_damage_scale": 2.5,
        },
        "reset_convars": {
            "citadel_melee_damage_scale": 1,
        },
    },
    "glass_cannon": {
        "label": "Glass Cannon",
        "description": "Hoher Schaden fuer schnelle, chaotische Teamfights.",
        "requires_cheats": True,
        "convars": {
            "citadel_dps_multiplier": 2,
            "citadel_melee_damage_scale": 1.5,
        },
        "reset_convars": {
            "citadel_dps_multiplier": 1,
            "citadel_melee_damage_scale": 1,
        },
    },
    "walljump_party": {
        "label": "Walljump Party",
        "description": "Mehr Mobilitaet fuer alberne Mobility-Runden.",
        "requires_cheats": True,
        "convars": {
            "citadel_initial_wall_jump_stamina_cost": 0,
            "citadel_air_jumps_enabled": 1,
        },
        "reset_convars": {
            "citadel_initial_wall_jump_stamina_cost": 0,
            "citadel_air_jumps_enabled": 1,
        },
    },
    "orb_madness": {
        "label": "Orb Madness",
        "description": "Orbs werden leichter und chaotischer claimbar.",
        "requires_cheats": True,
        "convars": {
            "citadel_orb_required_bullets_to_claim_override": 1,
            "citadel_orb_expire_percentage": 1,
        },
        "reset_convars": {
            "citadel_orb_required_bullets_to_claim_override": 0,
            "citadel_orb_expire_percentage": 1,
        },
    },
    "zipline_boost": {
        "label": "Zipline Boost",
        "description": "Mehr Druck auf Movement und Rotationen ueber Ziplines.",
        "requires_cheats": True,
        "convars": {
            "zipline_use_new_latch": 2,
            "citadel_debug_zipline_camera_height_add": 0,
        },
        "reset_convars": {
            "zipline_use_new_latch": 2,
            "citadel_debug_zipline_camera_height_add": 0,
        },
    },
}


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
    lobby_settings = await _get_tournament_lobby_settings(tournament_id)
    match_context = await _load_match_context(tournament_id, match_id)
    participant_rows = await _load_match_participants(tournament_id, match_id)
    participant_discord_ids = [str(row["discord_id"]) for row in participant_rows if row["discord_id"]]
    steam_ids = [str(row["steam_id"]).strip() for row in participant_rows if row["steam_id"]]

    create_payload: dict[str, Any] = {
        "tournament_id": tournament_id,
        "match_id": match_id,
        "game_mode": game_mode,
        "region_mode": region_mode,
    }
    if lobby_settings:
        create_payload["convars"] = lobby_settings

    result = await _run_steam_task(
        action="Lobby-Erstellung",
        task_type="GC_CREATE_CUSTOM_LOBBY",
        payload=create_payload,
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

    invite_result = await _invite_match_participants_to_lobby(party_id, steam_ids)

    discord_channel_id: str | None = None
    try:
        discord_channel_id = await create_match_channel(
            match_id,
            match_context["team1_name"],
            match_context["team2_name"],
        )
        async with get_db() as db:
            await db.execute(
                "UPDATE bracket_matches SET discord_channel_id = ? WHERE id = ? AND tournament_id = ?",
                (discord_channel_id, match_id, tournament_id),
            )
            await db.commit()
        await send_match_lobby_info(discord_channel_id, str(party_code), participant_discord_ids)
    except Exception:
        logger.exception(
            "Discord match channel setup failed (tournament=%s match=%s)",
            tournament_id,
            match_id,
        )
        discord_channel_id = None

    normalized_result = dict(result)
    normalized_result.update(
        {
            "success": True,
            "party_id": party_id,
            "party_code": str(party_code),
            "join_code": str(join_code),
            "lobby_settings": lobby_settings,
            "invite_result": invite_result,
            "discord_channel_id": discord_channel_id,
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
    match_context = await _load_match_context(tournament_id, match_id)

    await set_bot_spectator(tournament_id, match_id)
    await set_bot_ready(tournament_id, match_id)

    result = await _run_steam_task(
        action="Match-Start",
        task_type="GC_LOBBY_START_MATCH",
        payload={"party_id": match["steam_party_id"]},
        timeout_s=45,
    )

    participant_rows = await _load_match_participants(tournament_id, match_id)
    participant_discord_ids = [str(row["discord_id"]) for row in participant_rows if row["discord_id"]]
    try:
        await notify_users(
            participant_discord_ids,
            "match_start",
            (
                f"Euer Match zwischen {match_context['team1_name']} und {match_context['team2_name']} "
                f"läuft jetzt. Lobby-Code: {match['party_code'] or match['steam_party_id']}"
            ),
        )
    except Exception:
        logger.exception(
            "Match start notification failed (tournament=%s match=%s)",
            tournament_id,
            match_id,
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


async def list_match_event_presets() -> list[dict[str, Any]]:
    """Liefert die verfuegbaren Live-Event-Presets fuer das Admin-Panel."""
    presets: list[dict[str, Any]] = []
    for key, config in MATCH_EVENT_PRESETS.items():
        presets.append(
            {
                "key": key,
                "label": config["label"],
                "description": config["description"],
                "requires_cheats": bool(config.get("requires_cheats")),
                "convars": dict(config.get("convars") or {}),
                "reset_convars": dict(config.get("reset_convars") or {}),
            }
        )
    return presets


async def apply_match_convars(
    tournament_id: int,
    match_id: int,
    convars: dict[str, Any],
) -> dict[str, Any]:
    """Wendet beliebige ConVars live auf eine bestehende Match-Lobby an."""
    match = await _get_bracket_match(tournament_id, match_id)
    _require_match_has_live_lobby(match)
    normalized_convars = _normalize_convar_payload(convars)
    result = await _run_steam_task(
        action="Match-ConVars anwenden",
        task_type="GC_LOBBY_APPLY_CONVARS",
        payload={
            "party_id": str(match["steam_party_id"]),
            "convars": normalized_convars,
            "tournament_id": tournament_id,
            "match_id": match_id,
        },
        timeout_s=30,
    )
    normalized_result = dict(result)
    normalized_result.update(
        {
            "success": True,
            "match_id": match_id,
            "party_id": str(match["steam_party_id"]),
            "applied_convars": normalized_convars,
        }
    )
    return normalized_result


async def apply_match_event_preset(
    tournament_id: int,
    match_id: int,
    preset_key: str,
    *,
    enabled: bool = True,
) -> dict[str, Any]:
    """Wendet ein vordefiniertes Event-Preset auf eine Lobby an."""
    preset = MATCH_EVENT_PRESETS.get(str(preset_key).strip())
    if not preset:
        raise MatchStateError(f"Unbekanntes Event-Preset: {preset_key}")

    convars = preset["convars"] if enabled else preset.get("reset_convars") or {}
    result = await apply_match_convars(tournament_id, match_id, dict(convars))
    result.update(
        {
            "preset_key": str(preset_key).strip(),
            "enabled": bool(enabled),
            "label": preset["label"],
            "requires_cheats": bool(preset.get("requires_cheats")),
        }
    )
    return result


async def _get_bracket_match(tournament_id: int, match_id: int) -> dict[str, Any]:
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT bm.id, bm.tournament_id, bm.round, bm.position, bm.team1_id, bm.team2_id,
                   bm.winner_id, bm.status, bm.steam_party_id, bm.party_code,
                   bm.deadlock_match_id, bm.discord_channel_id,
                   bm.match_duration_s, bm.match_stats, bm.scheduled_at, bm.played_at,
                   t1.name AS team1_name, t2.name AS team2_name
            FROM bracket_matches bm
            LEFT JOIN teams t1 ON t1.id = bm.team1_id
            LEFT JOIN teams t2 ON t2.id = bm.team2_id
            WHERE bm.id = ? AND bm.tournament_id = ?
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


async def _get_tournament_lobby_settings(tournament_id: int) -> dict[str, Any]:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT lobby_settings FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        row = await cursor.fetchone()

    if not row:
        raise MatchNotFoundError(f"Turnier {tournament_id} nicht gefunden")

    raw_settings = row["lobby_settings"]
    if raw_settings in (None, "", "{}"):
        return {}
    if isinstance(raw_settings, dict):
        return dict(raw_settings)

    try:
        parsed = json.loads(str(raw_settings))
    except json.JSONDecodeError as exc:
        raise SteamTaskError("lobby_settings enthält kein gültiges JSON-Objekt") from exc

    if parsed is None:
        return {}
    if not isinstance(parsed, dict):
        raise SteamTaskError("lobby_settings muss ein JSON-Objekt sein")
    return parsed


async def _invite_match_participants_to_lobby(
    party_id: str,
    steam_ids: list[str],
) -> dict[str, Any]:
    if not steam_ids:
        return {
            "success": True,
            "party_id": party_id,
            "steam_ids": [],
            "invited": [],
            "failed": [],
            "skipped": [],
        }

    return await steam_bridge.invite_players_to_lobby(party_id, steam_ids)


async def _load_match_context(tournament_id: int, match_id: int) -> dict[str, Any]:
    match = await _get_bracket_match(tournament_id, match_id)
    return {
        "team1_name": match.get("team1_name") or f"Team {match['team1_id']}",
        "team2_name": match.get("team2_name") or f"Team {match['team2_id']}",
    }


async def _load_match_participants(tournament_id: int, match_id: int) -> list[dict[str, Any]]:
    match = await _get_bracket_match(tournament_id, match_id)
    team_ids = [match.get("team1_id"), match.get("team2_id")]
    team_ids = [int(team_id) for team_id in team_ids if team_id is not None]
    if not team_ids:
        return []

    placeholders = ", ".join("?" for _ in team_ids)
    async with get_db() as db:
        cursor = await db.execute(
            f"""
            SELECT tm.discord_id, tm.discord_name, tm.steam_id, t.id AS team_id, t.name AS team_name
            FROM team_members tm
            JOIN teams t ON t.id = tm.team_id
            WHERE t.id IN ({placeholders})
            ORDER BY t.id, tm.joined_at, tm.id
            """,
            team_ids,
        )
        rows = await cursor.fetchall()

    return [dict(row) for row in rows]


async def _load_match_participant_steam_ids(tournament_id: int, match_id: int) -> list[str]:
    match = await _get_bracket_match(tournament_id, match_id)
    team_ids = [match.get("team1_id"), match.get("team2_id")]
    team_ids = [int(team_id) for team_id in team_ids if team_id is not None]
    if not team_ids:
        return []

    async with get_db() as db:
        placeholders = ", ".join("?" for _ in team_ids)
        cursor = await db.execute(
            f"""
            SELECT DISTINCT steam_id
              FROM team_members
             WHERE team_id IN ({placeholders})
               AND steam_id IS NOT NULL
               AND steam_id != ''
            ORDER BY steam_id
            """,
            team_ids,
        )
        rows = await cursor.fetchall()

    return [str(row["steam_id"]).strip() for row in rows if row["steam_id"]]


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


def _require_match_has_live_lobby(match: dict[str, Any]) -> None:
    if match["status"] not in VALID_LEAVE_STATUSES:
        raise MatchStateError(
            "Live-Events koennen nur fuer Matches mit aktiver oder laufender Lobby gesetzt werden"
        )
    if not match.get("steam_party_id"):
        raise MatchStateError("Fuer dieses Match ist keine Party-ID gespeichert")


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


def _normalize_convar_payload(convars: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(convars, dict):
        raise MatchStateError("convars muessen als JSON-Objekt uebergeben werden")

    normalized: dict[str, Any] = {}
    for raw_name, raw_value in convars.items():
        name = str(raw_name or "").strip()
        if not name:
            raise MatchStateError("ConVar-Name darf nicht leer sein")

        value = raw_value
        if isinstance(value, str):
            stripped = value.strip()
            if stripped == "":
                raise MatchStateError(f"ConVar-Wert fuer {name} darf nicht leer sein")
            lowered = stripped.lower()
            if lowered in {"true", "on"}:
                value = 1
            elif lowered in {"false", "off"}:
                value = 0
            else:
                try:
                    value = int(stripped)
                except ValueError:
                    try:
                        value = float(stripped)
                    except ValueError:
                        value = stripped

        normalized[name] = value

    if not normalized:
        raise MatchStateError("Mindestens eine ConVar ist erforderlich")
    return normalized
