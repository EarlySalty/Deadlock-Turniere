# Register: PUG tournament #16 chaos

Evidence sources: `turnier.audit_log` (tournament 16), Caddy access logs `ddc.access-2026-11-14*` (earliest line 16:34:29 UTC), service journal, Discord #pug-chat message 1571211196634243204, broker probes on 127.0.0.1:8770. All times UTC.

## A. Random teams

- Mechanism (proven from audit log and code): audit id 241 at 17:16:45.881, action `finalize_checkin` by the organiser account, details `added_players=30`, `created_teams=[JohnnyBlazed Team, 1Sven1 Team, DonRadu Team, Yeezy Team, geansefeuschen Team]`, `advanced_to_bracket=true`, `matches_created=5`. Caddy shows three preview POSTs to `finalize-checkin` (17:16:31, :35, :44) and the confirming POST at :45. `persist/checkin.rs::finalize_checkin` chunks the checked-in solo pool by check-in order into teams of `team_size`, makes the first player of each chunk captain and names the team after him, then generates the bracket. The audit log of #16 holds zero team builder or admin team actions (only create, auto_advance x2, update x3, checkin x31, finalize x1), so the builder was never saved.
- Why possible: auto-formation of teams on finalize is unconditional; the preview modal "Check-in Vorschau" shows only the counts "Entfernte Spieler" and "Hinzugefügte Spieler", never which teams get created with which members; unsaved team builder drafts live in module memory and nothing warns when the admin switches to the check-in phase and finalizes; there is no PUG setting that forbids automatic team formation.
- Why unnoticed: all earlier tournaments had players building their own teams before check-in, so the solo pool at finalize was small; the preview counts looked plausible ("30 hinzugefügt"); the builder hint "Erst Speichern überträgt alles" is shown only inside the builder.
- Where else: `finalize_checkin` is called by the admin route and by the test-mode advance only. The scheduler checkin to bracket transition (`turnier-scheduler/src/transition.rs::advance_tournament_status`) seeds the bracket from stored teams and never forms teams, so it drops an unsaved solo pool instead of shuffling it. SoloBalancePanel and assign-random assign solos too but need an explicit admin action. With zero or one stored team the bracket builders refuse (`entries.len() < 2` validation), the transaction rolls back and the tournament stays in check-in.
- "Admin view differed from public view": not reproduced from logs. Most likely explanation is the unsaved builder draft still shown in the admin while the public page showed the stored auto-teams; verified on is_test below.
- Fix: see commit table.

## B. Team entry unusable

- Contributors (proven from DB):
  - Captains and players picked in Discord never signed up (several names from message 1571211196634243204 have no signup), and the builder can only place signups.
  - Duplicate signups from the guest-by-name path (a8478c2): Gembo (Discord signup plus guest), Trippymelo (Discord plus guest, guest in team 66), XerXes (two guests). `auth.rs::guest_login` allocates a new negative id per login and nothing checks the name inside the tournament.
  - 12 guests without rank, 14 of 32 Discord signups without rank, so the builder and balance show "Ohne Rang" for most.
  - Test tournaments are filtered out of the live admin list (`Admin.tsx`, `!t.is_test`), and the test generator only creates full teams, so the PUG flow could not be rehearsed.
  - No solo-only mode, so the public page offered "Team gründen" during a PUG.
- Why unnoticed: the PUG flow (solo pool, builder, finalize) was never rehearsed end to end; the test tooling did not support it.
- Where else: same duplicate risk for every guest signup in tournament 17.
- Fixed: a guest name that matches an existing signup in the tournament is refused (`ensure_participant_name_free`, under the tournament row lock), guest logins are limited per IP and hour (`limits.guest_logins_per_hour`, default 10, 89504ec). Still open: a Discord signup after a guest with the same name is not refused, because the Discord display name is not a reliable identity; the organiser sees both in the builder and can remove one.

## C. "Austragen" silently fails during check-in

- Mechanism (proven): Caddy shows `DELETE /api/tournaments/16/signup` answered 400 at 16:53:09 and 16:57:24. `public/signups.rs::cancel_solo_signup` rejects with "Anmeldung ist nicht geöffnet" when `status != "registration"`, while the frontend (`isSignupOpen`) offers the button during check-in too. `Tournament.tsx` renders errors only for `signupSoloMutation` and `guestLogin`; withdraw, leave, check-in, join, apply and invite errors are never shown.
- Why possible: server and client derive "signup open" separately; there is no shared error rendering for mutations.
- Why unnoticed: no withdraw happened during check-in in earlier tournaments; a 400 without UI feedback leaves no trace except the access log.
- Where else: withdraw also leaves the check-in row behind (twin in admin `delete_signup`), every other public mutation lacks error rendering.

## D. Admin cannot remove a signup

- Mechanism: `admin/teams.rs::delete_signup` exists with audit `signup_delete_admin`, but only for unassigned signups, its button lives in "Offene Solo-Anmeldungen" below the balance panel and is not reachable from the builder pool or the check-in phase; it leaves the check-in row behind.
- Why possible / unnoticed: P4 moved the participant UI into the builder and kept the old card only in the participants phase.
- Where else: check-in table has no remove action.

## E. Numbers and labels

- "0 Teams gemeldet": label counts teams only; a PUG has none until finalize.
- "15 / 33 bereit", later "29 / 43": true snapshots (signups at 16:17:30 were 33 with 15 check-ins, at 16:30 43 with 29), because signups stayed open during check-in. Not a counting bug; the label hides that signups are still open.
- "Double Elim.": `persist/double_elim.rs` falls back to `build_seeded_bracket` for team counts below 4 or not a power of two, logs a WARN and leaves `bracket_format=double_elimination` stored; Home and Tournament label from the stored field. Admin never sees the fallback before generation.
- Tournament 17 (group_stage, double_elimination, 4 teams so far, 19 signups): the bracket takes the top 2 per group, so any group count giving a qualifier count that is not a power of two (for example 3 groups, 6 qualifiers) hits the same fallback.

## F. Caster pool "Discord-Broker Fehler"

- Mechanism (proven): Turniere calls `POST http://127.0.0.1:8770/internal/master/v1/discord/role/members`, the broker answers 404 with an empty body, `broker.rs::error_detail` turns that into "Discord-Broker Fehler". The broker route is `GET /internal/master/v1/discord/role-members?guild_id=..&role_id=..` (Deadlock-Bots `dl-broker/src/handlers.rs::role_members`); the probe answered 200 with members.
- Why possible: path and method constants are duplicated across repos with no shared contract or startup probe.
- Why unnoticed: the error carries neither HTTP status nor path, the broker does not log 404, and the caster pool is opened rarely.
- Where else: all other Turniere broker paths answer 401 or 422 on an unauthenticated probe, so they exist; only ROLE_MEMBERS drifted.

## G. UI pass

Moli pass on a throwaway stack (test-db-run `dlcentral_test_turniere`, turnier-bot on 127.0.0.1:18916, Vite on 5191) at 390 and 1440 px: public registration, check-in and bracket of a PUG test tournament, admin list, builder, check-in, preview, start and bracket. Screenshots in `/home/nathanael/.claude/sichtpruefung/pug-chaos/`.

Fixed (d78a7e3):
- Check-in table sorted checked-in players above missing ones (inverted flag).
- Builder kept the module draft after a successful save, so a remount restored stale edits and blocked the preview; a failed save now keeps only the unsaved rest.
- Builder grid forced horizontal scroll at 390 px (fixed 17rem column); add-player field shrank to about 85 px at 1440 px (flex basis 0).
- Matches header always promised a loser bracket; now follows `effectiveBracketFormat`.
- Public page showed an empty "Gruppen" tab for bracket-only tournaments. The results and standings tabs keep their status gate (a d78a7e3 change to them was reverted in 89504ec).

Proven in the browser: solo_only finalize forms no teams; preview shows the unsaved-draft alert with disabled start, the single-elimination fallback alert and the remaining solos; after save the start works (stored `single_elimination`, 1 match); public shows "SINGLE ELIM." and "2 TEAMS GEMELDET"; builder X removes a signup (DELETE 200, notice "Brummbär wurde aus dem Turnier entfernt.", 8 to 6 signups over two removals); scrollWidth equals the viewport on every page at 390 px.

Taste, not changed:
- Header brand "Deutsche Deadlock Community" truncates at 390 px.
- Legacy duplicates "Teams & Teilnehmer" and "Offene Solo-Anmeldungen" below the builder (P4b, see TODO.md).
- ASCII umlauts in Match-Events and Presets ("ausloesen", "fuer", "ZURUECKSETZEN").
- Overall admin density in the participants phase.

## Independent review (one critic, Opus) and fixes in 89504ec

- Players added by name during registration could never be checked in (no admin check-in route), so finalize removed them. New `POST /api/admin/tournaments/{id}/checkins/{discord_id}` (audit `checkin_admin`) and a "Einchecken" button per missing row in the check-in table.
- solo_only still filled incomplete saved teams from the solo pool; the fill loop now shares the `auto_team_formation` flag with team creation.
- Twin: the scheduler moved a solo_only tournament from check-in to the bracket at the planned time without finalize or preview; `get_due_next_status` now leaves solo_only check-in to the organisers.
- Guest login had no rate limit (see B).
- Withdraw, admin add and admin delete checked the status before taking the tournament row lock; one `lock_tournament_status` helper now orders lock before check everywhere.
- Public label showed "1 Teams" and hid the open solo players; `registrationSummary` and `pluralize` now give "1 Team, 6 Spieler solo".
- Removed added doc comments and a `.expect`, restored detached doc comments, real umlauts in "Ungültige Discord-ID" and "Ungültige Actor-ID".

Browser proof on the fixture (Moli, fresh load per width): public label "1 Team, 6 Spieler solo" at 1440 and 390 (`public-pug-checkin-label-*`); admin "Einchecken" sends `POST .../checkins/910000000000050304`, notice "Feuerqualle ist eingecheckt.", missing buttons 5 to 4, eingecheckt 3 to 4 (`admin-pug-checkin-buttons-1440`, `admin-pug-checkin-done-{1440,390}`, scrollWidth 390 at 390); guest login: 10 times 201, then 429 with the German message, another IP 201.

## Open points

- A Discord signup after a same-named guest still creates a duplicate (see B).
- The stored fallback format stays when the bracket is reset and rebuilt with more teams; the preview snapshot token ignores `invite_mode` and `bracket_format`.
- Before generation the public label shows the configured "Double Elim." even when the fallback will apply.
- Manual "Gruppen neu generieren" uses 4 groups, the preview and finalize use the automatic group count, so only that manual path can differ from the warning.
- Stale action errors stay visible until the next action; negative participant ids are accepted on all admin routes (guests); the leave-team message in check-in is generic.
- ASCII umlauts in other crates and older UI texts (see G taste list).
- Live admin browser pass on the production host after deploy.

## H. Wave 2

P3b solo-only built in 392872f (`auto_team_formation` off, player team actions answer 403 with a German hint, public page hides "Team gründen"). Rest of wave 2 in TODO.md.

## Tournament 16 and 17

- 16 (not modified): stored `bracket_format` should be `single_elimination`, and the five teams should match the Discord picks in message 1571211196634243204. Correction left to the organisers.
- 17: with 3 groups (6 qualifiers) it hits the fallback; the preview now warns and the stored format follows the real bracket.

## Commits

| Finding | SHA | Summary |
|---|---|---|
| F | 4c87a61 | Caster pool via broker GET `role-members`, broker errors log method, path and status |
| A, B, C, D, E, H | 392872f | solo_only mode, team preview with members, unsaved-draft guard, name guard, admin add and remove, withdraw in check-in, error rendering, stored fallback format and labels |
| A, B, C, D, H tests | db74df4 | PUG contract test, migration count 39 to 41 |
| G | d78a7e3 | UI pass fixes |
| A, B, C, D, E (review) | 89504ec | admin check-in, solo_only fill loop and scheduler twin, guest login limit, lock before status check, count labels |

## Test evidence

- `cargo fmt --check` per crate and `cargo-slot clippy --workspace --all-targets -- -D warnings`: clean.
- `test-db-run dlcentral_test_turniere -- cargo-slot test --workspace --features testing --no-fail-fast` on ionos: 630 passed, 1 failed (`turnier-db --test migration`, stale count 39 since 2026-10-11, red on base too); after db74df4 the migration test passes 4/4. `pug_finalize_formation_withdraw_and_admin_signup_contract` ok.
- Frontend: `npm run lint` 0, `npm test` 31/31, `npm run build` 0, `tsc` 0.
