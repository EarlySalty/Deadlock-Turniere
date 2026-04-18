"""Draft-API-Routen."""
from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from auth.permissions import require_admin
from db import get_db
from draft.engine import get_draft_state, start_draft, take_action
from draft.heroes import DEADLOCK_HEROES
from tournament.models import UserSession

router = APIRouter(prefix="/api/draft", tags=["draft"])


@router.get("/heroes")
async def list_heroes() -> dict:
    return {"heroes": DEADLOCK_HEROES}


@router.post("/matches/{match_id}/start")
async def start_match_draft(
    match_id: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    async with get_db() as db:
        cursor = await db.execute(
            "SELECT id FROM bracket_matches WHERE id = ?", (match_id,)
        )
        if not await cursor.fetchone():
            raise HTTPException(status_code=404, detail=f"Bracket-Match {match_id} nicht gefunden")

    session_id = await start_draft(match_id, started_by=user.discord_id)
    return await get_draft_state(session_id)


@router.get("/sessions/{session_id}")
async def get_session(
    session_id: int,
    user: UserSession = Depends(require_admin),
) -> dict:
    del user
    try:
        return await get_draft_state(session_id)
    except ValueError as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc


class DraftActionRequest(BaseModel):
    hero_name: str
    taken_by: str
    force: bool = False


@router.post("/sessions/{session_id}/action")
async def submit_action(
    session_id: int,
    body: DraftActionRequest,
    user: UserSession = Depends(require_admin),
) -> dict:
    del user
    try:
        result = await take_action(session_id, body.hero_name, body.taken_by, force=body.force)
        state = await get_draft_state(session_id)
        return {**state, **result}
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
