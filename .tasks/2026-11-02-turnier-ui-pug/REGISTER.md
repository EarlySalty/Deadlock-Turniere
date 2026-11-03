| Package | Branch | SHA | Status | Blocker |
|---|---|---|---|---|
| LIGA-FIX | feat/turnier-liga | d1fa8c6 | fixer done, checks green (fmt, clippy, 128 DB tests, npm 19/19) | gate ALLOW 2026-11-03 (limit 800 KB, gate_hook.py 4b41a22), on main d1fa8c6, live 01:22 CET (service active, health 200, new bundle served); side finding: test DB lacks `deadlock` role, wrapper /tmp/turnier-liga-dbtest.sh |
| P1 multi-tournament | feat/multi-turnier | | started | waits for Liga on main before gate |
| P2 reminders | feat/reminder-input | eb9dc80 | on main, gate ALLOW (2 rounds) | cause: bracket_only tournaments never set bracket_start so start reminders were skipped; fix falls back to registration_end; screenshots /tmp/p2-shots; not yet deployed |
| P3a settings sections | feat/settings-sections | pushed | built, waiting for Liga on main | wires P2 ReminderOffsetsInput during rebase; screenshots /tmp/p3a-shots; Mindestrang left to P3b |
| P4 admin view + team builder | feat/admin-team-builder | f177cde | built, waiting for Liga on main | merges main (no rebase); screenshots /tmp/p4-shots |
| P5 Discord roles | (live guild + Deadlock-Bots) | n/a | done, live | rename to "Custom Games" + role "Turnier Teilnehmer" (1566909672676663447) live; explainer (discord-rollen-erklaerung.*) and onboarding (discord-onboarding-vorschlag.json) wait for user go; open: Scrims as onboarding option A/B, merge "Funny Custom Ping" into "Custom Games"; follow-up code package: Deadlock-Bots serversync.rs finds onboarding roles by name ("Events & Turniere" gone), ROLE_LFG_PING_ID points at renamed role |
| P3b solo-only + rank | | | wave 2 | |
| P6 Steam bot flag | | | wave 2 | |
| P7 lobby automation test | | | wave 2 | |
| P9 em-dash sweep | feat/no-em-dashes | | spawned 2026-11-03 | user: remove all em dashes in the repo (125 source files); conflicts with P1-P4 resolved by whoever lands later |
| P10 timezone fix | | | wave 2, after P3a/P4 land | admin form sends naive local time, DB stores it as UTC, times 1-2 h off (legacy Python read Berlin time); fix form + API to send zoned time; check existing rows incl. #16 |
| P11 registration reminder vs check-in + scheduler test harness | | | wave 2 | scheduler tests/common/mod.rs hard-codes a local PG socket, all 10 advance_status DB tests fail at setup on ionos (verified 2026-11-03 on main eb9dc80, log /tmp/turniere-p2-dbtest.log); port to DATABASE_URL via test-db-run; 15-minute registration reminder can never send because check-in ends registration status first; also gate nits: 0-minute reminder not addable, edit-form help text about defaults |
