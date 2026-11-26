# 07-steam-rang – P2 · Steam-Verknüpfung, Mindestrang und Rangauflösung

## Abhängigkeiten und Schreibbereich
Nach 06; Turniere, Steam-Bot nur falls nachweislich notwendiger Vertragsfix.

## Vorhandene Arbeit / Quellen
[P3b im Originalauftrag](https://github.com/EarlySalty/Deadlock-Turniere/tree/7ae479f1eefb87805c50d3b9ff4015779970a728/.tasks/2026-11-02-turnier-ui-pug); rust/docs/known-issues.md, turnier-steam resolver.rs, [Steam-Bot](https://github.com/EarlySalty/Deadlock-Steam-Bot).
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Implementiere die vereinbarten optionalen Teilnahmebedingungen serverseitig für alle relevanten Signup-/Join-/Invite-Wege. Default bleibt bisheriges Verhalten; Gast-PUG nicht unbeabsichtigt blockieren. Prüfe vorhandene Link-/Rank-Sources, Ranglos-Fallback und Cache-TTL. Kein erfundener Rang; unklare Produktsemantik (Gast-Ausnahme, unbekannter Rang, Admin-Override) vor Freigabe als konkrete Entscheidung vorlegen.

## Abnahmekriterien
Tests für ohne/mit Steam, unter/gleich/über Mindestrang, unbekannter/veralteter Rang, Gast, Admin-Override, Einzel-/Team-Anmeldung, parallel und Fehlermeldungen; UI-Einstellungen; Gate und Datenverträglichkeit.

## Aktueller Übergabestand und Ergänzung
Falls Gast-/Steam-/Rang-Policy nicht durch Originalauftrag und Code eindeutig festliegt, offene Produktentscheidung mit Vorschlag melden.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
