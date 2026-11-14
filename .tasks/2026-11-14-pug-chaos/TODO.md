# TODO: wave 2 items not built in this package

State checked 2026-11-14. Source rows: `.tasks/2026-11-02-turnier-ui-pug/REGISTER.md` and `git show origin/wip/turnier-ui-pug-todo:.tasks/2026-11-02-turnier-ui-pug/TODO.md`.

| Item | State | Next step |
|---|---|---|
| P3b Steam link and minimum rank | Not built. Solo-only (the PUG cause) is built in this package. | Separate package: optional Steam link requirement and minimum rank on signup, both enforced server side in `public/signups.rs` and shown in the settings. |
| P12 auto mode by team count | Not built. `admin/tournaments.rs` still passes `team_size` (players per team) into `determine_tournament_mode`, which expects a team count. | Pass the real team count or drop "Automatisch"; add a guard test. |
| P11 registration reminder vs check-in, scheduler test harness | Not built. Scheduler DB tests hard-code a local PG socket and fail at setup on ionos. | Move scheduler tests to `test_pool()` and the ionos runner, then fix the reminder ordering. |
| P10 timezone | Never started: worktree `~/.worktrees/turniere-p10` and branch `feat/timezone-fix` no longer exist. Admin form sends naive local time, DB stores it as UTC. | Fresh package from the P10 register row; then one-off time correction for tournament 16/17 with the user's go. |
| P1b home cards and P4b admin cleanup | Branch `feat/wave1-cleanup` at 6382527 in `~/.worktrees/turniere-cleanup`, 5 uncommitted files (Admin.tsx, Home.tsx, adminTournaments.ts), does not build. This package already made the builder the place for removing and adding signups (part of P4b). | Read that diff, rebase on the new main, remove the legacy "Teams & Teilnehmer" and "Offene Solo-Anmeldungen" sections. |
| P6 Steam bot flag | Not started. | Separate package. |
| P7 lobby automation test | Not started, needs the user. | Schedule with the user. |
| Captain draft | Out of scope, user is being asked. | Wait for the user's decision. |
