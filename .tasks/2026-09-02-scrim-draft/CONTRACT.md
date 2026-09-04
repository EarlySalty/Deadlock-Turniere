# Contract: Scrim-Draft-Tool mit Lobby-Automatik

status: aktiv
datum: 2026-09-02
klasse: hoch
repo: Deadlock-Turniere (Partner-Contract: Deadlock-Steam-Bot/.tasks/2026-09-02-scrim-lobby-automatik/CONTRACT.md)

Dieser Contract ist der Maßstab für Implementierung und Merge-Kritiker. Nach dem
Anlegen ist er unveränderlich: der Hook lässt nur noch die `status:`-Zeile und
Anhänge unter `## Amendments` zu. Wer ein REQ oder INV ändern will, schreibt ein
Amendment mit Begründung; Produkt-, API- oder Datenänderungen entscheidet der User.

## Ziel

Zwei Captains draften Helden für ein Scrim in einem Tool, das aussieht und sich anfühlt wie das Draft-Tool von deadlocklabs.gg, und am Ende steht automatisch der Join-Code einer fertigen Deadlock-Custom-Lobby plus ein Discord-Post, ohne dass Leo oder ein anderer Mensch Lobby anlegt, Code verteilt oder Ergebnisse abtippt.

## Anforderungen (user-sichtbares Verhalten)

- REQ-01: Jeder Besucher kann ohne Login unter `/turnier/draft` einen Draft anlegen. Einstellbar: Teamnamen (optional, Default "Team 1" und "Team 2"), Bans je Team 0 bis 6 (Default 2), Timer aus/30/45/60/90 Sekunden (Default 30). Format ist 6v6. Ergebnis: sechsstelliger Raum-Code, Link `/turnier/draft/<code>`, Knopf "Link kopieren". Keine Rollen- oder Login-Beschränkung (Entscheidung User 2026-09-02).
- REQ-02: Warteraum unter `/turnier/draft/<code>`: Statuszeile "Warten auf Spieler", Titel "<Team 1> vs <Team 2>" (Team 1 gold, Team 2 blau), Raum-Code groß mit "Link kopieren", Unterzeile mit Format, Bans, Timer. Zwei Team-Karten mit "Captain übernehmen"; nach Übernahme zeigt die Karte "Du bist Captain" mit "Bereit" und "Verlassen", für andere "Captain da, wartet". Zuschauer-Zähler ("1 schaut zu"). Sind beide Captains bereit, startet der Draft ohne weiteren Klick.
- REQ-03: Draft-Board: oben Phase ("BAN-PHASE 1/16" bzw. "PICK-PHASE 3/16"), aktives Team plus Aktion, "Du bist dran" für den aktiven Captain, Countdown. Darunter Ban-Leiste (je Team so viele Slots wie Bans). Links Team-1-Spalte, rechts Team-2-Spalte mit je 6 Slots (Nummer, nach Pick Portrait plus Name). Unten Hero-Leiste mit allen Helden als Portraits, Suchfeld, Modus-Badge "BAN" oder "PICK". Auswahl eines Helden zeigt die große Splash-Art rechts der Mitte, in Team-Farbe getönt, links unten Beschriftung "Bannen: <Held>" oder "<Team> wählt: <Held>", mittig der Aktionsknopf "Bannen" (rot) oder "Einloggen" (gold). Bestätigter Ban: großes rotes X mit "GEBANNT"-Stempel und Rot-Einfärbung als Übergang. Bestätigter Pick: Heldenname groß in Team-Farbe. Gebannte und gepickte Helden in der Leiste ausgegraut und nicht wählbar.
- REQ-04: Läuft der Timer ab, wählt das System automatisch (bestehende Auto-Logik) und markiert den Eintrag im Board als "Auto". Der Timer ist serverseitig maßgeblich, das Frontend zeigt nur.
- REQ-05: Endscreen nach der letzten Aktion: "Draft abgeschlossen", Titel "<Team 1> vs <Team 2>", Bans als kleine Portraits, je Team sechs Karten (Splash-Art, Pick-Nummer, Name). Knöpfe "Teilen" (Link kopieren), "Rematch" (neuer Draft mit gleichen Einstellungen, Seiten getauscht, Link zum neuen Raum), "Zurück".
- REQ-06: Nach Abschluss fordert das Backend beim Steam-Bot über dessen interne API eine Custom-Lobby an. Der Endscreen zeigt "Lobby wird erstellt" und danach den Join-Code groß mit "Code kopieren". Schlägt die Anfrage fehl oder dauert länger als 30 Sekunden, zeigt der Endscreen "Lobby konnte nicht erstellt werden, bitte selbst anlegen"; der Draft bleibt gültig und der Knopf "Erneut versuchen" wiederholt die Anfrage.
- REQ-07: Ist in der Config ein Discord-Kanal gesetzt (Default 1521522998199324853, #announcement-scrims), postet das Backend nach erfolgreicher Lobby-Erstellung genau eine Nachricht mit Teamnamen, Picks, Bans und Join-Code, und nach Match-Ende genau eine Nachricht mit Gewinner, Dauer und Match-ID. Ohne Config-Wert wird nichts gepostet. Kein Post bei Wiederholungen (idempotent je Draft).
- REQ-08: Board, Warteraum und Endscreen aktualisieren sich für beide Captains und alle Zuschauer live mit höchstens 2 Sekunden Verzögerung (bestehendes Polling ist ausreichend).
- REQ-09: Alle sichtbaren Texte auf Deutsch mit echten Umlauten, keine Em-Dashes. Team-1-Farbe ist das Marken-Gold aus `dl-brand/tokens.css`, Team-2-Farbe ein kräftiges Blau, Ban-Rot, Hintergrund Schwarz nach Marke. Der Look folgt `DESIGN.md` in diesem Ordner.
- REQ-10: Turnier-gebundene Drafts (mit `bracket_match_id`) laufen unverändert im selben neuen UI.
- REQ-11: Das UI ist auf 1920 breit primär, auf 1280 noch bedienbar (Hero-Leiste scrollt), kein Mobile-Ziel.

## Invarianten (darf sich nicht ändern)

- INV-01: Draft-Regeln (Sequenz, Gültigkeit, Auto-Pick, Timer-Deadline) leben ausschließlich in `turnier-draft` (`sequence.rs`, `repo.rs`). Das Frontend rechnet keine Regeln nach und entscheidet nichts.
- INV-02: Bestehende Routen `/api/draft/*` bleiben abwärtskompatibel; Erweiterungen sind additiv (neue Felder, neue Routen).
- INV-03: Datenbank-Änderungen nur als neue additive Migration in `turnier.*`, keine Änderung bestehender Migrationen.
- INV-04: Bestehende Tests werden weder gelöscht noch abgeschwächt; die 17 Draft-DB-Tests bleiben grün.
- INV-05: Der Steam-Bot wird nur über seine interne HTTP-API auf localhost angesprochen. Kein Zugriff auf seine Datenbank, Dateien oder Secrets.
- INV-06: Hero-Bilder kommen über `heroes_provider.rs` von deadlock-api. Kein Code, keine Assets, keine Stylesheets von deadlocklabs.gg werden kopiert; der Look wird nachgebaut.
- INV-07: Keine Secrets in Config-Dateien oder Umgebungsvariablen; Dienst-Token für den Steam-Bot kommen aus Infisical nach dem bestehenden Muster des Repos.
- INV-08: Discord-Kanäle nur per ID, nie per Name.

## Nicht-Ziele

- Spieler-Draft für PUGs (Captains wählen Spieler), "Habe Zeit"-Voting, Zusagen-Bot, Erinnerungen, Ersatz-Korridor (Phase 2 und 3).
- Steam-Einladungen an einzelne Spieler oder Freundschafts-Pflicht; Spieler treten per Join-Code bei.
- Login-Pflicht, Rollenprüfung, Draft-Verzeichnis, Team-Logo-Upload, eigener Sequenz-Editor, Shadow-Picks.
- Umbau von Polling auf WebSocket.
- Änderungen am Scrim-Scheduling (dl-coaching, scrimglue, scrim_adapter).

## Erlaubter Änderungsbereich

- `rust/crates/turnier-draft/**` (additiv: Lobby-Verknüpfung, Discord-Post-Status, Rematch)
- `rust/crates/turnier-api/src/draft.rs` und neue Module unter `rust/crates/turnier-api/src/` für Steam-Bot-Client und Discord-Post, Config-Felder in `turnier-api`
- `rust/crates/turnier-steam/**` nur, wenn dort der bestehende Steam-Bot-Client liegt (Research klärt)
- `frontend/src/pages/Draft*.tsx`, `frontend/src/components/draft/**`, `frontend/src/hooks/useDraftLobby.ts`, `frontend/src/types/draft*.ts`, Routen in `frontend/src/App.tsx`
- neue additive Migration unter dem bestehenden Migrationsordner
- `docs/plans/2026-07-16-draft-lobbys-community-tool.md` (Status nachziehen)
- `.tasks/2026-09-02-scrim-draft/**`

## Verbotene Änderungen

- `turnier-engine`, Bracket-, Check-in- und Turnier-Admin-Logik
- bestehende Migrationen
- Lint-, Format- und Build-Konfigurationen (eslint, tsconfig, Cargo-Workspace-Lints)
- andere Frontend-Seiten außer Routen-Registrierung
- Repos Website und Deadlock-Bots
- Caddy-Konfiguration

## Offene Produktfragen

- keine (Format-Defaults, Zugriff ohne Beschränkung, Reihenfolge Phase 1 bis 3 vom User am 2026-09-02 entschieden)

## Amendments

