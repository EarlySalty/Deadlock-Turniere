# Match-Integration — Lobby, Ergebnisse & Bracket-Advancement

## 1. Übersicht

Die Match-Integration verbindet das Tournament-System mit dem Steam Bot. Sie steuert den kompletten Lifecycle eines Turnier-Matches: von der Lobby-Erstellung über den Match-Start bis zur automatischen Ergebnis-Verarbeitung und Bracket-Weiterschaltung.

### Beteiligte Komponenten

| Komponente | Pfad | Rolle |
|------------|------|-------|
| Match Manager | `backend/match/manager.py` | Orchestriert den Match-Lifecycle |
| Steam Bridge | `backend/match/steam_bridge.py` | Schreibt/liest Tasks in die Steam Queue |
| Result Processor | `backend/match/result_processor.py` | Verarbeitet Ergebnisse, updated Bracket |
| Tournament Engine | `backend/tournament/engine.py` | Bracket-Advancement-Logik |
| Admin Routes | `backend/tournament/admin_routes.py` | REST API Endpoints |
| MatchAdminPanel | `frontend/src/components/admin/MatchAdminPanel.tsx` | Admin-UI für Match-Steuerung |
| ManualResultForm | `frontend/src/components/admin/ManualResultForm.tsx` | Manueller Ergebnis-Fallback |
| BracketMatch | `frontend/src/components/bracket/BracketMatch.tsx` | Match-Karte im Bracket |
| BracketView | `frontend/src/components/bracket/BracketView.tsx` | Bracket-Visualisierung |

### Datenfluss

```
Admin klickt "Lobby erstellen"
      │
      ▼
[MatchAdminPanel.tsx] ──POST──► [admin_routes.py: /create-lobby]
                                        │
                                        ▼
                                [manager.py: create_lobby()]
                                        │
                                        ▼
                          [steam_bridge.py: create_task("GC_CREATE_CUSTOM_LOBBY")]
                                        │
                                        ▼
                              [steam_tasks DB: status=PENDING]
                                        │
                                        ▼
                              [Steam Bot: verarbeitet Task]
                                        │
                                        ▼
                              [Deadlock GC: Party erstellt]
                                        │
                                        ▼
                              [steam_tasks DB: status=DONE, result={party_id, join_code}]
                                        │
                                        ▼
                          [steam_bridge.py: poll_task_result() → Ergebnis]
                                        │
                                        ▼
                                [manager.py: speichert party_code in DB]
                                        │
                                        ▼
                          [admin_routes.py: Response → Frontend]
                                        │
                                        ▼
                          [MatchAdminPanel: zeigt Party-Code an]
```

---

## 2. Match-Status State Machine

Jedes Bracket-Match durchläuft definierte Status-Übergänge:

```
                    ┌──────────────────────────────────────┐
                    │                                      │
                    ▼                                      │
┌─────────┐   ┌─────────┐   ┌───────────────┐   ┌────────────────┐   ┌───────────┐
│ pending  │──►│ checkin  │──►│ lobby_created │──►│  in_progress   │──►│ completed │
└─────────┘   └─────────┘   └───────────────┘   └────────────────┘   └───────────┘
     │             │               │                    │
     │             │               │                    │
     ▼             ▼               ▼                    ▼
┌───────────┐  ┌───────────┐  ┌───────────┐      ┌───────────┐
│ cancelled │  │  forfeit  │  │ cancelled │      │  forfeit  │
└───────────┘  └───────────┘  └───────────┘      └───────────┘
```

### Status-Definitionen

| Status | Bedeutung | Auslöser | DB-Felder gesetzt |
|--------|-----------|----------|-------------------|
| `pending` | Match wartet auf Lobby | Bracket-Generierung | team1_id, team2_id |
| `checkin` | Check-in-Phase läuft | **NOCH NICHT IMPLEMENTIERT** | — |
| `lobby_created` | Custom Lobby erstellt, Party-Code verfügbar | Admin: "Lobby erstellen" | steam_party_id, party_code |
| `in_progress` | Match läuft im Spiel | Admin: "Match starten" | deadlock_match_id |
| `completed` | Match beendet, Gewinner steht fest | Auto-Fetch oder manuell | winner_id, match_duration_s, match_stats, played_at |
| `forfeit` | Team hat aufgegeben/nicht erschienen | **NOCH NICHT IMPLEMENTIERT** | winner_id |
| `cancelled` | Match abgesagt | **NOCH NICHT IMPLEMENTIERT** | — |

### Status-Übergänge im Code

| Von → Nach | Auslöser | Code-Stelle |
|------------|----------|-------------|
| pending → lobby_created | Admin: create-lobby | `manager.py:create_lobby()` |
| lobby_created → in_progress | Admin: start | `manager.py:start_match()` |
| in_progress → completed | Auto-Fetch oder manuell | `result_processor.py:apply_bracket_match_result()` |
| pending → completed | BYE-Match bei Bracket-Generierung | `engine.py:generate_bracket()` |
| beliebig → completed | Admin: manuelles Ergebnis | `admin_routes.py: POST /matches/{id}/result` |

---

## 3. Datenbank-Schema

### bracket_matches Tabelle

```sql
CREATE TABLE bracket_matches (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tournament_id INTEGER NOT NULL REFERENCES tournaments(id),
    round INTEGER NOT NULL,                    -- Runde im Bracket (0 = erste Runde)
    position INTEGER NOT NULL,                 -- Position innerhalb der Runde (0-indexed)
    bracket_type TEXT NOT NULL DEFAULT 'winners', -- 'winners', 'losers', 'grand_final'
    team1_id INTEGER REFERENCES teams(id),     -- Oberes Team (NULL = TBD)
    team2_id INTEGER REFERENCES teams(id),     -- Unteres Team (NULL = TBD)
    winner_id INTEGER REFERENCES teams(id),    -- Gewinner (NULL = noch offen)
    status TEXT NOT NULL DEFAULT 'pending',
    -- Steam-Integration:
    steam_party_id TEXT,                        -- Steam Party-ID (für GC-Kommunikation)
    party_code TEXT,                            -- Formatierter Party-Code ("987-654-321")
    deadlock_match_id TEXT,                     -- Deadlock Match-ID nach Match-Start
    match_duration_s INTEGER,                   -- Match-Dauer in Sekunden
    match_stats TEXT,                           -- JSON mit Spieler-Stats
    -- Zeitstempel:
    scheduled_at TEXT,                          -- Geplanter Start (ISO 8601)
    played_at TEXT                              -- Tatsächlicher Abschluss (ISO 8601)
);
```

### match_results Tabelle

```sql
CREATE TABLE match_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bracket_match_id INTEGER REFERENCES bracket_matches(id),
    group_match_id INTEGER REFERENCES group_matches(id),
    winning_team INTEGER NOT NULL,             -- Team-ID des Gewinners
    duration_s INTEGER,
    player_stats TEXT,                         -- JSON: [{account_id, team, hero_id, kills, deaths, assists, ...}]
    source TEXT NOT NULL DEFAULT 'manual',      -- 'manual' oder 'automatic'
    created_at TEXT DEFAULT (datetime('now'))
);
```

### checkins Tabelle (Schema existiert, Logik NICHT implementiert)

```sql
CREATE TABLE checkins (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_type TEXT NOT NULL,                  -- 'bracket' oder 'group'
    match_id INTEGER NOT NULL,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    discord_id TEXT NOT NULL,
    checked_in_at TEXT DEFAULT (datetime('now'))
);
```

---

## 4. Backend — Funktionen im Detail

### 4.1 Match Manager (`backend/match/manager.py`)

Orchestriert den gesamten Match-Lifecycle. Delegiert GC-Kommunikation an `steam_bridge.py` und Ergebnis-Verarbeitung an `result_processor.py`.

#### `create_lobby(tournament_id, match_id, *, game_mode=1, region_mode=1)`

1. Validiert Match: `_require_match_ready_for_lobby()` — blockiert wenn status ∈ {completed, cancelled, forfeit} oder wenn team1/team2 NULL
2. Prüft auf Duplikate: `_ensure_no_duplicate_lobby_request()` — blockiert wenn `steam_party_id` schon gesetzt oder wenn bereits ein PENDING/RUNNING Task für dieses Match existiert
3. Erstellt Steam-Task: `steam_bridge.create_task("GC_CREATE_CUSTOM_LOBBY", {...})`
4. Pollt auf Ergebnis: `steam_bridge.poll_task_result(task_id, timeout_s=45)`
5. Extrahiert `party_id` und `party_code_display` oder `join_code` aus Result
6. Updated DB: `bracket_matches SET steam_party_id=?, party_code=?, status='lobby_created'`
7. Gibt Result zurück

**Fehlerfall:** `TimeoutError` wenn Bot nicht antwortet (45s), `RuntimeError` wenn Task FAILED

#### `start_match(tournament_id, match_id)`

1. Holt Match, prüft `steam_party_id` vorhanden und Status erlaubt Start
2. **Sequentiell 3 GC-Tasks:**
   - `set_bot_spectator()` → Task `GC_LOBBY_SET_SPECTATOR` (Bot auf Slot 31, 20s timeout)
   - `set_bot_ready()` → Task `GC_LOBBY_READY` (Bot ready, 20s timeout)
   - Task `GC_LOBBY_START_MATCH` (Match starten, 45s timeout)
3. Updated DB: `status='in_progress'`, `deadlock_match_id` gesetzt
4. Gibt `{success, match_id}` zurück

**Wichtig:** Jeder der 3 Schritte muss erfolgreich sein bevor der nächste beginnt.

#### `fetch_match_result(tournament_id, match_id)`

1. Holt `deadlock_match_id` und `steam_party_id` aus DB
2. Erstellt Task `GC_GET_MATCH_RESULT` mit `{match_id, party_id}`
3. Pollt auf Ergebnis (45s timeout)
4. Ruft `apply_bracket_match_result()` auf
5. Gibt vollständiges Result zurück (mit winner_id, duration, players)

#### `leave_lobby(tournament_id, match_id)`

1. Holt `party_id`, erstellt Task `GC_LOBBY_LEAVE`, pollt (20s)
2. Keine DB-Änderung (Lobby-Daten bleiben erhalten)

#### Sicherungen gegen Duplikate

- `_require_match_ready_for_lobby()` prüft: kein steam_party_id, status nicht lobby_created/in_progress
- `_ensure_no_duplicate_lobby_request()` prüft via `steam_bridge.has_active_task()`: kein PENDING/RUNNING Task für dieses Match in der Queue

---

### 4.2 Steam Bridge (`backend/match/steam_bridge.py`)

Abstrahiert die SQLite Task Queue unter `C:\Users\Nani-Admin\Documents\Deadlock\service\deadlock.sqlite3`.

#### `create_task(task_type, payload) → int`

- Schreibt PENDING Task in `steam_tasks` Tabelle
- Räumt vorher hängende RUNNING Tasks auf (`_fail_stale_running_tasks`)
- Gibt Task-ID zurück

#### `poll_task_result(task_id, *, timeout_s=30, poll_interval_s=0.5) → dict`

- Pollt alle 0.5s auf `status IN ('DONE','FAILED')`
- DONE: gibt `json.loads(result)` zurück
- FAILED: gibt `{success: False, error: ...}` zurück
- Timeout: wirft `TimeoutError`

#### `has_active_task(task_type, *, match_id, party_id) → bool`

- Prüft ob PENDING/RUNNING Task für gegebenen Typ + Match existiert
- Nutzt `json_extract(payload, '$.match_id')` für Payload-Filterung

#### `_fail_stale_running_tasks(db)`

- Markiert RUNNING Tasks als FAILED wenn `started_at` älter als 120 Sekunden
- Verhindert dass hängende Tasks die Queue blockieren

---

### 4.3 Result Processor (`backend/match/result_processor.py`)

Zentrale Funktion — wird von Auto-Fetch UND manuellem Ergebnis verwendet.

#### `apply_bracket_match_result(tournament_id, match_id, *, winning_team, duration_s, players, source)`

1. Lädt Match (team1_id, team2_id)
2. Mapping: `winning_team == 0` → team1_id (Amber), `winning_team == 1` → team2_id (Sapphire)
3. Updated `bracket_matches`: winner_id, status='completed', match_duration_s, match_stats (JSON), played_at
4. Fügt Zeile in `match_results` ein (mit source='automatic' oder 'manual')
5. Ruft `advance_bracket_winner()` auf

---

### 4.4 Bracket Advancement (`backend/tournament/engine.py`)

#### `advance_bracket_winner(tournament_id, match_id, winner_id)`

```
Logik:
  next_round   = aktuelle_runde + 1
  next_position = aktuelle_position // 2 (Integer-Division)

  Wenn Position gerade (0, 2, 4...): Gewinner wird team1_id im nächsten Match
  Wenn Position ungerade (1, 3, 5...): Gewinner wird team2_id im nächsten Match
  Wenn kein nächstes Match existiert: Turnier-Sieger (Return)
```

**Beispiel mit 8 Teams:**
```
Runde 0:                    Runde 1:                Runde 2 (Finale):
Pos 0: Team A vs B  ──┐
                       ├──► Pos 0: A? vs D?  ──┐
Pos 1: Team C vs D  ──┘                        ├──► Pos 0: ? vs ?
Pos 2: Team E vs F  ──┐                        │
                       ├──► Pos 1: E? vs H?  ──┘
Pos 3: Team G vs H  ──┘

Wenn Team A gewinnt (Pos 0, gerade):   → Runde 1 Pos 0: team1_id = Team A
Wenn Team D gewinnt (Pos 1, ungerade): → Runde 1 Pos 0: team2_id = Team D
```

#### BYE-Handling

Wenn Teamanzahl keine Zweierpotenz: `generate_bracket()` rundet auf. Matches ohne Gegner bekommen `team2_id=NULL`, `winner_id=team1_id`, `status='completed'`. `_propagate_byes()` schiebt BYE-Gewinner sofort in die nächste Runde.

---

## 5. Admin API Endpoints

Alle Endpoints sind Mod/Admin-geschützt via `require_mod` und loggen in `audit_log`.

### POST `/api/admin/tournaments/{tid}/matches/{mid}/create-lobby`

```
Voraussetzung: status ∈ {pending, checkin}, beide Teams gesetzt, keine existierende Lobby
Response 200: { success, party_id, party_code, join_code }
Response 502: Steam Bot Fehler (RuntimeError)
Response 504: Timeout (Bot antwortet nicht innerhalb 45s)
```

### POST `/api/admin/tournaments/{tid}/matches/{mid}/start`

```
Voraussetzung: steam_party_id vorhanden, status erlaubt Start
Intern: Bot → Spectator (20s) → Ready (20s) → Start (45s)
Response 200: { success, match_id }
Response 502/504: Fehler/Timeout
```

### POST `/api/admin/tournaments/{tid}/matches/{mid}/fetch-result`

```
Voraussetzung: deadlock_match_id oder steam_party_id vorhanden
Nebeneffekt: Bracket wird automatisch weitergeschaltet!
Response 200: { success, match_id, winner_id, winning_team, duration_s, players[] }
Response 502/504: Fehler/Timeout
```

### POST `/api/admin/tournaments/{tid}/matches/{mid}/leave-lobby`

```
Voraussetzung: steam_party_id vorhanden
Response 200: { success }
```

### POST `/api/admin/tournaments/{tid}/matches/{mid}/result` (Manuell)

```
Body: { "winner_id": 7 }
Validierung: winner_id muss team1_id oder team2_id sein
Nebeneffekt: Bracket wird automatisch weitergeschaltet
source='manual' in match_results
Response 200: { status: "ok", winner_id }
```

---

## 6. Frontend — Komponenten

### 6.1 MatchAdminPanel (`components/admin/MatchAdminPanel.tsx`)

Zeigt alle Bracket-Matches bei denen beide Teams gesetzt sind. Pro Match:

**Anzeige:**
- Match-Info: "Runde X, Position Y" + Status-Badge (farbig)
- Teams: "Team A vs Team B"
- Party-Code: Gross, fett, mit Copy-to-Clipboard (bei lobby_created/in_progress)
- Deadlock Match-ID: klein, grau (bei in_progress/completed)

**Button-Logik (Conditional Rendering):**

| Button | Sichtbar wenn | API Call |
|--------|---------------|---------|
| "Lobby erstellen" | Kein steam_party_id UND status ∈ {pending, checkin} | POST create-lobby |
| "Match starten" | status = lobby_created UND steam_party_id vorhanden | POST start |
| "Ergebnis abrufen" | status = in_progress UND deadlock_match_id vorhanden | POST fetch-result |
| "Lobby verlassen" | status ∈ {lobby_created, in_progress} UND steam_party_id vorhanden | POST leave-lobby |
| ManualResultForm | Immer (wenn nicht completed) | POST result |

**State Management:**
- `activeAction` trackt welcher Button gerade lädt
- Loading-Text: "Lobby wird erstellt...", "Match wird gestartet...", etc.
- Error: Rote Box, Success: Grüne Box
- Nach jeder Aktion: `onRefresh()` → invalidiert TanStack Query Cache

### 6.2 ManualResultForm

Radio-Buttons Team 1 / Team 2 → Submit → `submitMatchResult(tournamentId, matchId, {winner_id})`

### 6.3 BracketMatch (`components/bracket/BracketMatch.tsx`)

Match-Karte im Bracket mit Status-Indikator:
- pending/checkin → grauer Kreis
- lobby_created/in_progress → grüner Spinner
- completed/forfeit → grüner Haken
- Gewinner: fett + Primary Color, Verlierer: gedämpft, BYE: kursiv "Freilos"
- Party-Code als kleiner Badge rechts oben

### 6.4 TypeScript-Typen (`types/tournament.ts`)

```typescript
type MatchStatus = 'pending' | 'checkin' | 'lobby_created' | 'in_progress'
                 | 'completed' | 'forfeit' | 'cancelled'

interface BracketMatch {
  id: number; round: number; position: number
  bracket_type: 'winners' | 'losers' | 'final'
  team1_id: number | null; team2_id: number | null; winner_id: number | null
  status: MatchStatus
  steam_party_id: string | null; party_code: string | null
  deadlock_match_id: string | null
  match_duration_s: number | null; match_stats: string | null
  scheduled_at: string | null; played_at: string | null
}

interface LobbyCreateResult  { success: boolean; party_id: string; party_code: string; join_code: string }
interface MatchStartResult   { success: boolean; match_id: number | null }
interface MatchFetchResult   { success: boolean; match_id: number; winner_id: number;
                               winning_team: number; duration_s: number; players: MatchPlayerStats[] }
interface MatchPlayerStats   { account_id: number; team: number; hero_id: number;
                               kills: number; deaths: number; assists: number;
                               net_worth: number; last_hits: number }
```

### 6.5 TanStack Query Hooks (`hooks/useTournament.ts`)

```typescript
useCreateLobby()       // useMutation → invalidiert ['tournaments'] Cache
useStartMatch()        // useMutation → invalidiert Cache
useFetchMatchResult()  // useMutation → invalidiert Cache
useLeaveLobby()        // useMutation → invalidiert Cache
```

---

## 7. Vollständiger Match-Flow (End-to-End)

### Schritt 1: Bracket-Generierung

```
Admin: POST /api/admin/tournaments/{id}/bracket/generate
  → engine.generate_bracket()
    → Top-2-Teams pro Gruppe (nach Punkten)
    → Aufrunden zur nächsten Zweierpotenz
    → Seeding: Seed 1 vs Seed N, Seed 2 vs Seed N-1, etc.
    → BYE-Matches sofort completed + propagiert
```

### Schritt 2: Lobby erstellen

```
Admin klickt "Lobby erstellen" für Match 42
  → POST /create-lobby
  → manager.create_lobby() → steam_bridge.create_task("GC_CREATE_CUSTOM_LOBBY")
  → Steam Bot verarbeitet → GC Party + join_code via SO Cache
  → DB: steam_party_id, party_code, status='lobby_created'
  → Frontend zeigt Party-Code "987-654-321" mit Copy-Button
```

### Schritt 3: Spieler joinen

```
Admin teilt Party-Code (Discord/Website)
  → Spieler: Deadlock → Play → Custom Lobby → Join with Code
  → Alle 12 Spieler (6v6) müssen in der Lobby sein
  → Kein automatischer Check — Admin bestätigt visuell
```

### Schritt 4: Match starten

```
Admin klickt "Match starten"
  → POST /start
  → 3 sequentielle GC-Tasks:
    1. GC_LOBBY_SET_SPECTATOR (Bot → Slot 31)
    2. GC_LOBBY_READY (Bot → ready)
    3. GC_LOBBY_START_MATCH (GC weist Dedicated Server zu)
  → DB: status='in_progress', deadlock_match_id gesetzt
  → Frontend: Status "Läuft" mit pulsierendem Indikator
```

### Schritt 5: Match wird gespielt (30-45 Minuten)

```
Kein Backend-Involvement während des Matches.
Kein automatisches Polling. Admin wartet.
```

### Schritt 6a: Automatisches Ergebnis

```
Admin klickt "Ergebnis abrufen"
  → POST /fetch-result
  → GC_GET_MATCH_RESULT → winning_team, duration, player_stats
  → apply_bracket_match_result():
    → winning_team=0 → winner_id=team1_id (Amber)
    → winning_team=1 → winner_id=team2_id (Sapphire)
    → DB: winner_id, status='completed', duration, stats, played_at
    → INSERT match_results (source='automatic')
    → advance_bracket_winner(): Gewinner in nächste Runde
  → Frontend: Bracket aktualisiert
```

### Schritt 6b: Manuelles Ergebnis (Fallback)

```
Admin wählt Gewinner im ManualResultForm
  → POST /result {winner_id: 7}
  → Validierung: winner_id ∈ {team1_id, team2_id}
  → DB: winner_id, status='completed'
  → INSERT match_results (source='manual')
  → advance_bracket_winner()
  → Frontend: Bracket aktualisiert
```

### Schritt 7: Nächste Runde

```
Gewinner erscheint automatisch im nächsten Match.
Sobald beide Teams gesetzt → Admin kann erneut "Lobby erstellen" klicken.
Zyklus wiederholt sich bis zum Finale.
```

---

## 8. Implementierungsstatus

### Vollständig implementiert ✅

| Feature | Backend | Frontend |
|---------|---------|----------|
| Lobby erstellen (mit Duplikat-Schutz) | ✅ manager + bridge + route | ✅ Button + API |
| Match starten (Spectator → Ready → Start) | ✅ 3-Step-Sequenz | ✅ Button + API |
| Ergebnis abrufen (automatisch via GC) | ✅ Fetch + Result Processor | ✅ Button + API |
| Lobby verlassen | ✅ manager + bridge + route | ✅ Button + API |
| Manuelles Ergebnis (Fallback) | ✅ Route + Validierung | ✅ ManualResultForm |
| Bracket-Advancement (Gewinner → nächste Runde) | ✅ advance_bracket_winner() | ✅ Bracket-Refresh |
| BYE-Handling (Freilose) | ✅ generate + propagate | ✅ "Freilos" Anzeige |
| Party-Code Anzeige + Copy | ✅ In DB gespeichert | ✅ Copy-to-Clipboard |
| Match-Status State Machine (7 Werte) | ✅ Backend Enum | ✅ Farbige Badges |
| Audit Logging (alle Endpoints) | ✅ audit_log Tabelle | — |
| Spieler-Stats Speicherung | ✅ match_stats JSON | ❌ Nicht angezeigt |
| Stale-Task-Cleanup (hängende RUNNING Tasks) | ✅ 120s Timeout → FAILED | — |

### NICHT implementiert ❌

| Feature | Aktueller Stand | Was fehlt | Priorität |
|---------|----------------|-----------|-----------|
| **Check-in System** | `checkin.py` = 2 Zeilen Stub, DB-Tabelle existiert | Logik, Routes, Frontend | Hoch |
| **Match Scheduler** | `scheduler.py` = 2 Zeilen Stub | Background Tasks, Auto-Timeouts, APScheduler | Mittel |
| **Echtzeit-Updates** | Fehlt komplett | Kein WebSocket/SSE/Polling — nur manueller Refresh | Mittel |
| **Discord Webhooks** | `DISCORD_WEBHOOK_URL` in config, wird nicht verwendet | Notification-Logik für Match-Events | Mittel |
| **Forfeit/Cancelled** | Status-Werte existieren, kein Endpoint/UI | Admin-Endpoint + Button | Mittel |
| **Gruppen-Matches via Steam** | Nur manuelle Ergebnisse | Steam-Felder in group_matches, Integration | Niedrig |
| **Spieler-Stats Frontend** | Stats in DB gespeichert (match_stats JSON) | Anzeige-Komponente | Niedrig |

---

## 9. Was noch zu tun ist — Detail

### 9.1 Check-in System (`backend/tournament/checkin.py`)

**Aktuell:** Nur Docstring, keine Funktionen.

**Zu implementieren:**
1. `open_checkin(match_id, deadline_minutes=15)` — Öffnet Check-in, setzt status='checkin'
2. `checkin_player(match_id, discord_id)` — Spieler checkt ein, Eintrag in `checkins` Tabelle
3. `get_checkin_status(match_id)` — Welche Spieler pro Team eingecheckt
4. `close_checkin(match_id)` — Prüft ob alle da, sonst Forfeit für fehlendes Team
5. API Endpoints: Admin öffnet Check-in, Spieler checkt ein (public mit Auth)
6. Frontend: Check-in-Button für Spieler, Status-Übersicht für Admin

### 9.2 Match Scheduler (`backend/tournament/scheduler.py`)

**Aktuell:** Nur Docstring, keine Funktionen.

**Zu implementieren:**
1. Background-Task-System (APScheduler oder FastAPI lifespan)
2. Auto-Check-in-Deadline: Nach X Minuten ohne Check-in → Forfeit
3. Lobby-Timeout: Lobby erstellt aber nach 15min kein Match → Warnung
4. Optional: Auto-Ergebnis-Polling nach Match-Start (statt Admin-Klick)

### 9.3 Echtzeit-Updates

**Aktuell:** Frontend pollt nicht automatisch, nur manueller Refresh via onRefresh().

**Empfehlung:** SSE (Server-Sent Events) für Admin-Panel, Polling (30s) für öffentliches Bracket.

### 9.4 Discord Webhooks

**Aktuell:** `DISCORD_WEBHOOK_URL` in config, nirgends verwendet.

**Zu implementieren:**
- Webhook-Funktion: `send_discord_notification(title, message, color)`
- Events: Lobby erstellt (mit Party-Code), Match gestartet, Match beendet (Gewinner), Check-in geöffnet

### 9.5 Forfeit/Cancelled

**Zu implementieren:**
1. `POST /api/admin/tournaments/{tid}/matches/{mid}/forfeit` mit `{forfeiting_team_id}`
2. Setzt winner_id auf gegnerisches Team, ruft `advance_bracket_winner()` auf
3. `POST .../cancel` — setzt status='cancelled'
4. Frontend: Forfeit/Cancel Buttons im MatchAdminPanel

### 9.6 Gruppen-Matches via Steam (Zukunft)

Gleicher Flow wie Bracket möglich. Voraussetzung: `group_matches` Tabelle bräuchte die Steam-Felder (steam_party_id, party_code, deadlock_match_id, match_duration_s, match_stats).

---

## 10. Architektur-Entscheidungen

### Warum Poll statt Push für Steam-Ergebnisse?

Backend pollt `steam_tasks` statt Callback. Gründe:
- **Einfach:** Kein WebSocket/HTTP-Callback zwischen Bot und Backend
- **Robust:** Bei Backend-Restart einfach weiterpolten
- **Entkoppelt:** Bot und Backend haben keine direkte Verbindung
- **Nachteil:** ~0.5s Latenz (Poll-Intervall) — für Turniere akzeptabel

### Warum 3-Step Match-Start?

Der GC behandelt den Bot als regulären Spieler:
1. **Spectator:** Ohne Slot 31 blockiert Bot einen der 12 Spielerplätze
2. **Ready:** GC verlangt dass ALLE Members (inkl. Spectator) ready sind
3. **Start:** Erst wenn alle ready → GC weist Dedicated Server zu

### Warum separate match_results Tabelle?

- **Quellen-Tracking:** `source` unterscheidet auto vs manual
- **Wiederverwendbar:** Gleiche Tabelle für group_matches und bracket_matches
- **Audit:** match_results ist append-only, bracket_matches wird in-place updated

### Warum Stale-Task-Cleanup (120s)?

Wenn der Steam Bot crasht während ein Task RUNNING ist, würde der Task ewig in RUNNING hängen. `_fail_stale_running_tasks()` markiert Tasks die länger als 120s RUNNING sind automatisch als FAILED. Wird bei jedem `create_task()` und `get_task()` Aufruf ausgeführt.

---

## 11. Referenzen

| Ressource | Pfad |
|-----------|------|
| Steam Bridge GC-Detail | `docs/steam-bridge-implementation.md` |
| Match Manager | `backend/match/manager.py` |
| Steam Bridge | `backend/match/steam_bridge.py` |
| Result Processor | `backend/match/result_processor.py` |
| Tournament Engine | `backend/tournament/engine.py` |
| Admin Routes | `backend/tournament/admin_routes.py` |
| DB Schema | `backend/db.py` |
| Check-in Stub | `backend/tournament/checkin.py` |
| Scheduler Stub | `backend/tournament/scheduler.py` |
| Frontend Admin Panel | `frontend/src/components/admin/MatchAdminPanel.tsx` |
| Frontend Bracket | `frontend/src/components/bracket/BracketView.tsx` |
| Frontend API | `frontend/src/api/client.ts` |
| Frontend Types | `frontend/src/types/tournament.ts` |
| Frontend Hooks | `frontend/src/hooks/useTournament.ts` |
