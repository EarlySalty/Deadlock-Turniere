# Cutover — Python → Rust

Stand der Rust-Neuentwicklung unter `/rust` und was vor dem Scharfschalten nötig
ist. Der Python-Stand im Repo-Root bleibt unverändert und ist der Rollback-Pfad.

## Status

Vollständiger Port, 12 Crates + Binary (`tb-app`), baut als ein Workspace. Alle
Tests grün (`cargo test --workspace`, 37 Suites), `cargo clippy --workspace
--all-targets -- -D warnings` sauber, Release-Build OK. `tb-app --check` bootet
end-to-end (Config → Pool → Migration → AppState → Scheduler → Router) und
degradiert ohne externe Dienste sauber.

Verdrahtet in `tb-app`: HTTP-API (axum, ~213 Endpunkte) + Scheduler-Loop
(Phasenübergänge + Reminder) im selben Prozess, gegen die geteilte SQLite-DB.

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
4. **DB-Pfad**: `DATABASE_PATH` auf die echte `backend/data/tournament.db`
   (geteilt mit Python) zeigen lassen; `AVATAR_DIR` auf das bestehende
   Avatar-Verzeichnis.
5. **Erststart** gegen die echte DB prüfen (Dashboard/Frontend gegen die API),
   dann den Python-Dienst stoppen und `tb-app` den Port übernehmen lassen.

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

Python-Stand im Repo-Root ist unangetastet. Rollback = `tb-app` stoppen, Python
`uvicorn main:app` starten. Beide nutzen dieselbe `tournament.db`.
