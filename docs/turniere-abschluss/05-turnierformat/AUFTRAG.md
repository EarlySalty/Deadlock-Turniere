# 05-turnierformat – P1 · Automatik-Modus und Bracket-Regeln

## Abhängigkeiten und Schreibbereich
Nach 03; Turniere Engine/API/zugehöriges Frontend.

## Vorhandene Arbeit / Quellen
[P12 im Originalregister](https://github.com/EarlySalty/Deadlock-Turniere/tree/7ae479f1eefb87805c50d3b9ff4015779970a728/.tasks/2026-11-02-turnier-ui-pug); admin/tournaments.rs determine_tournament_mode(body.team_size ...); PUG-Fallback-Fixes aus 01.
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Korrigiere die Verwechslung Spieler pro Team/Teamanzahl. Nutze reale, feststehende Teilnehmerteams zum korrekten Zeitpunkt; prüfe, ob Auto-Modus persistiert werden muss und wann er bei Solo-PUGs tatsächlich entscheidbar ist. Explizite Modi nie überschreiben; Liga und 4h-Startabstand erhalten. Falls Schema nötig, nur additive Migration in zentraler DB, keine historische ändern. Verifiziere die bestehende ≥16-Regel und dokumentiere die Semantik; bei unklarer Produktentscheidung keine erfundene Regel live erzwingen.

## Abnahmekriterien
Testfälle unter/ab/über Schwelle, 1/5/6/8/16 Teams, Solo-PUG vs feste Teams, expliziter Modus, Gruppenqualifikation, Bracket-Fallback, parallele Starts, bestehende Turniere unverändert; verständliche Serverwerte und UI; Gate.

## Aktueller Übergabestand und Ergänzung
Keine Nutzerregel zur Moduswahl erfinden: ursprüngliche >=16-Regel und Zeitpunkt verifizieren.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
