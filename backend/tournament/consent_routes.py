"""Consent und User-Profil Routes."""
from __future__ import annotations

from datetime import datetime, timezone

from fastapi import APIRouter, Depends, HTTPException

from auth.permissions import require_auth
from db import get_db
from tournament.models import (
    ConsentCreate,
    ConsentStatus,
    UserProfile,
    UserProfileUpdate,
    UserSession,
)

router = APIRouter(prefix="/api", tags=["consent"])


@router.get("/consent")
async def get_consent(user: UserSession = Depends(require_auth)) -> ConsentStatus:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT consented_at, consent_version FROM user_consents WHERE discord_id = ?",
            (user.discord_id,),
        )
        row = await cursor.fetchone()
    if not row:
        return ConsentStatus(has_consent=False)
    return ConsentStatus(
        has_consent=True,
        consented_at=row["consented_at"],
        consent_version=row["consent_version"],
    )


@router.post("/consent", status_code=201)
async def set_consent(
    body: ConsentCreate,
    user: UserSession = Depends(require_auth),
) -> ConsentStatus:
    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        await db.execute(
            "INSERT OR REPLACE INTO user_consents (discord_id, consented_at, consent_version) VALUES (?, ?, ?)",
            (user.discord_id, now, body.consent_version),
        )
        await db.commit()
    return ConsentStatus(
        has_consent=True,
        consented_at=now,
        consent_version=body.consent_version,
    )


@router.get("/profile")
async def get_my_profile(user: UserSession = Depends(require_auth)) -> UserProfile:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM user_profiles WHERE discord_id = ?",
            (user.discord_id,),
        )
        row = await cursor.fetchone()
    if not row:
        return UserProfile(discord_id=user.discord_id)
    return UserProfile(**dict(row))


@router.put("/profile")
async def update_my_profile(
    body: UserProfileUpdate,
    user: UserSession = Depends(require_auth),
) -> UserProfile:
    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT discord_id FROM user_profiles WHERE discord_id = ?",
            (user.discord_id,),
        )
        existing = await cursor.fetchone()
        if existing:
            updates: dict[str, object] = {"updated_at": now}
            if body.bio is not None:
                if len(body.bio) > 1000:
                    raise HTTPException(
                        status_code=400,
                        detail="Bio darf maximal 1000 Zeichen lang sein",
                    )
                updates["bio"] = body.bio
            if body.invite_auto_accept is not None:
                updates["invite_auto_accept"] = 1 if body.invite_auto_accept else 0
            if body.notify_discord_dm is not None:
                updates["notify_discord_dm"] = 1 if body.notify_discord_dm else 0
            if body.notify_browser is not None:
                updates["notify_browser"] = 1 if body.notify_browser else 0
            set_clause = ", ".join(f"{key} = ?" for key in updates)
            await db.execute(
                f"UPDATE user_profiles SET {set_clause} WHERE discord_id = ?",
                (*updates.values(), user.discord_id),
            )
        else:
            bio = body.bio or ""
            if len(bio) > 1000:
                raise HTTPException(
                    status_code=400,
                    detail="Bio darf maximal 1000 Zeichen lang sein",
                )
            await db.execute(
                "INSERT INTO user_profiles (discord_id, bio, invite_auto_accept, notify_discord_dm, notify_browser, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
                (
                    user.discord_id,
                    bio if bio else None,
                    1 if body.invite_auto_accept else 0,
                    1 if (body.notify_discord_dm is None or body.notify_discord_dm) else 0,
                    1 if body.notify_browser else 0,
                    now,
                ),
            )
        await db.commit()
        cursor = await db.execute(
            "SELECT * FROM user_profiles WHERE discord_id = ?",
            (user.discord_id,),
        )
        row = await cursor.fetchone()
    return UserProfile(**dict(row))
