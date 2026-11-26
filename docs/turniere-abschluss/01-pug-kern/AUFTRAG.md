# 01-pug-kern – P0 · PUG-Kern reparieren und freigeben

## Abhängigkeiten und Schreibbereich
Agent 01 läuft separat; nicht parallel übernehmen. Turniere + zentrale DB/Broker als Testabhängigkeit.

## Vorhandene Arbeit / Quellen
[PUG-Fix b56dc0e](https://github.com/EarlySalty/Deadlock-Turniere/tree/b56dc0e076516f759f6dc3294fafff68bb0e264d) und [Originalaufträge/REGISTER/TODO/GATE-R1-TRIAGE](https://github.com/EarlySalty/Deadlock-Turniere/tree/b56dc0e076516f759f6dc3294fafff68bb0e264d/.tasks/2026-11-14-pug-chaos); [laufender PUG-Branch](https://github.com/EarlySalty/Deadlock-Turniere/tree/fix/turniere-closeout-01-pug), Remote-HEAD 3d1b69a zum Prüfzeitpunkt.
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Übernimm den vorhandenen PUG-Fix vollständig als Ausgangspunkt, nicht neu schreiben. Verifiziere aktuelle Remote-SHAs und die beiden Gast-Commits; integriere inzwischen gelandete Änderungen. Prüfe die Ursachen A–H und letzte Gate-BLOCK-Klassen. Stelle eine passende Deadlock-Bots-Revision im benachbarten Worktree bereit, prüfe die zentrale Migrationskette (solo_balance_runs/waitlist: 20261011120000_turnier_solo_balance.sql, bba52356) und den Broker-GET role-members/Voice-Vertrag. Reproduziere beide aus PR #34 bekannten central_routes-Fehler mit sauberer Test-DB; falls nur PR-spezifisch, mit Beleg an 03 übergeben. Keine doppelte Migration. Schließe die fehlende gemeinsame Broker-Vertragsprüfung für PUG ab. Erhalte den Schutz vor automatischer Teambildung, ungespeicherten Drafts, Gast-Dubletten, Row-Locks, falschen Status/Formatangaben und stummen Fehlern.

## Abnahmekriterien
Volle Rust-Suite mit Testfeatures auf isolierter DB, fmt/clippy, npm lint/test/tsc/build; Browser 390/1440 px: 30 Solo-Spieler, 5x6 Teams nach manuellen Picks, Draft in zweitem Tab, Save, Admin-Add/Remove/Check-in, Gastlimit/Dubletten, Withdraw, Start, 5/6 Teams und tatsächliches Format, Caster/Voice und Fehleranzeige. Kein automatischer Check-in-Abschluss. Frischer unabhängiger Review und Gate gegen aktuellen origin/main; 627 historische grüne Tests sind kein aktueller Nachweis. Releasefähiges Kernpaket, kein PR34- oder Brain-Sammelmerge.

## Aktueller Übergabestand und Ergänzung
**Nicht neu starten oder überschreiben.** Agent 01 ist separat zuständig. Remote-HEAD 3d1b69a; letzter vollständig berichteter Teststand 0277287: 628 Rust grün, 0 Fehler, 2 ignored; Clippy/fmt/Frontend grün, Broker-Vertragstest, PR34 15/15 grün. Aktuelle Main-Review-Regeln sind auf cd6516d aktualisiert; Branches divergent. Letzter Bericht ohne Freigabe/Live. Finalen PR/Abnahmebericht abwarten und gegen neuesten Stand prüfen. Wenn ausdrücklich zur Übernahme freigegeben, nur auf dem finalen Remote-HEAD fortsetzen. Zentralmigration bba52356 in Bots e65d3805 enthalten, kein Bots-Produktionscode-Schreibbedarf gemeldet. Historische Gate R1/R2 BLOCK; R3 ohne Urteil bei Quota zählt nicht. Keine erneute Implementierung.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
