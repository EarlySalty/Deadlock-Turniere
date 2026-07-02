# Cutover — Python → Rust

Stand der Rust-Neuentwicklung unter `/rust` und was vor dem Scharfschalten noetig
ist. Der Python-Stand im Repo-Root bleibt unveraendert, ist nach dem PG-Cutover
aber nur noch ein expliziter Rollback-/Dev-Pfad.

## Status

Vollstaendiger Port, 12 Crates + Binary (`turnier-bot`), baut als ein Workspace.
Im zentralen PG-Pfad baut `turnier-bot --check` Config, Pool, AppState,
Scheduler-Objekt und Router, ohne zu servieren oder Scheduler-Checks
auszufuehren.

Verdrahtet in `turnier-bot`: HTTP-API (axum, ~213 Endpunkte) + Scheduler-Loop
(Phasenuebergaenge + Reminder) im selben Prozess, gegen die zentrale
Postgres/TimescaleDB.

## Verifikation und Freigabe

- Build/Test/Clippy workspace-weit sind Teil der technischen Barriere.
- Der Boot-Smoke `cargo run -p turnier-bot -- --check` wurde noch nicht als
  freigegebener Cutover-Schritt gegen die Zielumgebung ausgefuehrt. Er steht in
  der Cutover-Vorbereitung aus und braucht die explizite Review-/Operator-
  Freigabe inklusive passender Oracle-Erwartung.
- Migration == Live-Schema (Diff-Test) + idempotent auf Live-DB-Kopie.
- Engine-Parität über die 7 portierten Python-Tests (Double-Elim-Verdrahtung,
  Seeding, Mini-Groups, auto-num-groups, Team-Naming) — wertgenau gegen das
  Original geprüft.
- Endpunkt-Abdeckung: POST/DELETE/PATCH deckungsgleich zur Python-Routenzahl.

## Voraussetzungen für den Cutover (operativ, brauchen echte Zugänge)

1. **Internal-API-Token**: `TURNIER_INTERNAL_API_TOKEN` (bzw. `MASTER_BROKER_TOKEN`
   …) setzen, sonst sind Discord-OAuth-Login und Discord-Effekte inaktiv.
2. **Master-Broker** (Deadlock-Bots) erreichbar unter
   `DISCORD_MASTER_BROKER_BASE_URL`/`DISCORD_OAUTH_INTERNAL_API_BASE_URL`
   (Default `127.0.0.1:8766`).
3. **Steam-Bridge-DB**: `STEAM_BRIDGE_DB_PATH` auf die Linux-Pfad-Variante der
   geteilten Deadlock-Bots-SQLite setzen (der Default ist ein Windows-Pfad).
4. **Zentrale DB**: `DEADLOCK_CENTRAL_DSN` muss gesetzt sein. Der Wert darf nicht
   geloggt oder in Dateien geschrieben werden. `DATABASE_PATH` ist Rust-seitig
   Legacy/ignoriert; es gibt keinen SQLite-Fallback. `AVATAR_DIR` auf das
   bestehende Avatar-Verzeichnis setzen.
5. **Erststart** gegen die echte DB prüfen (Dashboard/Frontend gegen die API),
   dann den Python-Dienst stoppen und `turnier-bot` den Port übernehmen lassen.

## T13-Handoff-Barriere

Der echte User-Service heisst `deadlock-turniere.service`. Aktueller Stand der
Unit: `30-rust-cutover.conf` biegt `ExecStart` auf
`scripts/run_turniere_backend_rust.sh` um. Dieses Ticket startet den Dienst nicht
neu; ein Restart ist erst nach expliziter Operator-Freigabe erlaubt:

```bash
systemctl --user restart deadlock-turniere.service
```

Vor einer Freigabe muessen diese Artefakte greifbar sein:

- **Vorheriges Release-Binary:** den vor Cutover aktiven Stand von
  `rust/target/release/turnier-bot` mit Zeitstempel sichern oder aus dem letzten
  freigegebenen Release reproduzierbar bereitstellen.
- **SQLite-Rollback-Datei:** `backend/data/tournament.db` nur als explizites
  Rollback-/Forensik-Artefakt sichern. Nach PG-Cutover darf Python nicht still
  gegen diese Datei produktiv weiterlaufen.
- **Infisical/DSN:** `DEADLOCK_CENTRAL_DSN` kommt ausschliesslich ueber
  Infisical/den Service-Launcher. Den Wert nicht ausgeben, nicht in Shell-History
  kopieren und nicht in Dateien schreiben; Diagnose nur als gesetzt/nicht gesetzt
  oder als read-only Counts/Booleans.

Rollback-Varianten:

- **Rust-Binary-Rollback:** gesichertes vorheriges `turnier-bot`-Release
  zuruecklegen und `deadlock-turniere.service` erst nach Freigabe neu starten.
- **Python-/SQLite-Rollback:** Drop-in `30-rust-cutover.conf` entfernen oder
  deaktivieren, `systemctl --user daemon-reload` ausfuehren, die bewusst
  gewaehlte SQLite-Datei bereitstellen und erst nach Freigabe neu starten. Diese
  Variante braucht eine klare Datenstrategie fuer seit dem PG-Cutover entstandene
  Aenderungen.

## Endpunkt-Paritäts-Audit (durchgeführt 2026-07-02)

Siehe [`audit/2026-07-02-endpoint-parity-audit.md`](audit/2026-07-02-endpoint-parity-audit.md):
79/79 Python-Endpunkte (23 public, 56 admin) haben ein Rust-Pendant, keine
fehlenden Pfade, keine Auth-Gate-Abweichung bei Normalpfaden. Vier akzeptierte
Restrisiken (0 kritisch, 1 hoch, 3 mittel) betreffen ausschließlich
Fehlerfall-/Edge-Case-Verhalten bei ungültigen Requests, nicht den Happy-Path:

- FastAPI-422-Listenform wird nicht überall 1:1 nachgebildet (teils 400/String).
- Rust parst Discord-IDs in einigen Admin-Endpunkten strikter zu `i64` (Python
  akzeptierte auch nicht-numerische Werte und lieferte dann 404/No-Op).
- `groups/generate.num_groups` und `apply-event-preset.enabled` akzeptieren in
  Rust keine String-Koerzion mehr (`"3"`/`"false"` wie in Python).

Bewusst als Cutover-Risiko akzeptiert: echte Discord-Snowflakes sind immer
numerisch, betrifft nur Admin-Tooling/Tests mit Dummy-Werten. Row-Count-Vergleich
SQLite vs. zentrale Postgres vor dem Cutover: alle 9 stichprobenartig geprüften
Tabellen (`tournaments`, `teams`, `team_members`, `tournament_signups`,
`bracket_matches`, `group_matches`, `sessions`, `user_consents`, `rank_cache`)
identisch — kein Delta seit dem SP1-ETL-Snapshot, kein Reconciliation-Bedarf.

## Bewusst zurückgestellt / dokumentiert

Siehe [`known-issues.md`](known-issues.md) — alle beim Port gefundenen Alt-Bugs
sind dort als Opt-in-Folgefixe gelistet (Verhalten 1:1 erhalten).

## Rollback

Python-Stand im Repo-Root ist unangetastet, darf aber nicht versehentlich
produktiv gegen `backend/data/tournament.db` weiterlaufen. Rollback auf Python
ist eine explizite Operator-Entscheidung und braucht eine eigene DB-Strategie;
der Rust-Pfad nutzt `DEADLOCK_CENTRAL_DSN`.
