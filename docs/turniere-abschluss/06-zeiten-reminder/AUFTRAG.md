# 06-zeiten-reminder – P1 · Zeitzonen und Reminder

## Abhängigkeiten und Schreibbereich
Nach 05; Scheduler, Zeit-API und Formularpfade.

## Vorhandene Arbeit / Quellen
[P10/P11 im Originalauftrag](https://github.com/EarlySalty/Deadlock-Turniere/tree/7ae479f1eefb87805c50d3b9ff4015779970a728/.tasks/2026-11-02-turnier-ui-pug); eecd2a5, PUG scheduler-Harness und reminders.rs.
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Prüfe den bereits teilweise implementierten Zeitzonenfix statt ihn neu zu schreiben. Alle Create/Edit/Automatik/Liga-Pfade müssen konsistente zonierte Zeiten und UTC-Speicherung haben; CET/CEST, DST-Lücken/doppelte Uhrzeiten und explizite Zeitzonen testen. Reminder-Fenster am tatsächlichen serverseitigen Registrierungsstatus und -ende ausrichten, Check-in mit offener Anmeldung einbeziehen. 0-Minuten-Offset, Dedupe, Wiederanlauf und tatsächliche Zustellung untersuchen. Keine stillen historischen Korrekturen an #16/#17.

## Abnahmekriterien
Deterministische Uhr-/DB-Tests (kein echter Versand), Check-in/Start/Registrierung/Match-Reminder, doppelte Scheduler-Ausführung, Neustart und DST; UI-Formtests, Gate. Falls Altzeitkorrektur nötig: read-only Soll/Ist-Liste und gesonderte Freigabe.

## Aktueller Übergabestand und Ergänzung
Bestehende Zeitdaten nicht mutieren. Isolierte Fake-Uhr und Mocks verwenden.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
