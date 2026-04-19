from __future__ import annotations

import asyncio
import json
import logging
import re
import unicodedata
from datetime import datetime, timezone
from typing import Any

import httpx
from fastapi import HTTPException, status

from config import settings
from db import get_db

logger = logging.getLogger(__name__)

INTERNAL_TOKEN_HEADER = "X-Internal-Token"
INTERNAL_API_TIMEOUT = httpx.Timeout(20.0, connect=5.0)
_NOTIFICATION_EVENT_COLUMNS = {
    "match_start": "notify_match_start",
    "checkin": "notify_checkin",
    "team_invite": "notify_team_invite",
    "tournament_news": "notify_tournament_news",
    "registration_reminder": "notify_registration_reminder",
}
_NOTIFICATION_DEFAULTS = {
    "notify_match_start": True,
    "notify_checkin": True,
    "notify_team_invite": True,
    "notify_tournament_news": False,
    "notify_registration_reminder": True,
}


def _slugify(value: str) -> str:
    normalized = unicodedata.normalize("NFKD", value)
    ascii_value = normalized.encode("ascii", "ignore").decode("ascii")
    cleaned = re.sub(r"[^a-zA-Z0-9]+", "-", ascii_value).strip("-")
    cleaned = re.sub(r"-{2,}", "-", cleaned)
    return cleaned.lower() or "team"


def _build_match_channel_name(team1_name: str, team2_name: str) -> str:
    name = f"match-{_slugify(team1_name)}-vs-{_slugify(team2_name)}"
    return name[:100]


def _broker_base_url() -> str:
    base_url = settings.DISCORD_MASTER_BROKER_BASE_URL.rstrip("/")
    if not base_url:
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Discord-Master-Broker ist nicht konfiguriert",
        )
    return base_url


def _broker_headers() -> dict[str, str]:
    token = settings.DISCORD_MASTER_BROKER_TOKEN.strip()
    if not token:
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Discord-Master-Broker ist nicht authentifiziert",
        )
    return {INTERNAL_TOKEN_HEADER: token}


async def _post_internal_api(path: str, payload: dict[str, Any]) -> dict[str, Any]:
    url = f"{_broker_base_url()}{path}"
    try:
        async with httpx.AsyncClient(timeout=INTERNAL_API_TIMEOUT, follow_redirects=False) as client:
            response = await client.post(url, json=payload, headers=_broker_headers())
    except httpx.HTTPError as exc:
        raise RuntimeError(f"Discord-Broker nicht erreichbar: {exc.__class__.__name__}") from exc

    if response.status_code != status.HTTP_200_OK:
        detail = "Discord-Broker Fehler"
        try:
            body = response.json()
        except ValueError:
            body = None
        if isinstance(body, dict):
            error = str(body.get("error") or body.get("detail") or "").strip()
            if error:
                detail = error
        elif response.text.strip():
            detail = response.text.strip()
        raise RuntimeError(detail)

    try:
        data = response.json()
    except ValueError as exc:
        raise RuntimeError("Discord-Broker lieferte kein JSON") from exc
    if not isinstance(data, dict):
        raise RuntimeError("Discord-Broker lieferte ein ungültiges Payload")
    return data


async def _create_task(task_type: str, payload: dict[str, Any]) -> int:
    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        cursor = await db.execute(
            """
            INSERT INTO discord_tasks (type, payload, status, created_at, updated_at)
            VALUES (?, ?, 'PENDING', ?, ?)
            """,
            (task_type, json.dumps(payload, default=str), now, now),
        )
        await db.commit()
        return int(cursor.lastrowid)


async def _update_task(
    task_id: int,
    *,
    status_value: str,
    result_payload: dict[str, Any] | None = None,
    error: str | None = None,
) -> None:
    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        await db.execute(
            """
            UPDATE discord_tasks
            SET status = ?,
                result_payload = ?,
                error = ?,
                updated_at = ?
            WHERE id = ?
            """,
            (
                status_value,
                json.dumps(result_payload, default=str) if result_payload is not None else None,
                error,
                now,
                task_id,
            ),
        )
        await db.commit()


def _unique_preserve_order(values: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for raw in values:
        value = str(raw).strip()
        if not value or value in seen:
            continue
        seen.add(value)
        result.append(value)
    return result


async def create_match_channel(match_id: int, team1_name: str, team2_name: str) -> str:
    task_id = await _create_task(
        "CREATE_CHANNEL",
        {"match_id": match_id, "team1_name": team1_name, "team2_name": team2_name},
    )
    await _update_task(task_id, status_value="RUNNING")
    payload = {
        "name": _build_match_channel_name(team1_name, team2_name),
        "category_id": settings.DISCORD_MATCH_CHANNEL_CATEGORY_ID,
        "topic": f"Match {match_id}: {team1_name} vs {team2_name}",
    }
    try:
        result = await _post_internal_api("/internal/master/v1/discord/create-channel", payload)
        channel_id = str(result.get("channel_id") or "").strip()
        if not channel_id:
            raise RuntimeError("Discord-Broker lieferte keine channel_id")
        await _update_task(task_id, status_value="DONE", result_payload={"channel_id": channel_id})
        return channel_id
    except Exception as exc:
        await _update_task(task_id, status_value="FAILED", error=str(exc))
        raise


async def send_match_lobby_info(
    channel_id: str | int,
    party_code: str,
    participant_discord_ids: list[str],
) -> dict[str, Any]:
    participant_ids = _unique_preserve_order(participant_discord_ids)
    task_id = await _create_task(
        "SEND_MATCH_INFO",
        {
            "channel_id": str(channel_id),
            "party_code": party_code,
            "participant_discord_ids": participant_ids,
        },
    )
    await _update_task(task_id, status_value="RUNNING")
    mention_line = " ".join(f"<@{discord_id}>" for discord_id in participant_ids)
    embed = {
        "title": "Lobby wurde erstellt",
        "description": "Die Steam-Lobby ist bereit.",
        "fields": [
            {"name": "Lobby-Code", "value": f"`{party_code}`", "inline": False},
            {
                "name": "Teilnehmer",
                "value": mention_line or "Keine Teilnehmer mit Discord-ID",
                "inline": False,
            },
        ],
    }
    payload = {
        "channel_id": int(channel_id),
        "content": mention_line or None,
        "embed": embed,
        "allowed_user_ids": participant_ids,
    }
    try:
        result = await _post_internal_api("/internal/master/v1/discord/send-rich-message", payload)
        await _update_task(task_id, status_value="DONE", result_payload=result)
        return result
    except Exception as exc:
        await _update_task(task_id, status_value="FAILED", error=str(exc))
        raise


async def delete_match_channel(channel_id: str | int) -> dict[str, Any]:
    task_id = await _create_task("DELETE_CHANNEL", {"channel_id": str(channel_id)})
    await _update_task(task_id, status_value="RUNNING")
    payload = {"channel_id": int(channel_id)}
    try:
        result = await _post_internal_api("/internal/master/v1/discord/delete-channel", payload)
        await _update_task(task_id, status_value="DONE", result_payload=result)
        return result
    except Exception as exc:
        await _update_task(task_id, status_value="FAILED", error=str(exc))
        raise


async def notify_users(
    discord_ids: list[str],
    event_type: str,
    message: str,
) -> dict[str, Any]:
    column_name = _NOTIFICATION_EVENT_COLUMNS.get(event_type)
    if column_name is None:
        raise ValueError(f"Unbekannter notification event_type: {event_type}")

    unique_ids = _unique_preserve_order(discord_ids)
    if not unique_ids:
        return {"sent": [], "skipped": [], "failed": []}

    placeholder_sql = ",".join("?" for _ in unique_ids)
    async with get_db() as db:
        cursor = await db.execute(
            f"""
            SELECT discord_id,
                   notify_discord_dm,
                   notify_match_start,
                   notify_checkin,
                   notify_team_invite,
                   notify_tournament_news,
                   notify_registration_reminder
            FROM user_profiles
            WHERE discord_id IN ({placeholder_sql})
            """,
            unique_ids,
        )
        rows = await cursor.fetchall()

    profile_flags: dict[str, dict[str, bool]] = {}
    for row in rows:
        values = dict(row)
        profile_flags[str(values["discord_id"])] = {
            "notify_discord_dm": bool(values.get("notify_discord_dm", 1)),
            "notify_match_start": bool(values.get("notify_match_start", 1)),
            "notify_checkin": bool(values.get("notify_checkin", 1)),
            "notify_team_invite": bool(values.get("notify_team_invite", 1)),
            "notify_tournament_news": bool(values.get("notify_tournament_news", 0)),
            "notify_registration_reminder": bool(values.get("notify_registration_reminder", 1)),
        }

    default_flag = _NOTIFICATION_DEFAULTS[column_name]
    summary = {"sent": [], "skipped": [], "failed": []}

    for discord_id in unique_ids:
        flags = profile_flags.get(discord_id)
        notify_discord_dm = default_flag if flags is None else flags["notify_discord_dm"]
        notify_event = default_flag if flags is None else flags[column_name]
        if not notify_discord_dm or not notify_event:
            summary["skipped"].append(discord_id)
            continue

        task_id = await _create_task(
            "SEND_DM",
            {"discord_id": discord_id, "event_type": event_type, "message": message},
        )
        await _update_task(task_id, status_value="RUNNING")
        try:
            result = await _post_internal_api(
                "/internal/master/v1/discord/send-message",
                {"user_id": int(discord_id), "content": message},
            )
            await _update_task(task_id, status_value="DONE", result_payload=result)
            summary["sent"].append(discord_id)
        except Exception as exc:
            await _update_task(task_id, status_value="FAILED", error=str(exc))
            summary["failed"].append({"discord_id": discord_id, "error": str(exc)})

    return summary


async def delete_match_channel_later(channel_id: str | int, delay_seconds: float = 300.0) -> None:
    await asyncio.sleep(max(0.0, delay_seconds))
    try:
        await delete_match_channel(channel_id)
    except Exception:
        logger.exception("Delayed deletion of Discord match channel %s failed", channel_id)


async def move_users_to_voice_channel(
    discord_ids: list[str],
    channel_id: int,
    *,
    guild_id: int,
) -> dict[str, Any]:
    """Verschiebt eine Liste von Discord-Usern in einen Voice-Kanal."""
    import secrets as _secrets

    unique_ids = _unique_preserve_order(discord_ids)
    results: dict[str, Any] = {"moved": [], "failed": []}
    for discord_id in unique_ids:
        payload = {
            "guild_id": guild_id,
            "user_id": int(discord_id),
            "channel_id": channel_id,
            "idempotency_key": f"move-{discord_id}-{channel_id}-{_secrets.token_hex(4)}",
        }
        try:
            await _post_internal_api("/internal/master/v1/discord/member/move-voice", payload)
            results["moved"].append(discord_id)
        except Exception as exc:
            logger.warning("move_voice failed for %s: %s", discord_id, exc)
            results["failed"].append({"discord_id": discord_id, "error": str(exc)})
    return results


async def get_voice_channel_members(channel_id: int) -> list[dict[str, Any]]:
    """Gibt die aktuellen Mitglieder eines Voice-Kanals zurück."""
    result = await _post_internal_api(
        "/internal/master/v1/discord/voice-channel/members",
        {"channel_id": channel_id},
    )
    return result.get("members") or []


async def get_role_members(guild_id: int, role_id: int) -> list[dict[str, Any]]:
    """Lädt Guild-Mitglieder mit einer bestimmten Rolle über den Broker."""
    result = await _post_internal_api(
        "/internal/master/v1/discord/role/members",
        {"guild_id": guild_id, "role_id": role_id},
    )
    members = result.get("members")
    return members if isinstance(members, list) else []


async def send_lobby_announcement(
    *,
    match_id: int,
    party_code: str,
    team1_name: str,
    team2_name: str,
    team1_discord_ids: list[str],
    team2_discord_ids: list[str],
) -> dict[str, Any]:
    """Postet Lobby-Code + Team-Zuordnung in den zentralen Turnier-Kanal."""
    import secrets as _sec
    from config import settings as _settings

    team1_mentions = " ".join(f"<@{uid}>" for uid in team1_discord_ids) or "—"
    team2_mentions = " ".join(f"<@{uid}>" for uid in team2_discord_ids) or "—"
    all_ids = [int(uid) for uid in team1_discord_ids + team2_discord_ids if uid]

    embed = {
        "title": f"Match {match_id} — Lobby bereit",
        "fields": [
            {"name": "Lobby-Code", "value": f"`{party_code}`", "inline": False},
            {"name": f"🔵 {team1_name}", "value": team1_mentions, "inline": True},
            {"name": f"🔴 {team2_name}", "value": team2_mentions, "inline": True},
        ],
    }
    content = " ".join(f"<@{uid}>" for uid in team1_discord_ids + team2_discord_ids if uid) or None

    payload = {
        "channel_id": _settings.DISCORD_TOURNAMENT_LOBBY_CHANNEL_ID,
        "content": content,
        "embed": embed,
        "allowed_user_ids": all_ids,
        "idempotency_key": f"lobby-ann-{match_id}-{_sec.token_hex(4)}",
    }
    return await _post_internal_api("/internal/master/v1/discord/send-rich-message", payload)


async def send_match_stats_to_channel(
    channel_id: str | int,
    *,
    match_id: int,
    deadlock_match_id: str | None,
    team1_name: str,
    team2_name: str,
    winner_name: str,
    duration_s: int | None,
    player_stats: list[dict] | None,
) -> dict[str, Any]:
    """Postet Match-Stats (Ergebnis + K/D/A) in den Discord-Match-Channel."""
    import secrets as _sec

    duration_str = f"{duration_s // 60}m {duration_s % 60}s" if duration_s else "unbekannt"

    stats_lines = []
    if player_stats:
        for p in player_stats[:12]:
            name = p.get("hero") or p.get("player_name") or p.get("discord_name") or "?"
            kills = p.get("kills", 0)
            deaths = p.get("deaths", 0)
            assists = p.get("assists", 0)
            stats_lines.append(f"**{name}** — {kills}/{deaths}/{assists}")

    embed = {
        "title": f"Match {match_id} — Ergebnis",
        "description": f"**Sieger: {winner_name}**\nDauer: {duration_str}",
        "fields": [
            {"name": "Match ID (Deadlock)", "value": deadlock_match_id or "—", "inline": True},
            {"name": "Teams", "value": f"{team1_name} vs {team2_name}", "inline": True},
        ],
    }
    if stats_lines:
        embed["fields"].append(
            {
                "name": "Spieler-Stats (K/D/A)",
                "value": "\n".join(stats_lines[:10]),
                "inline": False,
            }
        )

    payload = {
        "channel_id": int(channel_id),
        "content": None,
        "embed": embed,
        "allowed_user_ids": [],
        "idempotency_key": f"stats-{match_id}-{_sec.token_hex(4)}",
    }
    return await _post_internal_api("/internal/master/v1/discord/send-rich-message", payload)


async def notify_casters_match_created(
    match_id: int,
    channel_id: str | int,
    caster_discord_ids: list[str],
) -> dict[str, Any]:
    """Informiert zugewiesene Caster per DM und erwähnt sie im Match-Channel."""
    unique_ids = _unique_preserve_order(caster_discord_ids)
    summary = {"sent": [], "failed": []}
    channel_url = (
        f"https://discord.com/channels/{settings.DISCORD_GUILD_ID}/{int(channel_id)}"
        if settings.DISCORD_GUILD_ID
        else None
    )

    for discord_id in unique_ids:
        try:
            await _post_internal_api(
                "/internal/master/v1/discord/send-message",
                {
                    "user_id": int(discord_id),
                    "content": (
                        f"Du bist als Caster für Match #{match_id} eingetragen. "
                        f"Match-Channel: {channel_url or f'#{channel_id}'}"
                    ),
                },
            )
            summary["sent"].append(discord_id)
        except Exception as exc:
            summary["failed"].append({"discord_id": discord_id, "error": str(exc)})

    if unique_ids:
        await _post_internal_api(
            "/internal/master/v1/discord/send-rich-message",
            {
                "channel_id": int(channel_id),
                "content": " ".join(f"<@{discord_id}>" for discord_id in unique_ids),
                "embed": {
                    "title": "Caster informiert",
                    "description": "Die zugewiesenen Caster wurden benachrichtigt.",
                },
                "allowed_user_ids": [int(discord_id) for discord_id in unique_ids],
            },
        )

    return summary
