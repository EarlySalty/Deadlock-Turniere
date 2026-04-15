"""Consent und User-Profil Routes."""
from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

from fastapi import APIRouter, Depends, File, HTTPException, UploadFile, status
from fastapi.responses import FileResponse, RedirectResponse

from auth.permissions import require_auth
from config import settings
from db import get_db
from tournament.models import (
    ConsentCreate,
    ConsentStatus,
    UserProfile,
    UserProfileUpdate,
    UserSession,
)

router = APIRouter(prefix="/api", tags=["consent"])

_MAX_AVATAR_SIZE = 2 * 1024 * 1024
_AVATAR_MEDIA_TYPES = {
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".png": "image/png",
    ".webp": "image/webp",
}


def _normalize_display_name(display_name: str) -> str:
    value = display_name.strip()
    if not value:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Display Name darf nicht leer sein",
        )
    if len(value) > 32:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Display Name darf maximal 32 Zeichen lang sein",
        )
    if any(ord(char) < 32 for char in value):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Display Name enthält ungültige Steuerzeichen",
        )
    return value


def _detect_avatar_format(data: bytes) -> tuple[str, str]:
    if data.startswith(b"\xff\xd8\xff"):
        return ".jpg", _AVATAR_MEDIA_TYPES[".jpg"]
    if data.startswith(b"\x89PNG\r\n\x1a\n"):
        return ".png", _AVATAR_MEDIA_TYPES[".png"]
    if len(data) >= 12 and data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        return ".webp", _AVATAR_MEDIA_TYPES[".webp"]
    raise HTTPException(
        status_code=status.HTTP_400_BAD_REQUEST,
        detail="Erlaubt sind nur JPG, PNG oder WEBP Bilder",
    )


def _avatar_file_path(discord_id: str) -> Path | None:
    avatar_dir = Path(settings.AVATAR_DIR)
    for extension in (".jpg", ".jpeg", ".png", ".webp"):
        candidate = avatar_dir / f"{discord_id}{extension}"
        if candidate.exists():
            return candidate
    return None


def _serialize_profile_row(row: dict[str, object], user: UserSession) -> UserProfile:
    payload = dict(row)
    if not payload.get("display_name"):
        payload["display_name"] = user.discord_name
    return UserProfile(**payload)


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
        return UserProfile(discord_id=user.discord_id, display_name=user.discord_name)
    return _serialize_profile_row(dict(row), user)


@router.put("/profile")
async def update_my_profile(
    body: UserProfileUpdate,
    user: UserSession = Depends(require_auth),
) -> UserProfile:
    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT * FROM user_profiles WHERE discord_id = ?",
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
            if body.display_name is not None:
                updates["display_name"] = _normalize_display_name(body.display_name)
            if body.avatar_filename is not None:
                updates["avatar_filename"] = body.avatar_filename
            if body.notify_match_start is not None:
                updates["notify_match_start"] = 1 if body.notify_match_start else 0
            if body.notify_checkin is not None:
                updates["notify_checkin"] = 1 if body.notify_checkin else 0
            if body.notify_team_invite is not None:
                updates["notify_team_invite"] = 1 if body.notify_team_invite else 0
            if body.notify_tournament_news is not None:
                updates["notify_tournament_news"] = 1 if body.notify_tournament_news else 0
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
            display_name = (
                _normalize_display_name(body.display_name)
                if body.display_name is not None
                else None
            )
            await db.execute(
                "INSERT INTO user_profiles (discord_id, bio, invite_auto_accept, notify_discord_dm, notify_browser, display_name, avatar_filename, notify_match_start, notify_checkin, notify_team_invite, notify_tournament_news, updated_at) "
                "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    user.discord_id,
                    bio if bio else None,
                    1 if body.invite_auto_accept else 0,
                    1 if (body.notify_discord_dm is None or body.notify_discord_dm) else 0,
                    1 if body.notify_browser else 0,
                    display_name,
                    body.avatar_filename,
                    1 if (body.notify_match_start is None or body.notify_match_start) else 0,
                    1 if (body.notify_checkin is None or body.notify_checkin) else 0,
                    1 if (body.notify_team_invite is None or body.notify_team_invite) else 0,
                    1 if body.notify_tournament_news else 0,
                    now,
                ),
            )
        await db.commit()
        cursor = await db.execute(
            "SELECT * FROM user_profiles WHERE discord_id = ?",
            (user.discord_id,),
        )
        row = await cursor.fetchone()
    return _serialize_profile_row(dict(row), user)


@router.post("/profile/avatar")
async def upload_profile_avatar(
    avatar: UploadFile = File(...),
    user: UserSession = Depends(require_auth),
) -> UserProfile:
    data = await avatar.read()
    if len(data) > _MAX_AVATAR_SIZE:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Avatar darf maximal 2 MB groß sein",
        )

    extension, _media_type = _detect_avatar_format(data)
    avatar_dir = Path(settings.AVATAR_DIR)
    avatar_dir.mkdir(parents=True, exist_ok=True)
    avatar_path = avatar_dir / f"{user.discord_id}{extension}"

    for existing in avatar_dir.glob(f"{user.discord_id}.*"):
        if existing != avatar_path:
            existing.unlink(missing_ok=True)

    avatar_path.write_bytes(data)

    now = datetime.now(timezone.utc).isoformat()
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT discord_id FROM user_profiles WHERE discord_id = ?",
            (user.discord_id,),
        )
        existing = await cursor.fetchone()
        if existing:
            await db.execute(
                "UPDATE user_profiles SET avatar_filename = ?, updated_at = ? WHERE discord_id = ?",
                (avatar_path.name, now, user.discord_id),
            )
        else:
            await db.execute(
                "INSERT INTO user_profiles (discord_id, avatar_filename, updated_at, invite_auto_accept, notify_discord_dm, notify_browser, notify_match_start, notify_checkin, notify_team_invite, notify_tournament_news) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (user.discord_id, avatar_path.name, now, 0, 1, 0, 1, 1, 1, 0),
            )
        await db.commit()
        cursor = await db.execute(
            "SELECT * FROM user_profiles WHERE discord_id = ?",
            (user.discord_id,),
        )
        row = await cursor.fetchone()

    return _serialize_profile_row(dict(row), user)


@router.get("/avatars/{discord_id}", response_model=None)
async def get_avatar(discord_id: str) -> FileResponse | RedirectResponse:
    avatar_path = _avatar_file_path(discord_id)
    if avatar_path:
        media_type = _AVATAR_MEDIA_TYPES.get(avatar_path.suffix.lower(), "application/octet-stream")
        return FileResponse(path=avatar_path, media_type=media_type)

    async with get_db() as db:
        cursor = await db.execute(
            "SELECT discord_avatar FROM sessions WHERE discord_id = ? "
            "AND discord_avatar IS NOT NULL AND discord_avatar != '' "
            "ORDER BY created_at DESC LIMIT 1",
            (discord_id,),
        )
        row = await cursor.fetchone()

    if row and row["discord_avatar"]:
        return RedirectResponse(url=row["discord_avatar"], status_code=status.HTTP_302_FOUND)

    raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Avatar nicht gefunden")
