# Edge values per changed input

Server rules live in: result.rs apply_bracket_match_result (force), steam_ops::reset_match, phases.rs finalize-checkin, teams.rs delete_team, tournaments.rs advance.

## B1 result correction / reset of a completed bracket match
- Force with the same winner: server no-op for downstream; UI asks no overwrite confirm.
- Force with a new winner while the next match is pending, already played, or the final is played: downstream reset recursive, then new winner advances.
- Double elimination: winners match whose loser already played in the losers bracket; grand final 1 corrected while the reset match is pending or cancelled.
- Reset of a completed match: own winner, results and played_at cleared, downstream slots cleared, teams of the match itself stay.
- Match without both teams: server refuses, UI keeps the form hidden.
- Group matches: server refuses terminal group matches even with force, so the override is offered for bracket matches only.
- Null winner_id on a completed match (forfeit/cancelled): no overwrite confirm with an old winner name.

## B2/M1/M8 finalize modal
- No teams, all teams incomplete, every incomplete team ticked, none ticked.
- deleted_team_ids empty, deleted team that is also full (6/6 nobody checked in).
- Team delete refused by the server (locked team or wrong status): error shown inside the modal.
- After a delete the snapshot token is stale: preview is re-run.

## M2 check-in
- One pending check-in: only that row is busy.

## M4 finish
- Bracket with only pending matches, final played, double-elim reset match cancelled or pending.
- Finished tournament leaves the live filter: stays selected.

## M5/M6/M10 public page
- team_size 0 or members > team_size, recruitment open/application/closed.
- checkin_start null.
- Completed without bracket matches or with no final winner: no winner banner.
