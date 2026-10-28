# Docs refresh: Deadlock-Turniere (slug bots)

## Checked
- Live: user unit `deadlock-turniere.service` (active; unit file under `~/.config/systemd/user`, drop-ins), process `turnier-bot` from `~/repos/Deadlock-Turniere/rust/target/release/`, config outside git, port 127.0.0.1:8900, `/api/health` locally and via `/turnier/api/health` (200), `/turnier/` (200).
- Caddy `/turnier/*` routes from `~/repos/Caddy/hosts/v50671/Caddyfile`.
- Code on origin/main: crates in `rust/crates`, config keys in `config/bot.example.toml`, route table in `turnier-api`, DM opt-out in `turnier-discord` and `turnier-api/src/account.rs`, startup migration check in `turnier-bot/src/main.rs`.
- Path dependencies on a sibling Deadlock-Bots checkout (`rust/Cargo.toml`, `turnier-config/Cargo.toml`).

## Changed
- Root README rewritten: components, live layout, build, deploy, live check, DM opt-out, guild and language.
- `rust/README.md`: SQLite claims removed, crate table covers all 16 crates.
- `rust/docs/architecture.md`: four missing crates added.
- `docs/README.md`: index corrected.
- Historical header added to dated plans, specs, reports, audits, Python-era internal docs, cutover, known-issues, WORKFLOW.md.
- ADR 0001 marked partly superseded by ADR 0002.

## Open items
- Tournament DMs do not read the global `core.dm_optout` (Deadlock-Bots); only `turnier.tournament_dm_optout`. Product decision, not changed here.
- No multi-server or English support exists in the code. Documented as absent.
- Spot-checked only: `docs/internal/*.html`, `docs/observer-safe-mode.md`, `docs/comp-finder.md`, `docs/solo-balance.md`, `docs/token-storage-db.md`.
- `ops/systemd/deadlock-turniere.service.example` still shows `/opt/deadlock-turniere` paths; it is a template and was not edited.
- `docs/turniere.md` lives in the Deadlock-Bots repo and was not reviewed here.
