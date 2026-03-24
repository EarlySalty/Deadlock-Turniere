"""Role-based Access Control — Dependencies fuer FastAPI-Routen."""
from __future__ import annotations

from fastapi import Depends, HTTPException, status

from auth.middleware import get_current_user
from tournament.models import UserSession


async def require_auth(
    user: UserSession = Depends(get_current_user),
) -> UserSession:
    """Muss eingeloggt sein."""
    return user


async def require_mod(
    user: UserSession = Depends(get_current_user),
) -> UserSession:
    """Muss Moderator oder Admin sein."""
    if not user.is_mod:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Moderator-Berechtigung erforderlich",
        )
    return user


async def require_admin(
    user: UserSession = Depends(get_current_user),
) -> UserSession:
    """Muss Admin sein."""
    if not user.is_admin:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Admin-Berechtigung erforderlich",
        )
    return user
