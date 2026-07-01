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

## Verifikation, die schon gelaufen ist

- Build/Test/Clippy workspace-weit; `--check`-Boot erfolgreich.
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

## Empfohlener Folge-Schritt vor Live

Ein **endpunkt-genauer Paritäts-Audit** der zwei grossen Router (`public` aus
`routes.py`, `admin` aus `admin_routes.py`): Diese wurden parallel portiert und
sind durch Build/Clippy/Tests + Review abgesichert, haben aber (anders als die
Engine) keine endpunktweisen Paritätstests. Ein Audit gegen das Python-Original
(Request/Response-Form, Statuscodes, Auth-Gates) ist vor dem Scharfschalten
ratsam — analog zum Vorgehen bei den anderen Rust-Rewrites.

## Bewusst zurückgestellt / dokumentiert

Siehe [`known-issues.md`](known-issues.md) — alle beim Port gefundenen Alt-Bugs
sind dort als Opt-in-Folgefixe gelistet (Verhalten 1:1 erhalten).

## Rollback

Python-Stand im Repo-Root ist unangetastet, darf aber nicht versehentlich
produktiv gegen `backend/data/tournament.db` weiterlaufen. Rollback auf Python
ist eine explizite Operator-Entscheidung und braucht eine eigene DB-Strategie;
der Rust-Pfad nutzt `DEADLOCK_CENTRAL_DSN`.
