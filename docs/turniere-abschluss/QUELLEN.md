# Quellen – vollständig über GitHub erreichbar

## Repositories und Regeln
- [Deadlock-Turniere](https://github.com/EarlySalty/Deadlock-Turniere), [aktuelle CODE-REVIEW.md](https://github.com/EarlySalty/Deadlock-Turniere/blob/main/CODE-REVIEW.md), [AGENTS.md](https://github.com/EarlySalty/Deadlock-Turniere/blob/main/AGENTS.md).
- [Deadlock-Bots](https://github.com/EarlySalty/Deadlock-Bots) (zentrale DB, Discord/Broker), [Deadlock-Steam-Bot](https://github.com/EarlySalty/Deadlock-Steam-Bot), [Deadlock-Twitch-Bot](https://github.com/EarlySalty/Deadlock-Twitch-Bot), [caddy-config](https://github.com/EarlySalty/caddy-config), [Deadlock-Brain](https://github.com/EarlySalty/Deadlock-Brain). Aktuelle Repo-Regeln/CI lesen. Falls ein Repo nicht zugänglich ist, Zugriff vom Maintainer erfragen, nicht raten oder privat Zugangsdaten verwenden.

## Originalaufträge und Code
- PUG A–H: [Originalaufträge, Register, TODO und Gate-Triage](https://github.com/EarlySalty/Deadlock-Turniere/tree/b56dc0e076516f759f6dc3294fafff68bb0e264d/.tasks/2026-11-14-pug-chaos); [aktueller PUG-Arbeitsbranch](https://github.com/EarlySalty/Deadlock-Turniere/tree/fix/turniere-closeout-01-pug).
- Ursprünglicher P1–P12-Auftrag und GRILLME: [Originaldokumente](https://github.com/EarlySalty/Deadlock-Turniere/tree/7ae479f1eefb87805c50d3b9ff4015779970a728/.tasks/2026-11-02-turnier-ui-pug). Historisch, nicht automatisch neue Nutzerentscheidungen ableiten.
- Admin/Brain: [77d6f47](https://github.com/EarlySalty/Deadlock-Turniere/tree/77d6f470b83c183c2c6fbd40a6cbfc488c5f938a), [UI-Cleanup e92f123](https://github.com/EarlySalty/Deadlock-Turniere/tree/e92f12349e682d9cef0755f84c27555ddf2eab3c), [Brain-Tests 4004074](https://github.com/EarlySalty/Deadlock-Turniere/tree/4004074c4b4ff1e2fc7ae98004b5eaf61ef48e9a).
- Draft: [PR #34](https://github.com/EarlySalty/Deadlock-Turniere/pull/34), [Performance-Dokumente](https://github.com/EarlySalty/Deadlock-Turniere/tree/862d17c7d3f64e54115bef23473deecd7f5cc8a5/docs/performance).
- P8: [Code-Sicherung](https://github.com/EarlySalty/Deadlock-Bots/tree/3be6f7749f55bf24cbbbc3f84c4fee0c618a827f), [originale TODO-Datei](https://github.com/EarlySalty/Deadlock-Bots/blob/2299000eb8088f14d4a228c94f39304b00d8d649/.tasks/2026-11-03-onboarding-role-lookup/TODO.md).
- Zentral-DB: [Migration solo_balance](https://github.com/EarlySalty/Deadlock-Bots/blob/e65d3805ae0117afc12a8494c739ae8e6301818b/rust/crates/dl-central-db/migrations/20261011120000_turnier_solo_balance.sql).
- Twitch: [Promo-Fix-Branch](https://github.com/EarlySalty/Deadlock-Twitch-Bot/tree/fix/turnier-event-signup-closed). Fünf Commits 27dcb5833, cf4769ac8, c5fbe90ca, 49fd30ddb, ee002f044; quinn-Bump b92c42b3e nicht doppelt übernehmen (damals bereits afc3fba7d auf main).
- Caddy: [PR #10](https://github.com/EarlySalty/caddy-config/pull/10), [CI-Run 45859489137](https://github.com/EarlySalty/caddy-config/actions/runs/45859489137). Billing-Blocker laut Worker-Annotation, nicht Codefehler.
- Ältere CI-/Observer-Sicherungen: [historical-ci 6614c61](https://github.com/EarlySalty/Deadlock-Turniere/tree/6614c615bf393d23f5a865fe5117a861549e3d0d), [observer-stash d3fa2cb](https://github.com/EarlySalty/Deadlock-Turniere/tree/d3fa2cb024074f4b8c16cd3d8ea4eb09f42e4317), [CI-WIP a834475](https://github.com/EarlySalty/Deadlock-Turniere/tree/a834475b2bc5cd60a5576aafdebcdf2e8d992876). Nur relevante verbleibende Deltas.

## Wave 1 cleanup (P1b/P4b)

Originalbasis 6382527; Quellcode inzwischen gesichert in [sicherung/turniere-closeout-admin-cleanup-20261126 @ e92f123](https://github.com/EarlySalty/Deadlock-Turniere/tree/e92f12349e682d9cef0755f84c27555ddf2eab3c). Historischer Entwurf, nicht buildbar und nicht freigegeben.

## Done (uncommitted, unverified)
- frontend/src/lib/tournamentStart.ts: effectiveStart (mirrors rust turnier-engine persist/start_gap.rs effective_start), byEffectiveStart, sortByEffectiveStart, signupOpen (registration|checkin, matches public/helpers.rs ensure_registration_open).
- lib/adminTournaments.ts now re-exports those (admin list sorted like the server, fixes old checkin_start/mode mismatch).
- pages/Home.tsx: uses shared helper; CTA "Jetzt teilnehmen" only for registration/checkin, else "Zum Turnier" with ArrowRight.
- pages/Admin.tsx: list error card TournamentListError (message + "Erneut versuchen") for live and archive tabs instead of the empty state; phase grid keyed by liveDetail.id so all panels remount on tournament switch (stale spinner/busy state); ParticipantManager gets new prop builderActive (team_size > 1). ParticipantManager does NOT accept that prop yet, build fails until step 1.

## Next steps
1. ParticipantManager: add builderActive prop. When true render SoloBalancePanel plus one compact "Weitere Aktionen" card: per team row (name, n/size, recruiting select, "Bewerbungen offen" badge, "Spieler ersetzen" in group_phase/bracket for admins, delete team) and open solo signups with "Entfernen" only, plus feedback/error. Drop "Teams & Teilnehmer", "Offene Solo-Anmeldungen" with "Team wählen…", per-team cards (builder covers create, rename, captain, move, assign, remove member). When false (team_size 1) keep legacy view.
2. Phantom team: teamBuilderDrafts.ts add in-flight save registry (Map of tournamentId to Promise). TeamBuilderPanel.save: writeDraft EMPTY at start, writeDraft(rebased) on failure, register promise; on mount, if a save is in flight show "Speichert …" disabled and reload the draft after it settles.
3. Checks in frontend/: npm run lint, npm run build, npm test (exit codes). No Rust changes so far.
4. Browser screenshots with isolated mock endpoints for home and admin (include a list-failure scenario returning 500 on /admin/tournaments) at 1440 and 390: home running plus registration, admin Teilnehmer after cleanup, admin error state. Inspect every image.
5. Commit (English message), follow current repo review procedure against origin/main, fix actionable findings, publish feature branch and PR (never main). Report to maintainer.
