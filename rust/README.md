# Deadlock-Turniere — Rust

Neuentwicklung des Turnier-Backends in Rust. Der Python-Stand im Repo-Root
(`backend/`) bleibt während der Migration unverändert lauffähig und ist der
Rollback-Pfad; der gesamte neue Code lebt ausschließlich unter `/rust`.

Funktional identisch zum Python-Backend, technisch sauberer aufgebaut: ein
Cargo-Workspace aus kleinen Crates mit je einer klaren Zuständigkeit. Beide
Stände teilen sich dieselbe SQLite-Datenbank (`backend/data/tournament.db`),
sodass ein Wechsel in beide Richtungen ohne Datenmigration möglich ist.

## Crates

| Crate | Zuständigkeit |
|-------|---------------|
| `tb-core` | Domänen-Typen: Enums + Wire-DTOs (1:1 zu den Pydantic-Modellen). Kein I/O. |
| `tb-config` | Geschichtete Konfiguration (Datei → Env → Default), Rollen-/CORS-Ableitungen. |
| `tb-db` | `SqlitePool`, PRAGMA-Setup (WAL/FK/busy_timeout), konsolidierte Migration. |
| `tb-steam` | Rang-Resolver, read-only Steam-Bridge-DB, `rank_cache`. |
| `tb-discord` | Master-Broker-Client, Embeds, `discord_tasks`-Queue. |
| `tb-auth` | RBAC, Session-Resolver (opake Tokens), interner OAuth-Client. |
| `tb-tournament` | Bracket-Engine + Generierung/Standings/Status-Übergänge + Punkte. |
| `tb-match` | Match-Lebenszyklus: Lobby, Ergebnis, Serien, Spielmodi, Auto-Lobby. |
| `tb-draft` | Pick/Ban-Zustandsmaschine. |
| `tb-scheduler` | Hintergrund-Loop: Phasenübergänge + Reminder. |
| `tb-web` | axum-Router (alle Endpunkte), Extractoren, Static-Frontend. |
| `tb-app` | Composition-Root + Binary. |

## Dokumentation

- [`docs/architecture.md`](docs/architecture.md) — Schichten, Request-Lebenszyklus, Datenfluss
- [`docs/db-contract.md`](docs/db-contract.md) — DB-Schema-Vertrag (Single Source of Truth)
- [`docs/known-issues.md`](docs/known-issues.md) — beim Port gefundene Alt-Bugs/Inkonsistenzen
- [`docs/cutover.md`](docs/cutover.md) — Stand + Schritte zum Scharfschalten
- [`docs/adr/`](docs/adr/) — Architektur-Entscheidungen

## Build

```bash
cd rust
cargo build
cargo test
cargo clippy --workspace --all-targets -- -D warnings
```
