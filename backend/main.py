"""Deadlock Tournament Platform — FastAPI Backend."""
from __future__ import annotations

import asyncio
import contextlib
from contextlib import asynccontextmanager
from typing import AsyncIterator

from fastapi import Depends, FastAPI
from fastapi.middleware.cors import CORSMiddleware
from starlette.middleware.trustedhost import TrustedHostMiddleware

from auth.discord_oauth import router as auth_router
from admin.test_mode import router as admin_test_router
from auth.middleware import get_current_user
from config import settings
from db import init_db
from draft.routes import router as draft_router
from tournament.consent_routes import router as consent_router
from tournament.leaderboard_routes import router as leaderboard_router
from tournament.models import UserSession
from tournament.scheduler import start_scheduler
from tournament.admin_routes import router as admin_router
from tournament.routes import router as tournament_router


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Startup: DB initialisieren. Shutdown: Aufräumen."""
    await init_db()
    scheduler_task = asyncio.create_task(start_scheduler(app))
    try:
        yield
    finally:
        scheduler_task.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await scheduler_task


app = FastAPI(
    title="Deadlock Turniere",
    description="Tournament Platform für die Deutsche Deadlock Community",
    version="0.1.0",
    lifespan=lifespan,
    docs_url=settings.docs_url,
    redoc_url=settings.redoc_url,
    openapi_url=settings.openapi_url,
)

# --- CORS ---
app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.cors_allowed_origins,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)
app.add_middleware(TrustedHostMiddleware, allowed_hosts=settings.allowed_hosts)

# --- Router ---
app.include_router(auth_router)
app.include_router(tournament_router)
app.include_router(admin_router)
app.include_router(admin_test_router)
app.include_router(consent_router)
app.include_router(leaderboard_router)
app.include_router(draft_router)


# --- Auth: /api/me ---
@app.get("/api/me", tags=["auth"])
async def get_me(user: UserSession = Depends(get_current_user)) -> UserSession:
    """Gibt die aktuelle User-Session zurück."""
    return user


# --- Health ---
@app.get("/api/health", tags=["system"])
async def health() -> dict[str, str]:
    """Health-Check Endpoint."""
    return {"status": "ok", "service": "deadlock-turniere"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        "main:app",
        host=settings.BACKEND_HOST,
        port=settings.BACKEND_PORT,
        reload=True,
    )
