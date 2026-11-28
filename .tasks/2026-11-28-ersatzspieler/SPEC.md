# Bench (substitute) players per team

## Goal

A tournament can allow up to N bench players ("Ersatzspieler") per team in addition to the
`team_size` starters. Off by default; off behaves exactly like today.

Acceptance: with bench on (limit 1, team_size 6 and 8), a player joins a team as bench, the
captain moves members between starters and bench within the limits, and the check-in
start preview counts only starters against `team_size`; bench members never make a team
incomplete or full for starters.

Out of scope: auto-promotion of bench players, bench-aware draft or hero assignment, points
rules for bench players (bench members keep receiving the same participation points as any
member), production migration or deploy.

## Data model

| Column | Type | Default | Meaning |
|---|---|---|---|
| `turnier.tournaments.bench_slots` | `BIGINT NOT NULL CHECK 0..=5` | `0` | max bench players per team, `0` = bench off |
| `turnier.team_members.is_bench` | `BOOLEAN NOT NULL` | `false` | member is on the bench |
| `turnier.team_applications.as_bench` | `BOOLEAN NOT NULL` | `false` | applicant asked for a bench seat |

The migration lives in Deadlock-Bots `dl-central-db/migrations` (additive, defaults keep
all existing rows at bench off). "Enabled" in the UI is `bench_slots > 0`; there is no
separate flag, so "enabled with limit 0" cannot exist.

## Edge values (gate-learning step 0)

| Value | Writer | Readers and decision |
|---|---|---|
| `bench_slots` missing on create | Rust default `0` | DB stores 0, public header shows only "6 gegen 6", join offers no bench |
| `bench_slots = 0` (off, or limit 0) | create/update | every bench request is rejected with 400 "In diesem Turnier gibt es keine Ersatzplätze"; plain placement never overflows to bench |
| `bench_slots < 0` or `> 5` | create/update | 400 from `check_bench_slots`, DB CHECK as second line |
| `bench_slots` missing on update | Rust `Option` | column unchanged |
| lowering `bench_slots` below the bench count of any team (incl. to 0) | update | 409, the limit is not changed; admin moves or removes bench players first |
| bench limit reached | join/apply/accept/invite/admin add/move/lineup | explicit bench request: 400 "Alle Ersatzplätze sind belegt"; automatic placement with starters full: 400 "Team ist bereits voll" |
| starters full, bench free | plain public join (starter wanted) | 400 "Alle Stammplätze sind belegt. Tritt als Ersatzspieler bei." (never silently placed on the bench) |
| starters full, bench free | invite accept, application accept without wish, admin add/assign/move, team builder | placed on the bench (someone else already decided the player joins) |
| bench requested, starters free | join/apply as bench | placed on the bench (the wish wins) |
| `team_size` other than 6 (1, 8, 20) | all counts | starters compared with the stored `team_size`, never a literal 6 |
| bench player leaves / is kicked | leave/kick | deleted like any member, no promotion, starters unchanged |
| starter leaves while bench has players | leave/kick | no automatic promotion; captain or admin moves a bench player |
| captain moves the last non-captain starter to bench | lineup | allowed if bench has room; team then has 1 starter and is incomplete |
| captain to bench (self or by admin) | lineup | 400 "Der Captain bleibt Stammspieler" |
| change captain to a bench member | admin captain change, team builder | starters free: the new captain becomes a starter; starters full: the new captain swaps seats with the old captain, so the captain is always a starter |
| swap a bench player with a starter | lineup with `swap_member_id` (captain) or `swap_discord_id` (admin) | counts unchanged, works on a full team; 400 "Zum Tauschen braucht es einen Stammspieler und einen Ersatzspieler" for two players on the same seat, 400 "Der Captain bleibt Stammspieler" if the captain would go to the bench, 400 "In diesem Turnier gibt es keine Ersatzplätze" with `bench_slots = 0` |
| lineup in a `solo_only` tournament | captain | buttons hidden (backend answers 403); admin path stays |
| lineup move to starter with starters full | lineup | 400 "Alle Stammplätze sind belegt" |
| lineup move to the current seat | lineup | no-op, returns the team |
| lineup move outside the roster window | lineup | captain: only during registration (same gate as kick); admin: same statuses as other admin team edits |
| `is_bench` absent in old frontend | public/admin DTO | always serialized; TypeScript type marks it required |
| `as_bench` absent in apply body | apply | defaults to `false` |
| application `as_bench = true` but bench turned off before accept | accept | 400 "In diesem Turnier gibt es keine Ersatzplätze" |

## Completeness (one pure function, `turnier_core::bench`)

- `starters` = members with `is_bench = false`; complete iff `starters >= team_size`.
- Recruitment ("Team voll", "Plätze frei", "Beitreten"): starters against `team_size`;
  bench free shows separately.
- Check-in finalize:
  - removal of members without check-in: starters only; bench members stay.
  - fill from the solo pool: until starters reach `team_size`; pool players become starters.
  - `ready`: checked-in starters (keep rosters) or kept starters (fill mode).
  - delete rule (fill mode): a team is deleted only when it has no kept starters and none of its members checked in; its remaining bench members are then listed and removed like every other removed player. A team whose checked-in bench player is the only one left stays and is filled from the pool.
  - snapshot token includes `is_bench`, so a lineup move invalidates a stale preview.
  - preview roster shows the bench flag.
- Admin workflow "incomplete teams": starters against `team_size`.
- Group seeding average rank: starters only.
- Match participants (lobby, hero modes): all members, starters first.
- Reminders, voice moves, Discord roles, points, leaderboard history: all members.

## Readers of `turnier.team_members`

| File | Counts | Decision |
|---|---|---|
| admin/helpers.rs `count_team_members` | all | stays all (leave/dissolve logic); new `team_lineup` gives starters and bench |
| admin/helpers.rs `ensure_team_has_capacity` | all vs team_size | replaced by seat placement |
| public/teams.rs join/leave/kick | all | join uses placement; leave/kick unchanged |
| public/invitations.rs invite/accept/apply/accept application | all vs team_size | placement (Auto, or Bench for bench applications) |
| admin/teams.rs accept/add/assign/move/captain | all vs team_size | placement Auto; captain change rejects bench |
| engine/persist/checkin.rs | all | starters only (see above) |
| engine/persist/groups.rs seeding | all | starters only |
| match/modes.rs, match/repo.rs participants | all | all, starters first |
| engine/persist/points.rs | all | unchanged |
| scheduler/reminders.rs, admin/voice.rs, operations.rs | all | unchanged |
| admin/tournament_row.rs `player_count` | all | unchanged (players, not starters) |
| solo_balance.rs | singleton check | unchanged |
| leaderboard.rs, automatik/routine.rs, signups.rs, public/tournaments.rs | membership | unchanged |
| public/helpers.rs and admin/loaders.rs member loaders | DTO | add `is_bench`, order starters first |

## Endpoints

- `POST /api/tournaments/{t}/teams/{team}/join` body optional `{ "as_bench": bool }`.
- `POST /api/tournaments/{t}/teams/{team}/apply` body adds `as_bench`; application DTO shows it.
- New `PATCH /api/tournaments/{t}/teams/{team}/lineup` `{ "member_id": i64, "is_bench": bool, "swap_member_id"?: i64 }` (captain or mod; public member DTOs carry no discord_id, so the member row id addresses the seat).
- Join and apply bodies are parsed from raw bytes: an empty body (the current frontend sends JSON content type without a body) means `as_bench = false`, malformed JSON is 400.
- New `PATCH /api/admin/tournaments/{t}/teams/{team}/members/{discord_id}/lineup` `{ "is_bench": bool, "swap_discord_id"?: string }` (with a swap partner both seats flip, `is_bench` is ignored).
- Tournament create/update accept `bench_slots`; tournament and public detail DTOs return it.
- Member DTOs (`TeamMember`, `TeamMemberPublic`) return `is_bench`.

## UI text

- Header: "6 gegen 6 · bis zu 1 Ersatz" (real values; only when `bench_slots > 0`).
- Team card: "4 / 6 Stammspieler", bench members with label "Ersatz", "1 Ersatzplatz frei" / "2 Ersatzplätze frei".
- Buttons: "Als Ersatz beitreten", "Als Ersatz bewerben", "Auf die Ersatzbank", "In die Startaufstellung".
- Admin form: "Ersatzspieler pro Team" (0 = aus).

## Readers in SQL and TypeScript (edge values)

| Field | Reader | null / 0 / over-limit behaviour |
|---|---|---|
| `bench_slots` | SQL: CHECK `tournaments_bench_slots_range` (0..=5), `NOT NULL DEFAULT 0` | null impossible; out of range rejected by the DB after the Rust 400 |
| `bench_slots` | SQL: `TOURNAMENT_GUARD_SELECT`, admin tournament row, public detail | always selected, never null |
| `bench_slots` | TS `types/tournament.ts` (required number), `editForm.ts`/`createFormValues.ts` (`?? 0` for old cached payloads) | 0 hides every bench control and the "bis zu M Ersatz" suffix |
| `bench_slots` | TS `lib/lineup.ts` `lineupOf`, `teamFormatLabel` | 0 gives `benchFree = 0`, label "N gegen N"; negative bench count clamps to 0 |
| `bench_slots` | TS `ParticipantManager` capacity `team_size + bench_slots`, `TeamBuilderPanel` size | capacity is the total roster; the backend still places starters first and rejects over-limit |
| `is_bench` | SQL: `team_lineup` `COUNT(*) FILTER (WHERE NOT is_bench)`, check-in finalize, group seeding, member loaders `ORDER BY is_bench` | `NOT NULL DEFAULT false`, old rows are starters |
| `is_bench` | TS `TeamMember`/`TeamMemberPublic`/start plan entries (required boolean), `lib/lineup.ts` `starterCount` | absent (old payload) counts as starter |
| `is_bench` | TS public team card: starters first, then an "Ersatzbank" section; captain toggle only during registration with `bench_slots > 0` and not for the captain | |
| `as_bench` | SQL: application insert/select, accept reads it for `SeatRequest::Bench` | `NOT NULL DEFAULT false` |
| `as_bench` | TS application DTO, join/apply request body | omitted body means `false` |
