"""Discord OAuth delegation via Deadlock-Bots."""
from __future__ import annotations

import secrets
from datetime import datetime, timedelta, timezone

import httpx
from fastapi import APIRouter, Cookie, HTTPException, status
from fastapi.responses import RedirectResponse

from config import settings
from db import get_db

router = APIRouter(prefix="/auth/discord", tags=["auth"])

INTERNAL_TOKEN_HEADER = "X-Internal-Token"
AUTHORIZE_URL_PATH = "/internal/turnier/v1/discord/authorize-url"
SESSION_PATH = "/internal/turnier/v1/discord/session"
INTERNAL_API_TIMEOUT = httpx.Timeout(20.0, connect=5.0)
SESSION_LIFETIME = timedelta(days=7)


def _internal_api_base_url() -> str:
    base_url = settings.DISCORD_OAUTH_INTERNAL_API_BASE_URL.rstrip("/")
    if base_url:
        return base_url
    raise HTTPException(
        status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
        detail="Deadlock-Bots OAuth-Service ist nicht konfiguriert",
    )


def _internal_api_headers() -> dict[str, str]:
    token = settings.DISCORD_OAUTH_INTERNAL_API_TOKEN.strip()
    if not token:
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Deadlock-Bots OAuth-Service ist nicht authentifiziert",
        )
    return {INTERNAL_TOKEN_HEADER: token}


async def _post_internal_api(path: str, payload: dict[str, str]) -> dict:
    url = f"{_internal_api_base_url()}{path}"
    try:
        async with httpx.AsyncClient(timeout=INTERNAL_API_TIMEOUT, follow_redirects=False) as client:
            response = await client.post(url, json=payload, headers=_internal_api_headers())
    except httpx.HTTPError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=f"Deadlock-Bots OAuth-Service nicht erreichbar: {exc.__class__.__name__}",
        ) from exc

    if response.status_code != status.HTTP_200_OK:
        detail = "Deadlock-Bots OAuth-Service Fehler"
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
        raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail=detail)

    try:
        data = response.json()
    except ValueError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail="Deadlock-Bots OAuth-Service lieferte kein JSON",
        ) from exc
    if not isinstance(data, dict):
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail="Deadlock-Bots OAuth-Service lieferte ein ungültiges Payload",
        )
    return data


@router.get("/login")
async def discord_login() -> RedirectResponse:
    """Redirect to the centralized Discord OAuth service on Deadlock-Bots."""
    data = await _post_internal_api(
        AUTHORIZE_URL_PATH,
        {
            "redirect_uri": settings.DISCORD_REDIRECT_URI,
            "scope": "identify guilds.members.read",
        },
    )
    authorize_url = str(data.get("authorize_url") or "").strip()
    if not authorize_url:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail="Deadlock-Bots OAuth-Service lieferte keine Authorize-URL",
        )
    return RedirectResponse(url=authorize_url)


@router.get("/callback")
async def discord_callback(code: str | None = None, error: str | None = None) -> RedirectResponse:
    """Exchange the Discord code through Deadlock-Bots and create a local session."""
    if error or not code:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Discord OAuth Fehler: {error or 'Kein Code erhalten'}",
        )

    data = await _post_internal_api(
        SESSION_PATH,
        {
            "code": code,
            "redirect_uri": settings.DISCORD_REDIRECT_URI,
            "guild_id": settings.DISCORD_GUILD_ID,
        },
    )

    discord_id = str(data.get("discord_id") or "").strip()
    if not discord_id:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail="Deadlock-Bots OAuth-Service lieferte keine Discord-ID",
        )

    discord_name = str(data.get("discord_name") or "").strip()
    discord_avatar = str(data.get("discord_avatar") or "").strip()
    raw_roles = data.get("discord_roles") or []
    roles = [str(role).strip() for role in raw_roles if str(role).strip()]

    session_token = secrets.token_urlsafe(48)
    expires_at = datetime.now(timezone.utc) + SESSION_LIFETIME

    async with get_db() as db:
        await db.execute(
            "INSERT INTO sessions (token, discord_id, discord_name, discord_avatar, discord_roles, expires_at) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (
                session_token,
                discord_id,
                discord_name,
                discord_avatar,
                ",".join(roles),
                expires_at.isoformat(),
            ),
        )
        await db.commit()

    response = RedirectResponse(url=settings.FRONTEND_URL, status_code=status.HTTP_302_FOUND)
    response.set_cookie(
        key="session_token",
        value=session_token,
        httponly=True,
        secure=True,
        samesite="lax",
        max_age=int(SESSION_LIFETIME.total_seconds()),
        path="/",
    )
    return response


@router.get("/logout")
async def discord_logout(
    session_token: str | None = Cookie(None),
) -> RedirectResponse:
    """Delete the local session cookie created for the tournament frontend."""
    if session_token:
        async with get_db() as db:
            await db.execute("DELETE FROM sessions WHERE token = ?", (session_token,))
            await db.commit()

    response = RedirectResponse(url=settings.FRONTEND_URL, status_code=status.HTTP_302_FOUND)
    response.delete_cookie(key="session_token", path="/")
    return response
