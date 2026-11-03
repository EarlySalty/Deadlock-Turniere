# Tournament admin UI, PUG format, multi-tournament, Discord roles, Liga integration

Decisions: GRILLME.md in this folder (authoritative). Main orchestrator: Fable 5.1 T3 thread 69ad09ac. Workers: Opus 5.5 medium, native subagents, own worktrees.

Packages (Turniere packages branch from origin/feat/turnier-liga and rebase onto main once Liga lands):
- LIGA-FIX: fix gate findings on feat/turnier-liga, then full-branch gate after the size limit decision.
- P1 multi-tournament: drop single-active guard, 4 h start spacing, public list, scheduler per tournament.
- P2 reminders: diagnose why reminders did not send, fix; ReminderOffsetsInput component (presets + number/unit rows).
- P3a settings: Standard / Spezielle Einstellungen / Spielregeln sections in create and edit, Spielregeln as own area, "Als Vorlage nutzen" from archive.
- P4 admin start view + drag-and-drop team builder from solo signups + manual captain.
- P5 Discord roles (Deadlock-Bots / live guild): Turniere role unlocks section like Scrim Teilnehmer, rename Grind Custom Ping, explainer channel draft, onboarding proposal.
- Wave 2: P3b solo-only + mandatory Steam link + min rank (Ersatzspieler); P6 Steam bot flag (no auto friend requests/roles); P7 lobby automation test run; P8 Deadlock-Bots follow-up after the role rename: serversync.rs onboarding-role lookup by name, ROLE_LFG_PING_ID target, onboarding apply after user go.
