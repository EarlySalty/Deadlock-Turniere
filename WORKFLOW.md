# Turnier-Bot Upgrade – WORKFLOW

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
