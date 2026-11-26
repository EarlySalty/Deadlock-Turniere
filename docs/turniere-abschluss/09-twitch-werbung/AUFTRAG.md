# 09-twitch-werbung – P2 · Twitch-Turnierwerbung

## Abhängigkeiten und Schreibbereich
Parallel zu 01 möglich; separates Twitch-Repo; Wire-Vertrag mit 01 abstimmen.

## Vorhandene Arbeit / Quellen
[Twitch-Fix-Branch](https://github.com/EarlySalty/Deadlock-Twitch-Bot/tree/fix/turnier-event-signup-closed) @ ee002f044. Aktuellen main prüfen; fünf Promo-Fix-Commits: 27dcb5833, cf4769ac8, c5fbe90ca, 49fd30ddb, ee002f044; quinn-Bump b92c42b3e nicht doppelt übernehmen (damals bereits afc3fba7d auf main).
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Übernimm die vorhandenen fünf Fix-Commits gezielt auf aktuellen Twitch-Main. Verifiziere, dass alle Sendepfade denselben serverautoritativen Anmeldestatus nutzen, Links an die richtige Community-Base gebunden sind und Fehler fail-closed behandelt werden. Kein Werbeversand an echte Kanäle zum Test.

## Abnahmekriterien
Tests für offen/geschlossen, Check-in offen/geschlossen, API nicht erreichbar, stale cache, falscher Link und alle Sendepfade; aktuelle Turniere-API-Verträge, unabhängiges Gate/Release mit Mock oder freigegebener Testumgebung.

## Aktueller Übergabestand und Ergänzung
Beginne auf aktuellem Twitch-Main mit eigenem Feature-Branch; alte fünf Promo-Commits gezielt portieren. Benenne sämtliche gelesenen Turniere-API-Felder und prüfe Kompatibilität gegen aktuelle API-Contracts. Reale Live-Version nur anhand Maintainer-Nachweis; kein Werbeversand.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
