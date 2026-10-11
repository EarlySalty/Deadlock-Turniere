# Solo balance: package B handover

## Integration boundary

- Application: `feat/solo-balance` in `/home/nathanael/.worktrees/turniere-solo-balance`. Implementation was committed in `ddf7f62`; package B adds verified compiler, contract and responsive fixes.
- Companion migration: `feat/turnier-solo-balance-db`, `341159fb188b43aaeec1dde1c8ca7e48b1fe355f`, in `/home/nathanael/.worktrees/turniere-solo-balance-db`. Package A recorded ALLOW, exit 0. The companion checkout was not changed.
- Application review evidence: `/tmp/solo-balance-b-gate.log`. The final report gives the reviewed application HEAD and verdict.
- The integrator must integrate the companion migration, rebuild the central migrator and apply it before deploying the application. This worker does not merge or deploy. Preserve both feature branches and worktrees.

## Fixes and diagnostics

- Used existing crate-root Steam rank helper exports instead of the private `turnier_steam::rank` module in production and integration tests.
- Updated existing `async-trait` from 0.1.89 to 0.1.92. The old macro injected redundant `must_use` attributes rejected by current Clippy. The upgrade adds transitive `syn` 3.0.6; both declare Rust 1.71 compatibility, below the workspace MSRV of 1.85.
- Added infrastructure role `deadlock` as NOLOGIN in the owned disposable cluster so the existing central migration chain can execute. No applied migration or production database was changed.
- Corrected the rank-change contract to rebuild the real router/resolver after direct L2 changes. The real resolver retains L1 values for its configured lifetime; no fake replaced the resolver or database. Technical documentation now states this cache boundary.
- Corrected the random-assignment exclusion contract to reopen registration before invoking that route, assert zero new teams and preserve the waitlisted signup, then restore check-in. The route correctly rejects random assignment during check-in.
- Wrapped admin mode tabs at mobile width and gave assigned-team editors one explicit grid column. Before the fixes, the mobile document was 690 pixels wide and desktop editor controls escaped narrow cards.
- Removed duplicate Draft and Comp-Finder navigation entries. They caused six real React duplicate-key console errors on the admin flow. The final run has zero console errors and one entry per destination in mobile navigation.
- Moli refreshes layout on demand. Scratch drivers wait for DOM attachment and request a screenshot before geometry-dependent actions. They launch no other browser and keep fixture session values in memory.
- Shared sccache progressed and was not restarted or bypassed. Foreign builds, checkouts, services and personal browsers were not touched.

## Rust verification

The full touched-crate run produced 161 passes and two failures, exit 101. Its unaffected targets account for 149 passing tests. After fixing the central contracts and supplying the existing test-mode confirmation environment, the affected target passed 14 tests, zero failed, zero ignored, exit 0. Combined final coverage is 163 Rust tests; the fixture-serving invocation is not counted as a contract test.

```sh
CENTRAL_TEST_DSN=postgres://postgres@127.0.0.1:33139/deadlock_test \
SQLX_OFFLINE=true cargo-slot test --manifest-path rust/Cargo.toml \
  --config 'paths=["/home/nathanael/.worktrees/turniere-solo-balance-db/rust/crates/dl-central-db"]' \
  --release --jobs 3 -p turnier-api -p turnier-engine --features testing \
  --no-fail-fast -- --include-ignored --test-threads=2

CENTRAL_TEST_DSN=postgres://postgres@127.0.0.1:33139/deadlock_test \
DEADLOCK_CENTRAL_DSN=postgres://postgres@127.0.0.1:33139/deadlock_test \
TURNIER_TEST_DB_CONFIRM=throwaway-only SQLX_OFFLINE=true \
cargo-slot test --manifest-path rust/Cargo.toml \
  --config 'paths=["/home/nathanael/.worktrees/turniere-solo-balance-db/rust/crates/dl-central-db"]' \
  --release --jobs 3 -p turnier-api --features testing --test central_routes \
  -- --include-ignored --test-threads=2

SQLX_OFFLINE=true cargo-slot clippy --manifest-path rust/Cargo.toml \
  --config 'paths=["/home/nathanael/.worktrees/turniere-solo-balance-db/rust/crates/dl-central-db"]' \
  --release --jobs 3 -p turnier-api -p turnier-engine --all-targets \
  --features testing -- -D warnings

cargo fmt --manifest-path rust/Cargo.toml --all -- --check
```

Final Clippy and formatting exits: 0. Logs: `/tmp/solo-balance-b-tests-final.log`, `/tmp/solo-balance-b-central-confirmed.log`, `/tmp/solo-balance-b-clippy-final.log`, `/tmp/solo-balance-b-fmt-final.log`. The disposable DSN contains no password and is not a production connection.

## Frontend and browser verification

- Final `npm --prefix frontend test`: 17 passed, zero failed, zero skipped, exit 0.
- Final `npm --prefix frontend run lint` and `npm --prefix frontend run build`: exit 0 each. Build processed 2595 modules in 8.51 seconds. Built artifact `frontend/dist/assets/index-Cl4B8hUy.js` contains `Teams ausgleichen`.
- Logs: `/tmp/solo-balance-b-frontend-tests-final.log`, `/tmp/solo-balance-b-frontend-lint-final.log`, `/tmp/solo-balance-b-frontend-build-final.log`.
- Real Rust API and disposable PostgreSQL fixture at `http://localhost:5179/turnier/admin`, Admin > Live > Teilnehmer. No API response was replaced by a browser fake.
- Preview: 13 solo signups, two teams of six, one waiter, median 12, team sums 72 and 72, gap 0. Apply succeeded. Reload confirmed two six-member teams and persisted `Testspieler 13` on the waitlist. The integration contract also proves preview immutability, stale-input rejection, audit-failure rollback, captains, next-tournament priority, random/check-in exclusion, authorization/status checks and singleton replacement.
- `/tmp/solo-balance-b-ui-final.log`: exit 0. Preview and applied document/client widths equal viewport widths of 1440 and 390. Zero console errors, zero application request failures, zero API failures. Four failed requests to the optional external `https://cloudflareinsights.com/cdn-cgi/rum` endpoint are recorded separately, not suppressed or counted as application failures.
- `/tmp/solo-balance-b-team-editor-final.log`: exit 0. Assigned editor document widths and maximum element right edges equal 1440 and 390. Zero console errors and application failures; two additional Cloudflare RUM failures are recorded.
- Nine final screenshots were opened with Read. Final captures show no remaining visual findings. The mobile preview action is reachable within the 1100-pixel capture; the confirmation and saved waitlist remain readable after scrolling.

TESTNACHWEIS[TW-1]: 180 passed, 0 ignored | Baseline: not measured; no pre-existing failure claim. Rust 149 unaffected plus 14 corrected central tests; frontend 17.
SICHTPRUEFUNG[SP-1]: 9 Aufnahmen | 0 mit Befund | Bilder unter /home/nathanael/.claude/sichtpruefung/solo-balance | angesehen: desktop-button.png, desktop-preview.png, mobile-preview.png, mobile-preview-waitlist.png, desktop-applied.png, mobile-applied.png, mobile-navigation.png, desktop-team-editor.png, mobile-team-editor.png.
TEXTNACHWEIS[DR-1]: Gedankenstriche 0 | ae/oe/ue/ss-Ersatz 0 | Absolutwörter 5 belegt | Senke: unchanged German admin UI and API errors; English technical handover.

## Owned resources and completion procedure

Owned fixture port 18900, Vite port 5179 and Moli port 9337 use synthetic data. Disposable container `turniere-solo-balance-test-b` is labelled owner `turniere-solo-balance-b`, purpose `solo-balance-verification`, repository `Deadlock-Turniere`, expiry 2026-10-12; its loopback PostgreSQL port is 33139 and its data is tmpfs.

Commit and push the verified application branch, then review its HEAD:

```sh
python3 /home/nathanael/Documents/.claude/gpt-workers/gate_hook.py --review \
  --repo /home/nathanael/.worktrees/turniere-solo-balance --base origin/main --head HEAD
```

On BLOCK, use a fresh native fixer in an isolated worktree, at most five rounds. No further orchestrators or T3 threads. Before final handover, gracefully stop the owned API fixture so TestDb teardown runs, stop owned frontend and Moli, verify container ownership and remove that disposable container. Do not clean up either feature branch or worktree. The final report is at most five lines with application SHA, migration SHA, gate verdicts, check counts/exits and screenshot paths.
