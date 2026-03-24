"""Discord OAuth2 Flow — Login, Callback, Logout."""
from __future__ import annotations

import secrets
from datetime import datetime, timedelta, timezone
from urllib.parse import urlencode

import httpx
from fastapi import APIRouter, HTTPException, Request, Response, status
from fastapi.responses import RedirectResponse

from config import settings
from db import get_db

router = APIRouter(prefix="/auth/discord", tags=["auth"])

DISCORD_API = "https://discord.com/api/v10"
DISCORD_AUTHORIZE_URL = "https://discord.com/api/oauth2/authorize"
DISCORD_TOKEN_URL = "https://discord.com/api/oauth2/token"

SESSION_LIFETIME = timedelta(days=7)


@router.get("/login")
async def discord_login() -> RedirectResponse:
    """Redirect zu Discord authorize URL."""
    params = {
        "client_id": settings.DISCORD_CLIENT_ID,
        "redirect_uri": settings.DISCORD_REDIRECT_URI,
        "response_type": "code",
        "scope": "identify guilds.members.read",
    }
    return RedirectResponse(url=f"{DISCORD_AUTHORIZE_URL}?{urlencode(params)}")


@router.get("/callback")
async def discord_callback(code: str | None = None, error: str | None = None) -> RedirectResponse:
    """Token-Exchange, User-Info laden, Session erstellen, Redirect zum Frontend."""
    if error or not code:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Discord OAuth Fehler: {error or 'Kein Code erhalten'}",
        )

    # --- Token Exchange ---
    async with httpx.AsyncClient() as client:
        token_resp = await client.post(
            DISCORD_TOKEN_URL,
            data={
                "client_id": settings.DISCORD_CLIENT_ID,
                "client_secret": settings.DISCORD_CLIENT_SECRET,
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": settings.DISCORD_REDIRECT_URI,
            },
            headers={"Content-Type": "application/x-www-form-urlencoded"},
        )

    if token_resp.status_code != 200:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail="Discord Token-Exchange fehlgeschlagen",
        )

    token_data = token_resp.json()
    access_token = token_data["access_token"]

    # --- User Info ---
    headers = {"Authorization": f"Bearer {access_token}"}
    async with httpx.AsyncClient() as client:
        user_resp = await client.get(f"{DISCORD_API}/users/@me", headers=headers)
        if user_resp.status_code != 200:
            raise HTTPException(status_code=status.HTTP_502_BAD_GATEWAY, detail="User-Info Fehler")
        user_data = user_resp.json()

        # --- Guild Member (Rollen laden) ---
        roles: list[str] = []
        if settings.DISCORD_GUILD_ID:
            member_resp = await client.get(
                f"{DISCORD_API}/users/@me/guilds/{settings.DISCORD_GUILD_ID}/member",
                headers=headers,
            )
            if member_resp.status_code == 200:
                member_data = member_resp.json()
                roles = member_data.get("roles", [])

    discord_id = user_data["id"]
    discord_name = user_data.get("global_name") or user_data.get("username", "")
    avatar = user_data.get("avatar", "")

    # Avatar-URL zusammenbauen
    discord_avatar = ""
    if avatar:
        ext = "gif" if avatar.startswith("a_") else "png"
        discord_avatar = f"https://cdn.discordapp.com/avatars/{discord_id}/{avatar}.{ext}"

    # --- Session erstellen ---
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

    # Redirect zum Frontend mit Session-Cookie
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
    response: Response,
    session_token: str | None = None,
) -> dict[str, str]:
    """Session loeschen und Cookie entfernen."""
    if session_token:
        async with get_db() as db:
            await db.execute("DELETE FROM sessions WHERE token = ?", (session_token,))
            await db.commit()

    response.delete_cookie(key="session_token", path="/")
    return {"status": "logged_out"}
