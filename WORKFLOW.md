# Turnier-Bot Upgrade – WORKFLOW

---

## Neue Aufgabe (2026-04-25, Teil 2): Tournament-Caster, Live/Archiv-Trennung, Test-Modus

### Ziel
- **Caster pro Turnier statt pro Match**: Pool kommt bereits aus Discord-Rolle `1495154811799077067` (`DISCORD_CASTER_ROLE_ID`), Zuweisung wird auf Turnier-Ebene gehoben. Wer fürs Turnier eingetragen ist, wird automatisch in jedes Match übernommen.
- **Admin-Übersicht entwirren**: Live und Archiv klar getrennt, abgeschlossene Turniere mischen sich nicht mehr in die aktive Verwaltung. Read-Only-Detailansicht für archivierte Turniere.
- **Test-Modus**: `is_test`-Flag pro Turnier; Endpoints zum Erstellen von Test-Usern, Test-Turnieren, Match-Ergebnis-Simulation und Wipe. UI-Section, in der Test-Daten generiert/gelöscht werden.

### Status (2026-04-25)
→ **In Arbeit** — Backend-Implementierung durch GPT-Worker läuft

### Scope (Backend, an GPT delegiert)
1. **`backend/db.py`**:
   - Neue Tabelle `tournament_casters (tournament_id, discord_id, assigned_at, assigned_by, UNIQUE(tournament_id, discord_id))` + Migration in `_ensure_schema_upgrades`.
   - Neue Spalte `tournaments.is_test INTEGER NOT NULL DEFAULT 0` + Migration.
2. **Caster-Layer (`backend/tournament/admin_routes.py` + `backend/match/manager.py`)**:
   - Neue Endpoints: `GET/POST /admin/tournaments/{id}/casters`, `DELETE /admin/tournaments/{id}/casters/{discord_id}`.
   - Per-Match-Endpoints (`/tournaments/{id}/matches/{match_id}/casters`) als Read-Only erhalten — sie geben zukünftig die Tournament-Caster zurück (Backwards-Compat fürs Frontend übergangsweise).
   - `_load_match_casters(match_type, match_id)` in `manager.py`: Lookup tournament_id über match → liest aus `tournament_casters`. Falls Tournament-Liste leer, fallback auf alte `match_casters`-Einträge (Backwards-Compat).
   - Audit-Log-Einträge `tournament_caster_assign` / `tournament_caster_remove`.
3. **Test-Modus (`backend/admin/test_mode.py` neu, eingebunden in `main.py`)**:
   - `is_test` in `TournamentCreate/Update/TournamentDetailPublic` Pydantic-Modellen.
   - Neue Module-Routen `POST /admin/test/users` (body `{count}`), `GET /admin/test/users`, `DELETE /admin/test/users` — erzeugt/löscht User mit Discord-IDs `test_<6-stellig>` in `user_profiles` + simulierter `rank_cache`-Eintrag.
   - `POST /admin/test/tournaments` (body `{name, team_size, num_teams, mode, game_mode}`) — erstellt komplettes Test-Turnier mit `is_test=1`, Teams + Members aus Test-User-Pool, Captain-Naming, optional auto-checkin → Status auf `bracket` setzen für sofortigen Test.
   - `POST /admin/test/tournaments/{id}/simulate-round` — würfelt Ergebnisse für alle offenen Matches der aktuellen Runde, ruft `apply_bracket_match_result` / `apply_group_match_result`.
   - `DELETE /admin/test/wipe` — löscht alle Test-User + alle `is_test=1` Turniere + abhängige Daten (signups, teams, matches, mini_groups …).
   - Endpoints alle `require_owner` (oder `require_mod` wenn keine Owner-Trennung existiert).
4. **Tests**: Smoke-Test pro neuem Endpoint via `httpx.AsyncClient` (falls pytest-Setup vorhanden).

### Fortschritt GPT-Worker Backend (2026-04-25)
- Relevante Backend-Stellen geprüft: `db.py`, `tournament/models.py`, `tournament/admin_routes.py`, `tournament/routes.py`, `tournament/engine.py`, `match/manager.py`, `match/auto_lobby.py`, `notifications/discord_notifier.py`, `main.py`, `auth/permissions.py`
- Architekturentscheidung bestätigt: kein `require_owner` vorhanden, daher neuer Test-Mode-Router mit `require_mod`
- Umsetzung läuft in drei Blöcken: Schema/`is_test`-Propagation, Tournament-Caster-Layer mit Legacy-Fallback, neuer `admin/test_mode.py`-Router inkl. Simulationslogik
- Implementiert: DB-Migration für `tournament_casters` + `tournaments.is_test`, neue Turnier-Caster-Endpoints, Legacy-Fallback in `match.manager`, neue `backend/admin/test_mode.py`-Routen, Test-Mode-Skips für Auto-Lobby/Discord-Benachrichtigungen
- Verifikation gelaufen: `.venv/bin/python -m py_compile $(git ls-files 'backend/**/*.py')`, zusätzlicher Compile für neue untracked Dateien `backend/admin/test_mode.py` + `backend/__init__.py`, `.venv/bin/python -c "from backend import main; print('imports ok')"`, `.venv/bin/python -c "import asyncio; from backend.db import init_db; asyncio.run(init_db())"`
- Umgebungshinweis: `pytest` ist in `.venv` aktuell nicht verfügbar; `init_db()` hat ein lokales `data/`-Artefakt gemäß aktueller Config erzeugt

### Scope (Frontend, von Claude)
1. **`frontend/src/types/tournament.ts`**: `is_test`, `tournamentCasters`-Modelle.
2. **`frontend/src/api/client.ts` + `hooks/useTournament.ts`**: `useTournamentCasters`, `useAssignTournamentCaster`, `useRemoveTournamentCaster`, `useTestUsers`, `useCreateTestUsers`, `useWipeTestUsers`, `useCreateTestTournament`, `useSimulateRound`, `useWipeTestData`.
3. **`pages/Admin.tsx`**: Top-Level-Mode-State `'live' | 'archive' | 'test'` als Tabs oben; Sidebar zeigt nur passende Liste; Archiv-Detail-View mit Banner "Read-Only" und reduzierten Phase-Tabs; Test-Tab mit Test-Mode-Tools-Panel.
4. **Neue Komponenten**:
   - `TournamentCasterPanel` — wird in der Voice/Caster-Phase angezeigt (Live + Archiv), Multi-Select aus Discord-Rollen-Pool.
   - `ArchivedTournamentView` — gedimmter Read-Only-Container für archivierte Turniere.
   - `TestModePanel` — Buttons: Seed N Users, Erstelle Test-Turnier, Simuliere Runde, Wipe.
5. **`MatchAdminPanel.tsx`**: Per-Match-`CasterPanel` entfernen oder als Read-Only-Anzeige der Tournament-Caster lassen (Default: entfernen).

### Verifikation
- `cd backend && .venv/bin/python -m py_compile $(git ls-files 'backend/**/*.py')`
- `cd frontend && node_modules/.bin/tsc --noEmit -p tsconfig.app.json && node_modules/.bin/vite build`
- Bot-Restart `systemctl --user restart deadlock-turniere.service`
- Manueller Test: Test-Turnier mit 5 Test-Usern erstellen → Mini-RR + Caster-Auswahl + Round-Simulation prüfen.

---

## Neue Aufgabe (2026-04-25): Mini-RR-Bracket, Captain-Teamnamen, Game-Modes, Auto-Lobby

### Ziel
- BYE-freies Single-Elimination-Bracket via rekursiven Mini-Round-Robin-Slots
- Auto-generierte Teams nach Captain benennen statt Phonetic-Fallback
- Turnierweite Game-Modes mit Hero-Zuteilung und Lobby-Ankündigung
- Auto-Lobby-Erstellung nach Bracket-/Gruppen-Generierung und beim Advance
- API/Modelle/Tests für Mini-Groups und neue Match-Metadaten

### Status (2026-04-25)
→ **In Arbeit** — Backend-Refactor läuft

### Fortschritt
- Plan-Datei `/home/naniadm/.claude/plans/wir-m-ssen-den-turnier-sprightly-lemon.md` vollständig gelesen
- Relevante Backend-Dateien geprüft: `db.py`, `engine.py`, `models.py`, `manager.py`, `result_processor.py`, `scheduler.py`, `admin_routes.py`, `routes.py`, `discord_notifier.py`
- Umsetzung startet mit Schema-/Modell-Änderungen, danach Engine/Mini-Groups, dann Match-Flow/Routes/Tests
- Backend umgesetzt: DB-Migrationen, Mini-RR-Bracket, Captain-Teamnamen, Mini-Group-Resolver, Game-Modes, Auto-Lobby, Admin/Public-Routen
- Neue Backend-Dateien: `backend/tournament/mini_groups.py`, `backend/match/game_modes.py`, `backend/match/heroes.py`, `backend/match/auto_lobby.py`
- Neue Tests ergänzt: `test_engine_mini_group_seeding.py`, `test_engine_team_naming.py`, `test_auto_lobby_hooks.py`, `test_game_modes.py`
- Verifikation: `.venv/bin/python -m py_compile` für Ziel-Dateien + neue Tests erfolgreich; `.venv/bin/pytest` in dieser Umgebung nicht vorhanden

---

## Neue Aufgabe (2026-04-18): Feature-Erweiterungen — Voice, Bo3, Draft, Stats, Lobby

### Ziel
- **A** Voice-Kanal-Runden-Management: Start/Nächste Runde, Split Teams → VC1/VC2 → Sammelpunkt
- **B** Best of 3: series_format pro Turnier (Bo1/Bo3/Bo5), match_games Tabelle
- **C** Hero-Draft: Pick/Ban System (6 Bans + 12 Picks)
- **D** Match-Stats: Deadlock Match-ID + K/D/A in Discord + Dashboard
- **E** Lobby-Announcement: Lobby-Code immer in Kanal 1412411665713987635 mit User-Pings

### Status (2026-04-18)
→ **Gestartet** — GPT-Worker werden dispatched

### Fortschritt GPT-Worker A1 (2026-04-18)
- `service/master_broker.py` in `Deadlock-Bots` um `move-voice` und `voice-channel/members` erweitert
- Routing, Auth, Idempotency und 404/502-Fehlerpfade an bestehendes Broker-Muster angeglichen
- Verifikation: `python3 -m py_compile /home/naniadm/Documents/Deadlock-Bots/service/master_broker.py` erfolgreich
- Übergabe: Änderungen bleiben absichtlich uncommitted für Claude-Review

### Kritische Erkenntnis
Broker hat KEINEN move-voice Endpoint → muss in Deadlock-Bots ergänzt werden.

### Reihenfolge
1. Parallel: A1 (Broker) + B1/C1 (DB-Schema)
2. Parallel: A2-A4+E1-E3 (Backend Voice+Lobby) + B2-B4 (Series) + C2-C4 (Draft) + D1-D2 (Stats)
3. Frontend: A5 + B5 + C5 + D3

### Offene Punkte
- [x] A1: Broker move-voice + get-voice-members (Deadlock-Bots)
- [ ] B1+C1: DB schema_upgrades
- [x] A2-A4: config.py + notifier + admin_routes Voice
- [x] E1-E3: config + send_lobby_announcement + match/manager.py
- [x] B2-B4: models + series_manager.py + Routen
- [x] C2-C4: draft/heroes.py + draft/engine.py + draft/routes.py + main.py
- [x] D1-D2: send_match_stats + result_processor
- [ ] Frontend (A5, B5, C5, D3)

### Fortschritt GPT-Worker B1+C1+B2 (2026-04-18)
- `backend/db.py`: `series_format` in `tournaments` ergänzt, neue Tabellen `match_games`, `draft_sessions`, `draft_actions` im Schema und in `_ensure_schema_upgrades()` hinzugefügt
- `backend/tournament/models.py`: `series_format` in Tournament-Modelle aufgenommen, `MatchGame` ergänzt, `BracketMatch` um Series-Tracking erweitert
- Verifikation: `.venv/bin/python -m py_compile backend/db.py backend/tournament/models.py` erfolgreich
- Übergabe: Änderungen bleiben absichtlich uncommitted für Claude-Review

### Fortschritt GPT-Worker B3+B4+C2+C3+C4 (2026-04-18)
- `backend/match/series_manager.py` neu erstellt: Serien-Spiele anlegen, Ergebnisse pro Game speichern, Serienstand auswerten
- `backend/tournament/admin_routes.py` um Series-Start/Result-Endpunkte ergänzt; Match wird gegen `tournament_id` validiert und der Serien-Slot `1/2` auf die bestehende Bracket-Konvention `0/1` gemappt
- Neues Paket `backend/draft/` mit `heroes.py`, `engine.py`, `routes.py` erstellt; `backend/main.py` bindet den Draft-Router ein
- Verifikation steht als nächster Schritt an; Änderungen bleiben absichtlich uncommitted für Claude-Review

### Fortschritt GPT-Worker A2+A3+A4+E1+E2+E3 (2026-04-18)
- `backend/config.py`: Voice- und Turnier-Lobby-Channel-Settings ergänzt
- `backend/notifications/discord_notifier.py`: Voice-Moves, Voice-Member-Abfrage, Lobby-Announcement und Match-Stats-Posting ergänzt
- `backend/tournament/admin_routes.py`: Admin-Endpoints für VC-Split, Sammelpunkt, Einzel-Move und Channel-Member ergänzt
- `backend/match/manager.py`: Zentrales Lobby-Announcement nach Lobby-Erstellung ergänzt
- `backend/match/result_processor.py`: Non-blocking Stats-Posting in Discord-Match-Channels ergänzt
- Verifikation: `.venv/bin/python -m py_compile backend/config.py backend/notifications/discord_notifier.py backend/tournament/admin_routes.py backend/match/manager.py backend/match/result_processor.py` erfolgreich
- Übergabe: Änderungen bleiben absichtlich uncommitted für Claude-Review

### Fortschritt GPT-Review Backend kritisch (2026-04-18)
- Review-Scope gelesen: `backend/db.py`, `backend/tournament/models.py`, `backend/config.py`, `backend/notifications/discord_notifier.py`, `backend/tournament/admin_routes.py`, `backend/match/manager.py`, `backend/match/result_processor.py`, `backend/match/series_manager.py`, `backend/draft/*`, `backend/main.py`
- Syntax-Check: `.venv/bin/python -m py_compile` auf allen genannten Backend-Dateien erfolgreich, keine Syntax-Fehler
- Fokus des Reviews: Series-Flow, Discord-Notifier, Voice-Endpoints, Draft-Routen/Engine, DB-Migrationen
- Ergebnis wird als reine Issue-Liste ohne Fixes an Claude zurückgegeben

### Fortschritt GPT-Worker Backend Fix Review-Issues (2026-04-18)
- Scope strikt auf 4 Fixes begrenzt: `series_format` Validator, Pre-Write-Matchvalidierung in Series-Endpunkten, `match_id`-Check im Draft-Start, engeres Exception-Handling in `_ensure_schema_upgrades()`
- Betroffene Dateien aktualisiert: `backend/tournament/models.py`, `backend/tournament/admin_routes.py`, `backend/draft/routes.py`, `backend/db.py`
- Verifikation: `.venv/bin/python -m py_compile backend/tournament/models.py backend/tournament/admin_routes.py backend/draft/routes.py backend/db.py` erfolgreich
- Übergabe: Änderungen bleiben uncommitted für Claude-Review

### Fortschritt GPT-Worker Frontend A5+B5+C5+D3 (2026-04-18)
- `frontend/src/types/tournament.ts`: Series-, Voice- und Draft-Typen ergänzt; `BracketMatch` und `TournamentCreate/Update` erweitert
- `frontend/src/api/client.ts` + `frontend/src/hooks/useTournament.ts`: neue Voice-, Draft- und Series-Requests/Hooks im bestehenden `request()`-/React-Query-Muster ergänzt
- Neue Admin-Komponenten `VoiceChannelPanel.tsx` und `DraftPanel.tsx` erstellt; `MatchAdminPanel.tsx`, `CreateTournamentForm.tsx` und `pages/Admin.tsx` integriert
- Verifikation: `cd /home/naniadm/Documents/Deadlock-Turniere/frontend && npx tsc --noEmit` erfolgreich
- Übergabe: Änderungen bleiben absichtlich uncommitted für Claude-Review

### Plan-Datei
`/home/naniadm/.claude/plans/f-r-den-turnier-bot-bright-honey.md`

---

---

## Neue Aufgabe (2026-04-15): Auto Tournament Mode Management

### Ziel
Bot entscheidet automatisch, ob Gruppen-Phase oder nur Bracket basierend auf Team-Anzahl:
- **>= 12 Teams**: Group Stage + Bracket (wie EM/WM)
- **< 12 Teams**: Nur Bracket (schnell & einfach)
- **Admin-Override**: Admins können manuell entscheiden

Zusätzlich: Hilfe-Dokumentation für Turnier-Modi.

### Status (2026-04-15)
→ **Implementierung fertig** — Syntaktisch korrekt, Review & Commit ausstehend

### Implementierungs-Schritte
1. [x] `models.py`: `TournamentMode` Enum + `force_tournament_mode` Feld in Create/Update
2. [x] `engine.py`: `determine_tournament_mode(team_count, force_mode)` Funktion
3. [x] `scheduler.py`: Auto-Logik in `_get_due_next_status()` — wenn bracket_only → skip group_phase
4. [x] `db.py`: `tournament_mode` Spalte hinzugefügt via `_ensure_schema_upgrades()`
5. [x] `admin_routes.py`: Override-Flag in Create/Update, Mode-Berechnung, Validierung
6. [x] Hilfe-Datei `HELP_TOURNAMENT_MODES.md` erstellt (Erklärungen, Unterschiede, Ablauf, FAQ)
7. [ ] Frontend: Info-Text für Turnier-Modus (optional für nächste Phase)
8. [ ] Review, Test, Commit

### Was wurde implementiert
- **Auto-Logic**: >= 12 Teams → group_stage (Standard EM/WM), < 12 Teams → bracket_only
- **Admin-Override**: `force_tournament_mode` im TournamentCreate/Update (nur in Draft-Phase änderbar)
- **Scheduler-Integration**: Wenn bracket_only, skip group_phase und springe direkt zu bracket
- **Dokumentation**: Ausführliche Hilfe-Datei mit Beispielen, Vergleichen und FAQ

### Syntax-Check
✅ Alle Python-Dateien syntaktisch korrekt (models.py, engine.py, scheduler.py, db.py, admin_routes.py)

---

## Alte Aufgabe (2026-04-15): Profil-Upload, Namensänderung, Check-in-Start, Zeitplan-Deduplizierung

### Ziel
1. Profilbild-Upload und Anzeigename-Änderung im Frontend aktivieren (Backend bereits fertig)
2. `checkin_start` als konfigurierbaren Zeitstempel für Turniere ergänzen (Backend + Frontend)
3. Doppeltes „Zeitplan anpassen"-Panel im TournamentManager entfernen

### Status (2026-04-15)
→ **In Bearbeitung** — GPT-Worker laufen

### Fortschritt GPT-Worker 2 (2026-04-15)
- Frontend-Dateien für Profilbild/Anzeigename/`checkin_start` vollständig geprüft
- Implementierung in `tournament.ts`, `client.ts`, `useTournament.ts`, `PlayerProfile.tsx`, `TournamentManager.tsx`, `CreateTournamentForm.tsx` abgeschlossen
- Verifikation: `cd frontend && npx tsc --noEmit` erfolgreich, keine TypeScript-Fehler

### Fortschritt (2026-04-15)
- GPT-Worker 1: Backend für `checkin_start` umgesetzt: DB-Migration, Modelle, Admin-Create/Update, Scheduler
- Verifikation: Syntax-Check erfolgreich, `pytest` aktuell nicht ausführbar, weil in den verfügbaren Python-Umgebungen kein `pytest` installiert ist

### Offene Punkte
- [x] Backend: `checkin_start` DB-Migration, Modelle, Scheduler, Admin-Routes
- [ ] Frontend: Typen, API-Client, Hooks, Profil-Seite, TournamentManager, CreateTournamentForm
- [ ] Review, Verifikation, Commit & Push

### Wichtige Entscheidungen
- Avatar-Upload-Backend ist vollständig implementiert → keine Backend-Änderungen nötig
- `checkin_start` Fallback: falls nicht gesetzt → `registration_end` bleibt der Trigger
- Quick-Schedule-Panel wird entfernt (dupliziert das Hauptformular)
- Ergebnis-Eintrag ist bereits implementiert, erscheint automatisch nach Match-Generierung

---


## Ziel
Großes Feature-Upgrade mit 8 Phasen: DSGVO-Consent, Discord-ID-Schutz, Rangliste, Spieler-Profile, Recruiting-Status, Pick-Zeitfenster, Invite-Flow, Admin-Cards.

## Aufteilung
- **GPT-Worker**: Gesamtes Backend (db.py, models.py, routes.py, admin_routes.py, neue Backend-Dateien)
- **Claude**: Gesamtes Frontend (types, pages, components, App.tsx)

## Status
`IN ARBEIT`

## Erledigte Schritte
- [x] Worker A: Phase 1 (db.py Schema) + Phase 2 (models.py Public-Varianten) → Schema, Invite-/Recruiting-Felder und Public-/Consent-/Profil-Modelle umgesetzt
- [x] Worker B: `backend/tournament/routes.py` erweitert: Public Tournament-Detail, Consent-Gates, Recruiting-Status, Invite-by-Signup, Bewerbungs- und Einladungs-Flow umgesetzt
- [x] Worker C: admin_routes.py (Phase 4 Recruiting + Phase 5 Pick-Fenster) → Invite-Felder in Create/Update ergänzt, Recruiting-Patch sowie Applications List/Accept/Reject umgesetzt
- [x] Worker D: Neue Backend-Dateien (Phase 3 Consent/Profile + Phase 7 Spieler-Profil + Phase 8 Rangliste + points.py) erstellt, `main.py` angebunden, Import-Check mit `./.venv/bin/python` erfolgreich
- [x] Security-Fixes: `join_team` mit Consent-Gate ergänzt, Check-in-Status auf `checked_in_names` ohne Discord-ID umgestellt, Leaderboard-Fallback auf `"Unbekannt"` geändert; Frontend-Verweis in `CheckinManager.tsx` angepasst
- [ ] Frontend: types/tournament.ts erweitern (neue Typen, discord_id aus Public entfernen)
- [ ] Frontend: ConsentModal.tsx (Phase 3)
- [ ] Frontend: Tournament.tsx Solo-Tabelle + Invite-Flow (Phase 6)
- [ ] Frontend: ParticipantManager.tsx Cards (Phase 9)
- [ ] Frontend: CreateTournamentForm + TournamentManager Invite-Modus (Phase 5)
- [ ] Frontend: Leaderboard.tsx neue Seite (Phase 8)
- [ ] Frontend: PlayerProfile.tsx neue Seite (Phase 7)
- [ ] Frontend: App.tsx neue Routen
- [ ] Review, Verifikation, Commit, Push

## Offene Punkte
- Worker A abgeschlossen, Worker B/C/D können auf dem neuen Schema und den Public-Modellen aufsetzen
- Worker B ist implementiert und per `python3 -m py_compile backend/tournament/routes.py` syntaktisch geprüft; End-to-End API-Tests stehen noch aus
- Frontend-Seiten können parallel zur Worker-Arbeit gebaut werden

## Wichtige Entscheidungen
- Invite über signup_id statt discord_id (verhindert ID-Leak)
- Zwei Response-Schemas pro Modell: Public (kein discord_id) und Admin (mit discord_id)
- Consent: globales hartes Gate, einmalig pro User, Version 1
- player_points: nach jedem Turnier-Abschluss neu berechnet

## Relevante Dateien
**Backend:**
- `backend/db.py` – Schema + _ensure_schema_upgrades
- `backend/tournament/models.py` – Neue Public-Varianten
- `backend/tournament/routes.py` – Öffentliche Endpunkte
- `backend/tournament/admin_routes.py` – Admin-Endpunkte
- `backend/tournament/points.py` – NEU: Punkte-Berechnung
- `backend/main.py` – Neue Router einbinden

**Frontend:**
- `frontend/src/types/tournament.ts`
- `frontend/src/App.tsx`
- `frontend/src/pages/Tournament.tsx`
- `frontend/src/pages/Admin.tsx`
- `frontend/src/pages/Leaderboard.tsx` (NEU)
- `frontend/src/pages/PlayerProfile.tsx` (NEU)
- `frontend/src/components/admin/ParticipantManager.tsx`
- `frontend/src/components/admin/CreateTournamentForm.tsx`
- `frontend/src/components/admin/TournamentManager.tsx`
- `frontend/src/components/ConsentModal.tsx` (NEU)
