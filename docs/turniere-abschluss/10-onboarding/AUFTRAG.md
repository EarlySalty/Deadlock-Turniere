# 10-onboarding – P2 · Discord-Rollen und Onboarding

## Abhängigkeiten und Schreibbereich
Parallel zu 01 möglich, aber Bots-Schreib-/Releasefenster mit 01/07/08 koordinieren.

## Vorhandene Arbeit / Quellen
[Bots-P8 Code-Sicherung](https://github.com/EarlySalty/Deadlock-Bots/tree/3be6f7749f55bf24cbbbc3f84c4fee0c618a827f), [P8 TODO](https://github.com/EarlySalty/Deadlock-Bots/blob/2299000eb8088f14d4a228c94f39304b00d8d649/.tasks/2026-11-03-onboarding-role-lookup/TODO.md); alte Basis b47f06c0.
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Portiere den alten dirty P8-Stand auf aktuellen Main, nicht den 200 Commits alten Branch mergen. IDs statt Rollen-Namenssuche, Turnierrolle und Custom-Games-Zuordnung, journeyglue, Server-as-code-Ausnahmen und Tests. Prüfe aktuelle Rollen und Discord-Prompt-Grenzen read-only. Scrims A/B, Funny Custom Ping und Explainer/Onboarding-Apply sind offene Nutzerentscheidungen; ohne Entscheidung nur Code/Preview bereitstellen, keine Guild-Mutation.

## Abnahmekriterien
Tests für stabile IDs trotz Umbenennung, Prompt-Anzahl, fehlende Rolle, bestehende Konfiguration, exakte Rules-Ausnahme, Apply-Revalidation; Gate; live Apply erst nach ausdrücklicher Freigabe.

## Aktueller Übergabestand und Ergänzung
Die frühere Warteschranke entfällt, weil 01 keinen Bots-Produktionscode-Schreibbedarf meldete. Fremde aktive PRs prüfen. Alter WIP 200 Commits hinter historischer Bots-Basis: nicht blind mergen. Kein Guild-Apply ohne Entscheidung.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
