# Register: PUG tournament #16 chaos

Evidence sources: `turnier.audit_log` (tournament 16), Caddy access logs `ddc.access-2026-11-14*` (earliest line 16:34:29 UTC), service journal, Discord #pug-chat message 1571211196634243204, broker probes on 127.0.0.1:8770. All times UTC. Gate round 1 classes K1 to K13 are listed in `GATE-R1-TRIAGE.md` and referenced below.

## A. Random teams

- Mechanism (proven from audit log and code): audit id 241 at 17:16:45.881, action `finalize_checkin` by the organiser account, details `added_players=30`, `created_teams=[JohnnyBlazed Team, 1Sven1 Team, DonRadu Team, Yeezy Team, geansefeuschen Team]`, `advanced_to_bracket=true`, `matches_created=5`. Caddy shows three preview POSTs to `finalize-checkin` (17:16:31, :35, :44) and the confirming POST at :45. `persist/checkin.rs::finalize_checkin` chunks the checked-in solo pool by check-in order into teams of `team_size`, makes the first player of each chunk captain and names the team after him, then generates the bracket. The audit log of #16 holds zero team builder or admin team actions (only create, auto_advance x2, update x3, checkin x31, finalize x1), so the builder was never saved.
- Why possible: auto-formation of teams on finalize is unconditional; the preview modal "Check-in Vorschau" shows only the counts "Entfernte Spieler" and "Hinzugefügte Spieler", never which teams get created with which members; unsaved team builder drafts live in module memory and nothing warns when the admin switches to the check-in phase and finalizes; there is no PUG setting that forbids automatic team formation.
- Why unnoticed: all earlier tournaments had players building their own teams before check-in, so the solo pool at finalize was small; the preview counts looked plausible ("30 hinzugefügt"); the builder hint "Erst Speichern überträgt alles" is shown only inside the builder.
- Twins: `finalize_checkin` is called by the admin route and the test-mode advance. The scheduler moved every tournament from check-in to bracket or groups at the planned time without finalize, preview or draft guard (K4); audit history shows 6 of 6 real tournaments left check-in through `finalize_checkin`, so the scheduler no longer ends check-in in any mode. With zero or one stored team the bracket builders refuse and the transaction rolls back.
- "Admin view differed from public view": not reproduced from logs. Most likely the unsaved builder draft still shown in the admin while the public page showed the stored auto-teams.
- Fix: `invite_mode = solo_only` turns off team formation and the fill loop; the preview lists the teams with their members; the unsaved-draft alert blocks the start. Drafts of other tabs in the same browser count through a localStorage key per tab that the tab refreshes every 20 s and on `pageshow`; a key older than 3 minutes (three background timer floors of 60 s) counts as a closed tab and is removed, so a crashed tab blocks the start for at most 3 minutes. A second browser is covered by the member listing in the preview. Finalize plus auto lobby planning is one function (`finalize_checkin_and_plan_lobbies`) for the finalize route and test-mode confirm.

## B. Team entry unusable

- Mechanism (proven from DB): captains and players picked in Discord never signed up, and the builder can only place signups; duplicate signups from the guest-by-name path (a8478c2): Gembo (Discord plus guest), Trippymelo (Discord plus guest, guest in team 66), XerXes (two guests), because `auth.rs::guest_login` allocated a new negative id per login and nothing checked the name; 12 guests and 14 of 32 Discord signups without rank; no solo-only mode, so the public page offered "Team gründen" during a PUG.
- Why possible: guest ids were added as negative values into fields that the readers in K1 treated as Discord snowflakes, the name rule existed twice with different limits (K6), and only one writer checked names.
- Why unnoticed: test tournaments were filtered out of the live admin list and the test generator only creates full teams, so the PUG flow was never rehearsed.
- Twins: the signup writers solo signup, team create and join, invitation accept, application accept, admin add by name, admin add to team and solo balance apply; the Discord-bound readers of participant ids notifier mentions and DMs, voice moves, rank cache and lookup, DM opt-out, avatar upload, profile and avatar lookups by name, player points, the cross-tournament waitlist and the OAuth session parse.
- Fix: one `participant_name` rule in Rust (1 to 32 characters counted as `char`, no control or invisible format characters), no client copy: the UI sends the name and shows the server's 400 text. One `allocate_guest_participant_id`. Name equality is `participant_name_key` (Unicode trim plus the engine `name_key`), used by `ensure_participant_name_free` under the tournament row lock (reached through `ensure_guest_name_free` from the listed writers when the participant is a guest, and directly from admin add by name) and by `duplicate_name_ids`. A Discord signup with a guest's name is allowed (Discord display names are not identities, refusing would let guests block names). The admin tournament detail serves `guest_ids` and `duplicate_name_ids`; the builder and check-in table render "Gast" and "Name doppelt" from them. One `is_guest_participant` rule in turnier-core, and the listed readers skip guests. OAuth sessions parse strictly again; guest logins are limited per connection (IPv4 address or IPv6 /64) and hour by `rate_limit::record_hourly`, the same helper as draft and comp lobby creation, each with its own counter.

## C. "Austragen" silently fails during check-in

- Mechanism (proven): Caddy shows `DELETE /api/tournaments/16/signup` answered 400 at 16:53:09 and 16:57:24. `public/signups.rs::cancel_solo_signup` rejected with "Anmeldung ist nicht geöffnet" when `status != "registration"`, while the frontend (`isSignupOpen`) offers the button during check-in too. `Tournament.tsx` rendered errors only for the solo signup and guest login.
- Why possible: server and client derived "signup open" separately; there was no shared error rendering for mutations.
- Why unnoticed: no withdraw happened during check-in in earlier tournaments; a 400 without UI feedback leaves no trace except the access log.
- Twins: withdraw left the check-in row behind (also admin `delete_signup`); the other public mutations lacked error rendering; status checks ran before the tournament row lock in the mutating handlers (K2).
- Fix: one registration-window rule on both sides (`registration_window_open`, `isRegistrationWindowOpen`); withdraw removes the check-in row; `latestActionError` shows the error of the most recent action on the public page and in CheckinManager, ParticipantManager, TournamentManager, SoloBalancePanel and Leitstand. In turnier-api the status-gated mutators in public signups, teams and invitations, admin teams, solo balance, phases, brackets, league reschedule and `update_tournament` lock the row through `lock_tournament_row` or `lock_tournament_or_404` and check status and mode on the locked row. Named exceptions: `advance_tournament` reads status and mode without a lock and relies on `advance_tournament_status`, which locks the row itself and moves only from the expected status; the engine (`finalize_checkin`, `assign_random_teams`) and the scheduler transition take their own `FOR UPDATE` lock because they cannot depend on turnier-api.

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
- Fix: the configured format stays stored; `tournament_select` serves the played format (`PLAYED_BRACKET_FORMAT_SQL` from the generated matches) to the public and admin tournament readers; the preview warns before generation and serves `bracket_startable` and `min_bracket_entrants` (`MIN_BRACKET_ENTRANTS`, checked by `ensure_bracket_entrants` in the bracket builders); `engine::groups::group_count` serves group generation and the preview; the snapshot token covers both fields; the rule text `DOUBLE_ELIMINATION_RULE` lives in the engine and reaches the UI through the preview. Count labels use `pluralize` and `pluralWord` from `lib/countLabel.ts` in overview, test mode, balance, groups, league plan, builder, check-in, auto lobby, match events and the comp finder; "1 Team, 6 Spieler solo". `team_size` is checked to 1 to 20 by `check_team_size` on tournament create and update, presets and the approved automatik preset.
- Decision: before generation the public label shows the configured format, because the number of bracket teams is not fixed yet; the organiser sees the fallback warning in the preview.

## F. Caster pool "Discord-Broker Fehler"

- Mechanism (proven): Turniere called `POST http://127.0.0.1:8770/internal/master/v1/discord/role/members`, the broker answered 404 with an empty body, `broker.rs::error_detail` turned that into "Discord-Broker Fehler". The broker route is `GET /internal/master/v1/discord/role-members?guild_id=..&role_id=..` (Deadlock-Bots `dl-broker/src/handlers.rs::role_members`); the probe answered 200 with members.
- Why possible: path literals were spread over notifier, internal scrims and the bot binary with no single source.
- Why unnoticed: the error carried neither HTTP status nor path, three failure branches were not logged (K10), and the caster pool is opened rarely.
- Twins: all other broker paths answer 401 or 422 on an unauthenticated probe, so they exist; the OAuth client had the same silent default detail.
- Fix: the broker and OAuth broker paths live in `turnier_discord::paths`; the failure branches of `BrokerClient` and `OAuthClient` (transport, status, body, decode, missing configuration) log method, path and status; `broker::http_error_detail` is the one detail rule for both and carries the HTTP status; the strict caster route logs before its 503. The cross-repo contract has no shared test; that cause stays outside this repo.

## G. UI pass

- Mechanism: Moli pass on a throwaway stack (test-db-run `dlcentral_test_turniere`, turnier-bot on 127.0.0.1:18916, Vite on 5191) at 390 and 1440 px found: check-in table sorted by an inverted flag; builder kept the module draft after a save, so a remount restored stale edits; builder grid forced horizontal scroll at 390 px and the add-player field shrank to about 85 px at 1440 px; the matches header always promised a loser bracket; an empty "Gruppen" tab for bracket-only tournaments; the Leitstand headline said "Aktion erforderlich" with nothing to do; ASCII umlauts in user-facing errors and admin texts (K12).
- Why possible: these views were built for team tournaments and never opened with a PUG fixture or at phone width.
- Why unnoticed: no browser pass with a solo pool existed; the frontend tests cover helpers, not layouts.
- Twins: the same draft, label and umlaut patterns in the other admin panels; fixed together (K8, K9, K12).
- Proven in the browser: solo_only finalize forms no teams; preview shows the unsaved-draft alert with disabled start, the fallback alert and the remaining solos; after save the start works; builder X removes a signup; admin "Einchecken" works; guest login 10 times 201, then 429; scrollWidth equals the viewport on every page at 390 px. Screenshots in `/home/nathanael/.claude/sichtpruefung/pug-chaos/`.
- Proven after gate round 1 (Moli, fixture with a guest "kaltfront " next to the Discord signup "Kaltfront", 1440 and 390 px, no console or API errors): check-in table and builder show "Gast" and "Name doppelt" on both entries (chips made non-wrapping after the 390 px look); "Test" badge on the test tournament in the list and the overview rows; add-by-name form shown in check-in; a team created in tab A (stored draft count 1) makes the preview in tab B say "Im Team-Builder ist 1 Änderung noch nicht gespeichert, davon 1 in einem anderen Tab dieses Browsers."; scrollWidth equals the viewport (`r2-*` screenshots). The PresetForm save error and the hidden add form outside registration and check-in are covered by code review only.
- Proven after gate round 2 (Moli, same fixture, 1440 and 390 px, no console or API errors, scrollWidth equals the viewport, `r3-*` screenshots): "Gast" and "Name doppelt" now come from the server lists `guest_ids` and `duplicate_name_ids` and show on both entries in the check-in table and the builder, the chips sit next to the name and are not cut at 390 px; a draft key of another tab saved 10 minutes ago is removed on the preview and raises no alert, while a draft created in a live tab still makes the preview say "Im Team-Builder ist 1 Änderung noch nicht gespeichert, davon 1 in einem anderen Tab dieses Browsers."; the finalize preview of the 31-solo PUG fixture says "Für das Bracket fehlen Teams. Nötig sind mindestens 2, gerade sind es 0."; test mode offers no "Runde simulieren" for the test tournament in registration, and with the PUG box the label reads "Anzahl Teams (ergibt 24 Solo-Spieler)"; the Automatik tab and the setup phase render. The voice panel needs a generated bracket, which the fixture has not; it and the draft pick are covered by code review only.
- Taste, not changed: header brand truncates at 390 px; legacy "Teams & Teilnehmer" and "Offene Solo-Anmeldungen" below the builder (P4b, TODO.md); overall admin density.

## H. Wave 2

P3b solo-only built (`auto_team_formation` off, player team actions answer 403 with a German hint, public page hides "Team gründen", invitation accept and the recruiting badge, the admin hides the recruiting toggle, `InviteMode::SoloOnly` decoded as in GATE-R1-TRIAGE K3, team size 1 creates no team in solo_only). Rest of wave 2 in TODO.md.

## I. Gate round 3 pre-review

Round 3 on 58ad385 was lost to the reviewer's 5 h usage limit after about two hours (no verdict, not counted). Its five BLOCK reasons were treated as a pre-review, ledger lines appended (classes a to g), every class fixed with its twins:

- a, client reads fields the server never sends: auto lobby renders the server's `scheduled`/`skipped` report ("Keine Lobby erstellt" with the reason), voice move sends `target` (sammelpunkt, team1, team2) and the server maps it to the configured channel, so no 19-digit snowflake passes through `parseInt` (7d58e07).
- b, mutations without rendered rejection: every `useMutation` caller renders its error through `latestActionError` (24cca4a).
- c, snowflakes and guessed failure texts: Discord ids stay strings end to end; channel and guild ids are validated > 0 at config load (`turnier-config/src/file.rs:438-473`); the voice panel shows the server reason (7d58e07).
- d, client copies of server rules and option lists: bracket, series, mode, game mode and match objective options come from `components/admin/settings/options.ts`; the match objective is a select that keeps an unknown stored code (57710d3); registration window, solo balance and replacement players come from the server (`registration_open`, `replacements_open`, `GET .../solo-balance/availability` with the server's own refusal text) (cfc941a and the status commit).
- e, locks and row writers per crate: one tournament row lock and one writer per team, member and signup row in the lowest shared crate (0872f5e); one team name rule in the engine, solo teams named "Name (2)" on collision (0201953).
- f, two decoders and enum columns as strings: `tournament_from_row` is the one decoder; `bracket_format`, `played_bracket_format` and the tournament status decode into their enums (cfc941a). The status commit types `TournamentStatus` and `TournamentMode` through scheduler, engine, automatik, match and api: `DueStatusRow`, `advance_tournament_status`, `ensure_not_leaving_checkin`, `schedule_lobbies_best_effort`, `entered_status`, start gap, reminders, snapshot token settings, admin update, open/revert check-in, advance, league plan, consent and test-mode status lists (`TournamentStatus::PLANNED`, `PARTICIPATING`), local `status_str`/`mode_str`/`parse_status` copies removed.
- g, test mode with own rule copies: test mode calls the production rules and writers and serves its limits (2d1c7aa).

Narrowed claims, rest in `claude-config/orchestrierung/OFFENE-URSACHEN.md` with file:line: tournament status SQL text literals (fix belongs at the column as a CHECK constraint, new migration), and match, lobby, application and recruitment statuses still held as `String`.

Behaviour changes to know:

- Solo teams with a taken name get "Name (2)" instead of failing.
- Row timestamps come from the database `now()`.
- Test mode accepts `mode` `group_stage`, `bracket_only`, `league` (the old `group_then_bracket` is refused; no client sent it) and `advance_to` as enums; a wrong value gets axum's JSON rejection like every other route instead of a German 422 text. Simulate is refused outside group phase and bracket.
- The match objective in PresetForm is a select.
- `GET /api/admin/voice/channel-members/{channel_id}` is removed (no client caller since 7d58e07), the unused `DiscordNotifier::get_voice_channel_members` and its test are gone, both audit tables mark the route removed.
- The solo balance panel shows the server's refusal text ("Teams können nur nach der Anmeldung während des Check-ins ausgeglichen werden.", "Es existieren bereits Teams. ...") instead of a client guess.
- Name fallbacks go through `teamNameOr`/`teamLabel` and `playerName`.

Suite count 633 to 623 explained (test-round5.log against suite-r3.log): 12 OAuth and broker error detail tests collapsed into 2 `http_error_detail` tests (1ad2be7), `as_str_round_trips_through_parse` went with the InviteMode rework (dea04fb), `bracket_only_checkin_...` and `paralleler_group_phase_uebergang...` replaced by `checkin_endet_nur_ueber_finalize` and `paralleler_check_in_abschluss...` (a719ae7), one harness test renamed (dea04fb), `duplicate_name_ids_trim_unicode...` added.

Sichtprüfung cause layer: this repo has no visual check target in `claude-config/tools/sichtpruefung/ziele.json`, so the bridge only adds a note and nobody saw the UI bugs before tournament 16. `ziele.json` left unchanged; next package in OFFENE-URSACHEN, based on the Moli fixture of this branch (test-only API binary with every external URL on 127.0.0.1:9, vite proxy to it, Moli driver creating is_test tournaments).

Browser proofs on a55d848 (Moli, test-only API binary on the ionos throwaway DB, production bundle via `vite preview` and the dev server, is_test tournaments only; screenshots in `~/.claude/sichtpruefung/pug-chaos/r3b-*-1440.png`, `r3c-voice-1440.png`):

- Auto lobby: "Keine Lobby erstellt: Auto-Lobby ist für dieses Turnier aus oder es ist ein Testturnier." rendered under the button (1440 and 390).
- Voice move: request body `{"discord_id":"1300000000000000001","target":"sammelpunkt"}`, the 19-digit id comes back unchanged in the response, the panel renders "0 Spieler verschoben, 1 fehlgeschlagen: Discord-Master-Broker ist nicht authentifiziert" (1440 and 390).
- Draft: start and first pick work, "Test User 398307 Team bannt" shown (1440 and 390).
- PresetForm: team size 25 gets 400 `Teamgröße muss zwischen 1 und 20 liegen`, rendered above "Speichern" (1440 and 390); the selects carry the server option lists.
- Add-player form: hidden in bracket ("Spieler ersetzen" shown instead), shown in check-in (1440 and 390); the solo balance panel shows the server refusal text in both phases.
- 1440: scrollWidth equals the viewport on every page. 390: text and DOM proofs only. Moli stalls in layout on the admin tournament detail at 390 px (the CDP capture, `getBoundingClientRect` and `scrollIntoView` never return, the render thread spins, a debugger pause gets no answer); the 379b35e bundle stalls the same way (5 of 5), so it is not caused by this branch. Host load was 40 to 55 during the runs. A 390 px screenshot pass in a real browser stays open and needs the user's browser approval.

Gate tool causes, recorded in OFFENE-URSACHEN without gate code changes: `review_gate.py:4885-4887` shows stderr first, so the CLI sign-in warning hid "Kontingent erschöpft"; a late reviewer failure loses the whole run and the window is not checked before start.

## Tournament 16 and 17

- 16 (not modified): the five teams should match the Discord picks in message 1571211196634243204. Correction left to the organisers.
- 17: with 3 groups (6 qualifiers) it hits the fallback; the preview warns and the public label follows the played bracket once generated.

## Open points

- The broker contract between Turniere and Deadlock-Bots has no shared test or startup probe (outside this repo, `OFFENE-URSACHEN.md`).
- Live admin browser pass on the production host after deploy.
- 390 px screenshots of the admin tournament detail: Moli cannot lay the page out at that width (section I); needs a real browser with the user's approval.

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
| Gate round 2 guards, locks, readers | a719ae7 | shared lock and status guards in the turnier-api mutators, guest ids excluded from the cross-tournament readers |
| Gate round 2 limits | 1e1782b | `rate_limit::record_hourly` for guest, draft and comp lobby creation, config limits named, guest count in the lobby post |
| Gate round 2 broker and OAuth | 1ad2be7 | one cookie helper, `broker::http_error_detail`, every OAuth failure branch logs |
| Gate round 2 mode and format | dea04fb | typed `InviteMode`, `MIN_BRACKET_ENTRANTS`, `check_team_size` as the one team size rule |
| Gate round 2 frontend | 379b35e | server name marks, draft presence with heartbeat and stale removal, `latestActionError`, count labels, `playerName` |
| Gate round 2 lock twin and records | 58ad385 | league `reschedule_plan` locks through `lock_tournament_row`; register, triage and README after round 2 |
| Round 3 a, c | 7d58e07 | auto lobby renders the server report, voice move sends `target` and the server resolves the channel |
| Round 3 b | 24cca4a | every mutation renders its error, counts and names through the shared helpers |
| Round 3 e | 0201953 | one team name rule in the engine, "Name (2)" on collision |
| Round 3 e | 0872f5e | one tournament row lock and one writer per team, member and signup row |
| Round 3 g | 2d1c7aa | test mode calls the production rules and serves its limits |
| Round 3 d | 57710d3 | option lists from `settings/options.ts`, match objective select |
| Round 3 d, f | cfc941a | `tournament_from_row` as the one decoder, `registration_open` from the server |
| Round 3 f | a1cee3e | `TournamentStatus` and `TournamentMode` typed through scheduler, engine, automatik, match and api |
| Round 3 d | aff4188 | solo balance availability route and `replacements_open` from the server |
| Round 3 a, d | a55d848 | dead voice members route removed, name fallbacks through `teamNameOr` and `playerName` |

## Test evidence

- rustfmt `--check` on every changed file and `cargo-slot clippy --workspace --all-targets --features turnier-api/testing,turnier-db/testing -- -D warnings`: clean.
- `test-db-run dlcentral_test_turniere -- cargo-slot test --workspace --features testing --no-fail-fast` on ionos after the K1 to K13 fixes and again on the final tree (3a9021a plus records): 633 passed, 0 failed; the PUG contract test now also covers the played format and the snapshot token.
- After the gate round 2 commits: the full ionos suite at dea04fb 622 passed and at 379b35e 623 passed, 0 failed; clippy clean with the turnier-api and turnier-scheduler testing features.
- Frontend: `npm run lint` 0, `npm test` 31/31, `npx tsc -b` 0, `npm run build` 0.
- Before gate round 3, on a55d848: full ionos suite 627 passed, 0 failed, 2 ignored (cfc941a had 626); workspace check with tests, clippy with and without `turnier-scheduler/testing`, rustfmt `--check`, `npx tsc -b`, `npm run lint`, `npm test` 31/31 and `npm run build` all clean.
Commit messages of 1dcf35d, f7e7899, 5a84931 and 43e71e8 were reworded (layer 4 named Twins, claims matched to HEAD), and the K1 to K13 message was corrected on the add-by-name window; every rewritten commit has the tree of its original (checked per commit, plus git range-diff).
