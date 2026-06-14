-- Konsolidiertes Zielschema des Deadlock-Turniere-Backends.
-- Generiert 1:1 aus dem effektiven Live-Schema (backend/data/tournament.db),
-- inkl. aller historisch per ALTER TABLE nachgezogenen Spalten.
-- Idempotent (IF NOT EXISTS): No-op auf der bestehenden Live-DB, voller Aufbau bei frischer DB.

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
, invite_mode TEXT NOT NULL DEFAULT 'always', invite_window_start TEXT, invite_window_end TEXT, lobby_settings TEXT, checkin_start TEXT, tournament_mode TEXT NOT NULL DEFAULT 'group_stage', series_format INTEGER NOT NULL DEFAULT 1, exclude_from_leaderboard INTEGER NOT NULL DEFAULT 0, reminder_offsets TEXT DEFAULT '[1440,120,15]', tournament_game_mode TEXT NOT NULL DEFAULT 'standard', auto_lobby_enabled INTEGER NOT NULL DEFAULT 1, is_test INTEGER NOT NULL DEFAULT 0, rules TEXT, final_series_format INTEGER, match_objective TEXT NOT NULL DEFAULT 'auto', no_show_grace_minutes INTEGER NOT NULL DEFAULT 10, start_reminder_offsets TEXT DEFAULT '[1440,60]');

CREATE TABLE IF NOT EXISTS teams(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id),
    name TEXT NOT NULL,
    name_key TEXT NOT NULL,
    captain_discord_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')), recruitment_status TEXT NOT NULL DEFAULT 'open',
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
    signed_up_at TEXT NOT NULL DEFAULT (datetime('now')), discord_name TEXT, invited_by_team_id INTEGER,
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
, discord_channel_id TEXT, steam_party_id TEXT, party_code TEXT, deadlock_match_id TEXT, match_duration_s INTEGER, match_stats TEXT, hero_assignments TEXT);

CREATE TABLE IF NOT EXISTS bracket_mini_groups(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id),
    round INTEGER NOT NULL,
    position INTEGER NOT NULL,
    advances_to_match_id INTEGER REFERENCES bracket_matches(id),
    advances_to_slot INTEGER,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
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
    scheduled_at TEXT,
    played_at TEXT
, match_duration_s INTEGER, match_stats TEXT, source_match1_id INTEGER, source_match2_id INTEGER, discord_channel_id TEXT, mini_group_id INTEGER, source_mini_group1_id INTEGER, source_mini_group2_id INTEGER, hero_assignments TEXT, loser_to_match_id INTEGER, loser_to_slot INTEGER, on_stream INTEGER NOT NULL DEFAULT 1);

CREATE TABLE IF NOT EXISTS bracket_mini_group_teams(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mini_group_id INTEGER NOT NULL REFERENCES bracket_mini_groups(id) ON DELETE CASCADE,
    team_id INTEGER REFERENCES teams(id),
    seed_order INTEGER NOT NULL,
    source_match_id INTEGER REFERENCES bracket_matches(id),
    source_mini_group_id INTEGER REFERENCES bracket_mini_groups(id),
    UNIQUE(mini_group_id, seed_order),
    UNIQUE(mini_group_id, team_id)
);

CREATE TABLE IF NOT EXISTS match_games (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    bracket_match_id    INTEGER NOT NULL REFERENCES bracket_matches(id) ON DELETE CASCADE,
    game_number         INTEGER NOT NULL CHECK(game_number IN (1,2,3,4,5)),
    status              TEXT NOT NULL DEFAULT 'pending'
                            CHECK(status IN ('pending','lobby_created','in_progress','completed','cancelled')),
    steam_party_id      TEXT,
    party_code          TEXT,
    deadlock_match_id   TEXT,
    winner_team         INTEGER CHECK(winner_team IN (1,2)),
    duration_s          INTEGER,
    match_stats         TEXT,
    created_at          TEXT NOT NULL,
    completed_at        TEXT,
    UNIQUE(bracket_match_id, game_number)
);

CREATE TABLE IF NOT EXISTS draft_sessions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    bracket_match_id INTEGER NOT NULL REFERENCES bracket_matches(id) ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'pending'
                        CHECK(status IN ('pending','in_progress','completed','cancelled')),
    current_action_index INTEGER NOT NULL DEFAULT 0,
    started_by      TEXT,
    started_at      TEXT,
    completed_at    TEXT,
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS draft_actions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL REFERENCES draft_sessions(id) ON DELETE CASCADE,
    sequence_index  INTEGER NOT NULL,
    action_type     TEXT NOT NULL CHECK(action_type IN ('ban','pick')),
    team_slot       INTEGER NOT NULL CHECK(team_slot IN (1,2)),
    hero_name       TEXT,
    taken_by        TEXT,
    taken_at        TEXT,
    is_admin_forced INTEGER NOT NULL DEFAULT 0
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

CREATE TABLE IF NOT EXISTS tournament_checkins(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    discord_id TEXT NOT NULL,
    checked_in_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(tournament_id, discord_id)
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

CREATE TABLE IF NOT EXISTS rank_cache(
    discord_id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    steam_id TEXT,
    rank TEXT,
    rank_tier INTEGER,
    subrank INTEGER,
    rank_score INTEGER,
    cached_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE TABLE IF NOT EXISTS user_consents (
  discord_id TEXT PRIMARY KEY,
  consented_at TEXT NOT NULL,
  consent_version INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS user_profiles (
  discord_id TEXT PRIMARY KEY,
  bio TEXT CHECK(LENGTH(bio) <= 1000),
  invite_auto_accept INTEGER NOT NULL DEFAULT 0,
  notify_discord_dm INTEGER NOT NULL DEFAULT 1,
  notify_browser INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
, display_name TEXT CHECK(display_name IS NULL OR LENGTH(display_name) <= 32), avatar_filename TEXT, notify_match_start INTEGER NOT NULL DEFAULT 1, notify_checkin INTEGER NOT NULL DEFAULT 1, notify_team_invite INTEGER NOT NULL DEFAULT 1, notify_tournament_news INTEGER NOT NULL DEFAULT 0, notify_registration_reminder INTEGER NOT NULL DEFAULT 1);

CREATE TABLE IF NOT EXISTS team_applications (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
  discord_id TEXT NOT NULL,
  discord_name TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending',
  created_at TEXT NOT NULL,
  UNIQUE(team_id, discord_id)
);

CREATE TABLE IF NOT EXISTS player_points (
  discord_id TEXT PRIMARY KEY,
  total_points INTEGER NOT NULL DEFAULT 0,
  tournaments_played INTEGER NOT NULL DEFAULT 0,
  matches_played INTEGER NOT NULL DEFAULT 0,
  matches_won INTEGER NOT NULL DEFAULT 0,
  best_placement INTEGER,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS team_invitations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tournament_id INTEGER NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
  team_id INTEGER NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
  discord_id TEXT NOT NULL,
  signup_id INTEGER REFERENCES tournament_signups(id),
  status TEXT NOT NULL DEFAULT 'pending',
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  expires_at TEXT,
  UNIQUE(team_id, discord_id)
);

CREATE TABLE IF NOT EXISTS discord_tasks(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    type TEXT NOT NULL,
    payload TEXT,
    status TEXT DEFAULT 'PENDING',
    result_payload TEXT,
    error TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT
);

CREATE TABLE IF NOT EXISTS sent_tournament_reminders (id INTEGER PRIMARY KEY AUTOINCREMENT, tournament_id INTEGER NOT NULL, offset_minutes INTEGER NOT NULL, sent_at TEXT NOT NULL, UNIQUE(tournament_id, offset_minutes));

CREATE TABLE IF NOT EXISTS match_casters (id INTEGER PRIMARY KEY AUTOINCREMENT, match_id INTEGER NOT NULL, match_type TEXT NOT NULL DEFAULT 'bracket', discord_id TEXT NOT NULL, assigned_at TEXT NOT NULL DEFAULT (datetime('now')), assigned_by TEXT, UNIQUE(match_id, match_type, discord_id));

CREATE TABLE IF NOT EXISTS tournament_casters(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL,
    discord_id TEXT NOT NULL,
    assigned_at TEXT NOT NULL DEFAULT (datetime('now')),
    assigned_by TEXT,
    UNIQUE(tournament_id, discord_id),
    FOREIGN KEY (tournament_id) REFERENCES tournaments(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS sent_start_reminders(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    offset_minutes INTEGER NOT NULL,
    sent_at TEXT NOT NULL,
    UNIQUE(tournament_id, offset_minutes)
);

CREATE TABLE IF NOT EXISTS sent_match_reminders(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_type TEXT NOT NULL,
    match_id INTEGER NOT NULL,
    kind TEXT NOT NULL DEFAULT 'next_up',
    sent_at TEXT NOT NULL,
    UNIQUE(match_type, match_id, kind)
);

CREATE TABLE IF NOT EXISTS match_result_reports(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_type TEXT NOT NULL,
    match_id INTEGER NOT NULL,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE,
    reported_by TEXT NOT NULL,
    winner_team_id INTEGER REFERENCES teams(id),
    deadlock_match_id TEXT,
    is_no_show INTEGER NOT NULL DEFAULT 0,
    no_show_team_id INTEGER REFERENCES teams(id),
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at TEXT,
    resolved_by TEXT
);

CREATE TRIGGER IF NOT EXISTS cleanup_tournament_checkins_after_tournament_delete
AFTER DELETE ON tournaments
FOR EACH ROW
BEGIN
    DELETE FROM tournament_checkins WHERE tournament_id = OLD.id;
END;
