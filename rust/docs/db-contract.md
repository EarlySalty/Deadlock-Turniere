# DB-Vertrag — Single Source of Truth

Die maßgebliche Schemadefinition ist die konsolidierte Migration
[`crates/tb-db/migrations/0001_initial.sql`](../crates/tb-db/migrations/0001_initial.sql).
Sie wurde **1:1 aus dem effektiven Live-Schema** (`backend/data/tournament.db`)
generiert — inklusive aller historisch per `ALTER TABLE` nachgezogenen Spalten,
die im Python-`db.py`-Literal NICHT vollständig abgebildet waren. Ein
Integrationstest (`crates/tb-db/tests/migration.rs`) beweist:

- frische DB → die Migration baut alle 31 Tabellen auf;
- Kopie der Live-DB → die Migration läuft idempotent durch (No-op, alle CREATEs
  sind `IF NOT EXISTS`).

## Geteilte Datenbank

Rust und der Python-Stand nutzen **dieselbe Datei**. Konsequenzen:

- Migrationen dürfen die bestehenden FK-/CHECK-Constraints **nicht** verändern
  (sie sind No-ops auf der Live-DB). Korrekturen am Constraint-Design sind
  Opt-in-Folgefixe (siehe [`known-issues.md`](known-issues.md)), keine stillen
  Schema-Eingriffe.
- Zeitstempel werden tz-aware als ISO-8601 (`to_rfc3339`, Offset `+00:00`)
  geschrieben — kompatibel zu Pythons `isoformat()`, sodass beide Seiten beide
  Schreibweisen lesen.
- Bool-Spalten sind `INTEGER` (0/1); die Row-Mapper konvertieren explizit.

## Tabellen (31)

| Bereich | Tabellen |
|---------|----------|
| Turnier | `tournaments` (30 Spalten), `audit_log` |
| Teams | `teams`, `team_members`, `team_applications`, `team_invitations` |
| Anmeldung/Check-in | `tournament_signups`, `checkins`, `tournament_checkins` |
| Gruppenphase | `groups`, `group_teams`, `group_matches` |
| Bracket | `bracket_matches` (26 Spalten), `bracket_mini_groups`, `bracket_mini_group_teams`, `match_games` |
| Draft | `draft_sessions`, `draft_actions` |
| Ergebnis | `match_results`, `match_result_reports` |
| Caster/Stream | `match_casters`, `tournament_casters` |
| Reminder (Dedupe) | `sent_tournament_reminders`, `sent_start_reminders`, `sent_match_reminders` |
| Nutzer | `sessions`, `user_profiles`, `user_consents`, `player_points`, `rank_cache` |
| Integration | `discord_tasks` |

Plus 1 Trigger (`cleanup_tournament_checkins_after_tournament_delete`).

## Externe Datenbanken

- **Steam-Bridge-DB** (`STEAM_BRIDGE_DB_PATH`): read-only Lookup für Ränge
  (`steam_links`, `deadlock_subrank_roles`). Separate Datei, eigener Pool; fehlt
  sie, entfällt die Bridge-Stufe (Fallback auf Discord-Rollen).
- **Steam-Tasks-Queue** (`steam_tasks` in derselben Bridge-DB): Lobby-Aufträge an
  den Steam-/GC-Worker; fehlt die DB, degradieren die Lobby-Operationen sauber.
