# 12-altlasten – P2 · Verifizierte Altlasten und Release-Lücken

## Abhängigkeiten und Schreibbereich
Nach 08 und 11; read-only Audit kann früher beginnen, Code erst auf freigegebenem Main.

## Vorhandene Arbeit / Quellen
rust/docs/known-issues.md aus aktuellem Repo, [PUG-Originalregister](https://github.com/EarlySalty/Deadlock-Turniere/tree/b56dc0e076516f759f6dc3294fafff68bb0e264d/.tasks/2026-11-14-pug-chaos), [historische Sicherungen und Quellen](../QUELLEN.md), aktuelle GitHub-Advisories und Code.
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Klassifiziere historische Findings als reproduzierbar, bereits gefixt, Produktentscheidung oder überholt. Fokus auf match-/teambezogene Punkte, Double-Elim-Platzierung, doppelte Gruppenresultate, Draft-Validierung, Auth/Rollenstaleness, Consent, Status-Constraints und Reminder-Catch-up. Nur reproduzierte releasekritische Fehler sofort in fachlich zusammenhängenden Paketen fixen, nicht eine ungeprüfte Totalrefaktorierung. Nicht willkürlich Punkte/History neu berechnen.

## Abnahmekriterien
Belege pro Befund; rote Regression vor Fix, grüne danach; Daten-/Vertragskompatibilität, gezielte Tests plus Gate je notwendigem Paket. Nichtkritische Restpunkte mit Besitzer und Grund an 13; keine erfundene Freigabe.

## Aktueller Übergabestand und Ergänzung
13 Dependabot-Befunde auf dem damaligen Turniere-Standardbranch gemeldet (3 high, 7 moderate, 3 low). Aktuelle Betroffenheit prüfen, releasekritische Findings beheben. Historische CI-/Observer-Sicherungen nur relevante Deltas. Keine ungeprüften Major-Upgrades.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
