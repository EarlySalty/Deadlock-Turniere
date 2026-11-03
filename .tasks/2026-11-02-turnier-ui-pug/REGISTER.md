| Package | Branch | SHA | Status | Blocker |
|---|---|---|---|---|
| LIGA-FIX | feat/turnier-liga | d1fa8c6 | fixer done, checks green (fmt, clippy, 128 DB tests, npm 19/19) | gate running (limit raised to 800 KB on 2026-11-03, gate_hook.py 4b41a22); side finding: test DB lacks `deadlock` role, wrapper /tmp/turnier-liga-dbtest.sh |
| P1 multi-tournament | feat/multi-turnier | | started | waits for Liga on main before gate |
| P2 reminders | feat/reminder-input | | started | same |
| P3a settings sections | feat/settings-sections | pushed | built, waiting for Liga on main | wires P2 ReminderOffsetsInput during rebase; screenshots /tmp/p3a-shots; Mindestrang left to P3b |
| P4 admin view + team builder | feat/admin-team-builder | f177cde | built, waiting for Liga on main | merges main (no rebase); screenshots /tmp/p4-shots |
| P5 Discord roles | (live guild + Deadlock-Bots) | n/a | done, live | rename to "Custom Games" + role "Turnier Teilnehmer" (1566909672676663447) live; explainer (discord-rollen-erklaerung.*) and onboarding (discord-onboarding-vorschlag.json) wait for user go; open: Scrims as onboarding option A/B, merge "Funny Custom Ping" into "Custom Games"; follow-up code package: Deadlock-Bots serversync.rs finds onboarding roles by name ("Events & Turniere" gone), ROLE_LFG_PING_ID points at renamed role |
| P3b solo-only + rank | | | wave 2 | |
| P6 Steam bot flag | | | wave 2 | |
| P7 lobby automation test | | | wave 2 | |
