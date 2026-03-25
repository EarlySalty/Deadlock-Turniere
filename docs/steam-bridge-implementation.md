# Steam Bridge — Custom Match Integration

## 1. Übersicht & Architektur

Die Deadlock-Turniere-Plattform benötigt eine Integration mit dem bestehenden Steam Bot, um Custom Lobbies für Turnier-Matches automatisiert zu erstellen, zu starten und deren Ergebnisse abzurufen.

### Beteiligte Systeme

| System | Technologie | Port/Pfad |
|--------|-------------|-----------|
| Tournament Backend | FastAPI (Python) | Port 8900 |
| Steam Bot | Node.js | `C:\Users\Nani-Admin\Documents\Deadlock\cogs\steam\steam_presence\` |
| Task Queue | SQLite | `C:\Users\Nani-Admin\Documents\Deadlock\service\deadlock.sqlite3` |
| Deadlock GC | Valve Game Coordinator | Steam-Netzwerk |

### Kommunikationsfluss

Das Tournament Backend und der Steam Bot kommunizieren **nicht direkt**, sondern über eine SQLite-basierte Task Queue (`steam_tasks` Tabelle). Diese Entkopplung ermöglicht:

- Unabhängige Restarts beider Systeme
- Persistente Tasks die auch bei Bot-Neustart nicht verloren gehen
- Einfaches Debugging durch Einblick in die Queue

### Architektur-Diagramm

```
[Tournament Backend] --write--> [steam_tasks DB] --read--> [Steam Bot/Node.js]
   (FastAPI)                    (SQLite Queue)                    |
   Port 8900                                                      v
                                                           [Deadlock GC]
                                                            (Valve Steam)
                                                                  |
                                                                  v
[Tournament Backend] <--poll--- [steam_tasks DB] <--write-- [Steam Bot]
```

**Detaillierter Ablauf:**

1. Das Tournament Backend schreibt einen Task mit `status='PENDING'` und einem JSON-Payload in die `steam_tasks` Tabelle.
2. Der Steam Bot pollt periodisch nach `PENDING` Tasks, nimmt einen auf (`status='RUNNING'`).
3. Der Steam Bot sendet die entsprechende GC-Message an den Deadlock Game Coordinator.
4. Der GC antwortet (Response-Message + SO Cache Updates).
5. Der Steam Bot schreibt das Ergebnis als JSON in `steam_tasks.result` und setzt `status='DONE'` (oder `status='FAILED'` + `error`).
6. Das Tournament Backend pollt auf die Status-Änderung und verarbeitet das Ergebnis.

---

## 2. steam_tasks Tabelle — Schema & Protokoll

### Schema

```sql
CREATE TABLE steam_tasks(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  type TEXT NOT NULL,                       -- Task-Typ (z.B. 'GC_CREATE_CUSTOM_LOBBY')
  payload TEXT,                             -- JSON-encoded Payload
  status TEXT NOT NULL DEFAULT 'PENDING',   -- PENDING -> RUNNING -> DONE/FAILED
  result TEXT,                              -- JSON-encoded Ergebnis (bei DONE)
  error TEXT,                               -- Fehlermeldung (bei FAILED)
  created_at INTEGER NOT NULL,              -- Unix timestamp in Millisekunden
  updated_at INTEGER NOT NULL,              -- Unix timestamp in Millisekunden
  started_at INTEGER,                       -- Zeitpunkt der Verarbeitung (ms)
  finished_at INTEGER                       -- Zeitpunkt des Abschlusses (ms)
);
```

### Status-Lifecycle

```
PENDING  -->  RUNNING  -->  DONE
                       -->  FAILED
```

| Status | Bedeutung |
|--------|-----------|
| `PENDING` | Task wurde vom Backend erstellt, wartet auf Verarbeitung |
| `RUNNING` | Steam Bot hat den Task übernommen, GC-Kommunikation läuft |
| `DONE` | Erfolgreich abgeschlossen, `result` enthält JSON-Ergebnis |
| `FAILED` | Fehlgeschlagen, `error` enthält Fehlerbeschreibung |

### Protokoll-Regeln

1. **Backend** schreibt Task mit `status='PENDING'`, `payload` als JSON-String, `created_at` und `updated_at` als Unix-Timestamp in Millisekunden.
2. **Steam Bot** pollt nach `WHERE status = 'PENDING' ORDER BY created_at ASC LIMIT 1`, setzt `status='RUNNING'`, `started_at`, `updated_at`.
3. **Steam Bot** nach GC-Antwort: setzt `status='DONE'` + `result` (JSON), oder `status='FAILED'` + `error`. Setzt `finished_at` und `updated_at`.
4. **Backend** pollt auf `WHERE id = ? AND status IN ('DONE', 'FAILED')`.

### Concurrency

Da nur ein einziger Steam Bot und ein einziges Tournament Backend existieren, gibt es keine Race Conditions. Trotzdem wird empfohlen, den Status-Update atomar durchzuführen:

```sql
-- Bot nimmt Task auf (atomar):
UPDATE steam_tasks SET status = 'RUNNING', started_at = ?, updated_at = ?
WHERE id = ? AND status = 'PENDING';
```

---

## 3. Neue Task Types für Custom Matches

### 3.1 GC_CREATE_CUSTOM_LOBBY

**Zweck:** Erstellt eine private Custom Lobby über den Deadlock Game Coordinator.

**Payload (Backend -> Bot):**
```json
{
  "tournament_id": 1,
  "match_id": 42,
  "game_mode": 1,
  "region_mode": 1
}
```

| Feld | Typ | Beschreibung |
|------|-----|-------------|
| `tournament_id` | int | ID des Turniers |
| `match_id` | int | ID des Bracket-Matches |
| `game_mode` | int | `1` = Normal (6v6), `2` = 1v1 Test, `3` = Sandbox |
| `region_mode` | int | `0` = ROW, `1` = Europe, `2` = SEAsia, etc. |

**GC Message:** `CMsgClientToGCPartyCreate` (ID: 9123)

Protobuf-Felder:
- `is_private_lobby = true` (field 5, bool) — Erzwingt Private Lobby
- `game_mode = 1` (field 13, ECitadelGameMode) — Normal-Modus
- `region_mode = 1` (field 6, ECitadelRegionMode) — Europa
- Optional: `private_lobby_settings` (field 9, PrivateLobbySettings) — Map, Cheats, Min-Players, etc.

**GC Response:** `CMsgClientToGCPartyCreateResponse` (ID: 9124)
- `result` (field 1, EResponse) — `1` = Success
- `party_id` (field 2, fixed64) — Die Party-ID für alle weiteren Operationen

**SO Cache Push:** Nach der Party-Erstellung pusht der GC automatisch ein `CSOCitadelParty`-Objekt in den SO Cache des Bots. Dieses enthält den **join_code** — der Party-Code den Spieler im Spiel eingeben müssen.

**Wichtig:** Die Response-Message (9124) enthält die `party_id`, aber **nicht** den `join_code`. Der `join_code` kommt nur über den SO Cache Push als Teil des `CSOCitadelParty`-Objekts. Der Bot muss also:
1. Auf die Response (9124) warten -> `party_id` merken
2. Auf den SO Cache Update warten -> `join_code` extrahieren
3. Beides im Task-Result zusammenführen

**Result (Bot -> Backend):**
```json
{
  "success": true,
  "party_id": "12345678901234",
  "join_code": "987654321",
  "party_code_display": "987-654-321"
}
```

| Feld | Typ | Beschreibung |
|------|-----|-------------|
| `success` | bool | Ob die Lobby erfolgreich erstellt wurde |
| `party_id` | string | Die Party-ID (als String wegen uint64) |
| `join_code` | string | Der rohe Join-Code |
| `party_code_display` | string | Formatierter Code für Anzeige (XXX-XXX-XXX) |

---

### 3.2 GC_LOBBY_SET_SPECTATOR

**Zweck:** Setzt den Bot auf den Spectator-Slot, damit er keinen Spielerplatz (1-12) blockiert. Der Bot muss in der Lobby bleiben um Kontrolle zu behalten, darf aber nicht als Spieler zählen.

**Payload (Backend -> Bot):**
```json
{
  "party_id": "12345678901234"
}
```

**GC Message:** `CMsgClientToGCPartyAction` (ID: 9129)

| Field | Typ | Wert | Beschreibung |
|-------|-----|------|-------------|
| `party_id` | fixed64 | (aus Payload) | Die Party-ID |
| `target_account_id` | uint32 | (Bot's eigene AccountID) | Ziel-Spieler |
| `action_id` | uint32 | `10` (`k_eSetPlayerSlot`) | Aktion: Slot setzen |
| `uint_value` | uint32 | `31` | Slot 31 = Spectator |

**GC Response:** `CMsgClientToGCPartyActionResponse` (ID: 9130)
- `result` (field 1) — EResponse, `1` = Success

**Result (Bot -> Backend):**
```json
{
  "success": true
}
```

---

### 3.3 GC_LOBBY_READY

**Zweck:** Setzt den Bot auf "Ready"-Status. Notwendig bevor das Match gestartet werden kann — der GC verlangt, dass alle Members (inkl. Spectator) ready sind.

**Payload (Backend -> Bot):**
```json
{
  "party_id": "12345678901234"
}
```

**GC Message:** `CMsgClientToGCPartySetReadyState` (ID: 9142)

| Field | Typ | Wert | Beschreibung |
|-------|-----|------|-------------|
| `party_id` | fixed64 | (aus Payload) | Die Party-ID |
| `ready_state` | bool | `true` | Ready-Status setzen |

**GC Response:** `CMsgClientToGCPartySetReadyStateResponse` (ID: 9143)
- `result` (field 1) — EResponse, `1` = Success

**Result (Bot -> Backend):**
```json
{
  "success": true
}
```

---

### 3.4 GC_LOBBY_START_MATCH

**Zweck:** Startet das Custom Match. Alle Spieler müssen in der Lobby und ready sein.

**Payload (Backend -> Bot):**
```json
{
  "party_id": "12345678901234"
}
```

**GC Message:** `CMsgClientToGCPartyStartMatch` (ID: 9131)

| Field | Typ | Wert | Beschreibung |
|-------|-----|------|-------------|
| `party_id` | fixed64 | (aus Payload) | Die Party-ID |

**GC Response:** `CMsgClientToGCPartyStartMatchResponse` (ID: 9132)
- `result` (field 1) — EResponse, `1` = Success

**SO Cache Update:** Nach dem Match-Start updated der GC das `CSOCitadelParty`-Objekt mit Match-Informationen. Wenn der Dedicated Server zugewiesen wird, wird eine `match_id` verfügbar (entweder im Party-Objekt oder als separates `CSOCitadelLobby`-Objekt).

**Result (Bot -> Backend):**
```json
{
  "success": true,
  "match_id": 5678901234
}
```

| Feld | Typ | Beschreibung |
|------|-----|-------------|
| `success` | bool | Ob der Match-Start erfolgreich war |
| `match_id` | int/null | Die Deadlock Match-ID (wenn bereits verfügbar) |

---

### 3.5 GC_LOBBY_LEAVE

**Zweck:** Bot verlässt die Lobby nach dem Match-Start. Optional — der Bot kann auch in der Lobby bleiben um weiterhin SO Cache Updates (z.B. Match-Ende) zu erhalten.

**Payload (Backend -> Bot):**
```json
{
  "party_id": "12345678901234"
}
```

**GC Message:** `CMsgClientToGCPartyLeave` (ID: 9125)

| Field | Typ | Wert | Beschreibung |
|-------|-----|------|-------------|
| `party_id` | fixed64 | (aus Payload) | Die Party-ID |

**GC Response:** `CMsgClientToGCPartyLeaveResponse` (ID: 9126)
- `result` (field 1) — EResponse

**Result (Bot -> Backend):**
```json
{
  "success": true
}
```

---

### 3.6 GC_GET_MATCH_RESULT

**Zweck:** Holt das Match-Ergebnis (Gewinner-Team, Dauer, Spieler-Stats) nach Spielende.

**Payload (Backend -> Bot):**
```json
{
  "match_id": 5678901234
}
```

**GC Message:** `CMsgClientToGCGetMatchMetaData` oder vergleichbare Match-Query-Message.
- `match_id` (field 1) — Die Deadlock Match-ID

**Match-Ergebnis enthält:**
- `winning_team` — 0 (Amber/Team 1) oder 1 (Sapphire/Team 2)
- `duration_s` — Match-Dauer in Sekunden
- Spieler-Stats pro Player (Kills, Deaths, Assists, Net Worth, Last Hits, Hero)

**Result (Bot -> Backend):**
```json
{
  "success": true,
  "match_id": 5678901234,
  "winning_team": 0,
  "duration_s": 1845,
  "players": [
    {
      "account_id": 123456,
      "team": 0,
      "hero_id": 15,
      "kills": 8,
      "deaths": 3,
      "assists": 12,
      "net_worth": 45000,
      "last_hits": 210
    }
  ]
}
```

| Feld | Typ | Beschreibung |
|------|-----|-------------|
| `match_id` | int | Die Deadlock Match-ID |
| `winning_team` | int | `0` = Amber (Team 1), `1` = Sapphire (Team 2) |
| `duration_s` | int | Match-Dauer in Sekunden |
| `players` | array | Array mit Spieler-Stats |
| `players[].account_id` | int | Steam Account-ID (32-bit) |
| `players[].team` | int | Team-Zugehörigkeit (0 oder 1) |
| `players[].hero_id` | int | ID des gespielten Helden |
| `players[].kills` | int | Kills |
| `players[].deaths` | int | Deaths |
| `players[].assists` | int | Assists |
| `players[].net_worth` | int | Nettovermögen am Spielende |
| `players[].last_hits` | int | Last Hits (Creep Score) |

---

## 4. Protobuf-Definitionen (Komplett)

### 4.1 Benötigte .proto Dateien

**Quelle:** https://github.com/SteamDatabase/Protobufs/tree/master/deadlock

Relevante Dateien:
| Datei | Inhalt |
|-------|--------|
| `citadel_gcmessages_client.proto` | Party-Messages (Create, Leave, Action, StartMatch, SetReady) |
| `citadel_gcmessages_common.proto` | Shared Types (CSOCitadelParty, Enums, etc.) |
| `steammessages.proto` | Basis-Types (SO Cache Messages) |

Diese Dateien müssen heruntergeladen und in das Steam Bot Projekt eingebunden werden, damit `protobufjs` sie laden kann.

### 4.2 Message IDs (GC)

```
ID      Message                                    Richtung
------  -----------------------------------------  ------------------
9123    CMsgClientToGCPartyCreate                   Client -> GC
9124    CMsgClientToGCPartyCreateResponse           GC -> Client
9125    CMsgClientToGCPartyLeave                    Client -> GC
9126    CMsgClientToGCPartyLeaveResponse            GC -> Client
9129    CMsgClientToGCPartyAction                   Client -> GC
9130    CMsgClientToGCPartyActionResponse            GC -> Client
9131    CMsgClientToGCPartyStartMatch               Client -> GC
9132    CMsgClientToGCPartyStartMatchResponse       GC -> Client
9142    CMsgClientToGCPartySetReadyState            Client -> GC
9143    CMsgClientToGCPartySetReadyStateResponse    GC -> Client
```

**Hinweis:** Die Message-ID wird beim Senden mit der `PROTO_MASK` (0x80000000) ver-OR-t. Beim Empfangen muss die Mask wieder entfernt werden:

```javascript
// Senden:
const msgIdWithMask = 0x80000000 | 9123;
// Empfangen:
const msgId = receivedType & ~0x80000000;
```

### 4.3 Enums

```
ECitadelMatchMode:
  k_ECitadelMatchMode_Invalid       = 0
  k_ECitadelMatchMode_Unranked      = 1
  k_ECitadelMatchMode_PrivateLobby  = 2
  k_ECitadelMatchMode_CoopBot       = 3
  k_ECitadelMatchMode_Ranked        = 4
```

```
ECitadelGameMode:
  k_ECitadelGameMode_Invalid  = 0
  k_ECitadelGameMode_Normal   = 1   // Standard 6v6
  k_ECitadelGameMode_1v1Test  = 2   // 1v1 Test-Modus
  k_ECitadelGameMode_Sandbox  = 3   // Sandbox
```

```
ECitadelRegionMode:
  k_ECitadelRegionMode_ROW      = 0   // Rest of World
  k_ECitadelRegionMode_Europe   = 1   // Europa
  k_ECitadelRegionMode_SEAsia   = 2   // Südostasien
  k_ECitadelRegionMode_SAmerica = 3   // Südamerika
  k_ECitadelRegionMode_Russia   = 4   // Russland
  k_ECitadelRegionMode_Oceania  = 5   // Ozeanien
```

```
EAction (PartyAction, für CMsgClientToGCPartyAction):
  k_eKickUser        = 0    // Spieler aus Lobby kicken
  k_eSetPlayerType   = 1    // Spielertyp ändern
  k_eSetMemberTeam   = 2    // Spieler-Team setzen (0=Amber, 1=Sapphire)
  k_eSetPlayerSlot   = 3    // Spieler-Slot setzen (0-11=Spieler, 31=Spectator)
  k_eShuffleLobby    = 4    // Teams zufällig mischen
```

**Hinweis zu k_eSetPlayerSlot:** Der Wert des `action_id`-Feldes ist kontextabhängig. In der aktuellen Protobuf-Definition (Stand: SteamDatabase) hat `k_eSetPlayerSlot` den enum-Wert `10`. Dies kann sich ändern — immer die aktuelle .proto-Datei prüfen.

### 4.4 CSOCitadelParty Struktur (SO Cache)

Das zentrale Datenobjekt das der GC für jede Party/Lobby im SO Cache hält:

```protobuf
message CSOCitadelParty {
  optional uint64 party_id = 1;                              // Eindeutige Party-ID
  repeated Member members = 2;                               // Alle Members der Lobby
  repeated Invite invites = 3;                               // Offene Einladungen
  optional string chat_name = 4;                             // Chat-Kanal-Name
  optional uint64 join_code = 6;                             // PARTY-CODE (das was Spieler eingeben)
  optional ECitadelLobbyTeam team_preference = 7;            // Team-Präferenz
  optional ECitadelMatchMode match_mode = 9;                 // Match-Modus
  optional ECitadelGameMode game_mode = 10;                  // Spiel-Modus
  optional bool is_private_lobby = 16;                       // true bei Custom Lobbies
  optional PrivateLobbySettings private_lobby_settings = 17; // Lobby-Einstellungen

  message Member {
    optional uint32 account_id = 1;        // Steam AccountID (32-bit)
    optional ECitadelLobbyTeam team = 5;   // Team (Amber=0, Sapphire=1)
    optional uint32 player_slot = 6;       // Slot (0-11=Spieler, 31=Spectator)
    optional bool is_ready = 11;           // Ready-Status
    optional uint32 hero_id = 17;          // Gewählter Held
  }

  message PrivateLobbySettings {
    optional uint32 cheats_enabled = 2;    // Cheats an/aus
    optional uint32 min_players = 4;       // Min. Spieler zum Starten
    // Weitere Lobby-Settings (Map, etc.)
  }
}
```

**Wichtige Felder für die Integration:**
- `party_id` — Wird für ALLE nachfolgenden Operationen benötigt
- `join_code` — Der Code den Spieler im Deadlock-Client eingeben um der Lobby beizutreten
- `members` — Zeigt wer in der Lobby ist, in welchem Team und Slot
- `members[].is_ready` — Zeigt ob ein Spieler ready ist (relevant für Match-Start)

---

## 5. Implementierung Phase A — Steam Bot (Node.js)

### 5.1 Dateien und Pfade

**Basis-Verzeichnis:** `C:\Users\Nani-Admin\Documents\Deadlock\cogs\steam\steam_presence\`

| Datei | Aktion | Beschreibung |
|-------|--------|-------------|
| `src/custom_lobby.js` | **NEU** | Komplettes Custom-Lobby-Modul |
| `src/tasks.js` | **ÄNDERN** | Neue Task-Types im Switch registrieren |
| `src/events.js` | **ÄNDERN** | GC Response Handler + SO Cache Handler |

### 5.2 custom_lobby.js — Modul-Struktur

Folgt dem bestehenden Pattern im Bot: Export ist eine Funktion die `sharedCtx` empfängt und ein Objekt mit Handler-Funktionen zurückgibt.

```javascript
// src/custom_lobby.js
module.exports = function customLobbyModule(sharedCtx) {
  const { client, log, db, PROTO_MASK, DEADLOCK_APP_ID } = sharedCtx;
  const protobuf = require('protobufjs');

  // ============================================================
  // State
  // ============================================================

  // SO Cache: party_id -> CSOCitadelParty decoded data
  const partyCache = new Map();

  // Pending Promises: Message-ID -> { resolve, reject, timeout }
  // Damit wir auf GC-Responses warten können
  const pendingResponses = new Map();

  // ============================================================
  // Hilfsfunktionen
  // ============================================================

  /**
   * Sendet eine GC-Message und wartet auf die Response.
   * @param {number} sendMsgId - Message ID zum Senden (z.B. 9123)
   * @param {Buffer} payload - Encoded Protobuf Payload
   * @param {number} responseMsgId - Erwartete Response Message ID (z.B. 9124)
   * @param {number} timeoutMs - Timeout in ms (default: 15000)
   * @returns {Promise<Object>} Decoded Response
   */
  function sendAndWaitForResponse(sendMsgId, payload, responseMsgId, timeoutMs = 15000) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pendingResponses.delete(responseMsgId);
        reject(new Error(`GC Response timeout for msgId ${responseMsgId} after ${timeoutMs}ms`));
      }, timeoutMs);

      pendingResponses.set(responseMsgId, {
        resolve: (data) => {
          clearTimeout(timer);
          pendingResponses.delete(responseMsgId);
          resolve(data);
        },
        reject: (err) => {
          clearTimeout(timer);
          pendingResponses.delete(responseMsgId);
          reject(err);
        },
      });

      // An GC senden
      client.sendToGC(DEADLOCK_APP_ID, PROTO_MASK | sendMsgId, {}, payload);
      log.info(`[CustomLobby] Sent GC message ${sendMsgId}, waiting for ${responseMsgId}...`);
    });
  }

  /**
   * Wartet auf einen SO Cache Update der den join_code enthält.
   * @param {string} partyId - Die Party-ID
   * @param {number} timeoutMs - Timeout in ms
   * @returns {Promise<Object>} Das CSOCitadelParty Objekt mit join_code
   */
  function waitForJoinCode(partyId, timeoutMs = 10000) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error(`Timeout waiting for join_code for party ${partyId}`));
      }, timeoutMs);

      // Poll partyCache alle 200ms
      const interval = setInterval(() => {
        const party = partyCache.get(partyId);
        if (party && party.join_code) {
          clearInterval(interval);
          clearTimeout(timer);
          resolve(party);
        }
      }, 200);
    });
  }

  /**
   * Formatiert einen join_code als XXX-XXX-XXX für die Anzeige.
   */
  function formatPartyCode(joinCode) {
    const str = String(joinCode);
    // Format: XXX-XXX-XXX (3er-Gruppen von rechts)
    const parts = [];
    for (let i = str.length; i > 0; i -= 3) {
      parts.unshift(str.slice(Math.max(0, i - 3), i));
    }
    return parts.join('-');
  }

  // ============================================================
  // Task Handler
  // ============================================================

  async function createCustomLobby(task) {
    const payload = JSON.parse(task.payload);
    log.info(`[CustomLobby] Creating lobby for tournament=${payload.tournament_id} match=${payload.match_id}`);

    // CMsgClientToGCPartyCreate encodieren
    // Felder: is_private_lobby=true, game_mode, region_mode
    const encoded = encodePartyCreate({
      is_private_lobby: true,
      game_mode: payload.game_mode || 1,   // Normal
      region_mode: payload.region_mode || 1, // Europe
    });

    // Senden und auf Response warten
    const response = await sendAndWaitForResponse(9123, encoded, 9124, 15000);

    if (response.result !== 1) {
      return { success: false, error: `GC returned result=${response.result}` };
    }

    const partyId = String(response.party_id);
    log.info(`[CustomLobby] Party created: ${partyId}, waiting for join_code...`);

    // Auf SO Cache Update mit join_code warten
    const partyData = await waitForJoinCode(partyId, 10000);
    const joinCode = String(partyData.join_code);

    log.info(`[CustomLobby] Got join_code: ${joinCode}`);

    return {
      success: true,
      party_id: partyId,
      join_code: joinCode,
      party_code_display: formatPartyCode(joinCode),
    };
  }

  async function setSpectator(task) {
    const payload = JSON.parse(task.payload);
    const partyId = payload.party_id;
    log.info(`[CustomLobby] Setting bot to spectator in party ${partyId}`);

    // CMsgClientToGCPartyAction encodieren
    const encoded = encodePartyAction({
      party_id: partyId,
      target_account_id: sharedCtx.botAccountId, // Bot's eigene AccountID
      action_id: 10, // k_eSetPlayerSlot
      uint_value: 31, // Spectator Slot
    });

    const response = await sendAndWaitForResponse(9129, encoded, 9130, 10000);

    if (response.result !== 1) {
      return { success: false, error: `GC returned result=${response.result}` };
    }

    return { success: true };
  }

  async function setReady(task) {
    const payload = JSON.parse(task.payload);
    const partyId = payload.party_id;
    log.info(`[CustomLobby] Setting ready state in party ${partyId}`);

    // CMsgClientToGCPartySetReadyState encodieren
    const encoded = encodeSetReadyState({
      party_id: partyId,
      ready_state: true,
    });

    const response = await sendAndWaitForResponse(9142, encoded, 9143, 10000);

    if (response.result !== 1) {
      return { success: false, error: `GC returned result=${response.result}` };
    }

    return { success: true };
  }

  async function startMatch(task) {
    const payload = JSON.parse(task.payload);
    const partyId = payload.party_id;
    log.info(`[CustomLobby] Starting match in party ${partyId}`);

    // CMsgClientToGCPartyStartMatch encodieren
    const encoded = encodeStartMatch({
      party_id: partyId,
    });

    const response = await sendAndWaitForResponse(9131, encoded, 9132, 20000);

    if (response.result !== 1) {
      return { success: false, error: `GC returned result=${response.result}` };
    }

    // match_id kommt möglicherweise asynchron über SO Cache
    // Hier erstmal ohne match_id zurückgeben, später per GC_GET_MATCH_RESULT abfragen
    return {
      success: true,
      match_id: response.match_id || null,
    };
  }

  async function leaveLobby(task) {
    const payload = JSON.parse(task.payload);
    const partyId = payload.party_id;
    log.info(`[CustomLobby] Leaving party ${partyId}`);

    const encoded = encodePartyLeave({
      party_id: partyId,
    });

    const response = await sendAndWaitForResponse(9125, encoded, 9126, 10000);

    // Party aus Cache entfernen
    partyCache.delete(partyId);

    return { success: true };
  }

  async function getMatchResult(task) {
    const payload = JSON.parse(task.payload);
    const matchId = payload.match_id;
    log.info(`[CustomLobby] Fetching result for match ${matchId}`);

    const encoded = encodeGetMatchMetaData({
      match_id: matchId,
    });

    // Message-ID und Response-ID hängen von der konkreten Proto-Definition ab
    // CMsgClientToGCGetMatchMetaData und Response
    const response = await sendAndWaitForResponse(
      /* sendMsgId */ 9091, // Beispiel — exakte ID aus Proto prüfen
      encoded,
      /* responseMsgId */ 9092, // Beispiel — exakte ID aus Proto prüfen
      30000 // Längerer Timeout, da GC den Match ggf. erst laden muss
    );

    if (!response || response.result !== 1) {
      return { success: false, error: 'Match result not available' };
    }

    // Response parsen — Struktur hängt von der konkreten Proto ab
    const players = (response.players || []).map(p => ({
      account_id: p.account_id,
      team: p.team,
      hero_id: p.hero_id,
      kills: p.kills || 0,
      deaths: p.deaths || 0,
      assists: p.assists || 0,
      net_worth: p.net_worth || 0,
      last_hits: p.last_hits || 0,
    }));

    return {
      success: true,
      match_id: matchId,
      winning_team: response.winning_team,
      duration_s: response.duration_s,
      players,
    };
  }

  // ============================================================
  // Encoding-Funktionen (Protobuf)
  // ============================================================

  // Diese Funktionen müssen die Proto-Definitionen laden und Messages encodieren.
  // Konkretes Pattern hängt davon ab, wie protobufjs im Bot eingebunden ist.
  //
  // Beispiel mit protobufjs:
  //   const root = protobuf.loadSync('path/to/citadel_gcmessages_client.proto');
  //   const MsgType = root.lookupType('CMsgClientToGCPartyCreate');
  //   const encoded = MsgType.encode(MsgType.create({ ... })).finish();
  //
  // Alternativ: Manuelles Buffer-Encoding (wie teilweise im Bot verwendet)

  function encodePartyCreate(data) {
    // TODO: Implementierung mit protobufjs oder manuellem Encoding
    // Felder: is_private_lobby (field 5, bool), game_mode (field 13), region_mode (field 6)
    throw new Error('Not implemented — Proto encoding needed');
  }

  function encodePartyAction(data) {
    // Felder: party_id (field 1, fixed64), target_account_id (field 2, uint32),
    //         action_id (field 3, uint32), uint_value (field 4, uint32)
    throw new Error('Not implemented — Proto encoding needed');
  }

  function encodeSetReadyState(data) {
    // Felder: party_id (field 1, fixed64), ready_state (field 2, bool)
    throw new Error('Not implemented — Proto encoding needed');
  }

  function encodeStartMatch(data) {
    // Felder: party_id (field 1, fixed64)
    throw new Error('Not implemented — Proto encoding needed');
  }

  function encodePartyLeave(data) {
    // Felder: party_id (field 1, fixed64)
    throw new Error('Not implemented — Proto encoding needed');
  }

  function encodeGetMatchMetaData(data) {
    // Felder: match_id (field 1)
    throw new Error('Not implemented — Proto encoding needed');
  }

  // ============================================================
  // Event Handler (aufgerufen von events.js)
  // ============================================================

  /**
   * Wird aufgerufen wenn eine GC Response-Message empfangen wird.
   * Löst das entsprechende Promise in pendingResponses aus.
   */
  function handleGCResponse(msgId, decodedPayload) {
    const pending = pendingResponses.get(msgId);
    if (pending) {
      pending.resolve(decodedPayload);
    }
  }

  /**
   * Wird aufgerufen wenn ein SO Cache Update mit CSOCitadelParty kommt.
   * Updated den lokalen partyCache.
   */
  function handleSOCacheUpdate(partyData) {
    if (partyData && partyData.party_id) {
      const key = String(partyData.party_id);
      partyCache.set(key, partyData);
      log.debug(`[CustomLobby] SO Cache updated for party ${key}, join_code=${partyData.join_code || 'none'}`);
    }
  }

  // ============================================================
  // Public API
  // ============================================================

  return {
    // Task Handler (aufgerufen von tasks.js)
    createCustomLobby,
    setSpectator,
    setReady,
    startMatch,
    leaveLobby,
    getMatchResult,

    // Event Handler (aufgerufen von events.js)
    handleGCResponse,
    handleSOCacheUpdate,

    // State Access
    getPartyCache: () => partyCache,
  };
};
```

### 5.3 tasks.js — Neue Cases im Switch

Im bestehenden `processNextTask()` Switch-Block die neuen Task-Types registrieren:

```javascript
// In der switch(task.type) Struktur hinzufügen:

case 'GC_CREATE_CUSTOM_LOBBY':
  return await sharedCtx.customLobby.createCustomLobby(task);

case 'GC_LOBBY_SET_SPECTATOR':
  return await sharedCtx.customLobby.setSpectator(task);

case 'GC_LOBBY_READY':
  return await sharedCtx.customLobby.setReady(task);

case 'GC_LOBBY_START_MATCH':
  return await sharedCtx.customLobby.startMatch(task);

case 'GC_LOBBY_LEAVE':
  return await sharedCtx.customLobby.leaveLobby(task);

case 'GC_GET_MATCH_RESULT':
  return await sharedCtx.customLobby.getMatchResult(task);
```

Ausserdem muss beim Initialisieren des `sharedCtx` das customLobby-Modul eingebunden werden:

```javascript
const customLobbyModule = require('./custom_lobby');
sharedCtx.customLobby = customLobbyModule(sharedCtx);
```

### 5.4 events.js — SO Cache Handler und GC Response Routing

In der bestehenden `receivedFromGC` Handler-Funktion müssen die neuen Response-Messages geroutet werden:

```javascript
// Innerhalb des receivedFromGC Handlers:

const messageId = msgType & ~PROTO_MASK;

switch (messageId) {
  // ... bestehende Cases ...

  // === Custom Lobby Responses ===
  case 9124: // CMsgClientToGCPartyCreateResponse
  case 9126: // CMsgClientToGCPartyLeaveResponse
  case 9130: // CMsgClientToGCPartyActionResponse
  case 9132: // CMsgClientToGCPartyStartMatchResponse
  case 9143: // CMsgClientToGCPartySetReadyStateResponse
    if (sharedCtx.customLobby) {
      const decoded = decodeResponse(messageId, payload);
      sharedCtx.customLobby.handleGCResponse(messageId, decoded);
    }
    break;

  // === SO Cache Updates ===
  case 24: // CMsgSOCacheSubscribed
  case 25: // CMsgSOSingleObject
    // SO Cache Messages enthalten Type-IDs für verschiedene Objekte.
    // CSOCitadelParty hat eine bestimmte Type-ID (aus Proto ablesen).
    // Wenn der Typ CSOCitadelParty ist:
    if (sharedCtx.customLobby) {
      const partyData = extractPartyFromSOCache(payload);
      if (partyData) {
        sharedCtx.customLobby.handleSOCacheUpdate(partyData);
      }
    }
    break;
}
```

**SO Cache Parsing:** Der GC sendet SO Cache Updates als `CMsgSOCacheSubscribed` (kompletter Cache) oder `CMsgSOSingleObject` (einzelnes Objekt). Jedes Objekt hat einen `type_id` der identifiziert um welchen Typ es sich handelt. Die `type_id` für `CSOCitadelParty` muss aus den Proto-Dateien abgelesen werden.

### 5.5 Protobuf Encoding/Decoding

Zwei Ansätze sind möglich:

**Ansatz 1: protobufjs (empfohlen)**

```javascript
const protobuf = require('protobufjs');

// Proto-Dateien laden (einmalig beim Start)
const root = protobuf.loadSync([
  'protos/citadel_gcmessages_client.proto',
  'protos/citadel_gcmessages_common.proto',
]);

// Message-Types nachschlagen
const PartyCreate = root.lookupType('CMsgClientToGCPartyCreate');
const PartyCreateResponse = root.lookupType('CMsgClientToGCPartyCreateResponse');
const PartyAction = root.lookupType('CMsgClientToGCPartyAction');
// ... etc.

// Encoding
function encodePartyCreate(data) {
  const msg = PartyCreate.create(data);
  return PartyCreate.encode(msg).finish();
}

// Decoding
function decodePartyCreateResponse(buffer) {
  return PartyCreateResponse.decode(Buffer.from(buffer));
}
```

**Ansatz 2: Manuelles Buffer-Encoding**

Falls protobufjs-Loading Probleme macht (z.B. wegen fehlender Imports in den .proto Dateien), kann manuell encoded werden:

```javascript
// Protobuf Wire Format: field_number << 3 | wire_type
// Varint = 0, Fixed64 = 1, LengthDelimited = 2, Fixed32 = 5

function encodeVarint(value) {
  const bytes = [];
  while (value > 0x7f) {
    bytes.push((value & 0x7f) | 0x80);
    value >>>= 7;
  }
  bytes.push(value & 0x7f);
  return Buffer.from(bytes);
}

function encodeFixed64(value) {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(value));
  return buf;
}

function encodeField(fieldNumber, wireType, data) {
  const tag = encodeVarint((fieldNumber << 3) | wireType);
  return Buffer.concat([tag, data]);
}
```

### 5.6 GC Kommunikations-Pattern

```javascript
// === SENDEN ===
client.sendToGC(
  DEADLOCK_APP_ID,       // 1422450 (Deadlock App-ID)
  PROTO_MASK | msgId,    // 0x80000000 | 9123 = Protobuf-markierte Message-ID
  {},                    // Proto Header (normalerweise leer)
  encodedPayload         // Buffer mit dem encoded Protobuf Payload
);

// === EMPFANGEN ===
// In der receivedFromGC Callback:
client.on('receivedFromGC', (appId, msgType, payload) => {
  if (appId !== DEADLOCK_APP_ID) return;

  const messageId = msgType & ~PROTO_MASK;  // Mask entfernen
  // messageId ist jetzt z.B. 9124 für PartyCreateResponse

  // payload ist ein Buffer mit dem encoded Protobuf Response
  const decoded = ResponseType.decode(payload);
});
```

---

## 6. Implementierung Phase B — Tournament Backend (Python)

### 6.1 Dateien

| Datei | Aktion | Beschreibung |
|-------|--------|-------------|
| `backend/match/steam_bridge.py` | **ÄNDERN** | Task Queue Client (aktuell Stub) |
| `backend/match/manager.py` | **ÄNDERN** | Match Lifecycle Management (aktuell Stub) |
| `backend/tournament/admin_routes.py` | **ÄNDERN** | Neue Admin-API Endpoints |
| `backend/config.py` | **ÄNDERN** | `STEAM_BRIDGE_DB_PATH` hinzufügen |

### 6.2 config.py — Neue Einstellung

```python
# In backend/config.py oder settings:
STEAM_BRIDGE_DB_PATH = r"C:\Users\Nani-Admin\Documents\Deadlock\service\deadlock.sqlite3"
```

### 6.3 steam_bridge.py — Task Queue Client

```python
"""
Steam Bridge — Task Queue Client.
Kommuniziert mit dem Steam Bot über die steam_tasks SQLite-Tabelle.
"""

import asyncio
import json
import time
import aiosqlite
from backend.config import settings

STEAM_DB = settings.STEAM_BRIDGE_DB_PATH


async def create_task(task_type: str, payload: dict) -> int:
    """
    Schreibt einen Task in die steam_tasks Queue.

    Args:
        task_type: Task-Typ (z.B. 'GC_CREATE_CUSTOM_LOBBY')
        payload: Dict das als JSON in die payload-Spalte geschrieben wird

    Returns:
        Die ID des erstellten Tasks
    """
    now = int(time.time() * 1000)  # Unix timestamp in Millisekunden
    async with aiosqlite.connect(STEAM_DB) as db:
        cursor = await db.execute(
            "INSERT INTO steam_tasks (type, payload, status, created_at, updated_at) "
            "VALUES (?, ?, 'PENDING', ?, ?)",
            (task_type, json.dumps(payload), now, now),
        )
        await db.commit()
        return cursor.lastrowid


async def poll_task_result(task_id: int, timeout_s: int = 60, poll_interval_s: float = 2.0) -> dict:
    """
    Pollt auf Task-Ergebnis.

    Args:
        task_id: Die ID des Tasks
        timeout_s: Maximale Wartezeit in Sekunden
        poll_interval_s: Abstand zwischen Polls in Sekunden

    Returns:
        Das geparste result-Dict bei Erfolg

    Raises:
        TimeoutError: Wenn der Task nicht innerhalb von timeout_s fertig wird
        RuntimeError: Wenn der Task mit FAILED Status endet
    """
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        async with aiosqlite.connect(STEAM_DB) as db:
            db.row_factory = aiosqlite.Row
            row = await db.execute_fetchone(
                "SELECT status, result, error FROM steam_tasks WHERE id = ?",
                (task_id,),
            )
            if row and row["status"] == "DONE":
                return json.loads(row["result"])
            if row and row["status"] == "FAILED":
                raise RuntimeError(f"Steam task failed: {row['error']}")
        await asyncio.sleep(poll_interval_s)

    raise TimeoutError(f"Steam task {task_id} did not complete within {timeout_s}s")


async def get_task_status(task_id: int) -> dict | None:
    """
    Gibt den aktuellen Status eines Tasks zurück (ohne zu warten).
    """
    async with aiosqlite.connect(STEAM_DB) as db:
        db.row_factory = aiosqlite.Row
        row = await db.execute_fetchone(
            "SELECT id, type, status, result, error, created_at, updated_at FROM steam_tasks WHERE id = ?",
            (task_id,),
        )
        if row:
            return dict(row)
        return None
```

### 6.4 manager.py — Match Lifecycle

```python
"""
Match Manager — Steuert den Lifecycle von Custom Matches.
Orchestriert die Kommunikation mit dem Steam Bot über steam_bridge.
"""

import json
from backend.match import steam_bridge
from backend.database import get_db


async def create_lobby(tournament_id: int, match_id: int, game_mode: int = 1, region_mode: int = 1) -> dict:
    """
    Erstellt eine Custom Lobby für ein Bracket-Match.

    1. Schreibt GC_CREATE_CUSTOM_LOBBY Task
    2. Wartet auf Ergebnis (party_id + join_code)
    3. Speichert party_code in bracket_matches

    Returns:
        Dict mit party_id, join_code, party_code_display
    """
    # Task erstellen
    task_id = await steam_bridge.create_task(
        "GC_CREATE_CUSTOM_LOBBY",
        {
            "tournament_id": tournament_id,
            "match_id": match_id,
            "game_mode": game_mode,
            "region_mode": region_mode,
        },
    )

    # Auf Ergebnis warten (max 30s für Lobby-Erstellung)
    result = await steam_bridge.poll_task_result(task_id, timeout_s=30)

    if not result.get("success"):
        raise RuntimeError(f"Lobby creation failed: {result.get('error', 'unknown')}")

    # Party-Code in bracket_matches speichern
    async with get_db() as db:
        await db.execute(
            "UPDATE bracket_matches SET party_code = ?, party_id = ?, status = 'lobby_created' WHERE id = ?",
            (result["party_code_display"], result["party_id"], match_id),
        )
        await db.commit()

    return result


async def set_bot_spectator(match_id: int) -> dict:
    """
    Setzt den Bot auf Spectator-Slot in der Match-Lobby.
    Muss nach create_lobby und vor start_match aufgerufen werden.
    """
    party_id = await _get_party_id(match_id)

    task_id = await steam_bridge.create_task(
        "GC_LOBBY_SET_SPECTATOR",
        {"party_id": party_id},
    )
    result = await steam_bridge.poll_task_result(task_id, timeout_s=15)

    if not result.get("success"):
        raise RuntimeError(f"Set spectator failed: {result.get('error', 'unknown')}")

    return result


async def set_bot_ready(match_id: int) -> dict:
    """
    Setzt den Bot auf Ready in der Match-Lobby.
    Muss vor start_match aufgerufen werden.
    """
    party_id = await _get_party_id(match_id)

    task_id = await steam_bridge.create_task(
        "GC_LOBBY_READY",
        {"party_id": party_id},
    )
    result = await steam_bridge.poll_task_result(task_id, timeout_s=15)

    if not result.get("success"):
        raise RuntimeError(f"Set ready failed: {result.get('error', 'unknown')}")

    return result


async def start_match(tournament_id: int, match_id: int) -> dict:
    """
    Startet das Match nachdem alle Spieler in der Lobby sind.

    1. Setzt Bot auf Spectator (falls noch nicht geschehen)
    2. Setzt Bot auf Ready
    3. Startet das Match via GC

    Returns:
        Dict mit success und optionaler match_id
    """
    party_id = await _get_party_id(match_id)

    # Bot auf Spectator setzen
    await set_bot_spectator(match_id)

    # Bot auf Ready setzen
    await set_bot_ready(match_id)

    # Match starten
    task_id = await steam_bridge.create_task(
        "GC_LOBBY_START_MATCH",
        {"party_id": party_id},
    )
    result = await steam_bridge.poll_task_result(task_id, timeout_s=30)

    if not result.get("success"):
        raise RuntimeError(f"Match start failed: {result.get('error', 'unknown')}")

    # Match-Status updaten
    async with get_db() as db:
        await db.execute(
            "UPDATE bracket_matches SET status = 'in_progress', deadlock_match_id = ? WHERE id = ?",
            (result.get("match_id"), match_id),
        )
        await db.commit()

    return result


async def fetch_match_result(match_id: int) -> dict:
    """
    Holt das Match-Ergebnis vom GC und updated das Bracket.

    1. Liest deadlock_match_id aus bracket_matches
    2. Holt Ergebnis via GC_GET_MATCH_RESULT
    3. Bestimmt Gewinner-Team und updated bracket_matches
    4. Triggert Bracket-Advancement (Gewinner in nächste Runde)

    Returns:
        Dict mit winning_team, duration, player stats
    """
    async with get_db() as db:
        db.row_factory = lambda c, r: dict(zip([col[0] for col in c.description], r))
        row = await db.execute_fetchone(
            "SELECT deadlock_match_id, team1_id, team2_id FROM bracket_matches WHERE id = ?",
            (match_id,),
        )

    if not row or not row.get("deadlock_match_id"):
        raise RuntimeError(f"No deadlock_match_id found for match {match_id}")

    task_id = await steam_bridge.create_task(
        "GC_GET_MATCH_RESULT",
        {"match_id": row["deadlock_match_id"]},
    )
    result = await steam_bridge.poll_task_result(task_id, timeout_s=30)

    if not result.get("success"):
        raise RuntimeError(f"Match result fetch failed: {result.get('error', 'unknown')}")

    # Gewinner bestimmen: winning_team 0 = Team 1 (Amber), 1 = Team 2 (Sapphire)
    winner_id = row["team1_id"] if result["winning_team"] == 0 else row["team2_id"]

    # bracket_matches updaten
    async with get_db() as db:
        await db.execute(
            "UPDATE bracket_matches SET status = 'completed', winner_id = ?, "
            "match_duration_s = ?, match_stats = ? WHERE id = ?",
            (winner_id, result["duration_s"], json.dumps(result["players"]), match_id),
        )
        await db.commit()

    # TODO: Bracket-Advancement triggern (Gewinner in nächste Runde setzen)
    # await advance_bracket(match_id, winner_id)

    return result


async def leave_lobby(match_id: int) -> dict:
    """Bot verlässt die Lobby (optional, nach Match-Start)."""
    party_id = await _get_party_id(match_id)

    task_id = await steam_bridge.create_task(
        "GC_LOBBY_LEAVE",
        {"party_id": party_id},
    )
    result = await steam_bridge.poll_task_result(task_id, timeout_s=15)
    return result


async def _get_party_id(match_id: int) -> str:
    """Holt die party_id eines Matches aus der Datenbank."""
    async with get_db() as db:
        db.row_factory = lambda c, r: dict(zip([col[0] for col in c.description], r))
        row = await db.execute_fetchone(
            "SELECT party_id FROM bracket_matches WHERE id = ?",
            (match_id,),
        )

    if not row or not row.get("party_id"):
        raise RuntimeError(f"No party_id found for match {match_id}")

    return row["party_id"]
```

### 6.5 Neue Admin API Endpoints

```python
# In admin_routes.py hinzufügen:

from backend.match import manager as match_manager
from fastapi import HTTPException


@router.post("/api/admin/tournaments/{tid}/matches/{mid}/create-lobby")
async def create_lobby(tid: int, mid: int, user=Depends(require_mod)):
    """Erstellt eine Custom Lobby für ein Bracket-Match."""
    try:
        result = await match_manager.create_lobby(tid, mid)
        return {"party_code": result["party_code_display"], "party_id": result["party_id"]}
    except TimeoutError:
        raise HTTPException(status_code=504, detail="Steam Bot hat nicht rechtzeitig geantwortet")
    except RuntimeError as e:
        raise HTTPException(status_code=502, detail=str(e))


@router.post("/api/admin/tournaments/{tid}/matches/{mid}/start")
async def start_match(tid: int, mid: int, user=Depends(require_mod)):
    """Startet das Custom Match (alle Spieler müssen in der Lobby sein)."""
    try:
        result = await match_manager.start_match(tid, mid)
        return {"success": True, "match_id": result.get("match_id")}
    except TimeoutError:
        raise HTTPException(status_code=504, detail="Steam Bot hat nicht rechtzeitig geantwortet")
    except RuntimeError as e:
        raise HTTPException(status_code=502, detail=str(e))


@router.post("/api/admin/tournaments/{tid}/matches/{mid}/fetch-result")
async def fetch_result(tid: int, mid: int, user=Depends(require_mod)):
    """Holt Match-Ergebnis vom GC und updated das Bracket."""
    try:
        result = await match_manager.fetch_match_result(mid)
        return result
    except TimeoutError:
        raise HTTPException(status_code=504, detail="Match-Ergebnis konnte nicht abgerufen werden")
    except RuntimeError as e:
        raise HTTPException(status_code=502, detail=str(e))


@router.post("/api/admin/tournaments/{tid}/matches/{mid}/leave-lobby")
async def leave_lobby(tid: int, mid: int, user=Depends(require_mod)):
    """Bot verlässt die Lobby (optional, nach Match-Start)."""
    try:
        result = await match_manager.leave_lobby(mid)
        return result
    except Exception as e:
        raise HTTPException(status_code=502, detail=str(e))
```

### 6.6 bracket_matches Tabelle — Benötigte Spalten

Die `bracket_matches` Tabelle muss um folgende Spalten erweitert werden (falls nicht vorhanden):

```sql
ALTER TABLE bracket_matches ADD COLUMN party_id TEXT;           -- Steam Party-ID
ALTER TABLE bracket_matches ADD COLUMN party_code TEXT;          -- Formatierter Party-Code (XXX-XXX-XXX)
ALTER TABLE bracket_matches ADD COLUMN deadlock_match_id INTEGER; -- Deadlock Match-ID nach Start
ALTER TABLE bracket_matches ADD COLUMN match_duration_s INTEGER;  -- Match-Dauer in Sekunden
ALTER TABLE bracket_matches ADD COLUMN match_stats TEXT;          -- JSON mit Spieler-Stats
```

---

## 7. Implementierung Phase C — Frontend

### 7.1 Match-Admin Panel

Im Admin-Bereich soll für jedes Bracket-Match ein Steuerungs-Panel angezeigt werden:

**Buttons und Aktionen:**

| Button | Endpoint | Sichtbar wenn | Aktion |
|--------|----------|---------------|--------|
| "Lobby erstellen" | `POST create-lobby` | `status = pending/checkin` | Erstellt Lobby, zeigt Party-Code |
| "Match starten" | `POST start` | `status = lobby_created` | Startet Match (Bot -> Spectator -> Ready -> Start) |
| "Ergebnis abrufen" | `POST fetch-result` | `status = in_progress` | Holt Ergebnis vom GC, updated Bracket |
| "Lobby verlassen" | `POST leave-lobby` | `status = in_progress` | Bot verlässt Lobby (optional) |
| "Manuell eingeben" | Bestehender Endpoint | Jederzeit | Fallback: manuelles Ergebnis |

**Party-Code Anzeige:**
- Nach "Lobby erstellen" wird der Party-Code gross und gut lesbar angezeigt
- Copy-to-Clipboard Button
- Format: `XXX-XXX-XXX` (mit Bindestrichen für bessere Lesbarkeit)

**Loading States:**
- Waehrend GC-Kommunikation: Spinner + "Lobby wird erstellt..." / "Match wird gestartet..." etc.
- Timeout nach 30s: Fehlermeldung mit Retry-Option

### 7.2 Match-Status Anzeige

Im `BracketMatch`-Component den Status visuell darstellen:

| Status | Anzeige | Styling |
|--------|---------|---------|
| `pending` | "Ausstehend" | Grau |
| `checkin` | "Check-in läuft" | Gelb/Orange |
| `lobby_created` | "Lobby erstellt" + Party-Code | Blau |
| `in_progress` | "Laeuft" + pulsierender Indikator | Grün pulsierend |
| `completed` | Gewinner-Team hervorgehoben | Grün |
| `forfeit` | "Aufgegeben" | Rot |
| `cancelled` | "Abgesagt" | Grau durchgestrichen |

### 7.3 API-Calls (Frontend)

```typescript
// In api/client.ts oder ähnlich:

export async function createLobby(tournamentId: number, matchId: number) {
  const res = await fetch(`/api/admin/tournaments/${tournamentId}/matches/${matchId}/create-lobby`, {
    method: 'POST',
    credentials: 'include',
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json(); // { party_code, party_id }
}

export async function startMatch(tournamentId: number, matchId: number) {
  const res = await fetch(`/api/admin/tournaments/${tournamentId}/matches/${matchId}/start`, {
    method: 'POST',
    credentials: 'include',
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json(); // { success, match_id }
}

export async function fetchMatchResult(tournamentId: number, matchId: number) {
  const res = await fetch(`/api/admin/tournaments/${tournamentId}/matches/${matchId}/fetch-result`, {
    method: 'POST',
    credentials: 'include',
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json(); // { winning_team, duration_s, players }
}
```

---

## 8. Vollständiger Custom Match Flow

```
Schritt 1: Admin klickt "Lobby erstellen" im Match-Panel
           -> Frontend: POST /api/admin/tournaments/1/matches/42/create-lobby
           -> Backend: create_task('GC_CREATE_CUSTOM_LOBBY', {...}) in steam_tasks
           -> Steam Bot: liest Task, setzt status='RUNNING'
           -> Steam Bot: sendet CMsgClientToGCPartyCreate an Deadlock GC
           -> GC: antwortet mit CMsgClientToGCPartyCreateResponse (party_id)
           -> GC: pusht CSOCitadelParty mit join_code in SO Cache
           -> Steam Bot: extrahiert party_id + join_code
           -> Steam Bot: schreibt result JSON in steam_tasks, status='DONE'
           -> Backend: liest result, speichert party_code in bracket_matches
           -> Frontend: zeigt Party-Code gross an (z.B. "987-654-321")

Schritt 2: Admin kopiert Party-Code und teilt ihn den Spielern mit
           -> Spieler öffnen Deadlock -> Play -> Custom Lobby -> Join with Code
           -> Spieler geben den Code ein und joinen die Lobby
           -> Alle 12 Spieler (6v6) müssen in der Lobby sein

Schritt 3: Admin klickt "Match starten"
           -> Frontend: POST /api/admin/tournaments/1/matches/42/start
           -> Backend: Automatisch 3 Tasks sequentiell:
              1. GC_LOBBY_SET_SPECTATOR (Bot auf Spectator)
              2. GC_LOBBY_READY (Bot auf Ready)
              3. GC_LOBBY_START_MATCH (Match starten)
           -> GC: startet Dedicated Server, weist match_id zu
           -> Backend: updated bracket_matches status='in_progress'
           -> Frontend: zeigt "Laeuft" mit pulsierendem Indikator

Schritt 4: Spieler spielen das Match (typisch 30-45 Minuten)

Schritt 5: Match endet -> Admin klickt "Ergebnis abrufen"
           -> Frontend: POST /api/admin/tournaments/1/matches/42/fetch-result
           -> Backend: create_task('GC_GET_MATCH_RESULT', {match_id: ...})
           -> Steam Bot: fragt GC nach Match-Ergebnis
           -> GC: liefert winning_team, duration, player stats
           -> Backend: bestimmt winner_id, updated bracket_matches
           -> Backend: triggert Bracket-Advancement (Gewinner in nächste Runde)
           -> Frontend: aktualisiert Bracket-Ansicht, Gewinner hervorgehoben

Fallback:  Admin kann jederzeit manuell Ergebnis eingeben
           -> POST /api/admin/tournaments/1/matches/42/result (bestehender Endpoint)
           -> Ueberschreibt GC-Ergebnis falls nötig
```

---

## 9. Fehlerbehandlung & Edge Cases

### 9.1 GC nicht erreichbar

| Szenario | Verhalten |
|----------|-----------|
| GC antwortet nicht | Task bleibt `RUNNING`, Backend bekommt TimeoutError nach 30s |
| GC gibt Fehler zurück | Task wird `FAILED` mit error-Beschreibung |
| **Lösung** | Admin bekommt Fehlermeldung, kann Retry oder manuelles Ergebnis nutzen |

### 9.2 Steam Bot nicht eingeloggt

| Szenario | Verhalten |
|----------|-----------|
| Bot offline | Task bleibt `PENDING` (niemand verarbeitet) |
| Bot logged aus während Task | Task bleibt `RUNNING` bis Timeout |
| **Lösung** | Backend TimeoutError -> Admin informiert -> Bot neustarten -> Retry |

### 9.3 Lobby erstellt aber Match nicht gestartet

| Szenario | Verhalten |
|----------|-----------|
| Spieler joinen nicht | Lobby bleibt offen, Admin kann warten |
| Spieler verlassen Lobby | Admin muss ggf. neue Lobby erstellen |
| Bot disconnected | Lobby kann weiterexistieren, aber Bot kann nicht starten |
| **Lösung** | Timeout-Mechanismus implementieren, nach 15 Min Lobby als abgelaufen markieren |

### 9.4 Match-Ergebnis kommt nicht

| Szenario | Verhalten |
|----------|-----------|
| GC hat kein Ergebnis | Task FAILED oder leeres Result |
| Match läuft noch | GC liefert kein Ergebnis (Match nicht abgeschlossen) |
| **Lösung** | Admin nutzt manuellen Override über bestehenden Endpoint |

### 9.5 Rate Limiting

| Regel | Beschreibung |
|-------|-------------|
| Max 1 Lobby gleichzeitig | Nur ein GC_CREATE_CUSTOM_LOBBY Task zur Zeit |
| Delay zwischen GC-Messages | Min. 500ms zwischen aufeinanderfolgenden GC-Nachrichten |
| Retry-Limit | Max 3 Retries pro Task bevor permanent FAILED |
| **Implementierung** | Im Steam Bot: Queue mit Delay, im Backend: Check auf laufende Tasks |

### 9.6 Deadlock-Update / Protocol-Änderung

| Risiko | Massnahme |
|--------|-----------|
| Message-IDs ändern sich | Proto-Dateien aus SteamDatabase/Protobufs aktualisieren |
| Neue Pflichtfelder | Encoding-Funktionen anpassen |
| SO Cache Struktur ändert sich | Parser aktualisieren |
| **Monitoring** | SteamDatabase/Protobufs Watch auf GitHub für Deadlock-Updates |

---

## 10. Reihenfolge der Implementierung

### Phase A: Steam Bot (Node.js)

| Schritt | Beschreibung | Abhängigkeit |
|---------|-------------|---------------|
| A1 | Proto-Dateien herunterladen und in Bot-Projekt einbinden | Keine |
| A2 | `custom_lobby.js` erstellen mit allen Task Handlern | A1 |
| A3 | `tasks.js` erweitern (neue Cases im Switch) | A2 |
| A4 | `events.js` erweitern (Response + SO Cache Handler) | A2 |
| A5 | Manueller Test: Task per SQL INSERT -> Bot verarbeitet | A3, A4 |

### Phase B: Tournament Backend (Python)

| Schritt | Beschreibung | Abhängigkeit |
|---------|-------------|---------------|
| B1 | `config.py` um `STEAM_BRIDGE_DB_PATH` erweitern | Keine |
| B2 | `steam_bridge.py` implementieren (create_task, poll_task_result) | B1 |
| B3 | `manager.py` implementieren (create_lobby, start_match, fetch_result) | B2 |
| B4 | `admin_routes.py` erweitern (3 neue Endpoints) | B3 |
| B5 | `bracket_matches` Tabelle um neue Spalten erweitern | Keine |
| B6 | API-Test: Endpoints manuell testen | B4, B5, A5 |

### Phase C: Frontend (React)

| Schritt | Beschreibung | Abhängigkeit |
|---------|-------------|---------------|
| C1 | API-Client-Funktionen erstellen | B4 |
| C2 | Match-Admin-Panel Component | C1 |
| C3 | Match-Status Anzeige im Bracket | C1 |
| C4 | End-to-End Test | Alles |

### Gesamt-Timeline (geschaetzt)

| Phase | Aufwand |
|-------|---------|
| A: Steam Bot | 2-3 Tage (Protobuf-Encoding ist der Hauptaufwand) |
| B: Backend | 1 Tag |
| C: Frontend | 1 Tag |
| Testing & Bugfixes | 1-2 Tage |
| **Gesamt** | **5-7 Tage** |

---

## 11. Testplan

### 11.1 Manueller Test (Phase A — Steam Bot)

Direkt in der SQLite-Datenbank einen Task erstellen und prüfen ob der Bot ihn verarbeitet:

```sql
-- In deadlock.sqlite3:
INSERT INTO steam_tasks (type, payload, status, created_at, updated_at)
VALUES (
  'GC_CREATE_CUSTOM_LOBBY',
  '{"tournament_id":1,"match_id":1,"game_mode":1,"region_mode":1}',
  'PENDING',
  strftime('%s','now')*1000,
  strftime('%s','now')*1000
);
```

**Erwartetes Ergebnis:**
1. Bot loggt "Creating lobby for tournament=1 match=1"
2. GC antwortet mit party_id
3. SO Cache liefert join_code
4. Task status wechselt zu `DONE`
5. `result` Spalte enthält JSON mit `party_id`, `join_code`, `party_code_display`

**Verifikation:**
```sql
SELECT id, status, result, error FROM steam_tasks ORDER BY id DESC LIMIT 1;
```

### 11.2 API-Test (Phase B — Backend)

```bash
# Lobby erstellen (als Admin eingeloggt):
curl -X POST http://localhost:8900/api/admin/tournaments/1/matches/1/create-lobby \
  -H "Cookie: session=..." \
  -v

# Match starten:
curl -X POST http://localhost:8900/api/admin/tournaments/1/matches/1/start \
  -H "Cookie: session=..." \
  -v

# Ergebnis abrufen:
curl -X POST http://localhost:8900/api/admin/tournaments/1/matches/1/fetch-result \
  -H "Cookie: session=..." \
  -v
```

### 11.3 End-to-End Test

1. Turnier mit 2 Teams erstellen (über Admin-UI)
2. Bracket generieren lassen
3. "Lobby erstellen" klicken -> Party-Code erscheint
4. Mit mindestens 2 Spielern im Spiel der Lobby joinen (Code eingeben)
5. "Match starten" klicken -> Match beginnt im Spiel
6. Match spielen (oder `min_players` in PrivateLobbySettings auf 2 setzen für Schnelltest)
7. "Ergebnis abrufen" klicken -> Bracket wird aktualisiert
8. Gewinner rückt automatisch in nächste Runde vor

### 11.4 Edge-Case Tests

| Test | Beschreibung | Erwartung |
|------|-------------|-----------|
| Bot offline | Task erstellen während Bot aus ist | TimeoutError nach 30s |
| Doppelte Lobby | "Lobby erstellen" zweimal klicken | Fehler oder zweite Lobby |
| Match ohne Spieler starten | "Match starten" ohne Spieler in Lobby | GC-Fehler, FAILED Task |
| Ergebnis zu frueh abrufen | "Ergebnis abrufen" während Match läuft | GC-Fehler oder leeres Result |
| Manueller Override | Nach GC-Ergebnis manuell anderes Ergebnis setzen | Manuelles Ergebnis ueberschreibt |

---

## 12. Referenzen

| Ressource | URL/Pfad |
|-----------|----------|
| SteamDatabase Protobufs (Deadlock) | https://github.com/SteamDatabase/Protobufs/tree/master/deadlock |
| Steam Bot Basis | `C:\Users\Nani-Admin\Documents\Deadlock\cogs\steam\steam_presence\` |
| Tournament Backend | `C:\Users\Nani-Admin\Documents\Deadlock-Turniere\backend\` |
| Task Queue DB | `C:\Users\Nani-Admin\Documents\Deadlock\service\deadlock.sqlite3` |
| node-steam-user Docs | https://github.com/DoctorMcKay/node-steam-user |
| protobufjs Docs | https://github.com/protobufjs/protobuf.js |
