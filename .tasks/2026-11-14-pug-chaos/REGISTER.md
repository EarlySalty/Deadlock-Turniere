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

See section after the Moli run.

## H. Wave 2

P3b solo-only built; rest in TODO.md.

## Commits

| Finding | SHA | Summary |
|---|---|---|
