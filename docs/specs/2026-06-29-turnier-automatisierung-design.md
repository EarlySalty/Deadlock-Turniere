# Turnier-Automatisierung — Design-Spec

**Datum:** 2026-06-29
**Repos:** `Deadlock-Turniere` (tb-app / Rust, LIVE) + `Deadlock-Bots` (dl-bot / Rust, LIVE)
**Status:** Design freigegeben (Grillme 2026-06-29). Implementierung an Codex (gpt-5.5/xhigh). User-sichtbare Texte schreibt Claude.

---

## 1. Wahres Ziel (Nordstern)

Turniere passieren **regelmäßig und mit gutem Zulauf, ohne dass ein Mensch sich um Organisation kümmern muss.** Der Bot übernimmt Routine, Reichweite und Erinnerung; Caster geben nur frei. Manuelles Planen bleibt jederzeit möglich, wird aber stark entlastet.

Ausgangsproblem: Niemand hat Lust, sich um Turnier-Planung und -Umsetzung zu kümmern. Folge: Turniere finden zu selten statt.

## 2. Betriebsmodell — Hybrid

- **Bot schlägt vor:** autonom, vorerst **1 Vorschlag / 2 Wochen** (Kadenz = Config-Wert, nicht hartkodiert). Vorschlag muss von **mindestens 1 Caster** bestätigt werden.
- **Mensch plant manuell:** weiterhin möglich; ein Admin-erstelltes Turnier startet direkt in `GEPLANT` (Admin = bereits autorisiert) und nutzt dieselbe Automatik danach (Event, Ankündigung, DM, Erinnerung).
- **Regel:** Bot-Vorschläge brauchen das Caster-Gate. Manuelle Turniere nicht.

## 3. Architektur — zwei Repos, dünner Relay

```
┌─────────────────────────────┐         Master-Broker (HTTP, :8770)        ┌──────────────────────────────┐
│  Deadlock-Turniere (tb-app)  │  ── POST Vorschlag-mit-Buttons ─────────▶ │  Deadlock-Bots (dl-bot, Rust) │
│  Rust, Web-Service, LIVE     │  ── POST Scheduled-Event-erstellen ─────▶ │  Gateway, Rust, LIVE          │
│                              │  ── POST DM-Broadcast (Rolle+Opt-out) ──▶ │                              │
│  • Presets, Abos/Opt-out     │                                           │  • Interaction-Handler        │
│  • Vorschlags-Scheduler      │ ◀── Callback: Button/Modal-Ergebnis ───── │  • postet, was tb-app sagt   │
│  • Turnier-State-Machine     │                                           │  • relayt Klicks zurück      │
│  • MiniMax 💬-Feedback-Parse │                                           │    (rendert NICHTS selbst)   │
│  • Template-Rendering (DE)   │                                           └──────────────────────────────┘
│  • Signal-Logging            │
└─────────────────────────────┘
```

**Leitprinzip:** dl-bot bleibt ein dünner Relay ohne Turnier-Logik. Alle Intelligenz (State, Templates, LLM-Feedback, Presets, Empfänger-Berechnung) lebt in tb-app. Minimale Kopplung; der dl-bot-Cutover wird nicht belastet.

**Discord-Empfang:** dl-bot empfängt Button-/Modal-Klicks über sein vorhandenes Gateway (`interaction_create`) — kein Ed25519/Webhook nötig. **3-Sekunden-Regel:** dl-bot defert sofort (`DeferredMessageUpdate`), ruft tb-app, editiert die Nachricht erst danach. So unabhängig von der MiniMax-Laufzeit.

### Discord-IDs & Config (Single Source: Config, nicht hartkodiert)

| Zweck | ID |
|---|---|
| Caster-Rolle (Freigabe-Ping) | `1495154811799077067` |
| Vorschlags-Channel | `1474543558793887937` |
| DM-Rolle „Grind" (Comp) | `1407086020331311144` |
| DM-Rolle „Fun/Spaß" | `1407085699374649364` |
| Vorschlags-Kadenz | `14 Tage` (Config-Wert, anpassbar) |
| Min. Caster-Zustimmungen | `1` (Config-Wert) |

## 4. Phasen

| Phase | Inhalt | DoD |
|---|---|---|
| **0 — Vorbedingung** | (a) Crate-Rename `tb-*` → `turnier-*` (erster, separat verifizierter Commit). (b) Adversarialer Py→Rust-Vollständigkeits-Audit + Lücken schließen. (c) Rust nach `main` mergen. | Build grün, Dienst läuft aus neuem Binary-Pfad, jede Python-Route/Funktion hat Rust-Pendant ODER dokumentierte bewusste Auslassung; `main` = Live-Stand |
| **1 — Automatik-Grundloop** | Presets, Abos/Opt-out, Vorschlags-Scheduler (1/2 Wo), Caster-Freigabe (Buttons+Modal+MiniMax-💬), Template-Ankündigung, Scheduled-Event, DM-Broadcast (Grind/Fun minus Opt-out), Erinnerungen, Signal-Logging aller 4 Signale | Bot schlägt vor → Caster gibt frei → Turnier läuft automatisch durch; Signale werden geloggt |
| **2 — Self-Learning** | Doppel-gegateter LLM-Loop (s. §9) | Datenbasierte Verbesserungs-Vorschläge an Caster, nie auto-apply |

## 5. Zustandsmaschine (Phase 1)

```
[Bot-Trigger 1/2Wo] ─┐
                     ├─▶ ENTWURF ─▶ FREIGABE_OFFEN ─(✅ ≥1 Caster)─▶ GEPLANT ─▶ ANMELDUNG ─▶ LIVE ─▶ FERTIG ─▶ [Signale-Snapshot]
[Mensch: manuell] ───┘       ▲           │  │
(manuell = direkt            └──(💬)─────┘  └─(❌ alle / Timeout)─▶ VERWORFEN
 GEPLANT, kein Gate)      MiniMax editiert
                          Entwurf, neu posten
```

**Begriffs-Mapping:** `ENTWURF`/`FREIGABE_OFFEN`/`VERWORFEN` sind Zustände von `tournament_proposals.state` (`draft`/`pending_approval`/`rejected`+`expired`). `GEPLANT`/`ANMELDUNG`/`LIVE`/`FERTIG` sind Zustände des bestehenden `tournaments.status` — der Vorschlag (`approved`) übergibt an den unveränderten Turnier-Lebenszyklus.

Bei Freigabe (`approved`): `tournaments`-Zeile aus `config_json` erzeugen → `create-scheduled-event` → öffentliche Ankündigung (Template) → `dm-broadcast` an Grind/Fun minus Opt-out.

## 6. Datenmodell

**Bot-Vorschlag ist noch kein Turnier:** Vorschläge leben in eigenen Tabellen; erst die Caster-Freigabe erzeugt die echte `tournaments`-Zeile. Bestehender Turnier-Lebenszyklus bleibt unangetastet.

**Bereits vorhanden (wiederverwenden, nicht neu bauen):** `tournament_casters` (Caster pro Turnier), `sent_start_reminders`/`sent_tournament_reminders`/`sent_match_reminders` (Erinnerungs-Infra), `tournament_signups`/`tournament_checkins`/`no_show_grace_minutes` (No-Show-Daten), `reminder_offsets`/`start_reminder_offsets`. Broker-Call `get_role_members` existiert.

**Neue Tabellen (englisch, konsistent mit `tournament_*`-Schema):**

| Tabelle | Zweck | Kern-Spalten |
|---|---|---|
| `tournament_presets` | Config-Bündel | `name`, `category` (`fun`\|`comp`), alle Turnier-Felder (team_size, bracket_format, series_format, final_series_format, tournament_mode, tournament_game_mode, match_objective, invite_mode, reminder_offsets, start_reminder_offsets, rules, description_template), `active`, `created_by`, Zeitstempel |
| `tournament_proposals` | Automatik-/Freigabe-Zustand | `preset_id`, `source` (`bot`\|`manual`), `proposed_start`, `config_json`, `state` (`draft`\|`pending_approval`\|`approved`\|`rejected`\|`expired`), `proposal_message_id`, `channel_id`, `tournament_id` (NULL bis approved), Zeitstempel |
| `tournament_proposal_votes` | Min-1-Caster + Audit | `proposal_id`, `caster_discord_id`, `decision` (`approve`\|`reject`), `created_at`, UNIQUE(proposal_id, caster_discord_id) |
| `tournament_proposal_feedback` | 💬-Änderungswünsche | `proposal_id`, `caster_discord_id`, `raw_text`, `applied_change_json`, `created_at` |
| `tournament_dm_optout` | DM-Unterdrückung (Suppression) | `discord_id`, `scope` (`fun`\|`comp`\|`all`), `created_at`, UNIQUE(discord_id, scope) |
| `tournament_signals` | Phase-2-Lerndaten (Snapshot bei FERTIG) | `tournament_id`, `participants`, `teams`, `no_shows`, `poll_up`, `poll_down`, `poll_message_id`, `feedback_summary` (Ph2), `collected_at` |

**Erweiterung `tournaments` (additiv, idempotent):** `scheduled_event_id TEXT`, `source TEXT DEFAULT 'manual'`, `preset_id INTEGER`.

**DM-Zielgruppe = Rolle, nicht Abo-Liste:** Wer die **Grind-Rolle** (`1407086020331311144`, Comp) bzw. **Fun-Rolle** (`1407085699374649364`) hat, ist abonniert. tb-app holt Rollen-Mitglieder via `get_role_members` und filtert gegen `tournament_dm_optout`. Gespeichert werden nur Opt-outs → Rolle bleibt Single Source of Truth. Re-Opt-in über Website oder `/turnier abo`.

## 7. Cross-Repo-API

**tb-app → dl-bot (Master-Broker `:8770`, Token-Auth, `idempotency_key` an jedem Call):**

| Endpoint | Zweck | Payload / Antwort |
|---|---|---|
| `POST …/discord/post-proposal` | Vorschlag mit Buttons posten | → channel_id, fertiges DE-Embed, Button-Spec (`custom_id` trägt `proposal_id` + Aktion), Caster-Rollen-Ping · ← `message_id` |
| `POST …/discord/create-scheduled-event` | Discord Scheduled Event | → Name, Beschreibung, Start/Ende, Ort · ← `event_id` |
| `POST …/discord/dm-broadcast` | DMs + Opt-out-Buttons | → gefilterte `discord_ids`, Embed, Opt-out-Button-Spec · ← pro-ID Erfolg/Fehler |
| *(bestehend)* `get-role-members`, `send-rich-message` | Rollen lesen, öffentliche Ankündigung | |

> **Port-Verifikation Phase 0:** Broker live auf `:8770`; Turnier-Notifier-Default ist `:8766`. Beim Verdrahten den effektiven Wert prüfen und angleichen.

**dl-bot → tb-app (Callback, interner Token):**
```
POST /internal/turnier/v1/interaction
{ typ: "button"|"modal", custom_id, actor_discord_id, freitext? }
```
tb-app verarbeitet, antwortet mit Anzeige-Anweisung für dl-bot:
- `approve` → Vote speichern; bei ≥1 → `angenommen` → Turnier + Event + Ankündigung + DM-Broadcast.
- `reject` → Vote speichern; bei alle ❌ / Timeout → `verworfen`.
- `feedback` (Modal) → MiniMax parst Freitext → `config_json` editieren → Vorschlag-Nachricht in-place neu rendern.
- `dm_optout` → `tournament_dm_optout` schreiben → DM bestätigt Abmeldung.

## 8. User-sichtbare Texte (Claude schreibt, kein Runtime-LLM)

**Öffentliche Ankündigung:** feste deutsche Templates mit Platzhaltern (Name, Modus, Termin, Anmelde-Link). Kein Runtime-LLM → null Umlaut-/Qualitätsrisiko.

**Caster-Vorschlag — 3 Buttons:**
- ✅ **„Annehmen & einplanen"** (1 Caster genügt)
- ❌ **„Ablehnen / keine Zeit"**
- 💬 **„Änderung vorschlagen"** (öffnet Freitext-Modal)

**Opt-out in jeder Turnier-DM** (Wort „DMs" macht eindeutig: keine Nachrichten, kein Ausschluss vom Turnier):
- 🔕 **„Keine DMs mehr über Fun-Turniere"** bzw. **„…über Comp-Turniere"** (je nach Kategorie der DM)
- 🔕 **„Komplett abmelden – gar keine Turnier-DMs"**
- Bestätigung nach Klick: „Abgemeldet. Wieder anmelden jederzeit über die Website oder `/turnier abo`."

## 9. Phase 2 — Self-Learning mit Doppel-Gate

Vier geloggte Signale: Teilnehmer-/Team-Zahl, No-Show-/Abbruch-Quote, Post-Turnier-Umfrage (👍/👎), Freitext-Community-Feedback. In Phase 1 werden Teilnehmer/No-Show automatisch geloggt; die Umfrage wird gepostet; Freitext erst in Phase 2 destilliert.

```
[Signale] ─▶ Daten-Report (nüchtern, aggregiert)
   │
   ▼
① EINGANGS-GATE: Caster bestätigt "Annahmen/Daten stimmen"   ◀── verhindert Lernen aus Müll-Daten
   │  (ohne Bestätigung läuft MiniMax NICHT)
   ▼
MiniMax erzeugt Verbesserungsvorschlag (nur auf bestätigter Basis)
   │
   ▼
② AUSGANGS-GATE: Caster nimmt an / lehnt ab                   ◀── erst Annahme ändert Preset/Termin
   ▼
[Preset/Kadenz angepasst]  — NIE auto-apply
```

Glutensneak (YouTube) als Format-Inspiration fürs Preset-Seeding (welche Modi/Formate kommen an) — nicht Teil des Lern-Loops selbst.

## 10. Modul-Benennung (Phase-0-Rename)

Crates deutsch-klar (`turnier-*`, im Repo menschen-lesbar), DB-Tabellen englisch (Schema-konsistent).

| neu | war | was es ist |
|---|---|---|
| `turnier-bot` | tb-app | ausführbarer Dienst (Binary) |
| `turnier-api` | tb-web | HTTP-Endpunkte = Website-Backend |
| `turnier-core` | tb-core | gemeinsame Basis: Typen, Fehler, Helfer |
| `turnier-db` | tb-db | Datenbankzugriff + Migrationen |
| `turnier-config` | tb-config | Konfiguration + Secrets-Laden |
| `turnier-auth` | tb-auth | Discord-OAuth-Login + Sessions |
| `turnier-scheduler` | tb-scheduler | Zeitsteuerung: Phasen, Erinnerungen, Vorschlags-Takt |
| `turnier-discord` | tb-discord | Broker-Client für Discord-Effekte |
| `turnier-engine` | tb-tournament | Bracket-, Seeding-, Turnier-Logik |
| `turnier-draft` | tb-draft | Draft-/Pick-Phase |
| `turnier-match` | tb-match | Match-Verwaltung + Ergebnisse |
| `turnier-steam` | tb-steam | Steam-/Rang-Anbindung |
| **`turnier-automatik`** | *(neu)* | das ganze neue Feature: Vorschläge, Freigabe, Presets, DM-Abos, Signal-Logging |

**Deploy-Folge:** Run-Script (`scripts/run_turniere_backend_rust.sh`) + systemd-Service (`deadlock-turniere.service`) auf neuen Binary-Pfad `rust/target/release/turnier-bot` umstellen, sonst startet der Dienst ins Leere.

## 11. Phase-0-Audit-Methodik

1. Codex erstellt **Feature-/Endpunkt-Inventar** aus Python (`backend/`: routes.py, admin_routes.py, alle Module) → mappt auf Rust-Crates → **Lücken-Liste** (fehlend / abweichend / bewusst ausgelassen).
2. **Adversarialer zweiter Codex-Kritiker** prüft die Lücken-Liste gegen Over-/Under-Reporting (Lektion Twitch/Steam: Triage über-feuert „genuine").
3. Claude verifiziert Stichproben gegen echte Signale (Code/DB/Endpoint), nicht nur Modell-Annahmen.
4. **DoD:** endpunktweise Parität für `public`- und `admin`-Router (Request/Response-Form, Statuscodes, Auth-Gates); jede Python-Funktion hat Rust-Pendant oder dokumentierte Auslassung in `rust/docs/known-issues.md`.

## 12. Fehlerbehandlung

- Broker-Calls idempotent (`idempotency_key`) + Retry; bei dl-bot-Ausfall bleibt Vorschlag in DB, Scheduler re-tried.
- Discord 3-Sekunden-Regel via Defer-dann-Edit.
- MiniMax-Feedback unverständlich → Caster bekommt „Konnte die Änderung nicht sicher umsetzen, bitte präziser" statt Crash/Fehlinterpretation.
- `thiserror`/`anyhow`, kein `.unwrap()` in Produktionspfaden. Validation nur an System-Grenzen (Discord-Callback, Website-Input).

## 13. Tests (TDD)

- **Unit:** Preset-CRUD, Proposal-State-Machine-Übergänge, Opt-out-Filter (Empfänger = Rolle minus Opt-out), Template-Rendering (Snapshot), Idempotenz.
- **Integration:** Mock-Broker für post-proposal/create-event/dm-broadcast; Callback-Verarbeitung (approve/reject/feedback/optout).
- **Cross-Repo-Contract:** Payload-Form post-proposal ↔ dl-bot-Handler ↔ callback (Vertragstest gegen Schema-Drift).
- Engine bleibt durch bestehende wertgenaue Tests abgesichert.

## 14. Dokumentation (Liefergegenstand)

- **Intern:** `docs/` im Repo — Architektur, Datenmodell, Flows, Cross-Repo-Vertrag, Deploy. Lückenlos, für künftige Agenten.
- **Community:** einfache Erklärung (Was sind Fun/Comp, wie abonniere/opt-oute ich, wie läuft ein Vorschlag) → in den **FAQ-Bot** (dort liegt die Turnier-Doku schon, CHANGELOG #9). Community-Text schreibt Claude.

## 15. Delegation & Reihenfolge

- Claude = Orchestrator (Spec, Plan, Review externer Signale, alle user-sichtbaren Texte). Codex (gpt-5.5/xhigh) = Implementierung + adversariale Kritik + Rework-Loop.
- Reihenfolge: Phase 0a Rename → Phase 0b Audit → Phase 0c Lücken-Fix → **Checkpoint: Claude prüft + Merge nach `main` (irreversibel, hier anhalten)** → Phase 1 → Phase 2.
- Codex baut an user-sichtbaren Text-Stellen nur `"Platzhalter"` + meldet Datei:Zeile; Claude schreibt den finalen Text.
