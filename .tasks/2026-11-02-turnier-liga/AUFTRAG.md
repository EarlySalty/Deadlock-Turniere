# Round-robin league format in the tournament platform

## Goal
A tournament admin never needs the spreadsheet `Turnierplan_FINAL_Aktiveteams.xlsx` again. Everything it does works directly in the platform, and the admin UI makes it obvious that this is possible. If the UI does not show it, it does not count as done.

## What the spreadsheet does
One table for up to 10 teams, 9 time slots starting at a set time, 50 minutes apart, W or L per slot per team, win +2, loss -1, live ranking. It has no pairings, breaks on ties and on fewer than 10 teams, and counts each W and L twice because COUNTIF ignores case.

## Gaps today
- One group is impossible: `clamp(2, 8)` in `turnier-api/src/admin/brackets.rs` and `.max(2)` in `turnier-engine/src/persist/groups.rs`.
- The group phase must lead to a bracket: `valid_next_statuses` allows only GroupPhase -> Bracket.
- Points are fixed at +3/0: `turnier-match/src/result.rs` and `turnier-api/src/admin/matches.rs`.
- `group_matches.scheduled_at` is displayed but nothing writes it. `round_robin_pairs` has no rounds.

## Product decisions, already made
1. New third `tournament_mode` value `league`, German label "Jeder gegen jeden", next to "Nur Bracket" and "Gruppenphase + Bracket". Existing modes keep their behaviour and +3/0 points exactly.
2. All teams in one table, any team count from 2 up. Everyone plays everyone once.
3. Rounds by the circle method: no team plays twice in a round, a bye for odd team counts. Each match gets `scheduled_at = start + (round - 1) * slot length`. Start comes from the existing group phase start. Slot length defaults to 50 minutes and is editable in the admin UI.
4. Win +2, loss -1 in league mode only.
5. Ranking: points, then wins, then direct comparison for a two-way tie, then a stable order. Negative and zero points must display correctly.
6. No bracket. Status path Checkin -> GroupPhase -> Completed. Final table positions are the placements used by player points.

## Acceptance
Prove all of this against the real runtime with a test-mode tournament (`is_test`), not only with unit tests:
- Admin: choose the format with a short German explanation, set start and slot length, generate the plan, see matches grouped by round with time, enter or correct a winner quickly, finish without a bracket. No bracket wording or buttons in this mode.
- Public tournament page: table with S, N, Pkt and the plan by round with times. No "qualified" highlight on the top 2 and no bracket tab in this mode.
- Result reset or correction restores points exactly, through every write path. Also check the automatic result path, status transitions, the scheduler, reminders, Discord notifications, test mode and archived views.
- Automatic lobbies must not open all matches at once in league mode. Only the current round may get lobbies.
- Automatik presets: offer `league` only if it works end to end, otherwise leave it out and say so.
- Screenshots at 390 and 1440 px, inspected.
- 10 teams give 45 matches in 9 rounds of 5, matching the spreadsheet's 9 slots.

## Boundaries
- No Excel import or export.
- Prefer no migration. Migrations belong to Deadlock-Bots `dl-central-db`; only add one if there is no clean alternative, and say why.
- No code comments. No AI calls. No `.unwrap()` in production paths.
