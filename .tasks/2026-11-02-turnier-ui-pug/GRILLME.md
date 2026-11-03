# Grillme: tournament admin UI, PUG format, multi-tournament, Discord roles (2026-11-02)

Legend: BUILD-P0/P1/P2 = build (priority), DROP = do not build, KLAEREN = open, MODERN = build with deliberate modernisation.

## Inventory (looked up, not asked)

- Single-active guard: `rust/crates/turnier-api/src/admin/helpers.rs:19` (statuses draft..bracket, non-test) used in create/update/advance and scheduler `transition.rs:184`; frontend warning in `Admin.tsx:288`. Public `Home.tsx:29` picks one active tournament via `find`.
- Solo signup + rank balancing exists: `admin/solo_balance.rs`, `SoloBalancePanel.tsx`, button "Solo-Spieler zuweisen", waitlist priority, invite mode `never` = auto balance at check-in end (see `.tasks/2026-10-11-solo-balance/AUFTRAG.md`).
- Reminder fields are comma-separated minute strings: `CreateTournamentForm.tsx:215` (`reminder_offsets`, `start_reminder_offsets`), default 1440,120,15 in `helpers.rs:26`.
- No template/clone of an archived tournament exists (`ArchivedTournamentView.tsx` read-only).
- Deadlock-Bots has a generic reaction-role system with dashboard CRUD (`dl-community/src/reaction_roles.rs`, `dl-dashboard/src/reaction_roles.rs`) and an onboarding concept with "#customize" role selection (`docs/onboarding-redesign/2026-07-02-konzept.md`).

## Agenda

- A Multi-tournament: lift single-active guard; public page, sidebar, scheduler behaviour.
- B PUG format: what differs from the existing solo signup + balance flow; rank gate (Oracle+, Emissary as sub).
- C Reminder UI: presets, time picker rows, "+" button; both reminder fields.
- D Reuse archived tournament as template: what gets copied.
- E Settings sections: "Spielmodus & Lobby-Automatik" as own section in edit form; "Erweiterte Regler" wording/clarity; Einladungs-Modus wording.
- F Admin empty state / layout (small sidebar, huge empty panel).
- G Discord roles: Scrims / Fun Customs / Turniere; native "Kanäle und Rollen" vs bot reaction roles; what the roles unlock.

## Decisions

(filled during the interview)

### B PUG format (partial, 2026-11-02)
- BUILD-P0: tournament option "solo signup only" (no team signups allowed). User: "Optionen da keine Teams zu erlauben, dass man sich nur Solo eintragen kann".
- BUILD-P0: manual team formation. Admin forms teams in Discord (humans decide), then manages teams and members in the admin UI. Bot does not auto-form teams in this mode; existing rank balance stays an option.
- BUILD-P1: manual captain selection by the admin per team.
- Interview language: German in chat (user request 2026-11-02); subagent briefs in English.

### B PUG format (continued)
- BUILD-P0: admin builds teams by drag-and-drop from the solo signup list in the admin UI (players must have signed up in the bot for that tournament beforehand).
- BUILD-P0: settings form split into "Standard-Einstellungen" and "Spezielle Einstellungen"; "Teambildung durch Spieler nicht erlauben" (solo only) is one of the special settings.

### A Multi-tournament
- BUILD-P0: lift the single-active guard. Public page lists all open tournaments.
- BUILD-P1: overlap rule: same day is fine, but tournament starts must be at least 4 hours apart (server-side check on create/update; first idea, exact value is config/constant).

### C Reminder UI
- BUILD-P0: reminders stay relative to the tournament start (offset before). No absolute calendar picker.
- BUILD-P0: input = preset buttons (1 Tag, 2 Std, 1 Std, 15 Min) plus a free row "number + unit" (Minuten/Stunden/Tage/Wochen) with "+" for more rows; each reminder is a chip that can be removed. Same component for both reminder fields.
- BUILD-P0 (bug): user reports the reminders/announcements did not go out last time. Diagnose the scheduler path (`turnier-scheduler/src/reminders.rs`) against the live DB and logs before touching the UI; fix the cause.

### D Reuse archived tournament
- BUILD-P0: "Als Vorlage nutzen" on an archived tournament copies settings only (name, description, rules, team size, format, modes, lobby settings, reminders, special settings) into the create form with empty dates. Teams and participants are never copied; players sign up again.

### E Settings sections
- BUILD-P0: three areas in create and edit: "Standard" (name, description, rules text, dates, team size, format, reminders), "Spezielle Einstellungen" (e.g. no player-made teams / solo only, invite mode, no-show grace, leaderboard exclusion), "Spielregeln" (game mode, ConVar preset, advanced sliders, lobby automation). Spielregeln moves out of the create form into its own tab/section of the tournament management, editable later.
- KLAEREN/TEST: automatic lobby creation via the Steam bot has never been tested end to end. Needs a real test run (test mode) before relying on it for the PUG tournament.

### F Admin start view
- BUILD-P0: two panels stay. Left panel = "Turnier erstellen" entry. Right panel = tournament overview: the selected tournament is shown expanded with full details (dates, signup counts, phase), the other planned tournaments sit below as collapsed rows and can be selected to swap. Collapsed = compact row, expanded = detail card. No empty "Kein Turnier ausgewählt" state: default-select the next upcoming tournament.

### G Discord roles
- BUILD-P0: reuse existing roles; check the live guild first. "Grind Custom Ping" is to be renamed and used for fun customs. Turniere role works like "Scrim Teilnehmer": it unlocks the tournament section with sensible member permissions.
- BUILD-P0: both ways to get the role: native Discord "Kanäle & Rollen" onboarding plus an explainer channel under Information/Willkommen telling members how to pick roles and find the sections.
- BUILD-P1: Welcome/Willkommen area needs a rework anyway (scope separate, note only).
- BUILD-P1 (Deadlock-Brain, separate repo note): everything members need to know about roles, channels, tournaments goes into the Brain knowledge. Longer term the Brain gets permission-scoped knowledge (public vs internal, by user rights) so internal docs can be served to entitled users; and the Brain should be able to create such knowledge entries when we plan them. Record as a Brain task, not part of this build.

### B PUG format (rank gate) and H Steam bot behaviour
- BUILD-P0: Steam link is mandatory for tournament signup (Verlinkungspflicht). Rank is read from the Deadlock API via the linked Steam ID, not via Steam bot friendship.
- BUILD-P1: minimum rank per tournament as a special setting (e.g. Oracle). Assumption to confirm: signups below the minimum are kept and marked "Ersatzspieler", not rejected.
- BUILD-P1 (Deadlock-Steam-Bot repo, separate package): stop automatic friend requests and automatic Discord roles. Default = a flag that suppresses them; a Discord button lets a user opt in, which clears the flag. Ranked mode makes the Steam bot friendship mostly unnecessary.

## Coverage
A, B, C, D, E, F, G decided; H added from the interview. Open: E lobby automation test run (TEST), B sub-marking assumption (confirm), G Welcome rework and Brain permission-scoped knowledge (separate later tasks).

### I Liga format (taken over from the stalled native session of 2026-11-02)
- State found: branch `feat/turnier-liga` (0bd9a60) = backend + frontend for mode `league` ("Jeder gegen jeden"), tested on a throwaway DB with 10 teams / 45 matches / 9 rounds. Gate round 1 BLOCK fixed; round 2 BLOCK only for size (164 KB > 150 KB). Backend slice `feat/turnier-liga-backend` (2ef2716, 131 KB) was pushed and its gate run died with the session. Brief: `.tasks/2026-11-02-turnier-liga/AUFTRAG.md` on that branch.
- Decision (technical, Fable): integrate Liga first, backend slice then frontend slice, before the UI restructure packages, because the Liga frontend touches the same files (CreateTournamentForm, TournamentManager, Admin.tsx, Home.tsx). The new settings layout gets "Jeder gegen jeden" as third Turniermodus.
