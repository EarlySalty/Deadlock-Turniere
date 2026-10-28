# Deadlock-Turniere: Rust backend

The tournament backend is a Cargo workspace of small crates, each with one responsibility. It runs against the central PostgreSQL/TimescaleDB database (schema `turnier`). The binary is `turnier-bot`. The Python tree in `backend/` is legacy and is not started.

## Crates

| Crate | Responsibility |
|-------|----------------|
| `turnier-core` | Domain types: enums and wire DTOs. No I/O. |
| `turnier-config` | Layered configuration (TOML file, then secrets, then defaults), role and CORS derivations. Operating values are an immutable snapshot. |
| `turnier-db` | PostgreSQL pool foundation over `dl-central-db`, error type, test-pool adapter. Production migrations are not run from this crate. |
| `turnier-auth` | Opaque session tokens, RBAC (`User < Mod < Admin`), OAuth client against the master broker. |
| `turnier-steam` | Rank resolver (cache, Steam bridge, Discord roles). |
| `turnier-discord` | Master-broker client (channels, embeds, DMs, voice), tournament DM opt-out checks. |
| `turnier-engine` | Bracket generation, seeding, mini-groups, standings, status transitions, points. |
| `turnier-match` | Match lifecycle: lobby, results, best-of series, game modes, auto-lobby. |
| `turnier-match-core` | Storage-independent result mappings shared by match adapters. |
| `turnier-draft` | Pick/ban state machine with compare-and-swap on the action index. |
| `turnier-automatik` | Automatic tournament planning: presets, proposals, votes, feedback, DM opt-out, signals. |
| `turnier-scheduler` | Background loop: phase transitions and reminders. |
| `turnier-scrim` | Scrim domain foundation: pure decisions separated from repositories and effects. |
| `turnier-observer` | Automatic in-game observer for scrims. |
| `turnier-api` | axum HTTP layer: router, extractors, error mapping, static routes. |
| `turnier-bot` | Composition root and binary. `--check-config` validates the file without secrets or a pool. |

## Documentation

- [`docs/architecture.md`](docs/architecture.md): layers and request lifecycle
- [`docs/db-contract.md`](docs/db-contract.md): database contract; the schema lives in the central migrations of Deadlock-Bots
- [`docs/known-issues.md`](docs/known-issues.md): historical list of issues found while porting (historical header)
- [`docs/cutover.md`](docs/cutover.md): historical cutover checklist
- [`docs/adr/`](docs/adr/): architecture decision records
- [`docs/audit/`](docs/audit/): dated audit snapshots (historical)

## Build and checks

The workspace depends on a sibling `Deadlock-Bots` checkout (`../../Deadlock-Bots/rust/crates/...`). Build from a layout where that checkout is a sibling of this repository directory, in the same parent directory.

```bash
cd rust
cargo build --release -p turnier-bot
cargo test
cargo clippy --workspace --all-targets -- -D warnings
```

Deploy builds and deploy steps are in the root `README.md`.
