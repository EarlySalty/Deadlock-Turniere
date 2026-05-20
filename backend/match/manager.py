"""Match Manager — Orchestriert Steam-Lobby-Workflow fuer Bracket- und Group-Matches."""
from __future__ import annotations

import json
import logging
from typing import Any

from config import settings
from db import get_db
from match.game_modes import prepare_match_assignments, resolve_match_objective
from match import steam_bridge
from notifications.discord_notifier import (
    create_match_channel,
    notify_casters_match_created,
    notify_users,
    send_lobby_announcement,
    send_match_lobby_info,
    move_users_to_voice_channel,
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
    """Erstellt eine Steam-Custom-Lobby fuer ein Bracket-Match."""
    return await _create_lobby_for_match(
        "bracket",
        tournament_id,
        match_id,
        game_mode=game_mode,
        region_mode=region_mode,
    )


async def create_group_lobby(
    tournament_id: int,
    match_id: int,
    *,
    game_mode: int = DEFAULT_GAME_MODE,
    region_mode: int = DEFAULT_REGION_MODE,
) -> dict[str, Any]:
    """Erstellt eine Steam-Custom-Lobby fuer ein Group-Match."""
    return await _create_lobby_for_match(
        "group",
        tournament_id,
        match_id,
        game_mode=game_mode,
        region_mode=region_mode,
    )


async def _create_lobby_for_match(
    match_type: str,
    tournament_id: int,
    match_id: int,
    *,
    game_mode: int = DEFAULT_GAME_MODE,
    region_mode: int = DEFAULT_REGION_MODE,
) -> dict[str, Any]:
    match = await _get_match(match_type, tournament_id, match_id)
    _require_match_ready_for_lobby(match)
    await _ensure_no_duplicate_lobby_request(match_type, match_id, match)
    lobby_settings = await _get_tournament_lobby_settings(tournament_id)
    match_context = await _load_match_context(match_type, tournament_id, match_id)
    participant_rows = await _load_match_participants(match_type, tournament_id, match_id)
    participant_discord_ids = [str(row["discord_id"]) for row in participant_rows if row["discord_id"]]
    steam_ids = [str(row["steam_id"]).strip() for row in participant_rows if row["steam_id"]]
    is_test_tournament = await _is_test_tournament(tournament_id)
    mode_payload = await prepare_match_assignments(tournament_id, match_type, match_id)
    merged_convars = dict(lobby_settings or {})
    merged_convars.update(mode_payload["convars"])
    hero_assignments = mode_payload["hero_assignments"]
    hero_assignments_text = mode_payload["announcement_lines"]
    hero_assignments_json = json.dumps(hero_assignments) if hero_assignments else None

    create_payload: dict[str, Any] = {
        "tournament_id": tournament_id,
        "match_id": match_id,
        "match_type": match_type,
        "game_mode": game_mode,
        "region_mode": region_mode,
    }
    if merged_convars:
        create_payload["convars"] = merged_convars

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
            f"""
            UPDATE {_match_table(match_type)}
            SET steam_party_id = ?, party_code = ?, status = 'lobby_created', hero_assignments = ?
            WHERE id = ? AND {_match_scope_column(match_type)} = ?
            """,
            (
                party_id,
                str(party_code),
                hero_assignments_json,
                match_id,
                _match_scope_value(match_type, match),
            ),
        )
        await db.commit()

    invite_result = await _invite_match_participants_to_lobby(party_id, steam_ids)

    discord_channel_id: str | None = None
    if not is_test_tournament:
        try:
            discord_channel_id = await create_match_channel(
                match_id,
                match_context["team1_name"],
                match_context["team2_name"],
            )
            async with get_db() as db:
                await db.execute(
                    f"UPDATE {_match_table(match_type)} SET discord_channel_id = ? "
                    f"WHERE id = ? AND {_match_scope_column(match_type)} = ?",
                    (discord_channel_id, match_id, _match_scope_value(match_type, match)),
                )
                await db.commit()
            await send_match_lobby_info(discord_channel_id, str(party_code), participant_discord_ids)
            caster_ids = await _load_match_casters(match_type, match_id)
            if caster_ids:
                await notify_casters_match_created(match_id, discord_channel_id, caster_ids)
            team1_ids = [
                str(row["discord_id"])
                for row in participant_rows
                if row["discord_id"] and row["team_id"] == match.get("team1_id")
            ]
            team2_ids = [
                str(row["discord_id"])
                for row in participant_rows
                if row["discord_id"] and row["team_id"] == match.get("team2_id")
            ]
            objective_text: str | None = None
            try:
                async with get_db() as db:
                    cursor = await db.execute(
                        "SELECT match_objective, team_size FROM tournaments WHERE id = ?",
                        (tournament_id,),
                    )
                    objective_row = await cursor.fetchone()
                if objective_row is not None:
                    _, objective_text = resolve_match_objective(
                        objective_row["match_objective"],
                        int(objective_row["team_size"]),
                    )
            except Exception:
                logger.exception(
                    "Objective-Auflösung für Match %s fehlgeschlagen (non-critical)",
                    match_id,
                )

            try:
                await send_lobby_announcement(
                    match_id=match_id,
                    party_code=str(party_code),
                    team1_name=match_context["team1_name"],
                    team2_name=match_context["team2_name"],
                    team1_discord_ids=team1_ids,
                    team2_discord_ids=team2_ids,
                    hero_assignments_text=hero_assignments_text or None,
                    objective_text=objective_text,
                )
            except Exception:
                logger.exception(
                    "Lobby-Announcement für Match %s fehlgeschlagen (non-critical)",
                    match_id,
                )
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
            "lobby_settings": merged_convars,
            "hero_assignments": hero_assignments,
            "invite_result": invite_result,
            "discord_channel_id": discord_channel_id,
        }
    )
    return normalized_result


async def set_bot_spectator(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Setzt den Bot auf den Spectator-Slot."""
    party_id = await _get_party_id("bracket", tournament_id, match_id)
    return await _run_steam_task(
        action="Spectator-Slot setzen",
        task_type="GC_LOBBY_SET_SPECTATOR",
        payload={"party_id": party_id},
        timeout_s=20,
    )


async def set_bot_ready(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Setzt den Bot in der Lobby auf ready."""
    party_id = await _get_party_id("bracket", tournament_id, match_id)
    return await _run_steam_task(
        action="Ready-Status setzen",
        task_type="GC_LOBBY_READY",
        payload={"party_id": party_id},
        timeout_s=20,
    )


async def start_match(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Startet ein Bracket-Match ueber den Steam-Bot."""
    return await _start_match_for_match("bracket", tournament_id, match_id)


async def start_group_match(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Startet ein Group-Match ueber den Steam-Bot."""
    return await _start_match_for_match("group", tournament_id, match_id)


async def _start_match_for_match(
    match_type: str,
    tournament_id: int,
    match_id: int,
) -> dict[str, Any]:
    match = await _get_match(match_type, tournament_id, match_id)
    _require_match_ready_for_start(match)
    match_context = await _load_match_context(match_type, tournament_id, match_id)

    party_id = await _get_party_id(match_type, tournament_id, match_id)
    await _run_steam_task(
        action="Spectator-Slot setzen",
        task_type="GC_LOBBY_SET_SPECTATOR",
        payload={"party_id": party_id},
        timeout_s=20,
    )
    await _run_steam_task(
        action="Ready-Status setzen",
        task_type="GC_LOBBY_READY",
        payload={"party_id": party_id},
        timeout_s=20,
    )

    result = await _run_steam_task(
        action="Match-Start",
        task_type="GC_LOBBY_START_MATCH",
        payload={"party_id": party_id, "match_type": match_type, "match_id": match_id},
        timeout_s=45,
    )

    participant_rows = await _load_match_participants(match_type, tournament_id, match_id)
    participant_discord_ids = [str(row["discord_id"]) for row in participant_rows if row["discord_id"]]
    is_test_tournament = await _is_test_tournament(tournament_id)
    if not is_test_tournament:
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

    caster_ids = await _load_match_casters(match_type, match_id)
    if caster_ids and not is_test_tournament:
        try:
            await move_users_to_voice_channel(
                caster_ids,
                settings.DISCORD_CASTER_VOICE_CHANNEL_ID,
                guild_id=int(settings.DISCORD_GUILD_ID),
            )
        except Exception:
            logger.exception("Caster voice move failed (match_type=%s match=%s)", match_type, match_id)

    match_id_value = _coerce_optional_int(
        result.get("match_id") or result.get("deadlock_match_id"),
        "match_id",
    )

    async with get_db() as db:
        await db.execute(
            f"""
            UPDATE {_match_table(match_type)}
            SET status = 'in_progress',
                deadlock_match_id = COALESCE(?, deadlock_match_id)
            WHERE id = ? AND {_match_scope_column(match_type)} = ?
            """,
            (
                str(match_id_value) if match_id_value is not None else None,
                match_id,
                _match_scope_value(match_type, match),
            ),
        )
        await db.commit()

    normalized_result = dict(result)
    normalized_result.update({"success": True, "match_id": match_id_value})
    return normalized_result


async def _load_match_casters(match_type: str, match_id: int) -> list[str]:
    async with get_db() as db:
        if match_type == "group":
            cursor = await db.execute(
                """
                SELECT g.tournament_id
                FROM group_matches gm
                JOIN groups g ON g.id = gm.group_id
                WHERE gm.id = ?
                """,
                (match_id,),
            )
        else:
            cursor = await db.execute(
                "SELECT tournament_id FROM bracket_matches WHERE id = ?",
                (match_id,),
            )
        row = await cursor.fetchone()
        tournament_id = int(row["tournament_id"]) if row and row["tournament_id"] is not None else None

        if tournament_id is not None:
            cursor = await db.execute(
                """
                SELECT discord_id
                FROM tournament_casters
                WHERE tournament_id = ?
                ORDER BY assigned_at, discord_id
                """,
                (tournament_id,),
            )
            tournament_rows = await cursor.fetchall()
            if tournament_rows:
                return [str(caster_row["discord_id"]) for caster_row in tournament_rows if caster_row["discord_id"]]

        cursor = await db.execute(
            "SELECT discord_id FROM match_casters WHERE match_type = ? AND match_id = ? ORDER BY assigned_at, discord_id",
            (match_type, match_id),
        )
        rows = await cursor.fetchall()
    return [str(row["discord_id"]) for row in rows if row["discord_id"]]


async def fetch_match_result(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Holt das Match-Ergebnis ueber die Steam-Bridge und uebernimmt es ins Bracket."""
    return await _fetch_match_result_for_match("bracket", tournament_id, match_id)


async def fetch_group_match_result(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Holt das Match-Ergebnis ueber die Steam-Bridge und uebernimmt es ins Group-Match."""
    return await _fetch_match_result_for_match("group", tournament_id, match_id)


async def _fetch_match_result_for_match(
    match_type: str,
    tournament_id: int,
    match_id: int,
) -> dict[str, Any]:
    match = await _get_match(match_type, tournament_id, match_id)
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
        if match_type == "bracket":
            applied = await apply_bracket_match_result(
                tournament_id,
                match_id,
                winning_team=_coerce_optional_int(winning_team_raw, "winning_team"),
                winner_id=_coerce_optional_int(winner_id_raw, "winner_id"),
                duration_s=_coerce_optional_int(result.get("duration_s"), "duration_s"),
                players=result.get("players"),
                source="automatic",
            )
        else:
            applied = await _apply_group_match_result(
                tournament_id,
                match_id,
                winning_team=_coerce_optional_int(winning_team_raw, "winning_team"),
                winner_id=_coerce_optional_int(winner_id_raw, "winner_id"),
                deadlock_match_id=result.get("match_id") or result.get("deadlock_match_id"),
                duration_s=_coerce_optional_int(result.get("duration_s"), "duration_s"),
                players=result.get("players"),
                source="automatic",
            )
    except MatchResultError as exc:
        raise SteamTaskError(f"Ungueltige Steam-Ergebnisdaten: {exc}") from exc

    normalized_result = dict(result)
    normalized_result.update(applied)
    normalized_result["success"] = True
    return normalized_result


async def leave_lobby(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Laesst den Bot eine Bracket-Lobby verlassen."""
    return await _leave_lobby_for_match("bracket", tournament_id, match_id)


async def leave_group_lobby(tournament_id: int, match_id: int) -> dict[str, Any]:
    """Laesst den Bot eine Group-Lobby verlassen."""
    return await _leave_lobby_for_match("group", tournament_id, match_id)


async def _leave_lobby_for_match(
    match_type: str,
    tournament_id: int,
    match_id: int,
) -> dict[str, Any]:
    match = await _get_match(match_type, tournament_id, match_id)
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


async def _get_group_match(tournament_id: int, match_id: int) -> dict[str, Any]:
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT gm.id, gm.group_id, gm.team1_id, gm.team2_id, gm.winner_id, gm.status,
                   gm.steam_party_id, gm.party_code, gm.deadlock_match_id, gm.discord_channel_id,
                   gm.match_duration_s, gm.match_stats, gm.scheduled_at, gm.played_at,
                   g.tournament_id,
                   t1.name AS team1_name, t2.name AS team2_name
            FROM group_matches gm
            JOIN groups g ON g.id = gm.group_id
            LEFT JOIN teams t1 ON t1.id = gm.team1_id
            LEFT JOIN teams t2 ON t2.id = gm.team2_id
            WHERE gm.id = ? AND g.tournament_id = ?
            """,
            (match_id, tournament_id),
        )
        row = await cursor.fetchone()

    if not row:
        raise MatchNotFoundError(f"Group-Match {match_id} im Turnier {tournament_id} nicht gefunden")
    return dict(row)


async def _get_match(match_type: str, tournament_id: int, match_id: int) -> dict[str, Any]:
    if match_type == "group":
        return await _get_group_match(tournament_id, match_id)
    return await _get_bracket_match(tournament_id, match_id)


async def _get_party_id(match_type: str, tournament_id: int, match_id: int) -> str:
    match = await _get_match(match_type, tournament_id, match_id)
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


async def _is_test_tournament(tournament_id: int) -> bool:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT is_test FROM tournaments WHERE id = ?",
            (tournament_id,),
        )
        row = await cursor.fetchone()
    return bool(row["is_test"]) if row else False


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


async def _load_match_context(match_type: str, tournament_id: int, match_id: int) -> dict[str, Any]:
    match = await _get_match(match_type, tournament_id, match_id)
    return {
        "team1_name": match.get("team1_name") or f"Team {match['team1_id']}",
        "team2_name": match.get("team2_name") or f"Team {match['team2_id']}",
    }


async def _load_match_participants(match_type: str, tournament_id: int, match_id: int) -> list[dict[str, Any]]:
    match = await _get_match(match_type, tournament_id, match_id)
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


async def _load_match_participant_steam_ids(match_type: str, tournament_id: int, match_id: int) -> list[str]:
    match = await _get_match(match_type, tournament_id, match_id)
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


async def _ensure_no_duplicate_lobby_request(match_type: str, match_id: int, match: dict[str, Any]) -> None:
    if match.get("steam_party_id"):
        raise MatchStateError("Für dieses Match existiert bereits eine Lobby")
    if await steam_bridge.has_active_task(
        "GC_CREATE_CUSTOM_LOBBY",
        match_id=match_id,
        match_type=match_type,
    ):
        raise MatchStateError("Für dieses Match läuft bereits eine Lobby-Erstellung")


def _match_table(match_type: str) -> str:
    return "group_matches" if match_type == "group" else "bracket_matches"


def _match_scope_column(match_type: str) -> str:
    return "group_id" if match_type == "group" else "tournament_id"


def _match_scope_value(match_type: str, match: dict[str, Any]) -> int:
    return int(match["group_id"] if match_type == "group" else match["tournament_id"])


async def _apply_group_match_result(
    tournament_id: int,
    match_id: int,
    *,
    winning_team: int | None = None,
    winner_id: int | None = None,
    deadlock_match_id: Any = None,
    duration_s: int | None = None,
    players: Any = None,
    source: str = "manual",
) -> dict[str, Any]:
    async with get_db() as db:
        cursor = await db.execute(
            """
            SELECT gm.*, g.tournament_id
            FROM group_matches gm
            JOIN groups g ON g.id = gm.group_id
            WHERE gm.id = ? AND g.tournament_id = ?
            """,
            (match_id, tournament_id),
        )
        match = await cursor.fetchone()

        if not match:
            raise MatchNotFoundError(f"Group-Match {match_id} im Turnier {tournament_id} nicht gefunden")
        if match["status"] in {"completed", "cancelled", "forfeit"}:
            raise MatchStateError(
                f"Group-Match {match_id} kann aus Status {match['status']} nicht verarbeitet werden"
            )

        if winner_id is None:
            if winning_team == 1:
                winner_id = int(match["team1_id"])
            elif winning_team == 2:
                winner_id = int(match["team2_id"])

        if winner_id not in {match["team1_id"], match["team2_id"]}:
            raise MatchResultError("winner_id muss eines der beiden Teams im Match sein")

        winning_team_value = 1 if winner_id == match["team1_id"] else 2
        loser_id = match["team2_id"] if winning_team_value == 1 else match["team1_id"]
        match_stats = json.dumps({"players": players}, ensure_ascii=True) if players is not None else None

        await db.execute(
            """
            UPDATE group_matches
            SET winner_id = ?,
                status = 'completed',
                deadlock_match_id = COALESCE(?, deadlock_match_id),
                match_duration_s = COALESCE(?, match_duration_s),
                match_stats = COALESCE(?, match_stats),
                played_at = datetime('now')
            WHERE id = ?
            """,
            (
                winner_id,
                str(deadlock_match_id) if deadlock_match_id is not None else None,
                duration_s,
                match_stats,
                match_id,
            ),
        )

        await db.execute(
            "UPDATE group_teams SET wins = wins + 1, points = points + 3 "
            "WHERE group_id = ? AND team_id = ?",
            (match["group_id"], winner_id),
        )
        await db.execute(
            "UPDATE group_teams SET losses = losses + 1 "
            "WHERE group_id = ? AND team_id = ?",
            (match["group_id"], loser_id),
        )
        await db.execute("DELETE FROM match_results WHERE group_match_id = ?", (match_id,))
        await db.execute(
            "INSERT INTO match_results (group_match_id, winning_team, duration_s, player_stats, source) "
            "VALUES (?, ?, ?, ?, ?)",
            (
                match_id,
                winning_team_value,
                duration_s,
                json.dumps(players, ensure_ascii=True) if players is not None else None,
                source,
            ),
        )
        await db.commit()

    return {
        "match_id": match_id,
        "winner_id": int(winner_id),
        "winning_team": winning_team_value,
        "duration_s": duration_s,
        "source": source,
    }


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
