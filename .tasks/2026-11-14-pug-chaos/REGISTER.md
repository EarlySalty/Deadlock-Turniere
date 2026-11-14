# Register: PUG tournament #16 chaos

Evidence sources: `turnier.audit_log` (tournament 16), Caddy access logs `ddc.access-2026-11-14*` (earliest line 16:34:29 UTC), service journal, Discord #pug-chat message 1571211196634243204, broker probes on 127.0.0.1:8770. All times UTC. Gate round 1 classes K1 to K13 are listed in `GATE-R1-TRIAGE.md` and referenced below.

## A. Random teams

- Mechanism (proven from audit log and code): audit id 241 at 17:16:45.881, action `finalize_checkin` by the organiser account, details `added_players=30`, `created_teams=[JohnnyBlazed Team, 1Sven1 Team, DonRadu Team, Yeezy Team, geansefeuschen Team]`, `advanced_to_bracket=true`, `matches_created=5`. Caddy shows three preview POSTs to `finalize-checkin` (17:16:31, :35, :44) and the confirming POST at :45. `persist/checkin.rs::finalize_checkin` chunks the checked-in solo pool by check-in order into teams of `team_size`, makes the first player of each chunk captain and names the team after him, then generates the bracket. The audit log of #16 holds zero team builder or admin team actions (only create, auto_advance x2, update x3, checkin x31, finalize x1), so the builder was never saved.
- Why possible: auto-formation of teams on finalize is unconditional; the preview modal "Check-in Vorschau" shows only the counts "Entfernte Spieler" and "Hinzugefügte Spieler", never which teams get created with which members; unsaved team builder drafts live in module memory and nothing warns when the admin switches to the check-in phase and finalizes; there is no PUG setting that forbids automatic team formation.
- Why unnoticed: all earlier tournaments had players building their own teams before check-in, so the solo pool at finalize was small; the preview counts looked plausible ("30 hinzugefügt"); the builder hint "Erst Speichern überträgt alles" is shown only inside the builder.
- Twins: `finalize_checkin` is called by the admin route and the test-mode advance. The scheduler moved every tournament from check-in to bracket or groups at the planned time without finalize, preview or draft guard (K4); audit history shows 6 of 6 real tournaments left check-in through `finalize_checkin`, so the scheduler no longer ends check-in in any mode. With zero or one stored team the bracket builders refuse and the transaction rolls back.
- "Admin view differed from public view": not reproduced from logs. Most likely the unsaved builder draft still shown in the admin while the public page showed the stored auto-teams.
- Fix: `invite_mode = solo_only` turns off team formation and the fill loop; the preview lists every team with members, the unsaved-draft alert blocks the start (drafts of other tabs in the same browser count via localStorage, a second browser is covered by the member listing in the preview).

## B. Team entry unusable

- Mechanism (proven from DB): captains and players picked in Discord never signed up, and the builder can only place signups; duplicate signups from the guest-by-name path (a8478c2): Gembo (Discord plus guest), Trippymelo (Discord plus guest, guest in team 66), XerXes (two guests), because `auth.rs::guest_login` allocated a new negative id per login and nothing checked the name; 12 guests and 14 of 32 Discord signups without rank; no solo-only mode, so the public page offered "Team gründen" during a PUG.
- Why possible: guest ids were added as negative values into fields every reader treated as Discord snowflakes (K1), the name rule existed twice with different limits (K6), and only one writer checked names.
- Why unnoticed: test tournaments were filtered out of the live admin list and the test generator only creates full teams, so the PUG flow was never rehearsed.
- Twins: every signup writer (solo signup, team create and join, invitation accept, admin add by name, admin add to team) and every Discord-bound reader of participant ids (notifier mentions and DMs, voice moves, rank cache and lookup, DM opt-out, avatar upload, OAuth session parse).
- Fix: one `participant_name` rule (1 to 32 characters, no control or invisible format characters) in Rust and `PARTICIPANT_NAME_MAX_LENGTH` in the UI; one `allocate_guest_participant_id`; a guest name that matches any signup or team member of the tournament is refused under the tournament row lock in every writer; a Discord signup with a guest's name is allowed (Discord display names are not identities, refusing would let guests block names) and both entries carry "Name doppelt" and "Gast" marks in the builder and check-in table; one `is_guest_participant` rule in turnier-core and every Discord-bound reader skips guests; OAuth sessions parse strictly again; guest logins limited per connection (IPv4 address or IPv6 /64) and hour.

## C. "Austragen" silently fails during check-in

- Mechanism (proven): Caddy shows `DELETE /api/tournaments/16/signup` answered 400 at 16:53:09 and 16:57:24. `public/signups.rs::cancel_solo_signup` rejected with "Anmeldung ist nicht geöffnet" when `status != "registration"`, while the frontend (`isSignupOpen`) offers the button during check-in too. `Tournament.tsx` rendered errors only for the solo signup and guest login.
- Why possible: server and client derived "signup open" separately; there was no shared error rendering for mutations.
- Why unnoticed: no withdraw happened during check-in in earlier tournaments; a 400 without UI feedback leaves no trace except the access log.
- Twins: withdraw left the check-in row behind (also admin `delete_signup`); every other public mutation lacked error rendering; status checks ran before the tournament row lock in every mutating handler (K2).
- Fix: one registration-window rule on both sides (`registration_window_open`, `isRegistrationWindowOpen`); withdraw removes the check-in row; the page shows the error of the most recent action; every status-gated mutator locks the tournament row inside its transaction and checks status and mode on the locked row.

## D. Admin cannot remove a signup

- Mechanism: `admin/teams.rs::delete_signup` existed with audit `signup_delete_admin`, but only for unassigned signups, its button lived in "Offene Solo-Anmeldungen" below the balance panel, not reachable from the builder pool or the check-in phase, and it left the check-in row behind.
- Why possible / unnoticed: P4 moved the participant UI into the builder and kept the old card only in the participants phase.
- Twins: the check-in table had no remove or check-in action; admin check-in and self check-in used different eligibility rules (K13).
- Fix: X per pool player in the builder (disabled while a removal runs), admin check-in route plus button, one `ensure_checkin_signup` eligibility rule for admin and self check-in, add-by-name only while registration or check-in is open (the form is hidden otherwise).

## E. Numbers, labels and the double-elimination fallback

- Mechanism: "0 Teams gemeldet" counted teams only; "15 / 33 bereit", later "29 / 43", were true snapshots while signups stayed open during check-in; `persist/double_elim.rs` fell back to a single-elimination bracket for counts below 4 or not a power of two and overwrote the stored `bracket_format`.
- Why possible: the format rule lived in several places (engine, UI copy "4, 8, 16 oder 32"), the label read the configured field, and count labels were built without a plural helper.
- Why unnoticed: earlier tournaments had 4 or 8 teams; the overwrite looked like the setting.
- Twins: tournament 17 (group stage, 2 qualifiers per group) hits the fallback with 3 groups; plural labels in overview, test mode, balance, groups, league plan and builder (K8); the preview snapshot token ignored `invite_mode` and `bracket_format` (K5).
- Fix: the configured format stays stored; the engine serves the played format (`PLAYED_BRACKET_FORMAT_SQL` from the generated matches) to public and admin reads; the preview warns before generation; the snapshot token covers both fields; one `DOUBLE_ELIMINATION_RULE` copy; `pluralize` at every count label; "1 Team, 6 Spieler solo".
- Decision: before generation the public label shows the configured format, because the number of bracket teams is not fixed yet; the organiser sees the fallback warning in the preview.

## F. Caster pool "Discord-Broker Fehler"

- Mechanism (proven): Turniere called `POST http://127.0.0.1:8770/internal/master/v1/discord/role/members`, the broker answered 404 with an empty body, `broker.rs::error_detail` turned that into "Discord-Broker Fehler". The broker route is `GET /internal/master/v1/discord/role-members?guild_id=..&role_id=..` (Deadlock-Bots `dl-broker/src/handlers.rs::role_members`); the probe answered 200 with members.
- Why possible: path literals were spread over notifier, internal scrims and the bot binary with no single source.
- Why unnoticed: the error carried neither HTTP status nor path, three failure branches were not logged (K10), and the caster pool is opened rarely.
- Twins: all other broker paths answer 401 or 422 on an unauthenticated probe, so they exist; the OAuth client had the same silent default detail.
- Fix: all broker paths in `turnier_discord::paths`, every failure branch logs method, path and status, the default detail carries the HTTP status, same for the OAuth client. The cross-repo contract has no shared test; that cause stays outside this repo.

## G. UI pass

- Mechanism: Moli pass on a throwaway stack (test-db-run `dlcentral_test_turniere`, turnier-bot on 127.0.0.1:18916, Vite on 5191) at 390 and 1440 px found: check-in table sorted by an inverted flag; builder kept the module draft after a save, so a remount restored stale edits; builder grid forced horizontal scroll at 390 px and the add-player field shrank to about 85 px at 1440 px; the matches header always promised a loser bracket; an empty "Gruppen" tab for bracket-only tournaments; the Leitstand headline said "Aktion erforderlich" with nothing to do; ASCII umlauts in user-facing errors and admin texts (K12).
- Why possible: these views were built for team tournaments and never opened with a PUG fixture or at phone width.
- Why unnoticed: no browser pass with a solo pool existed; the frontend tests cover helpers, not layouts.
- Twins: the same draft, label and umlaut patterns in the other admin panels; fixed together (K8, K9, K12).
- Proven in the browser: solo_only finalize forms no teams; preview shows the unsaved-draft alert with disabled start, the fallback alert and the remaining solos; after save the start works; builder X removes a signup; admin "Einchecken" works; guest login 10 times 201, then 429; scrollWidth equals the viewport on every page at 390 px. Screenshots in `/home/nathanael/.claude/sichtpruefung/pug-chaos/`.
- Proven after gate round 1 (Moli, fixture with a guest "kaltfront " next to the Discord signup "Kaltfront", 1440 and 390 px, no console or API errors): check-in table and builder show "Gast" and "Name doppelt" on both entries (chips made non-wrapping after the 390 px look); "Test" badge on the test tournament in the list and the overview rows; add-by-name form shown in check-in; a team created in tab A (stored draft count 1) makes the preview in tab B say "Im Team-Builder ist 1 Änderung noch nicht gespeichert, davon 1 in einem anderen Tab dieses Browsers."; scrollWidth equals the viewport (`r2-*` screenshots). The PresetForm save error and the hidden add form outside registration and check-in are covered by code review only.
- Taste, not changed: header brand truncates at 390 px; legacy "Teams & Teilnehmer" and "Offene Solo-Anmeldungen" below the builder (P4b, TODO.md); overall admin density.

## H. Wave 2

P3b solo-only built (`auto_team_formation` off, player team actions answer 403 with a German hint, public page hides "Team gründen", `InviteMode::SoloOnly` everywhere, team size 1 creates no team in solo_only). Rest of wave 2 in TODO.md.

## Tournament 16 and 17

- 16 (not modified): the five teams should match the Discord picks in message 1571211196634243204. Correction left to the organisers.
- 17: with 3 groups (6 qualifiers) it hits the fallback; the preview warns and the public label follows the played bracket once generated.

## Open points

- The broker contract between Turniere and Deadlock-Bots has no shared test or startup probe (outside this repo, `OFFENE-URSACHEN.md`).
- Live admin browser pass on the production host after deploy.

## Commits

| Finding | SHA | Summary |
|---|---|---|
| F | 1dcf35d | Caster pool via broker GET `role-members`, broker errors log method, path and status |
| A, B, C, D, E, H | f7e7899 | solo_only mode, team preview with members, unsaved-draft guard, name guard, admin add and remove, withdraw in check-in, error rendering, labels |
| A, B, C, D, H tests | 3ac4461 | PUG contract test, migration count 39 to 41 |
| G | 5a84931 | UI pass fixes |
| A, B, C, D, E (review) | 43e71e8 | admin check-in, solo_only fill loop, guest login limit, lock before status check, count labels |
| Config test | 987525d | `guest_logins_per_hour` as defaulted key |
| K1 to K13 | ad2ae59 | gate round 1 classes |
| K5, K2, K11 self-review | 3a9021a | played format and token test, update_tournament row lock, shared window rule, records |

## Test evidence

- rustfmt `--check` on every changed file and `cargo-slot clippy --workspace --all-targets --features turnier-api/testing,turnier-db/testing -- -D warnings`: clean.
- `test-db-run dlcentral_test_turniere -- cargo-slot test --workspace --features testing --no-fail-fast` on ionos after the K1 to K13 fixes and again on the final tree (3a9021a plus records): 633 passed, 0 failed; the PUG contract test now also covers the played format and the snapshot token.
- Frontend: `npm run lint` 0, `npm test` 31/31, `npx tsc -b` 0, `npm run build` 0.
Commit messages of 1dcf35d, f7e7899, 5a84931 and 43e71e8 were reworded (layer 4 named Twins, claims matched to HEAD), and the K1 to K13 message was corrected on the add-by-name window; every rewritten commit has the tree of its original (checked per commit, plus git range-diff).
