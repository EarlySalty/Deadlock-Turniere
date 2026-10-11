# Solo tournament with balanced teams

Role: leaf worker (worker_mittel). Do not start further orchestrators or T3 threads. Native subagents only for the gate fix loop.
Client: main orchestrator (Claude Opus 5.5 session in T3, this thread). Report only at task end or when stuck.

## Goal

Admins can run a tournament where players sign up solo (`team_size > 1`) and, with one click, get teams of equal strength based on Deadlock rank from the Steam bot. Players that do not fit go to a waitlist and get priority in the next solo-balanced tournament.

## User decisions (binding)

1. Trigger: admin clicks a button after registration. No automatic run.
2. Leftover players (count not divisible by `team_size`, or over capacity): waitlist. In the next solo-balanced tournament, previously waitlisted players are picked first (priority over players who played last time). Among equal priority keep signup order.
3. Unranked players: use the median `rank_score` of the ranked players in that tournament.

## Existing building blocks (reuse, do not rebuild)

- Solo signup: `rust/crates/turnier-api/src/public/signups.rs` writes `turnier.tournament_signups` with `rank`, `rank_score`.
- Rank source: `RankResolver::rank_profiles()` batch in `rust/crates/turnier-steam/src/resolver.rs` (cache, Steam bridge, Discord roles). Refresh ranks at balance time with this batch, not the stale signup values.
- Score: `rank_score()` in `rust/crates/turnier-steam/src/rank.rs`. Note `load_rank_input` stores 0 for unknown and `rank_score` returns 3 for tier 0; both mean "unranked" and must be treated by the median rule.
- Manual assignment logic (member insert, captain, `signups.team_id`): `rust/crates/turnier-api/src/admin/teams.rs` around the signup-assign handler. Reuse its helpers for apply.
- Snake order helper: `rust/crates/turnier-engine/src/engine/seeding.rs`.

## Scope

1. Pure balancing function in `turnier-engine` (new module): input players with score and priority, team count, team size; output teams plus waitlist. Algorithm: select participants (priority first, then signup order), sort by score, snake distribution, then pairwise swap improvement until the max-min team sum gap stops shrinking. Deterministic.
2. Admin API: preview endpoint (teams, per-team score sum, gap, waitlist; no writes) and apply endpoint (creates teams and members in one transaction, sets `team_id`, marks waitlisted signups). Admin auth like the existing admin team routes. Refuse apply if teams already exist for the tournament unless they are only 1-man teams irrelevant here; refuse in wrong tournament status.
3. Waitlist persistence: new Postgres migration (never edit applied ones) recording waitlisted players per tournament, and priority lookup for the next solo-balanced tournament. Find where turnier migrations live (`turnier-db::run_migrations` / `dl_central_db`).
4. Admin frontend (`frontend/src/pages/Admin.tsx` area): button "Teams ausgleichen" opening the preview, a confirm button "Teams übernehmen", waitlist list. German UI text with real umlauts. Check visually in the browser with Moli (read `/home/nathanael/Documents/claude-config/wissen/agent-browser.md`).

Out of scope: automatic triggering, changes to normal team signup, Discord announcements.

## Rules

- Rust, no comments, no `.unwrap()` in production paths, small functions.
- Tests only for the balancing function contract (gap, waitlist, priority, median) and the apply transaction if cheap.
- Rust checks via `cargo-slot ... --jobs 3` (see `/home/nathanael/Documents/claude-config/machines/v50671/RUST-BUILDS.md`): fmt, clippy, tests of touched crates. Frontend: lint, build, browser check.
- Commit verified steps, push branch `feat/solo-balance` promptly. No force push, no hook bypass.
- Gate: `gate_hook.py --review` with skill `rolle-merge-schleuse`. On BLOCK start a fresh native fixer subagent per round, rerun until ALLOW, max five rounds, then report blocker.
- Do not merge to main or deploy. Hand over branch + SHA + gate verdict.

## Acceptance

- Preview on a test tournament with e.g. 13 solo signups, `team_size` 6: 2 teams, 1 on waitlist, score gap minimal, unranked players counted as median.
- Apply writes teams/members/team_id; second solo-balanced tournament picks the waitlisted player first.
- Admin page shows button, preview and waitlist (screenshot path in report).

## Final report (max 5 lines)

Branch, SHA, gate verdict, checks run with exit codes, screenshot path, open points.
