# Turnier-Bot Upgrade – WORKFLOW

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
