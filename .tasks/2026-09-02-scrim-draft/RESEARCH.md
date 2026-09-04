# Research: Scrim-Draft-Tool mit Lobby-Automatik

status: aktiv
datum: 2026-09-02
klasse: hoch

## Auftrag

Zwei Captains draften Helden unter `/turnier/draft`, am Ende steht automatisch der
Join-Code einer fertigen Deadlock-Custom-Lobby plus ein Discord-Post, ohne dass
ein Mensch eingreift. Der Look folgt DESIGN.md. Dieses Dokument klärt Bestand,
Verträge und Risiken vor dem Bau.

## Beobachtungen (belegt, siehe EVIDENCE.md)

### Draft-Kern steht und ist live
- Die Regeln (Sequenz, Gültigkeit, Auto-Pick, Deadline) leben komplett in
  `turnier-draft` (`sequence.rs`, `repo.rs`). Auto-Pick schreibt `is_auto = TRUE`
  bei abgelaufener `deadline_at`. INV-01 ist damit bereits erfüllt, das Frontend
  muss nur anzeigen.
- Die freie Lobby existiert vollständig: `POST /api/draft/lobbies` legt an,
  `GET .../{code}` liefert den Vollzustand (`bans`, `picks_team1`, `picks_team2`,
  `current_team_slot`, `actions` mit `is_auto`), `POST .../{code}/action` zieht.
  Alle Endpunkte sind live (8900 und über Caddy unter `/turnier/api`).
- Token-Modell: Der Captain-Token IST die Team-Identität. `take_lobby_action`
  bildet `token == team1_token` auf Team 1 ab, sonst Team 2, sonst `InvalidToken`.
  Kein separater Team-Slot im Link; der Token wird per `?t=` übergeben und pro
  Code im localStorage gemerkt. Zuschauer haben keinen Token.
- Polling steht (1000 ms, stoppt bei `completed`). REQ-08 ist ohne Umbau erfüllt.
- Rate-Limit: 10 Lobbys pro IP pro Stunde, im Prozessspeicher.

### Frontend-Fundament passt zum DESIGN
- `framer-motion ^12`, Tailwind v4 (CSS-config via `@tailwindcss/vite`, keine
  `tailwind.config`), `lucide-react`, React 19, react-router 7, tanstack-query 5
  sind vorhanden. dl-brand-Gold `#c8a86b` liegt als `--color-primary` in
  `brand-tokens.css` (aus `Website/dl-brand/tokens.css`).
- Routen `draft` und `draft/:code` sind registriert. Build `tsc -b && vite build`
  nach `frontend/dist` (Vite `base '/turnier/'`), Dev-Server `npm run dev` auf
  5173 mit Proxy auf 8900. Caddy serviert das gebaute `dist` (Reverse-Proxy).

### Discord-Post-Weg existiert bereits fertig
- `turnier-discord` postet über den Master-Broker (`BrokerClient`, HTTP-POST an
  `http://127.0.0.1:8766` mit Header `X-Internal-Token`). `send_lobby_announcement`
  (Lobby-Code plus Teams) und `send_match_stats_to_channel` (Sieger, Dauer,
  Match-ID) sind exakt die zwei Posts, die REQ-07 verlangt. Kanal-ID kommt aus
  Config. Wiederverwenden statt neu bauen.

### Config und Secrets
- `Config::from_env()` liest über einen Schicht-Resolver (Datei aus
  `CREDENTIALS_DIRECTORY`, dann Env, dann Default). Der Launcher zieht Secrets
  über Infisical (`dl-infisical-env --profile all`) und systemd-Credentials. Neue
  Config-Felder (Discord-Scrim-Kanal-ID, Steam-Bot-Basis-URL) gehören additiv in
  `struct Config`. Ein interner Token (`turnier_internal_api_token` /
  `MASTER_BROKER_TOKEN`) ist bereits vorhanden und für neue Dienst-Pfade
  wiederverwendbar.

### Migrationen liegen außerhalb des Repos
- `run_migrations` ist ein No-op; produktiv migriert `dl-central-migrate`. Die
  Draft-Tabellen (Migration `2026071610`) sind hier nicht eingecheckt. Eine
  additive Migration (INV-03) muss folglich im zentralen Migrations-Repo landen,
  nicht in Deadlock-Turniere. Der Contract-Punkt "neue Migration unter dem
  bestehenden Migrationsordner" trifft auf dieses Repo nicht zu.

### Tests
- Die 17 Draft-DB-Tests sind 11 in `draft_db.rs` plus 6 in `lobby_db.rs`, gegen
  eine echte Wegwerf-PG (`turnier_db::test_pool()`, braucht `DEADLOCK_CENTRAL_DSN`).
  Lauf: `cargo test -p turnier-draft` mit gesetztem Test-DSN. Toolchain stable
  (Workspace `rust-version 1.85`); Nutzervorgabe `/home/nathanael/.cargo/bin/cargo`.

## Kernrisiken

### R1 (hoch): Steam-Bot-Anbindung widerspricht INV-05
Es gibt heute KEINEN HTTP-Client zum Steam-Bot. Der einzige Kanal ist die geteilte
SQLite-`steam_tasks`-Queue (`SteamBridge`, Task `GC_CREATE_CUSTOM_LOBBY`), und die
bestehende Lobby-Erstellung braucht eine Bracket-/Group-Match-Zeile mit
tournament_id. Ein freier Scrim-Draft hat keine solche Zeile. INV-05 verlangt
aber ausdrücklich HTTP-only auf localhost, ohne Zugriff auf DB/Dateien des
Steam-Bots. Das erzwingt einen neuen HTTP-Client (Partner-Endpoint aus dem
Deadlock-Steam-Bot-Contract) statt der SQLite-Queue. Das ist die zentrale
Vorentscheidung; solange sie offen ist, kein Implement des Lobby-Pfades.

### R2 (hoch): Splash-Art (`icon_hero_card`) wird serverseitig weggeworfen
`Hero` hat nur ein `image_url`, und `From<ApiHero>` bevorzugt das kleine Portrait
(`icon_image_small_webp`); `icon_hero_card_webp` fällt nur ein, wenn kein Portrait
da ist. DESIGN Screen 3/4 und REQ-03/05 brauchen die große Splash-Art. Der Live-
Endpoint liefert `..._sm.webp`. Nötig: additives Splash-Feld auf `Hero`, `ApiHero`
und der `/api/draft/heroes`-Antwort (INV-02 additiv, INV-06 Bilder nur über den
Provider). Der statische Fallback hat gar keine Bilder.

### R3 (mittel): Ban-Anzahl 0-6 pro Team ist nicht modellierbar
REQ-01 will Bans 0 bis 6 je Team, aber es gibt nur drei feste Presets (0/1/2 Bans).
Die Sequenz kommt aus `preset(name)`. Nötig ist entweder ein Sequenz-Generator aus
einem Ban-Parameter oder neue Presets, plus ein neues Request-Feld (`bans`), das
`CreateLobbyRequest`/`CreateLobbyBody` bisher nicht kennen. INV-01 verlangt, dass
die Sequenz-Erzeugung in `turnier-draft` bleibt.

### R4 (mittel): Warteraum, Captain-Claim und Ready sind nicht im Datenmodell
`draft_sessions` hat keine Captain-Claim- oder Ready-Spalte, und `create_lobby`
setzt die Deadline sofort (Draft läuft ab Anlegen). REQ-02 (Captain übernehmen,
Bereit, Start erst wenn beide bereit) braucht neuen Server-Zustand: additive
Spalten plus ein Start-Übergang. Ob der Claim rein tokenbasiert (wer den Link hat)
oder als expliziter Server-Claim läuft, ist eine Produktentscheidung im Rahmen des
Contracts.

### R5 (niedrig): Timer- und Reserve-Defaults weichen ab
REQ-01 will Timer aus/30/45/60/90 und Bans-Default 2. Das aktuelle UI hat feste
Presets und Reserve-Default 120 s. Reine Frontend-/Validierungsarbeit, `round_seconds`
0/30/45/60/90 muss die Backend-Grenze 10..=300 respektieren (0 = "aus" braucht
Sonderbehandlung, weil 0 unter der Untergrenze 10 liegt).

## Hypothesen (unbelegt, nicht als Fakt weiterreichen)

- Der Partner-Contract (Deadlock-Steam-Bot/2026-09-02-scrim-lobby-automatik) baut
  einen HTTP-Endpoint zum match-losen Lobby-Erstellen. Prüfen: den Partner-Contract
  lesen, bevor der turnier-seitige Client-Vertrag festgezurrt wird.
- Der Discord-Scrim-Post kann `send_lobby_announcement` fast unverändert nutzen,
  auch wenn kein `match_id` existiert (Signatur erwartet `match_id: i64`). Prüfen:
  ob eine synthetische ID oder eine neue schlanke Post-Methode nötig ist.
- Caddy serviert `frontend/dist` als statisches Root und proxyt `/turnier/api`.
  Prüfen: Caddyfile liegt außerhalb des Repos (INV/Verbot: Caddy nicht ändern),
  Annahme aus `vite.config` und README, nicht am Caddyfile selbst verifiziert.

## Wahrscheinlich zu ändernde Dateien

- `rust/crates/turnier-draft/src/heroes_provider.rs`: Splash-Feld additiv (R2).
- `rust/crates/turnier-draft/src/sequence.rs`, `repo.rs`: Ban-0-6-Sequenz, Warteraum-/Ready-Zustand (R3, R4).
- `rust/crates/turnier-api/src/draft.rs`: neue/erweiterte Request-Felder, Heroes-Splash, Lobby-/Discord-Trigger.
- neues Modul `rust/crates/turnier-api/src/` (Steam-Bot-HTTP-Client) plus Config-Felder in `turnier-config` (R1).
- `rust/crates/turnier-discord/src/notifier.rs`: ggf. schlanke Scrim-Post-Methode.
- `frontend/src/pages/DraftLobbyNeu.tsx`, `DraftBoard.tsx`, neue `components/draft/**`, `hooks/useDraftLobby.ts`, `types/tournament.ts`, `App.tsx`.
- additive Migration im zentralen `dl-central-migrate` (nicht in diesem Repo).

## Risiken / Seiteneffekte

- Neue Spalten auf `draft_sessions` betreffen `DraftSessionRow`/`LobbySessionRow`
  und die 17 DB-Tests (INV-04, dürfen nicht brechen).
- Der Steam-Bot-Client ist ein neuer Außenpfad (SSRF/Timeout/Idempotenz beachten;
  REQ-06 verlangt 30-s-Deckel und "Erneut versuchen").
- Discord-Post muss idempotent je Draft sein (REQ-07), der Broker hat bereits
  `idempotency_key`.

## Offene Fragen

- R1: HTTP-Client zum Steam-Bot oder match-lose Queue-Variante? Blockiert den
  Lobby-Pfad, muss vor dem Plan entschieden werden (Partner-Contract lesen).
- Warteraum-Claim: tokenbasiert oder expliziter Server-Claim mit neuer Spalte?
- Timer "aus": eigener Nullwert-Pfad oder sehr großer Wert? Backend-Grenze klären.
