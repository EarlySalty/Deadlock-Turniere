"""Deadlock Tournament Platform — FastAPI Backend."""
from __future__ import annotations

from contextlib import asynccontextmanager
from typing import AsyncIterator

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from auth.discord_oauth import router as auth_router
from config import settings
from db import init_db


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    """Startup: DB initialisieren. Shutdown: Aufraemen."""
    await init_db()
    yield


app = FastAPI(
    title="Deadlock Turniere",
    description="Tournament Platform fuer die Deutsche Deadlock Community",
    version="0.1.0",
    lifespan=lifespan,
)

# --- CORS ---
app.add_middleware(
    CORSMiddleware,
    allow_origins=[
        "https://turnier.earlysalty.com",
        "http://localhost:5173",
    ],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# --- Router ---
app.include_router(auth_router)


# --- Health ---
@app.get("/api/health", tags=["system"])
async def health() -> dict[str, str]:
    """Health-Check Endpoint."""
    return {"status": "ok", "service": "deadlock-turniere"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        "main:app",
        host="0.0.0.0",
        port=settings.BACKEND_PORT,
        reload=True,
    )
