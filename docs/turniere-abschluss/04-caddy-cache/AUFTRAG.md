# 04-caddy-cache – P1 · Caddy Cache für Turnier-Assets

## Abhängigkeiten und Schreibbereich
Parallel zu 01 möglich; eigenes Repo, live erst kompatibel mit Turniere-Release.

## Vorhandene Arbeit / Quellen
[EarlySalty/caddy-config PR #10](https://github.com/EarlySalty/caddy-config/pull/10), feat/turnier-hashed-cache-20261123 @ 06c7af132be8e4b1e2fa77510629f2d22b4182a9; Zielbranch master, historische Basis 42cdc3c. PR offen und Draft.
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Übernimm den vorbereiteten Matcher für gehashte Turnier-JS/CSS/Woff2-Assets inkl. Preview. HTML-Fallback revalidieren, API/Auth/Admin/private Daten niemals öffentlich cachen. Prüfe aktuelle Caddy-Routen, Head- und Cache-Verhalten, validiere in isolierter eigener Umgebung, keine Produktionsrechte voraussetzen.

## Abnahmekriterien
caddy adapt/validate, Matcher-Tests und GET/HEAD-Beweise für gehashte Assets, HTML, API/Auth sowie unpassende Pfade. Gate; nur kontrollierter Caddy-Reload mit Rollback, keine fremden Siteänderungen.

## Aktueller Übergabestand und Ergänzung
**Aktueller Restumfang:** 06c7af1, historisches Gate R3 BLOCK. R1/R2-Befunde laut Bericht behoben. Videos/Builds fehlende Assets und POST/SPA-Fallback korrigiert; 23/23 relevante Tests, drei unveränderte Baselinefehler (23/26 vs 21/24). Einmalige Erweiterung für nachweislich gleiche Cache-Fehler in Brand, Yoshi und Patchnotes sowie echte Geschwister. Gesamte Fehlerklasse vor nächster Prüfung erfassen, nicht jedes Mal einzeln. 404/405/416/5xx und falsche SPA-Fallbacks no-store; 200/206/304 korrekt, API/Auth privat. CI Run 45859489137 nicht gestartet (Billing/Spending Limit laut Worker-Annotation). Keine Billing-Änderung. Isolierte Caddy-Instanz und portables adapt/validate; keine Hostwerkzeuge voraussetzen. Aktuelle Caddy-Repo-Review-Regeln lesen; historische R4 als Folgeprüfung, fehlendes proprietäres Gate nur melden. PR, kein Reload. Asset-Layout-Kompatibilität dokumentieren.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
