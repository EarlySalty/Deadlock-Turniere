# Deadlock-Turniere

Tournament platform of the Deutsche Deadlock Community: tournaments, team and solo signups, check-in, brackets, match lobbies, results and Discord notifications. The user interface is German.

## Components

- Backend: Rust workspace in `rust/`, binary `turnier-bot` (crate list in `rust/README.md`). One process serves the JSON API and runs the scheduler.
- Database: central PostgreSQL/TimescaleDB, schema `turnier`. Migrations are owned by Deadlock-Bots (`dl-central-migrate`), not by this repo. The SQLite schema and `backend/data/*.db` are legacy.
- Frontend: React, TypeScript and Vite in `frontend/`. `npm run build` writes `frontend/dist`, which Caddy serves as static files.
- Legacy, not started on v50671: Python `backend/`, `ops/restart.ps1` and `start_backend.ps1` (NSSM/Windows). Do not start them against a database.
- Historical docs: `WORKFLOW.md`, `docs/plans/`, `docs/specs/`, `docs/superpowers/`, `docs/internal/`, `rust/docs/audit/`, `rust/docs/cutover.md` and `rust/docs/known-issues.md` carry a historical header. The code on `origin/main` is authoritative.

## Live layout (host v50671, checked 2026-10-28)

| Piece | Where | Check |
|---|---|---|
| User unit `deadlock-turniere.service` | `systemctl --user`; file `~/.config/systemd/user/deadlock-turniere.service`, drop-ins in `deadlock-turniere.service.d/` | `systemctl --user status deadlock-turniere.service` |
| Failure notice | `unit-failure-notify@deadlock-turniere.service.service` (OnFailure drop-in) | `systemctl --user status unit-failure-notify@deadlock-turniere.service.service` |
| Binary | `~/repos/Deadlock-Turniere/rust/target/release/turnier-bot`, started through the `dl-infisical-env` launcher (sudo; secrets from Infisical) | `pgrep -a turnier-bot` |
| Config | `~/.config/deadlock-turniere/bot.toml` (outside git; template `config/bot.example.toml`) | `turnier-bot --config <file> --check-config` |
| HTTP port | `127.0.0.1:8900` (`backend_port`) | `ss -ltnp 'sport = :8900'` |
| Public route | Caddy system service, `~/repos/Caddy/hosts/v50671/Caddyfile`: `/turnier/api/*` and `/turnier/auth/*` go to port 8900 with the `/turnier` prefix stripped; other `/turnier*` paths serve `frontend/dist` | see below |

Public origin: `https://deutsche-deadlock-community.de/turnier`. `admin.deutsche-deadlock-community.de` is not used for Turniere user pages.

Services the backend calls, by config key: Discord master broker `discord_master_broker_base_url` (127.0.0.1:8770), Discord OAuth internal API `discord_oauth_internal_api_base_url` (127.0.0.1:8766), Steam bot `steam_bot_base_url` (127.0.0.1:8782), observer Steam bot `observer_steam_bot2_base_url` (127.0.0.1:8784), Deadlock API `observer_deadlock_api_base_url`.

## Build

Path dependencies point to a sibling `Deadlock-Bots` checkout: `rust/Cargo.toml` uses `../../Deadlock-Bots/rust/crates/dl-central-db`, and `turnier-config` uses `dl-token-secrets`. The Bots checkout must be a sibling of this repository directory, at the revision used for the deploy.

```bash
cd rust
cargo build --release -p turnier-bot
cargo test
cargo clippy --workspace --all-targets -- -D warnings

cd ../frontend
npm ci
npm run build
npm test
```

Host build rules (shared cache, build slots, deploy lock) are in `RUST-BUILDS.md` in the claude-config repo. Use them for every release build.

## Deploy

Sequence used for the 2026-10-11 deploy. Repeat it for each release:

1. Create detached worktrees at `origin/main` for this repo and for Deadlock-Bots, side by side, e.g. `~/.worktrees/turniere-deploy-<date>/{Deadlock-Turniere,Deadlock-Bots}`. The canonical Bots checkout is not usable for this build.
2. In the Turniere worktree build the binary as a deploy build: `CARGO_SLOT_DEPLOY=1 cargo-slot build --release --locked -p turnier-bot`.
3. Install the binary to `~/repos/Deadlock-Turniere/rust/target/release/turnier-bot`, the path the unit runs.
4. Build the frontend in the checkout at `origin/main` (`npm run build` in `frontend/`), so Caddy serves the new `dist`.
5. If the release needs central schema changes, run `dl-central-migrate` from Deadlock-Bots first. The binary does not apply migrations. At start it checks that the required central draft migrations are recorded and refuses to start otherwise.
6. Restart with `systemctl --user restart deadlock-turniere.service`. The unit runs `--check-config` and the deploy preflight (`~/Documents/Admin-Scripts/deploy-preflight.sh`) before it starts.

## Check that it is live

```bash
systemctl --user is-active deadlock-turniere.service
pgrep -a turnier-bot
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8900/api/health
curl -s -o /dev/null -w '%{http_code}\n' https://deutsche-deadlock-community.de/turnier/api/health
curl -s -o /dev/null -w '%{http_code}\n' https://deutsche-deadlock-community.de/turnier/
```

Expected: `active`, one `turnier-bot` process from the path above, and `200` for all three URLs.

## Discord notifications and DM opt-out

- Tournament DMs are sent by `DiscordNotifier` (`rust/crates/turnier-discord`). Before sending, it skips users who have a row in `turnier.tournament_dm_optout`, whatever the scope, and respects `user_profiles.notify_discord_dm` and the per-event flag.
- Users manage the tournament opt-out through `GET /api/me/dm-optout`, `PUT /api/me/dm-optout` with `{"scope": "fun" | "comp" | "all"}`, and `DELETE /api/me/dm-optout/{scope}`.
- The global table `core.dm_optout` in Deadlock-Bots is not read by this repo. A global opt-out does not stop tournament DMs today.

## Guild and language

- Tournament Discord operations use one guild, `discord_guild_id` in `bot.toml`. Scrims have their own `scrim_guild_id`. The code has no per-server configuration, so one deployment serves one Discord server.
- The user interface and bot texts are German only. The code has no English UI or locale switch.

## Repository layout

- `rust/`: backend workspace (see `rust/README.md`)
- `frontend/`: web frontend
- `config/bot.example.toml`: template for the live `bot.toml`
- `ops/systemd/`: example unit and launcher config; the live unit uses the paths above
- `scripts/`: gate helper scripts, and a legacy wrapper `run_turniere_backend_rust.sh` that the live unit does not call
- `docs/`: feature docs (see `docs/README.md`)
- `backend/`: Python legacy

<!-- DDC-LINK-START -->

## Deutsche Deadlock Community

Dieser Bot ist Teil der **[Deutschen Deadlock Community](https://deutsche-deadlock-community.de/)** — organisiert Community-Turniere.

- 🌐 Website: [deutsche-deadlock-community.de](https://deutsche-deadlock-community.de/)
- 💬 Discord: [discord.gg/PhkP3WgY7w](https://discord.gg/PhkP3WgY7w)
- 🎮 [Helden-Übersicht auf Deutsch](https://deutsche-deadlock-community.de/helden/) · [Anfänger-Guide](https://deutsche-deadlock-community.de/guides/anfaenger/)

<!-- DDC-LINK-END -->
