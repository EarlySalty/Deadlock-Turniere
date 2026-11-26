# 02-admin-ui – P1 · Admin-Workflow, Team-Builder und Home

## Abhängigkeiten und Schreibbereich
Nach 01; Schreibbereich Turniere-Frontend, notwendige API-Anpassungen abgestimmt.

## Vorhandene Arbeit / Quellen
[Neuere Admin-/Brain-Sicherung 77d6f47](https://github.com/EarlySalty/Deadlock-Turniere/tree/77d6f470b83c183c2c6fbd40a6cbfc488c5f938a), [alter Cleanup e92f123](https://github.com/EarlySalty/Deadlock-Turniere/tree/e92f12349e682d9cef0755f84c27555ddf2eab3c), [Cleanup-TODO](../QUELLEN.md#wave-1-cleanup-p1b-p4b), [altes Register P1b/P4b](https://github.com/EarlySalty/Deadlock-Turniere/tree/7ae479f1eefb87805c50d3b9ff4015779970a728/.tasks/2026-11-02-turnier-ui-pug).
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Vergleiche neuere UI-Sicherung und altes Cleanup; portiere nur fehlende fachliche Verbesserungen auf den freigegebenen PUG-Stand. AdminWorkflowPanel, klare Phasen, Home-Karten und Sortierung, ein Team-Builder ohne doppelte Bedienflächen, stabiler Save bei Navigation und parallelen Tabs, Lade-/Fehlerzustände, mobile Darstellung. Erhalte Team löschen, Recruiting, Ersatzspieler, Anmeldungen entfernen und Solo-Balance. Vermeide neue Duplikate von Serverregeln. WIP-MCP-Dateien bleiben ausdrücklich draußen (11).

## Abnahmekriterien
Build/lint/tsc/tests, gezielte Regressionen für Save-Fehler, Remount, Turnierwechsel, Draft, Home-CTA nach Status und Fehlerliste; Browser 390/1440 px mit Solo- und Teamturnier, Admin/Public-Konsistenz; Gate und eigene Release-Abnahme.

## Aktueller Übergabestand und Ergänzung
Alte UI-Sicherung historisch nicht buildbar (builderActive-Prop fehlt). Die neue Version zuerst auf Überdeckung prüfen. Alle Details im Cleanup-TODO in QUELLEN.md. Keine historischen CTA-Regeln übernehmen, wenn der neue serverautoritative Anmeldestatus abweicht.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
