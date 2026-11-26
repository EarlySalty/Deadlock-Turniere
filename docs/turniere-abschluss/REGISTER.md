# Turnier-Abschlussprogramm – Register
Stand 26.11.2026, 22:11 CET. Grundlage: [Bestandsaufnahme](BESTANDSAUFNAHME.md), manuell übermittelte Berichte, erneut geprüfte Remote-Refs. GitHub-only Übergabe; kein separater Tracker oder T3-Orchestrator.

| ID | Auftrag | Abhängigkeit / Schreibbereich | Stand | SHA / Gate / Live |
|---|---|---|---|---|
| [01-pug-kern](01-pug-kern/AUFTRAG.md) | P0 · PUG-Kern | Turniere, Bots nur als Testabhängigkeit | IMPLEMENTIERT, ABNAHME OFFEN | fix/turniere-closeout-01-pug @ 3d1b69a32e4c245f1ff4a08d94492071d9ed08b9; Remote bestätigt; Worker berichtet für 0277287 628 Rust grün/0 Fehler/2 ignored, Clippy/fmt/frontend grün; Broker-Vertragstest; Browser + unabhängiger Kritiker + Gate R3 offen; live nein |
| [02-admin-ui](02-admin-ui/AUFTRAG.md) | P1 · Admin-Workflow, Team-Builder und Home | Nach 01; Schreibbereich Turniere-Frontend, notwendige API-Anpassungen abgestimmt. | BEREIT (ggf. abhängig) | – |
| [03-draft-performance](03-draft-performance/AUFTRAG.md) | P1 · Draft-Arena und Performance | Nach 02; Turniere-Frontend/API, keine Änderungen an Caddy (04). | BEREIT (ggf. abhängig) | – |
| [04-caddy-cache](04-caddy-cache/AUFTRAG.md) | P1 · Caddy Cache | Caddy; gleiche Cache-Fehlerklasse gesamthaft, nach Reproduktion | R3 BLOCK, Fortsetzung R4 | feat/turnier-hashed-cache-20261123 @ 06c7af132be8e4b1e2fa77510629f2d22b4182a9; Remote bestätigt, Worker 23/23 relevant, 3 Bestandsfehler, CI Billing/Spending Limit; Brand/Yoshi/Patchnotes offen; live nein |
| [05-turnierformat](05-turnierformat/AUFTRAG.md) | P1 · Automatik-Modus und Bracket-Regeln | Nach 03; Turniere Engine/API/zugehöriges Frontend. | BEREIT (ggf. abhängig) | – |
| [06-zeiten-reminder](06-zeiten-reminder/AUFTRAG.md) | P1 · Zeitzonen und Reminder | Nach 05; Scheduler, Zeit-API und Formularpfade. | BEREIT (ggf. abhängig) | – |
| [07-steam-rang](07-steam-rang/AUFTRAG.md) | P2 · Steam-Verknüpfung, Mindestrang und Rangauflösung | Nach 06; Turniere, Steam-Bot nur falls nachweislich notwendiger Vertragsfix. | BEREIT (ggf. abhängig) | – |
| [08-lobby-steam](08-lobby-steam/AUFTRAG.md) | P2 · Steam-Bot-Flag und Lobby-Automatik | Nach 07; Turniere/Deadlock-Steam-Bot, koordiniertes Schnittstellenpaket. | BEREIT (ggf. abhängig) | – |
| [09-twitch-werbung](09-twitch-werbung/AUFTRAG.md) | P2 · Twitch-Turnierwerbung | Parallel zu 01 möglich; separates Twitch-Repo; Wire-Vertrag mit 01 abstimmen. | KARTE BEREIT, parallel | ee002f044, 33 hinter origin/main 025d237aa; 5 Promo-Commits portieren, quinn-Bump entfällt; Gate –; live nein |
| [10-onboarding](10-onboarding/AUFTRAG.md) | P2 · Discord-Rollen/Onboarding | Bots; 01 meldet keinen Bots-Code-Schreibbedarf | FREI ZUR ISOLIERTEN BEARBEITUNG nach Prüfung fremder aktiver Bots-Arbeit | alter Branch b47f06c0, WIP sichern/portieren; kein Live-Apply ohne Nutzerentscheidung |
| [11-brain-mcp](11-brain-mcp/AUFTRAG.md) | P2 · Turniere-Brain/MCP-Adapter | Nach 03 oder später; Turniere MCP separat von UI. Kein gleichzeitiger Merge in Turniere. | BEREIT (ggf. abhängig) | – |
| [12-altlasten](12-altlasten/AUFTRAG.md) | P2 · Verifizierte Altlasten und Release-Lücken | Nach 08 und 11; read-only Audit kann früher beginnen, Code erst auf freigegebenem Main. | BEREIT (ggf. abhängig) | – |
| [13-abnahme-release](13-abnahme-release/AUFTRAG.md) | P0 · Gesamtintegration, Abnahme und Abschluss | Nach allen releasekritischen Paketen; 04/09/10/11 getrennt geprüft, dokumentierte Produktentscheidungen dürfen als ausdrücklich offene Erweiterungen verbleiben. | BEREIT (ggf. abhängig) | – |

## Start und Integration
Hauptpfad: 01 → 02 → 03 → 05 → 06 → 07 → 08 → 12 → 13; 11 nach 03 in einem freien Turniere-Schreibfenster, spätestens vor 12. 04 und 09 dürfen separat parallel vorbereitet werden; 10 nur mit koordiniertem Bots-Schreibfenster. Je fachlich eigenständigem grünem Paket den freigegebenen Main-/Release-Stand nach lokalen Regeln herstellen, nicht alle Deltas bis zum Schluss stapeln. 13 ist die Gesamt-Abnahme und Restintegration, nicht der erste Merge.

Offene Entscheidungen: Captain-Draft vs Discord-Picks (kein neuer Captain-Draft ohne Entscheidung); Gast-/Steam-/Rang-Semantik falls Originalauftrag nicht eindeutig; P7 echte Lobbyprobe; Korrekturen #16/#17; P8 Scrims A/B, Funny Custom Ping, Explainer/Apply; aktives Event/Releasefenster (#16 Status bracket seit 14.11., #17 registration; erster Turniere-Deploy braucht Nutzerentscheidung). Nichtkritische Erweiterungen dürfen nach ausdrücklicher Entscheidung getrennt bleiben, aber keine fehlgeschlagene Kernfunktion als fertig markieren.

## Übergebene Nachweise und nächste Schritte (19:17 CET)
- 01: Worker-Bericht meldet 627/0/2 auf b56dc0e, 628/0/2 auf 0277287; zentraler Test 17/17; PR34 auf 862d17c mit aktueller Bots-Revision e65d3805 und sauberer DB 15/15. Remote-SHA unabhängig bestätigt; Tests aus Worker-Bericht, nicht selbst neu ausgeführt. Bots unverändert. Nächster Auftrag: Browserabnahme, unabhängige Review/Gate R3 nach aktuellen Repo-Regeln. Kein ALLOW bisher.
- 04: Worker-Bericht meldet adapt/validate/Cache-Tests grün; make test 22/3 vs Basis 21/3. R1 BLOCK; nach e76ff9c noch keine weitere Gate-Runde. CI remote bestätigt fehlgeschlagen, keine Steps sichtbar. Nächster Auftrag: Scope nur für nachgewiesene identische Cache-Fehler in Videos/Builds erweitern, isolierte Regressionen, CI-Ursache und Gate R2; keine fremden Site-Umbauten. Nicht live.
- 02/03: kein Abschlussbericht vorliegend; bestehende isolierte Vorarbeiten nicht verwerfen. 03 muss historische DB-Funde nicht neu erfinden, aber auf integriertem Stand prüfen. 02/03 Integration erst nach jeweiligem Vorgänger. 09 bereit; 10 frei zur isolierten Arbeit (kein Bots-Code-Konflikt mit 01), andere aktive Arbeiten zuerst prüfen.
- Frühere T3-Kommunikationspflicht aufgehoben. Alle aktualisierten Prompts sind in PROMPTS.md; alte Startkarten mit abweichender Kommunikationsregel werden durch ARBEITSRAHMEN.md übersteuert.

## Konsolidierte Übergabe 26.11.2026, nach Gate R3 Caddy
- Maßgeblicher Restauftrag: RESTAUFTRAG.md, kurzer Startprompt: RESTPROMPT.md. Auch remote auf sicherung/turniere-abschluss-plan-20261126 @ b72838c25ca2518dada1be5dc66d1631f5ee53d8.
- Agent 01 läuft weiter; offizieller Remote-HEAD 0277287 zum Zeitpunkt der Prüfung. Bytegenauer separater Zwischenstand der drei laufenden Änderungen auf sicherung/turniere-closeout-01-wip-20261126 @ 12d03a3. Kein Eingriff in Worktree oder Arbeitsbranch; finaler Push und Abnahme verbleiben bei 01.
- Caddy 06c7af1 verifiziert. Scope-Erweiterung einmalig auf nachweislich gleiche verbleibende Cache-Fehler (Brand/Yoshi/Patchnotes und echte Geschwister), keine fremden Site-Umbauten. Gate R4. CI-Billing nicht ändern. Drei Baselinefehler unverändert. Kein Merge/Reload.
- Neue Remote-Sicherungen: admin-cleanup e92f123, brain-tests 4004074, historical-ci 6614c61, observer-stash d3fa2cb, aktiver PUG-Snapshot 12d03a3. Bereits vorhandene Sicherungen P8/CI-WIP geprüft. Keine privaten Daten, Screenshots oder Betriebsartefakte erforderlich. 13 Dependabot-Befunde (3 high) gemeldet; tatsächliche Betroffenheit prüfen.
- Kein Produktiv-Deployment und keine Main-/Master-Änderung durchgeführt. Weitere Agentenfortschritte immer auf Remote-Branch sichern; kein Anspruch, laufende Änderungen permanent zu überwachen.

## Remote-Update 22:11 CET
- Turniere main cd6516df2126f414edaa9a0c8c41ef16a1eab238 enthält sieben neue Review-Doku-Commits. PUG-Branch 3d1b69a32e4c245f1ff4a08d94492071d9ed08b9 (Organizer-managed withdrawal), noch divergent. Kein neuer vollständiger Worker-/Gate-/Live-Bericht erhalten. Verwende den aktuellen Remote-HEAD, nicht die Sicherung 12d03a3. Aktuelle Review-Regeln: [CODE-REVIEW.md](https://github.com/EarlySalty/Deadlock-Turniere/blob/main/CODE-REVIEW.md).
- Caddy PR #10 weiterhin offen, Head 06c7af1 bestätigt. Historische Gate-BLOCK-Befunde bleiben offen bis nachweislich abgearbeitet.
- GitHub-Ablage [RESTAUFTRAG.md](RESTAUFTRAG.md), [ARBEITSRAHMEN.md](ARBEITSRAHMEN.md), [QUELLEN.md](QUELLEN.md), [README.md](README.md). Keine fremden Rechnerpfade nötig.
