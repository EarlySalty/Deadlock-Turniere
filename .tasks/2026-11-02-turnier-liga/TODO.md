# League format: state and how to continue

State on 2026-11-02: stopped by quota during the reading phase. No code changed, nothing committed. Branch `feat/turnier-liga` is still at 99fe588. The contract is `AUFTRAG.md` in this folder.

These are design choices from reading the code. None of them has been built or tested yet.

## Still unread
- Frontend: `TournamentManager.tsx`, `AdminPhaseNav.tsx`, `pages/Admin.tsx`, `pages/Tournament.tsx`, `GroupMatchAdminPanel.tsx`, `CreateTournamentForm.tsx`, `CheckinManager.tsx`, `TestModePanel.tsx`, `ArchivedTournamentView.tsx`, `automatik/PresetForm.tsx`, `automatik/copy.ts`.
- Backend: `turnier-api/src/internal_automatik.rs` and the proposal approval path, `turnier-match/src/lobby.rs` around line 201, the rest of `turnier-engine/src/persist/checkin.rs`, and `run_all_checks` in `turnier-scheduler/src/loop_runner.rs`.
- Check for other consumers that assume a bracket: champion or winner display, Discord texts, `turnier-bot`.

## Design: no migration
- `TournamentMode::League` serialises as `league`. Add it to `mode_str` in `admin/tournaments.rs` and to the frontend type.
- Round is `DENSE_RANK() OVER (ORDER BY scheduled_at)`. It is exposed as an optional `round` on the `GroupMatch` DTO in both `admin/loaders.rs` and `public/helpers.rs`, skipped when null so other modes keep their wire format.
- Slot length lives in the plan itself, derived from the gap between `scheduled_at` values. Default is 50 minutes.
- New endpoint recalculates times and keeps pairings: `start` plus `slot_minutes`, updating `group_phase_start` as well. A PUT that changes `group_phase_start` on a league with a plan must reschedule too.
- Known limit to report: a slot length chosen before the plan exists cannot be stored without a migration. It is passed when the plan is generated. The scheduler path uses 50 minutes and the admin can change it afterwards.
- No `group_phase_start` at generation: use now and write it back.

## Design: engine
New `turnier-engine` league module, pure logic with tests:
- Circle method rounds. 10 teams give 45 matches in 9 rounds of 5, no team twice in a round, bye for odd counts.
- Points constants +2 and -1.
- Ranking: points, wins, direct comparison for a run of exactly two tied teams, then stable order.

Persistence:
- One group, skipping `clamp(2, 8)` in `admin/brackets.rs` and `.max(2)` in `persist/groups.rs`.
- Central helper `record_group_standing_in_tx`: league recomputes wins, losses and points for the group from completed matches, so reversal is exact. Other modes keep the +3/0 increments unchanged.

## Twin paths and mode checks to change
- `turnier-match/src/result.rs:415` and `turnier-api/src/admin/matches.rs:203`: both go through the central helper. In league mode the admin route delegates to `apply_group_match_result` with a correction flag that may overwrite a completed match. It must lock the match row and replace the `match_results` row.
- `admin/steam_ops.rs` `reset_match`: refuses completed matches, never clears `winner_id` and never reverses points. League must allow reset of a completed match, clear winner and results, and recompute.
- Probable existing bug, same function: line 264 reads BIGINT `discord_channel_id` as `Option<String>`. `sqlx` `row.get` should panic when the value is non-null. Confirm and fix, it is on the reset path.
- `persist/checkin.rs:989`: `== "group_stage"` sends league into the bracket branch. Must generate the plan.
- `status.rs` `valid_next_statuses_for_mode`: league GroupPhase -> Completed only.
- `admin/tournaments.rs`: PUT and `advance_tournament` use the mode-blind table. Switch to the mode-aware one. Reject mode changes involving league once in group phase. PUT status must not bypass the points recompute.
- `scheduler/transition.rs`: `get_due_next_status` must never go group_phase -> bracket for league, even with `bracket_start` set. `advance_tournament_status` generates the plan for league and refuses completion while matches are open, with a German message.
- `scheduler/reminders.rs`: league start is `group_phase_start`, already correct through the fallback. `check_and_send_match_reminders` only reads bracket matches, so league has no match reminders. Decide and report.
- `scheduler/loop_runner.rs:148`: check the `bracket_start` value for league routine plans.
- `persist/points.rs`: placements and wins for league come from the final table and group matches, not bracket matches.
- `match/auto_lobby.rs`: league gets lobbies only for the lowest round with open matches. Call it after each league result and from a scheduler tick. Decide whether a round opens only once its `scheduled_at` is due.
- `is_test` turns auto lobbies off, so split candidate selection into a function a DB test can call directly.
- `api/test_mode.rs`: accept mode `league`, reject `advance_to=bracket` for it, simulate only the current round.

## Frontend
- `GroupStandings.tsx`: top 2 highlight off and backend order kept in league mode, negative points displayed correctly.
- `GroupMatchList.tsx`: its client-side `buildRounds` guesses rounds. Use the backend `round` and show times.
- `adminPhase.ts`, `AdminPhaseNav.tsx`, `TournamentManager.tsx:54` ("Zum Bracket"), `pages/Admin.tsx`, `pages/Tournament.tsx` tabs, `TestModePanel.tsx`, `ArchivedTournamentView.tsx:112` (prints the raw mode).
- Format choice "Jeder gegen jeden" with a short German explanation in `CreateTournamentForm.tsx` and `TournamentManager.tsx`.
- Admin plan view: matches by round with time, quick winner entry and correction, slot length, finish button.
- `PresetForm.tsx`: offer league only if a routine tournament is proven end to end. Otherwise leave it out and say so.

## Tests and proof still owed
- Unit: round generator, ranking ties. DB through `test-db-run`: league points with reset and correction, status path, lobby selection.
- Runtime: local instance with an `is_test` tournament. Moli screenshots at 390 and 1440 px, inspected.
- Checks: `cargo-slot` on affected crates, `npm run build`, lint and relevant tests in `frontend/`.

## Continue
Start from "Still unread", then build engine, write paths, status and scheduler, API, frontend, and proof in that order. Commit verified states and push `feat/turnier-liga`. The final report format is in the original briefing: at most 12 lines with SHA, commands and exit codes, screenshot paths, whether presets offer league, whether a migration was needed, and all open points.

## Orchestrator state, 2026-11-02 12:54
Claude quota is out on all accounts, so the build worker stopped before writing code. This file is uncommitted. Nothing is merged or live.

Next steps on resume:
1. Commit this file on `feat/turnier-liga` and send the branch to origin.
2. Start a fresh worker (model from `~/.t3/userdata/pyramide.json`, Opus at effort medium) with `AUFTRAG.md` and this file. Do not resume the stopped worker.
3. Give it these decisions on the two open points. A league round's lobbies open only once its `scheduled_at` is due and only for the lowest round with open matches. League match reminders get built if the existing reminder query extends cleanly, otherwise they go in the final report as an open point.
4. After the worker: independent critic against `AUFTRAG.md`, `gate-review --repo <worktree> --base origin/main`, merge, deploy per README, live check with an `is_test` tournament, remove branch and worktrees.
