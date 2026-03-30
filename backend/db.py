from __future__ import annotations

import contextlib
from pathlib import Path
from typing import AsyncIterator

import aiosqlite

from config import settings

# --- SQL Schema ---

_SCHEMA = """
CREATE TABLE IF NOT EXISTS tournaments(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    description TEXT,
    team_size INTEGER NOT NULL DEFAULT 6,
    registration_start TEXT,
    registration_end TEXT,
    group_phase_start TEXT,
    bracket_start TEXT,
    bracket_format TEXT NOT NULL DEFAULT 'single_elimination',
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS teams(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id),
    name TEXT NOT NULL,
    name_key TEXT NOT NULL,
    captain_discord_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tournament_id, name_key)
);

CREATE TABLE IF NOT EXISTS team_members(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    discord_id TEXT NOT NULL,
    discord_name TEXT,
    steam_id TEXT,
    rank TEXT,
    rank_score INTEGER DEFAULT 0,
    role TEXT NOT NULL DEFAULT 'member',
    joined_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(team_id, discord_id)
);

CREATE TABLE IF NOT EXISTS tournament_signups(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id),
    discord_id TEXT NOT NULL,
    steam_id TEXT,
    rank TEXT,
    rank_score INTEGER DEFAULT 0,
    team_id INTEGER REFERENCES teams(id),
    signed_up_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tournament_id, discord_id)
);

CREATE TABLE IF NOT EXISTS groups(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id),
    name TEXT NOT NULL,
    seeding_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS group_teams(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id INTEGER NOT NULL REFERENCES groups(id),
    team_id INTEGER NOT NULL REFERENCES teams(id),
    wins INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    points INTEGER NOT NULL DEFAULT 0,
    UNIQUE(group_id, team_id)
);

CREATE TABLE IF NOT EXISTS group_matches(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id INTEGER NOT NULL REFERENCES groups(id),
    team1_id INTEGER NOT NULL REFERENCES teams(id),
    team2_id INTEGER NOT NULL REFERENCES teams(id),
    winner_id INTEGER REFERENCES teams(id),
    status TEXT NOT NULL DEFAULT 'pending',
    scheduled_at TEXT,
    played_at TEXT
);

CREATE TABLE IF NOT EXISTS bracket_matches(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id),
    round INTEGER NOT NULL,
    position INTEGER NOT NULL,
    bracket_type TEXT NOT NULL DEFAULT 'winners',
    team1_id INTEGER REFERENCES teams(id),
    team2_id INTEGER REFERENCES teams(id),
    winner_id INTEGER REFERENCES teams(id),
    status TEXT NOT NULL DEFAULT 'pending',
    steam_party_id TEXT,
    party_code TEXT,
    deadlock_match_id TEXT,
    match_duration_s INTEGER,
    match_stats TEXT,
    scheduled_at TEXT,
    played_at TEXT
);

CREATE TABLE IF NOT EXISTS match_results(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bracket_match_id INTEGER REFERENCES bracket_matches(id),
    group_match_id INTEGER REFERENCES group_matches(id),
    winning_team INTEGER,
    duration_s INTEGER,
    player_stats TEXT,
    source TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS checkins(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_type TEXT NOT NULL,
    match_id INTEGER NOT NULL,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    discord_id TEXT NOT NULL,
    checked_in_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sessions(
    token TEXT PRIMARY KEY,
    discord_id TEXT NOT NULL,
    discord_name TEXT,
    discord_avatar TEXT,
    discord_roles TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS audit_log(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action TEXT NOT NULL,
    user_id TEXT,
    details TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"""


async def init_db() -> None:
    """Erstellt alle Tabellen und aktiviert WAL-Mode + Foreign Keys."""
    db_path = Path(settings.DATABASE_PATH)
    db_path.parent.mkdir(parents=True, exist_ok=True)

    async with aiosqlite.connect(str(db_path)) as db:
        await db.execute("PRAGMA journal_mode=WAL;")
        await db.execute("PRAGMA foreign_keys=ON;")
        await db.executescript(_SCHEMA)
        await _ensure_schema_upgrades(db)
        await db.commit()


@contextlib.asynccontextmanager
async def get_db() -> AsyncIterator[aiosqlite.Connection]:
    """Async Context-Manager für DB-Verbindungen."""
    db = await aiosqlite.connect(str(settings.DATABASE_PATH))
    try:
        await db.execute("PRAGMA foreign_keys=ON;")
        db.row_factory = aiosqlite.Row
        yield db
    finally:
        await db.close()


async def _ensure_schema_upgrades(db: aiosqlite.Connection) -> None:
    """Ergänzt Spalten in bestehenden Installationen idempotent."""
    await _ensure_column(db, "bracket_matches", "match_duration_s", "INTEGER")
    await _ensure_column(db, "bracket_matches", "match_stats", "TEXT")
    await _ensure_column(db, "tournament_signups", "discord_name", "TEXT")


async def _ensure_column(
    db: aiosqlite.Connection,
    table_name: str,
    column_name: str,
    column_sql: str,
) -> None:
    cursor = await db.execute(f"PRAGMA table_info({table_name})")
    rows = await cursor.fetchall()
    existing_columns = {row[1] for row in rows}
    if column_name in existing_columns:
        return
    await db.execute(
        f"ALTER TABLE {table_name} ADD COLUMN {column_name} {column_sql}"
    )
