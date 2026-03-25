"""Auth Middleware — Session-basierte Authentifizierung via Cookie oder Header."""
from __future__ import annotations

from datetime import datetime, timezone

from fastapi import Cookie, Depends, Header, HTTPException, status

from config import settings
from db import get_db
from tournament.models import UserSession


async def get_current_user(
    authorization: str | None = Header(None),
    session_token: str | None = Cookie(None),
) -> UserSession:
    """Liest Session-Token aus Cookie oder Authorization Header und gibt UserSession zurück."""
    token: str | None = None

    # Token aus Authorization Header extrahieren
    if authorization and authorization.startswith("Bearer "):
        token = authorization[7:]

    # Fallback: Cookie
    if not token:
        token = session_token

    if not token:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Nicht authentifiziert",
        )

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT discord_id, discord_name, discord_avatar, discord_roles, expires_at "
            "FROM sessions WHERE token = ?",
            (token,),
        )
        row = await cursor.fetchone()

    if not row:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Session ungültig oder abgelaufen",
        )

    # Ablauf prüfen
    expires_at = datetime.fromisoformat(row["expires_at"])
    if expires_at.tzinfo is None:
        expires_at = expires_at.replace(tzinfo=timezone.utc)
    if datetime.now(timezone.utc) > expires_at:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Session abgelaufen",
        )

    # Rollen parsen (komma-separiert gespeichert)
    roles_str: str = row["discord_roles"] or ""
    roles = [r.strip() for r in roles_str.split(",") if r.strip()]

    # Admin/Mod Check
    role_set = set(roles)
    is_admin = bool(role_set & settings.admin_role_ids)
    is_mod = is_admin or bool(role_set & settings.mod_role_ids)

    return UserSession(
        discord_id=row["discord_id"],
        discord_name=row["discord_name"],
        discord_avatar=row["discord_avatar"],
        roles=roles,
        is_admin=is_admin,
        is_mod=is_mod,
    )
