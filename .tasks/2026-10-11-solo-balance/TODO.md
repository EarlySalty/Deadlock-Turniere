# Solo balance: blocked verification handover

## State

- Application worktree: `/home/nathanael/.worktrees/turniere-solo-balance`, branch `feat/solo-balance`.
- Application local and pushed HEAD: `55efe682f06c2565bb3b509481f800657b6a21dc`. Feature implementation remains uncommitted. Do not treat this SHA as containing the feature.
- Companion worktree: `/home/nathanael/.worktrees/turniere-solo-balance-db`, branch `feat/turnier-solo-balance-db`, pushed migration commit `341159fb`.
- Migration local review: ALLOW, exit 0. Application gate has not run.
- No merge, deployment, production database write or service restart.

## Blocker observed on 2026-10-11

Shared sccache compiler clients stalled. There were no active `rustc` processes; cache counters stayed unchanged across a three-minute measurement after clients had already waited about six minutes. The fixture build stopped progressing at 64 log lines. Only the owned fixture build was cancelled, allowing the queued full suite a fresh attempt. That suite advanced to three crate announcements but stalled again without an active compiler or completed test run.

The shared server was not restarted because foreign builds were using it and host rules prohibit interrupting them. This is an observed compiler/cache stall, not a proven application compiler error. Do not disable the shared cache or remove artifacts as a workaround.

The originally queued clippy and owned Moli tasks reached their 30-minute background limits and were killed. Neither produced verification evidence. Only owned test/build/frontend/browser-wait processes were stopped. The owned labelled tmpfs test container was removed after checking its identity; foreign Cargo processes, services, browser sessions and checkouts were left alone.

## Completed checks

- `cargo-slot test --manifest-path rust/Cargo.toml --release --jobs 3 -p turnier-engine --lib`: exit 0, 35 passed, 0 failed, 0 ignored. Five are new balancing contract tests. Log: `/tmp/solo-balance-engine-test.log`.
- `/home/nathanael/.cargo/bin/cargo fmt --manifest-path rust/Cargo.toml --check --package turnier-api --package turnier-engine`: exit 0. Log: `/tmp/solo-balance-fmt-final.log`.
- `npm --prefix frontend run lint`: exit 0 on final source. Log: `/tmp/solo-balance-frontend-lint-final.log`.
- `npm --prefix frontend run build`: exit 0 on final source. Log: `/tmp/solo-balance-frontend-build-final.log`.
- `npm --prefix frontend test`: exit 0, 17 passed, 0 failed, 0 skipped. Log: `/tmp/solo-balance-frontend-tests.log`.
- Additive migration syntax/basic references executed in a reversible transaction on disposable Postgres, then rolled back successfully. The full central migration chain has not executed for this feature verification.
- Design detector: empty findings. `/tmp/solo-balance-ui-detector.json`.
- Application API/DB tests: interrupted during compilation, no test results.
- Clippy: timed out while queued, no lint result.
- Browser: no screenshots, no visual or functional verification. An initial scratch-script import failed because this host installs `playwright-core`, not `playwright`; the import is corrected. The corrected script was cancelled while waiting for the fixture.

TESTNACHWEIS[TW-1]: 52 passed, 0 ignored | Baseline: not measured; no pre-existing failure claim.
SICHTPRUEFUNG[SP-1]: 0 Aufnahmen | 0 mit Befund | Bilder unter /home/nathanael/.claude/sichtpruefung/solo-balance | angesehen: none; verification pending.

## Implemented, still awaiting integration/runtime verification

- Pure deterministic selection, median, snake and gap-reducing swaps in `turnier-engine/src/balance.rs`.
- Admin preview/apply/waitlist routes with refreshed ranks, stale-preview rejection, one transaction and eligible singleton replacement.
- Persistent waitlist history separated by test/live category.
- Guards in existing random assignment and check-in finalization prevent waitlist refill.
- German admin panel under Admin > Live > Teilnehmer.
- Actual API integration contract covers 13 signups, median/gap, preview immutability, changed-rank rejection, forced audit rollback, assignment/captains, next-tournament priority, random/check-in exclusion, authorization/status rejection, multi-player-team rejection and singleton replacement.
- `docs/solo-balance.md` records behavior and the central migration deployment prerequisite.
- Cargo.lock gained four transitive packages because the existing central DB dependency uses chrono-tz. This diff was inspected and retained; review with the clean dependency before committing.

## Resume steps

1. Check that the shared compiler cache is progressing again, using `sccache --show-stats`. Do not restart it while foreign builds are running.
2. Recreate an owned disposable Timescale test database. Use tmpfs, loopback-only dynamically allocated port, trust authentication and ownership/expiry labels. Example:

```sh
docker run -d --name turniere-solo-balance-test \
  --label purpose=solo-balance-verification --label repository=Deadlock-Turniere \
  --label source=55efe682f06c2565bb3b509481f800657b6a21dc \
  --label owner=turniere-solo-balance --label expires=2026-10-12 \
  --tmpfs /var/lib/postgresql/data --publish 127.0.0.1::5432 \
  -e POSTGRES_HOST_AUTH_METHOD=trust -e POSTGRES_DB=deadlock_test \
  -e TIMESCALEDB_TELEMETRY=off timescale/timescaledb:2.17.2-pg16 \
  -c shared_buffers=128MB -c max_connections=40 -c max_worker_processes=16 \
  -c max_parallel_workers=4 -c max_parallel_workers_per_gather=2
```

Check `pg_isready`. Inspect the newly allocated port, not the previous port. Set `CENTRAL_TEST_DSN` in memory to the credential-free `postgres://postgres@127.0.0.1:PORT/deadlock_test` with that port. Never use production secrets or a production DSN.

3. Run the touched suites with the clean central migration dependency and bounded test concurrency:

```sh
SQLX_OFFLINE=true cargo-slot test --manifest-path rust/Cargo.toml \
  --config 'paths=["/home/nathanael/.worktrees/turniere-solo-balance-db/rust/crates/dl-central-db"]' \
  --release --jobs 3 -p turnier-api -p turnier-engine --features testing \
  --no-fail-fast -- --include-ignored --test-threads=2

cargo-slot clippy --manifest-path rust/Cargo.toml \
  --config 'paths=["/home/nathanael/.worktrees/turniere-solo-balance-db/rust/crates/dl-central-db"]' \
  --release --jobs 3 -p turnier-api -p turnier-engine --all-targets \
  --features testing -- -D warnings
```

Fix actual failures and rerun only affected checks. Recheck formatting after Rust edits. Use a background timeout long enough for the shared build queue.

4. Start the actual API fixture with `SOLO_BALANCE_BROWSER_PORT=18900`, the same disposable `CENTRAL_TEST_DSN`, the same Cargo path override and `SQLX_OFFLINE=true`:

```sh
cargo-slot test --manifest-path rust/Cargo.toml \
  --config 'paths=["/home/nathanael/.worktrees/turniere-solo-balance-db/rust/crates/dl-central-db"]' \
  --release --jobs 3 -p turnier-api --features testing --test central_routes \
  solo_balance_preview_apply_rollback_and_next_tournament_priority \
  -- --include-ignored --nocapture
```

This fixture-serving mode returns before contract assertions. Do not count it as the normal integration test passing. It exposes only synthetic data and an in-memory test-login cookie on loopback. Never print or save the cookie/token.

5. Recheck owned ports, then start only Moli and the scratch frontend proxy:

```sh
/home/nathanael/.local/bin/moli serve --host 127.0.0.1 --port 9337 --layout --image --font
node /tmp/solo-balance-vite.mjs
node /tmp/solo-balance-ui-check.mjs
```

The frontend proxy runs on 5179 and targets owned fixture port 18900. It preserves the localhost Host header required by the actual backend guard. Port 8900 belongs to the running tournament service; do not reuse or stop it. The browser client connects to owned Moli via CDP using installed `playwright-core`; it does not launch another browser.

The scratch script checks desktop 1440 and mobile 390 widths, previews two teams and one waiter, applies teams, checks persisted waitlist and collects console/network/overflow diagnostics. It saves images outside git in `/home/nathanael/.claude/sichtpruefung/solo-balance/`. Open both desktop and mobile screenshots with Read. Fix real findings in one batch, then confirm once. No Brave or alternative browser fallback.

6. Record real counts, exit codes, acceptance evidence and actual opened image paths in the task document. Stage only owned files, commit verified application changes and push `HEAD:feat/solo-balance`. Run:

```sh
python3 /home/nathanael/Documents/.claude/gpt-workers/gate_hook.py --review \
  --repo /home/nathanael/.worktrees/turniere-solo-balance --base origin/main --head HEAD
```

On BLOCK use a fresh native fixer in an isolated worktree each round, maximum five rounds. No additional orchestrators/T3 threads. Preserve both feature branches for the integrator. Do not merge or deploy.

7. Gracefully stop the owned fixture so TestDb teardown runs. Stop owned frontend/Moli, verify labels and remove only the owned disposable database. Final handover is at most five lines: both branch prerequisites, application SHA, gate verdicts, checks/counts/exits, actual screenshot path and remaining integration prerequisite.
