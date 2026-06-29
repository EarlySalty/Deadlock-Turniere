-- Phase 1a: Fundament der Turnier-Automatik.
-- Additive Tabellen fuer Presets, Vorschlaege, DM-Opt-out und Signals.

CREATE TABLE IF NOT EXISTS tournament_presets(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    category TEXT NOT NULL CHECK(category IN ('fun','comp')),
    team_size INTEGER NOT NULL DEFAULT 6,
    bracket_format TEXT NOT NULL DEFAULT 'single_elimination',
    series_format INTEGER NOT NULL DEFAULT 1,
    final_series_format INTEGER,
    tournament_mode TEXT NOT NULL DEFAULT 'group_stage',
    tournament_game_mode TEXT NOT NULL DEFAULT 'standard',
    match_objective TEXT NOT NULL DEFAULT 'auto',
    invite_mode TEXT NOT NULL DEFAULT 'always',
    reminder_offsets TEXT DEFAULT '[1440,120,15]',
    start_reminder_offsets TEXT DEFAULT '[1440,60]',
    rules TEXT,
    description_template TEXT,
    active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0, 1)),
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tournament_proposals(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    preset_id INTEGER REFERENCES tournament_presets(id) ON DELETE SET NULL,
    source TEXT NOT NULL CHECK(source IN ('bot','manual')),
    proposed_start TEXT,
    config_json TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','pending_approval','approved','rejected','expired')),
    proposal_message_id TEXT,
    channel_id TEXT,
    tournament_id INTEGER REFERENCES tournaments(id) ON DELETE SET NULL,
    decided_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tournament_proposal_votes(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proposal_id INTEGER NOT NULL REFERENCES tournament_proposals(id) ON DELETE CASCADE,
    caster_discord_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK(decision IN ('approve','reject')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(proposal_id, caster_discord_id)
);

CREATE TABLE IF NOT EXISTS tournament_proposal_feedback(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proposal_id INTEGER NOT NULL REFERENCES tournament_proposals(id) ON DELETE CASCADE,
    caster_discord_id TEXT NOT NULL,
    raw_text TEXT NOT NULL,
    applied_change_json TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tournament_dm_optout(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    discord_id TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('fun','comp','all')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(discord_id, scope)
);

CREATE TABLE IF NOT EXISTS tournament_signals(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    participants INTEGER,
    teams INTEGER,
    no_shows INTEGER,
    poll_up INTEGER,
    poll_down INTEGER,
    poll_message_id TEXT,
    feedback_summary TEXT,
    collected_at TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE tournaments ADD COLUMN scheduled_event_id TEXT;
ALTER TABLE tournaments ADD COLUMN source TEXT NOT NULL DEFAULT 'manual' CHECK(source IN ('bot','manual'));
-- SQLite kann bei ALTER TABLE ADD COLUMN keinen FK fuer preset_id nachruesten.
ALTER TABLE tournaments ADD COLUMN preset_id INTEGER;
