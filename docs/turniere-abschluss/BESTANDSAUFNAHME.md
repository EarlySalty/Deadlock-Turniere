# Turniere / PUG – zentrales Abschlussregister
Stand: 26.11.2026. Bestandsaufnahme via Codex-MCP. Historischer Recherchebericht. Keine Produktivdaten geändert, kein Deployment. Nicht als aktueller Arbeitsstand lesen; maßgeblich ist das [REGISTER.md](REGISTER.md).

## 1. Verifizierter Ist-Zustand
- Damals geprüfter Turniere-Checkout, clean, main e707677, zwei Commits vor origin/main 67a90cc. Remote-SHAs per git ls-remote bestätigt.
- Lokal zusätzlich a8478c2 (Gastanmeldung per Name) und e707677 (Kommentarbereinigung). Ungeprüftes Gate für diese zwei laut historischem Auftrag.
- User-Service deadlock-turniere.service active, GET /api/health status ok. Binary mtime 14.11.2026 12:02 CET, frontend/dist/index.html mtime 23.11.2026 21:26 CET. Exakte Frontend-/Binary-Build-SHA nicht unabhängig verifiziert. Live-Liste #17 registration, #16 bracket; keine Mutation. Alte API-Feldmenge in GET /api/tournaments. Weder Health noch Zeitstempel belegen Bugfreiheit.
- Kein Projektdatei-Treffer über ChatGPT; frühere Chatverläufe nicht vollständig zugänglich. Rekonstruktion aus Git, Worktrees, Aufgabenregister und Live-Metadaten.

## 2. Quellen und Arbeitsstände
| Paket | Branch / SHA / Ablage | Zustand |
|---|---|---|
| PUG #16, A–H | fix/pug-chaos-20261114 @ b56dc0e; .tasks/2026-11-14-pug-chaos/{AUFTRAG,REGISTER,TODO,GATE-R1-TRIAGE}.md | Remote gesichert, clean, 28 Commits über lokalem main, 158 Dateien. Kein offener PR dafür in abgefragter PR-Liste. Letzter Gateversuch Runde 3 ohne Urteil (Quota); danach Klassen behoben. Historische Tests 627 grün, 2 ignored, Frontend 31/31, lint/build/tsc/clippy grün; NICHT neu ausgeführt, kein ALLOW belegt. |
| Draft/Performance | feat/draft-arena-ux-20261123 @ 862d17c; https://github.com/EarlySalty/Deadlock-Turniere/pull/34 | Draft-PR offen, 8 Commits, 41 Dateien. Basis origin/main ohne Gastfixes. Nur GitGuardian Check auf GitHub sichtbar; historische central_routes 13/15, kein Merge. Preview vom Live-Backend getrennt. |
| Admin UX/Home/Brain MCP | sicherung/turniere-main-wip-20261125 @ 77d6f47 | Remote gesicherter Einzelcommit mit 24 Dateien (darunter AdminWorkflowPanel, adminWorkflow, homeTournaments, TeamBuilder, MCP-Adapter); stammt von lokalem e707677. Nicht als ein Paket blind übernehmen; MCP unabhängig reviewen. |
| Cleanup P1b/P4b | feat/wave1-cleanup @ 6382527| Uncommitted, drei modifizierte + zwei untracked Pfade, historisch nicht buildbar. WIP vom 25.11. zuerst auf Überdeckung prüfen, keinen alten Stand überschreiben. |
| Ursprüngliches Register | .tasks/2026-11-02-turnier-ui-pug/REGISTER.md; wip/turnier-ui-pug-todo @ 7ae479f | Wave 1 (Liga, P1, P2, P3a, P4, P9) laut Register am 03.11. live; alte TODOs mit aktuellem Code abgleichen. |
| P8 Discord Onboarding | Deadlock-Bots: feat/onboarding-role-lookup @ b47f06c0; sicherung/sweep-20261125/bots-p8-wip @ 3be6f774 und bots-p8-untracked @ 2299000e | Dirty, 200 Commits hinter origin/main e65d3805. Änderungen an serversync, journeyglue, onboarding; Tests/Regelausnahme fehlen. Erst neu gegen aktuelle Bots integrieren, Rollenänderungen nicht live ohne Entscheidung. |
| Twitch Ankündigung | Deadlock-Twitch-Bot: fix/turnier-event-signup-closed @ ee002f044 (Fix-Commit) | Clean, Branch remote, Commit nicht in origin/main. Separat Review/Integration; tatsächlichen Release prüfen. |
| Caddy Cache | https://github.com/EarlySalty/caddy-config/pull/10 | Draft offen, UNSTABLE. Kein Live-Fix belegt. |
| Zentral-DB | Deadlock-Bots rust/crates/dl-central-db/migrations/20261011120000_turnier_solo_balance.sql, bba52356 | solo_balance_runs/waitlist existieren im aktuellen Bots-Checkout. PR34-Testfehler 42P01 plausibel falsche DB-Abhängigkeit/Testmigration, erst gepinnte Quellen/Harness verifizieren; keine neue doppelte Migration. |

## 3. PUG-Paket: dokumentierte Ursachen, bereits auf Branch gelöst
A: Auto-Team-Formation beim Check-in; audit 241 belegt fünf Teams aus 30 Spielern; Builder nicht gespeichert. Solo-only, Teamvorschau, Draft-Guard, Scheduler schließt Check-in nicht selbst.
B: Gast-Dubletten, fehlende Anmeldungen/Ränge, Team-Builder und negative Discord-IDs. Gemeinsame Regeln, Gast-/Dublettenmarkierung, Admin-Add-by-name, Limitierung.
C: Austragen während Check-in 400 ohne Feedback. Gemeinsames Fenster, Check-in-Zeile mitlöschen, Mutationsfehler sichtbar.
D: Admin konnte Anmeldung kaum entfernen/einchecken. Builder-Pool und Admin-Check-in.
E: irreführende Zähler und stiller Double-Elim-Fallback. Vorschau, tatsächliches Format, Bracket-Minimum.
F: Caster-Broker POST 404 statt GET role-members. Korrekte Route, aussagekräftige Fehler, gemeinsamer Pfad. Cross-Repo-Contract-Test noch offen.
G: Admin-/Mobile-UI, stale drafts, Phantomteams, Tabs, Fehlermeldungen. Browserbelege im Originalregister; verbleibende UI-Prüfungen (Voice/Draft, 390px Detail, live) nicht als bestanden behandeln.
H: Solo-only, zentrale Typen, Regeln, Locks, Row-Writers, Testmodus. Restliche Wave 2 offen.

## 4. Integrationskonflikte (git merge-tree --write-tree --name-only, keine Worktree-Änderung)
PUG + PR34: 6 Konfliktdateien: frontend/src/components/admin/ParticipantManager.tsx, frontend/src/pages/{Admin,Tournament}.tsx, rust/crates/turnier-api/src/{admin/tournaments,leaderboard,public/tournaments}.rs.
PUG + 77d6f47: 3 Konflikte: frontend/src/components/admin/TeamBuilderPanel.tsx, frontend/src/pages/{Admin,Tournament}.tsx.
Keine automatische ours/theirs-Strategie. Serververträge aus PUG erhalten, UI/Performance gezielt darauf portieren, MCP aus WIP separat behandeln.

## 5. Priorisierte Abschlussreihenfolge
1. P0: isoliertes frisches Worktree-Paar Turniere/Bots auf b56dc0e + kompatible aktuelle zentrale DB. Test-DB nur isoliert, niemals Produktionsdaten. Rerun vollständige Rust-/Frontend-Suite, Gate und unabhängiger Review gegen origin/main inkl. zwei Gast-Commits. Browser PUG #16-ähnliche 30 Solo-Anmeldungen, 5x6, Check-in, Builder-Save, Start, Withdraw, Gäste, Caster/Voice, Bracket 5/6 Teams, 390/1440 px. Keine Gate-Freigabe behaupten.
2. P0: DB-/Broker-Verträge testen, einschließlich PR34 central_routes beide Fehler (solo_balance_preview_apply_rollback_and_next_tournament_priority, test_mode_seed_simulate_and_wipe_only_touch_test_data). Zweiter 500 ungeklärt. Alle bestehenden Migrationen der richtigen Revision anwenden.
3. P1: P1b/P4b konsolidieren mit 77d6f47, keine zweite UI-Implementierung; alte Legacy-Abschnitte entfernen ohne Funktion zu verlieren. Persistenter Save, Fehler statt Empty-State, Phase und CTA, Sortierung. Danach Draft-UX/Performance PR34 konfliktbewusst einarbeiten und isoliert messen. Caddy #10 separat validieren.
4. P1: P12 Automatik-Modus. Noch auf main nachgewiesen: admin/tournaments.rs:184 determine_tournament_mode(body.team_size as usize, ...), verwechselt Spieler/Team und Anzahl Teams. Modusregel für Solo-PUG und Teamanmeldung festlegen und testen.
5. P1: P11 Registrierungserinnerungen: PUG reminders.rs weiterhin nur status registration oder draft; Check-in erlaubt aber Anmeldungen, daher Reminder-Fenster nicht konsistent. Dedupe/0-Minuten/Neustart testen. Scheduler-Harness im PUG-Fix umgebaut, nicht erneut blind schreiben.
6. P1: P10 Zeitzonen ist TEILWEISE erledigt: eecd2a5 auf main, editForm nutzt localInputToIso. Nicht als nie begonnen behandeln. Create/Edit/Automatik/Liga/API mit CET/CEST und DST testen; bestehende Daten #16/#17 nur auf ausdrückliche Entscheidung korrigieren.
7. P2: P3b Rest Steam-Verknüpfung/Mindestrang serverseitig (Gast-PUG nicht versehentlich blockieren), P6 Steam-Bot-Flag, P7 echte Lobby-Automation zusammen mit Nutzer; Twitch-Branch + P8 getrennt abschließen.
8. P2: Historische Rust known-issues.md nach aktuellem Code prüfen: Rangfallback/Ranking, Consent und Rechte, Match-/Gruppenresultate/Idempotenz, Scheduler-Catch-up, Status-DB-Constraints. Nicht alle historischen Einträge sind noch reproduzierbar; vor Priorisierung verifizieren.
9. Abnahme/Deploy: erst ALLOW und grüne Tests; zusammenhängende kompatible Backend-/Frontend-Version, Rollback-Artefakte, Health + Live-Browser auf Testturnier. #16/#17 nicht eigenmächtig ändern. Nach erfolgreicher Abnahme Altbranches erst archivieren, nicht löschen.

## 6. Produktentscheidungen und offene Nachweise
- Captain-Draft im Tool vs Discord-Picks; Steam-Pflicht/Mindestrang optional pro Event, Auswirkung auf Gast-Anmeldung.
- P7 Steam-Lobby-End-to-End mit Nutzer. P6 konkreter Flag-Umfang anhand ursprünglichem Auftrag.
- Korrektur #16 Teams und Zeiten #16/#17 nur mit Zustimmung. P8 Scrims Onboarding A/B, Funny Custom Ping, explainer/apply offen.
- Live-Service ist aktiv, aber weder vollständiger PUG-Fix noch PR34-Backend als deployed nachgewiesen. Kein Restart, Push, PR-Merge oder produktiver DB-Schreibzugriff in dieser Bestandsaufnahme.
