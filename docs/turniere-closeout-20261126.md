# Turnier-Bot – konsolidierter Restauftrag (26.11.2026)
## Ziel und Arbeitsmodus
Alle vorhandenen Turnier-/PUG-Fixes, UI-/Draft-Entwürfe und Integrationen vollständig fertigstellen, testen und nach gültigen Freigaben produktiv bringen. Kein separater T3-Orchestrator, keine T3-Delegation, keine neuen Paket-Worker. Der Nutzer übermittelt Fragen/Ergebnisse an seinen ChatGPT-Hauptchat. Selbstständig arbeiten; normale technische Entscheidungen selbst treffen. Bei echten Produkt-, Rechte-, Sicherheits- oder Gate-Blockern die unabhängigen Arbeiten fortsetzen und konkret berichten. Keine Abkürzungen bei Gates, keine Live-Testmutationen, kein Force-Push.
 
## 0. Gesicherte Ausgangsstände
Diese Branches sind Sicherungen oder Arbeitsstände, nicht automatisch freigegeben. Stand vor Übergabe: Turniere origin/main 67a90cc; lokale Gast-Commits e707677/a8478c2 sind im PUG-Branch enthalten. GitHub-Refs erneut abgleichen, laufenden Agent 01 nicht übernehmen oder überschreiben.
| Repo | Branch / SHA | Zweck |
|---|---|---|
| Deadlock-Turniere | fix/turniere-closeout-01-pug @ 0277287 | PUG-Kern, Agent 01 arbeitet weiter |
| Deadlock-Turniere | sicherung/turniere-closeout-01-wip-20261126 @ 12d03a3 | Separater bytegenau geprüfter Zwischenstand laufender PUG-Abnahme, nicht mergen; Agent 01 bleibt maßgebliche Quelle |
| Deadlock-Turniere | fix/pug-chaos-20261114 @ b56dc0e | Historische Ursache A–H, Register und Gate-Nachweise |
| Deadlock-Turniere | sicherung/turniere-main-wip-20261125 @ 77d6f47 | Neuerer Admin-/Home-Umbau, MCP-Adapter |
| Deadlock-Turniere | sicherung/turniere-closeout-admin-cleanup-20261126 @ e92f123 | Alter P1b/P4b-UI-Entwurf, vier Quelldateien gesichert, nicht buildbar |
| Deadlock-Turniere | feat/draft-arena-ux-20261123 @ 862d17c, PR #34 | Draft/Performance |
| Deadlock-Turniere | sicherung/turniere-closeout-brain-tests-20261126 @ 4004074 | Zwei ältere Charakterisierungstests; prüfen statt blind übernehmen |
| Deadlock-Turniere | sicherung/turniere-closeout-historical-ci-20261126 @ 6614c61 | Historischer CI-Fix, zunächst prüfen, ob überholt |
| Deadlock-Turniere | sicherung/turniere-closeout-observer-stash-20261126 @ d3fa2cb | Alter Scrim/Observer-Stash (größtenteils Formatierung); nur relevante Deltas |
| Deadlock-Turniere | sicherung/turniere-required-pr-gate-20260924 @ a834475 | Bestehende CI-WIP-Sicherung; aktueller tracked Diff identisch |
| Deadlock-Bots | sicherung/sweep-20261125/bots-p8-wip @ 3be6f774; bots-p8-untracked @ 2299000e | Alter Onboarding-WIP, lokal tracked Diff und TODO identisch |
| Deadlock-Twitch-Bot | fix/turnier-event-signup-closed @ ee002f044 | Fünf Promo-Fix-Commits, gegen aktuellen Main portieren |
| caddy-config | feat/turnier-hashed-cache-20261123 @ 06c7af1, PR #10 | Turnier-/Videos-/Builds-Cache, Gate R3 BLOCK |

Links: https://github.com/EarlySalty/Deadlock-Turniere/pull/34 ; https://github.com/EarlySalty/caddy-config/pull/10 . Die fünf neuen Sicherungsbranches wurden remote verifiziert; Admin-/Brain-/aktiver PUG-Snapshot mit exakt übereinstimmenden Blob-SHAs. Keine Änderungen an Main/Master oder laufenden Worktrees. Weitere neue Worker-Commits bis Abschluss jeweils pushen und Remote-SHA prüfen. Keine privaten Logs, Zugangsdaten, Screenshots mit personenbezogenen Daten oder lokale Laufzeitdateien in öffentliche Repos hochladen.

## 1. P0 – PUG-Kern (Paket 01, laufender Agent)
- [ ] Agent 01 abschließen lassen; auf neuestem tatsächlichem Remote-HEAD fortsetzen, nicht b56dc0e neu implementieren. Keine parallelen Änderungen im selben Paket. 0277287 ist nur der letzte zum Zeitpunkt der Sicherung bestätigte offizielle Branch-HEAD.
- [ ] Browserabnahme 390/1440 px: 30 Solo-Anmeldungen, 5 manuell gebaute Teams mit je 6 Spielern, Captain, Gast-/Namensdubletten, Admin-Add/Remove/Check-in, Withdraw, Save/Entwürfe anderer Tabs, Start, tatsächliches Bracket-Format, Voice/Caster. Keine ungewollte automatische Teambildung oder automatische Beendigung des Check-ins; Admin/Public konsistent. Der Zwischenstand 12d03a3 sichert laufende Korrekturen zur Austrage-Semantik bereits zugeteilter Solo-only-Spieler, nicht freigegeben.
- [ ] Unabhängiger Kritiker, gültiges Gate (historisch R1/R2 BLOCK, R3 offen laut letztem vollständigem Bericht), neue Befunde beheben. Werkzeugblocker regelkonform lösen oder konkrete Übergabe an berechtigte Prüfumgebung.
- [ ] Testnachweise der letzten finalen Revision: Rust, Clippy, fmt, Frontend lint/test/tsc/build; gemeinsamer Broker-Vertragstest; Release-Freigabe. Zuletzt gemeldet: 628 Rust bestanden, 0 Fehler, 2 ignored; central_routes 17/17 auf PUG-Basis, PR34-Vergleich 15/15 grün mit aktueller Bots-Abhängigkeit und isolierter DB. Keine neue Solo-Balance-Migration duplizieren.
- [ ] Nach ALLOW kontrollierte Integration/Deployment gemäß Betriebsregeln; bestehende Events schützen.

## 2. P0/P1 – Caddy (Paket 04; sofort unabhängig bearbeitbar)
- [ ] Auf 06c7af1 weiterarbeiten. Aktuell Gate R3 BLOCK; alle R1/R2-Befunde laut Worker behoben, 23/23 relevante Tests grün, drei Alias-Bestandsfehler (Gesamt 23/26, Basis 21/24); nicht als vollständig grüne Gesamtsuite ausgeben.
- [ ] Einmalige eng begrenzte Scope-Erweiterung auf alle verbleibenden nachweislich gleichartigen öffentlichen Cache-Fehler im Caddyfile: /brand/*, Yoshi-Assets, /patchnotes/* plus weitere echte Geschwister gesammelt prüfen. Nur reproduzierte gleiche Fehlerklasse korrigieren, keine Site-Redesigns oder unrelated Änderungen. Erfolgreiche GET/HEAD-Assets korrekt cachen, 404/405/416/5xx und irrtümliche SPA-Fallbacks nicht öffentlich cachen; Range 206 und ETag 304 korrekt; API/Auth/private Daten weiterhin no-store. HTML revalidieren. Tests gegen adaptierte Konfiguration mit echten HTTP-Antworten, alle betroffenen Apps.
- [ ] Caddy adapt/validate über zulässigen Betriebsweg, Regressionstests, Baseline-Vergleich, unabhängiges Review und Gate R4 nach lokalen Regeln. Kein Gate-Bypass. CI Run 45859489137 wurde laut offizieller Annotation wegen GitHub Billing/Spending Limit nicht gestartet (0 Steps); lokale Tests ersetzen keine grüne Remote-CI. Keine Abrechnung ändern; wenn vorgeschriebene Remote-Checks blockieren, explizit eskalieren statt Schutzregeln umgehen.
- [ ] Nach Freigabe und bestätigter Asset-Kompatibilität kontrolliertes Release mit Rollback und realen HTTP-/Cache-Header-Belegen, kein ungeprüfter Reload.

## 3. P1 – Admin-UI/Home (Paket 02, nach freigegebenem 01)
- [ ] Neueren WIP 77d6f47 und gesicherten alten Cleanup e92f123 fachlich vergleichen. P1b/P4b zusammenführen, nicht beide Versionen blind mergen; MCP-Dateien separat in 11.
- [ ] Klarer Admin-Workflow, Phasen, Übersicht, Startseiten-Karten/CTA nur bei offener Anmeldung, Sortierung nach effektivem Start; Lade-/Fehlerzustände statt leerer Listen, Remount/Turnierwechsel ohne hängende Spinner.
- [ ] Team-Builder als zentrale Bedienung; alte doppelte Teilnehmer-/Solo-Karten entfernen ohne Team-Löschen, Recruiting, Ersatzspieler, Entfernen und Solo-Balance zu verlieren. In-flight-Save, Navigation und parallele Tabs gegen Phantomteams und verlorene Änderungen absichern.
- [ ] Mobile/PC-Browserabnahme, Regressionstests und Gate.

## 4. P1 – Draft/Performance (Paket 03, nach 02)
- [ ] Draft-PR #34 auf freigegebenen Stand portieren; bekannte Konflikte in ParticipantManager, Admin, Tournament und Rust admin/tournaments, leaderboard, public/tournaments fachlich auflösen; serverautoritative PUG-Verträge behalten.
- [ ] Draft-Arena, Hero-/Pick-/Ban-UX, Teamaufstellungen, Profile, Polling (Hintergrundtabs), Route-Splitting, Fonts und Query-Optimierungen prüfen. Observer-Reconnect und aktive/inaktive Zustände testen. Kein blindes Cache-/Pool-Tuning.
- [ ] Beide alten roten central_routes-Tests auf integriertem Stand wiederholen; Agent 01 meldete sie auf unverändertem PR34 mit aktueller Bots-Migration bereits 15/15 grün. Nicht neu erfinden. Saubere isolierte DB, echte Draft-Abläufe und begrenzte Performance-Benchmarks; Gate, PR34 erst nach nachgewiesener Übernahme schließen.

## 5. P1 – Turnierregeln (Paket 05)
- [ ] P12: Auto-Modus verwechselt team_size mit tatsächlicher Teamanzahl; ursprüngliche Schwelle und Zeitpunkt der Entscheidung prüfen, insbesondere bei Solo-PUG. Explizite Modi, Liga und 4h-Abstand erhalten; additive Migration falls nötig. Tests mit 1/5/6/8/16 und über Schwelle, Gruppensieger/Bracket-Fallback, Parallelität und bestehenden Events.

## 6. P1 – Zeit, Scheduler, Reminder (Paket 06)
- [ ] P10: Teilfix eecd2a5 bereits vorhanden, Create/Edit/Automatik/Liga/API auf korrekte ISO/UTC- und Berlin-CET/CEST-Semantik, Sommerzeitlücken und doppelte Uhrzeiten prüfen. Alte Turnierzeiten nicht eigenmächtig korrigieren.
- [ ] P11: Anmelde-Reminder trotz Check-in mit offener Anmeldung, 0-Minuten-Offset, Dedupe, Restart/Catch-up und tatsächlich versendete Meldungen prüfen. Deterministische Testuhr/isolierte DB, keine echten DMs.

## 7. P2 – Steam, Ränge, Lobby (Pakete 07/08)
- [ ] Rest P3b: optionale Steam-Verknüpfung und Mindestrang serverseitig über alle Signup-/Join-/Invite-Wege; Gast-PUG nicht versehentlich blockieren, Standardverhalten erhalten. Unbekannte/veraltete Ränge, Ranglos-Fallback/TTL, Admin-Ausnahmen und UI-Fehler prüfen.
- [ ] P6: exakten ursprünglichen Steam-Bot-Flag-Auftrag rekonstruieren. P7: automatische Lobby-Kette, Task-Idempotenz, Retry/Timeout, Offline/Restart, Lobby-/Matchzuordnung, Voice/Caster, geplante/übersprungene Lobbys absichern.
- [ ] Echte Steam-Lobby-Abnahme nur mit ausdrücklich freigegebenem Testevent und Nutzer; bis dahin klar als noch nicht vollständig abgenommen markieren.

## 8. P2 – Begleitbots (Pakete 09/10, unabhängig von Turniere-Kern vorbereitbar)
- [ ] Twitch: fünf Promo-Fix-Commits aus ee002f044 auf aktuellen Main portieren; überall gleicher tatsächlicher Signup-Status, gebundene Community-Links, Fail-closed, Cache-Staleness/Check-in, keine Testwerbung an echte Kanäle; Gate.
- [ ] Discord: P8 aus den beiden vorhandenen WIP-Sicherungen gegen aktuellen Bots-Main portieren; Rollen-IDs statt Namenssuche, Turnier-/Custom-Games-Rollen, journeyglue, Server-as-code, Promptzahl, fehlende Rollen, Apply-Revalidation und exakte Rules-Ausnahme testen. Agent 01 benötigt laut Bericht keinen Bots-Code-Schreibbedarf; fremde aktive Arbeiten trotzdem prüfen. Kein Guild-Apply ohne Nutzerentscheidung. Scrims A/B, Funny Custom Ping, Explainer/Onboarding und falsch bezeichnete Journey-Rolle klären.

## 9. P2 – Brain, Altlasten, Sicherheit (Pakete 11/12)
- [ ] MCP-Adapter aus 77d6f47 und vorhandene Company-Brain-Implementierung/Charakterisierungstests vergleichen. Nur wirklich fehlenden Vertrag übernehmen; Auth/Authority, Rollenentzug, fremde Event-IDs, read/write scope, Gäste/PII, Logs, Fehler; falls ersetzt, belegter No-op.
- [ ] Historische rust/docs/known-issues.md gegen tatsächlichen aktuellen Code prüfen: match-/teambezogene Punkte, Double-Elim-Platzierung, Gruppen-Tiebreaker, doppelte Ergebnisse/Idempotenz, Draft-Pick/Ban-/Teamvalidierung, Auth-Rollenstaleness, Consent, Status-DB-Constraints, Scheduler-Catch-up, Discord-Aufgaben/Verzögerungen, Bridge-Fallback. Nur reproduzierte relevante Fehler beheben; Produktentscheidungen separat. Keine ungeprüfte Komplettrefaktorierung oder historischen Punkte-Rewrites.
- [ ] Historische CI-/Observer-Sicherungen nur auf noch fehlende nützliche Deltas prüfen; GitHub meldete beim Sicherungspush 13 Dependabot-Befunde auf Standardbranch (3 high, 7 moderate, 3 low); tatsächliche Advisories/Betroffenheit und kompatible Updates prüfen, releasekritische Befunde priorisieren. Keine pauschalen Major-Upgrades.
- [ ] Nicht freigegebene oder nicht mehr reproduzierbare historische Arbeit dokumentieren, nicht als ungeprüften Fehlerfix ausliefern.

## 10. P0 – Gesamtabnahme und Release (Paket 13)
- [ ] Sämtliche relevanten freigegebenen Änderungen tatsächlich in Main/Master und kompatiblen Releases nachweisen; nur nach ALLOW und Integrationsfreigabe mergen/deployen, keine Main-Änderungen während laufender Release-Builds.
- [ ] End-to-End-Testturnier: Erstellung, 30 Anmeldungen, Check-in, 5x6 Teams, Draft, Lobby, Caster/Voice, Match/Ergebnis, Rangliste/Archiv; zusätzlich Teamturnier, Gruppen-/Double-Elim, Liga, Mobile/PC, Gast-/Discord-Anmeldung, Fehlerpfade.
- [ ] Zentral-DB, Broker, Steam, Twitch, Caddy, Logs, reale Build-SHAs und Health/HTTP prüfen. Rollback-Binary, Frontend und Config sichern. Kein Test mit echten Nutzern oder produktiven Daten; bestehende Turniere #16/#17, reale Lobbyprobe und disruptive Releases nur nach expliziter Zustimmung bei Bedarf.
- [ ] Kein unfertiger Stand nur lokal: jede fertige Änderung auf Remote-Feature-Branch, SHA verifizieren. Sicherungsbranches nicht blind mergen oder löschen; nach nachgewiesener Übernahme archivieren. Abschlussbericht mit Commit, Tests, Gate, Live-Beweis und konkret verbleibenden Blockern.

## Offene Nutzerentscheidungen (nur bei tatsächlicher Notwendigkeit vorlegen)
Captain-Draft in der Website vs manuelle Discord-Picks; Gast-/Steam-/Mindestrang-Ausnahmen falls nicht aus Originalauftrag ableitbar; echte Steam-Lobbyprobe; alte Team-/Zeitkorrekturen #16/#17; Discord-Onboarding A/B und Rollen-/Explainer-Apply; sichere Releasefenster; GitHub Billing falls verpflichtende CI blockiert. Normale technische Entscheidungen selbst treffen.

## Reihenfolge
Jetzt 04 abschließen; 09/10 separat vorbereiten, solange 01 noch läuft. Nach finaler 01-Übergabe 02 → 03 → 05 → 06 → 07 → 08; 11 in freiem Schreibfenster nach 03, 12 danach; 13 Gesamtabschluss. Bei einem einzelnen Agenten sequentiell, unabhängige Aufgaben bei Blockern nutzen. Keine zweite Implementierung von 01. 
