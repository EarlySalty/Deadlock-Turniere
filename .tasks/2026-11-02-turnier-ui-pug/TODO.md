# TODO (written 2026-11-03 ~03:10 CET, Claude quota exhausted)

State: REGISTER.md in this folder is current up to commit 148057d on main. Wave 1 is fully live (Liga, P2, P3a, P9, P4, P1; last deploy 02:49 CET, main 27cfcdf). DM to minillii sent 02:52 CET. Orchestrator thread: T3 69ad09ac (Fable 5.1).

## Running or interrupted workers (all native Opus subagents of thread 69ad09ac)
- CLEANUP (P1b + P4b), branch feat/wave1-cleanup, worktree /home/nathanael/.worktrees/turniere-cleanup/Deadlock-Turniere (+ Bots sibling). Stopped on quota, nothing committed, build currently broken (Admin.tsx passes `builderActive` to ParticipantManager which does not accept it yet). Its own TODO: /home/nathanael/.worktrees/turniere-cleanup/Deadlock-Turniere/.tasks/2026-11-03-wave1-cleanup/TODO.md. Do NOT resume the cold agent; start a fresh worker with the original brief (REGISTER rows P1b/P4b) plus that TODO as state.
- P10 timezone, branch feat/timezone-fix, worktree /home/nathanael/.worktrees/turniere-p10/Deadlock-Turniere (+ Bots sibling). State unknown at quota stop; check `git -C <wt> status` and log before restarting with a fresh worker.
- P8 Deadlock-Bots onboarding lookup, branch feat/onboarding-role-lookup, worktree /home/nathanael/.worktrees/bots-p8/Deadlock-Bots. State unknown; same procedure.

## Next steps for the orchestrator on resume
1. Commit and push this TODO.md (docs commit, `git push origin HEAD:main` from /home/nathanael/repos/Deadlock-Turniere).
2. For each of the three worktrees: inspect state, then spawn a fresh coder (Opus medium, or per pyramide.json fixer roles gpt-6.1-sol / grok-4.6 if Claude quota is still short) with the original brief and the worktree state. Briefs are in the transcript; the essentials are in REGISTER.md rows.
3. Integrate on ALLOW: push HEAD:main from the worktree, deploy (clean worktree pair at main, `CARGO_SLOT_DEPLOY=1 cargo-slot build --release --locked -p turnier-bot`, `--check-config`, `npm run build` in canonical frontend, install binary to rust/target/release/turnier-bot, `systemctl --user restart deadlock-turniere.service`, health 200 on 127.0.0.1:8900/api/health, public 200), clean up.
4. Queued packages: P12 auto mode by team count (REGISTER row), P11 registration reminder + scheduler test harness, P3b solo-only + Steam link + min rank, P6 Steam bot flag. P7 lobby automation test needs the user.
5. Bots sibling worktrees get rustfmt noise from `cargo fmt --all` in Turniere; discard with `git checkout -- .` before `worktree remove`.

## Open user decisions
- Scrims onboarding variant A (role directly) or B (`/scrim-signup` only).
- Merge "Funny Custom Ping" into "Custom Games" or leave.
- Go for explainer channel (discord-rollen-erklaerung.*) and onboarding apply (discord-onboarding-vorschlag.json).
- One-off time correction for tournament #16 and league rows after P10 reports.

## Side notes
- Dependabot count on the repo rose from 8 to 13 during wave 1 (lockfile change, probably P2 `npm ci`); check after wave 2.
- Backups of replaced binaries: /tmp/turnier-bot.before-{liga,batch2,batch3,batch4}-20261103.
