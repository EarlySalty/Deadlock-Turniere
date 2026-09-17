# Comp-Finder

Eigenständiger Modus auf der Turnier-Website: `/turnier/comp` und
`/turnier/comp/:code`. Er ist über Hauptnavigation und den Moduswechsel auf der
Draft-Startseite erreichbar. Bestehende Drafts werden nicht verändert; dieser
Modus erzeugt keine Steam-Lobby und sendet keine Discord-Nachrichten.

## Ablauf

Eine Person erstellt eine Lobby mit ihrem Anzeigenamen und teilt Link oder
achtstelligen Code. Bis zu sechs Personen können beitreten, ohne Login.
Jede Person markiert Helden als spielbar (0), bevorzugt (1) oder höchste
Priorität (2). Nicht markierte Helden dürfen nicht zugeteilt werden. Erst
„Auswahl speichern“ veröffentlicht die Änderung für das Team.

Der Rust-Optimierer liefert bis zu zehn unterschiedliche Helden-Sets mit der
jeweils besten Spielerzuordnung. Ein Spieler erhält genau einen Helden, jeder
Held kommt höchstens einmal vor. Sortierung: Wunschpunkte absteigend, Anzahl
der höchsten Prioritäten absteigend, anschließend stabile Spieler-/Heldenfolge.
Keine behauptete Meta-, Rollen- oder Synergie-Bewertung. Fehlende Auswahlen und
konkrete gemeinsame Heldenengpässe werden getrennt angezeigt.

## Daten und Zugriff

Zentrale Migration: `Deadlock-Bots/rust/crates/dl-central-db/migrations/2026091801_turnier_comp_finder.sql`.
Die beiden Tabellen `turnier.comp_lobbies` und `turnier.comp_members` enthalten
nur Lobbyzustand, Namen, Wünsche und gehashte Spieler-Capabilities. Code/Link
berechtigen zum Lesen; Änderungen benötigen `X-Comp-Token`. Tokens stehen
weder im Link noch in öffentlichen JSON-Antworten. Browser speichert das Token
pro Lobby im Sitzungsspeicher; ein Reload desselben Tabs behält den Platz.

Lobbys sind 24 Stunden zugänglich. Abgelaufene Räume sind sofort nicht mehr
lesbar/schreibbar und werden beim nächsten Erstellen einer Lobby bereinigt.
Alle Mutationen nehmen denselben Postgres-Zeilenlock. Sechs-Plätze-Grenze,
idempotenter Beitritt, konkurrierende Speicherungen und Gastgeberwechsel sind
abgesichert. Gastgeber können andere Plätze freigeben; beim Austreten geht
die Gastgeberrolle an den nächsten Spieler. Letztes Austreten entfernt die Lobby.

## Verifikation vom 18.09.2026

- Frontend: 16 Node-Tests, TypeScript/Vite-Produktionsbuild und ESLint der betroffenen Dateien erfolgreich.
- Comp: 8 Unit-Tests, einschließlich Optimierer-Abgleich mit vollständiger Suche über 200 erzeugte Fälle.
- Wegwerf-Postgres: 5 Comp-Persistenztests, 1 vollständiger Comp-HTTP-Test,
  15 bestehende Draft-HTTP-Tests und 4 Migrationstests erfolgreich.
- Clippy für `turnier-api`/`turnier-draft` samt Comp-Tests mit `--no-deps -D warnings` erfolgreich.

Der breite Workspace-Testlauf ist ausdrücklich **nicht vollständig grün**:
Sechs Fehler in Automatik-, Scrim-Lobby-, Rematch- und Discord-Mention-Tests
wurden auf dem unveränderten `origin/main`-Stand `4c8a893` reproduziert.
Der Gesamtlauf erreichte außerdem ein Timeout bei `routine_scheduler`.
Der wegen der zwei neuen Tabellen veraltete Schema-Zähltest wurde auf 39
Tabellen erweitert und separat erfolgreich ausgeführt. Bestehende Clippy-
Befunde in Observer-Code und älteren Scrim-Testtypen liegen außerhalb dieses
Features und wurden nicht durch pauschale Lint-Ausnahmen verdeckt.

## Live-Verifikation (18.09.2026)

Der Modus ist unter `https://deutsche-deadlock-community.de/turnier/comp` live.
Anwendungsrelease: `d71fd5a1dd5d3db47eb87e249fd7adde9d30ceee`;
zentrale Migration: `e47f88f5a337c5fa808b2989be0c15ea0b6bdcdf`.
Beide Änderungen wurden auf den jeweiligen Remote-`main` übernommen.

- Neustart ausschließlich mit `bot-restart turniere`; PID wechselte von
  `488042` auf `1032401`. Das laufende `exe` ist nicht `(deleted)` und sein
  SHA-256 stimmt mit dem gebauten Release überein.
- Der einmalige zentrale Migrationslauf hat den Marker erfolgreich entfernt.
  Öffentliche Health- und Helden-Endpunkte liefern HTTP 200; Comp-Lobbys sind
  über die neue API erreichbar.
- Playwright-Live-Test mit sechs isolierten Browser-Sitzungen erfolgreich:
  Draft-Moduswechsel, Erstellen/Beitreten, Heldenprioritäten speichern,
  Sitzung nach Reload behalten, sechs eindeutige Helden bei zwölf Wunschpunkten,
  siebten Beitritt mit HTTP 409 und unautorisiertes Speichern mit HTTP 401
  ablehnen. Desktop und 390-Pixel-Mobilansicht ohne horizontalen Überlauf;
  keine Browser-Laufzeitfehler. Alle Testplätze und die Testlobby wurden entfernt.
- Journal seit Deploy: keine Einträge mit Priorität `err` oder höher.
  Es gibt jedoch weiterhin eine Anwendungs-ERROR-Meldung aus dem bestehenden
  Routine-Scheduler: `proposal_publish_failed`, Vorschlag 10,
  `Interne Authentifizierung fehlt`. Derselbe Fehler wurde unter der alten PID
  vor dem Deploy 75-mal in den vorherigen drei Tagen gefunden. Er ist kein
  Comp-Finder-Fehler und wurde durch dieses Deployment nicht behoben.

Release-Backup und maschinenlesbarer Nachweis liegen lokal unter
`rust/target/deploy-backups/comp-d71fd5a1dd5d/`. Alte Frontend-Assets bleiben für
bereits offene Tabs verfügbar; `index.html` wurde erst nach gesundem Backend
atomar veröffentlicht.

## Deployment

Migration und Anwendung müssen auf dem jeweiligen Remote-`main` liegen.
Frontend außerhalb des produktiven `dist` bauen; Assets vor `index.html`
veröffentlichen und alte gehashte Assets für offene Tabs behalten.
Backend als separates Release-Artefakt bauen und vor dem Austausch sichern.
Den zentralen Migrator mit der neuen Migration bauen. Der bestehende Launcher
unterstützt die einmalige Migration über
`$XDG_RUNTIME_DIR/deadlock-turniere-apply-central-migrations-once`.
Neustart ausschließlich über `bot-restart turniere`.

Nach Restart: PID-Wechsel, `/proc/<pid>/exe` ohne `(deleted)`, Fehlerjournal
seit Deploy und öffentlicher HTTP-/Browser-Ablauf prüfen. Testlobbys anschließend
über den Leave-Endpunkt vollständig entfernen.
