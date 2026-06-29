# Turnier-Automatik — Phase 1 Plan (Automatik-Grundloop)

> Basis: Spec `docs/specs/2026-06-29-turnier-automatisierung-design.md`. Phase 0 (Rename+Audit+Fixes) ist auf `main`. Implementierung an Codex (gpt-5.5/xhigh); user-sichtbare Texte schreibt Claude. Branch `feat/turnier-automatik-phase1`.

**Goal:** Bot schlägt Turniere vor → ≥1 Caster gibt frei → Turnier wird automatisch erstellt, angekündigt, beworben (DM) und erinnert.

## Sub-Phasen (nach Abhängigkeit geschnitten)

- **1a — Fundament (DIESE Welle, ohne Discord/Cross-Repo):** Migration (neue Tabellen + tournaments-Spalten) + neue Crate `turnier-automatik` mit Presets-CRUD, Proposal-State-Machine (reine DB+Logik), Opt-out/Empfänger-Berechnung, Signal-Logging. Voll unit-testbar, keine externen Dienste.
- **1b — Website-API + Frontend:** turnier-api-Endpunkte für Preset-Verwaltung, Abo/Opt-out, Proposal-Admin, manuelle Planung mit Preset-Auswahl. Frontend-UI. (user-sichtbare Texte = Claude)
- **1c — dl-bot-Broker + Interactions (Cross-Repo):** neue Broker-Endpoints (post-proposal, create-scheduled-event, dm-broadcast) + Interaction-Handler (Buttons/Modal) + Callback `POST /internal/turnier/v1/interaction`. Defer-dann-Edit (3s-Regel).
- **1d — Scheduler + Templates + MiniMax + Verdrahtung:** Vorschlags-Takt (1×/2 Wo, Config), DE-Templates (Claude), MiniMax-💬-Feedback-Parsing, End-to-End-Verdrahtung + Live-Deploy.

Jede Sub-Phase: Codex-impl → frischer Codex-Kritiker → Claude verifiziert (Build/Clippy/Test) + committet. Merge nach `main` je Sub-Phase wenn lauffähig.

---

## Phase 1a — Fundament (detaillierter Scope + DoD)

### Migration `rust/crates/turnier-db/migrations/0002_automatik.sql` (additiv, idempotent `IF NOT EXISTS` / additive `ALTER`)
Tabellen (englisch, konsistent mit `tournament_*`-Schema; alle mit `id INTEGER PRIMARY KEY AUTOINCREMENT`, `created_at TEXT NOT NULL DEFAULT (datetime('now'))` wo sinnvoll):
- `tournament_presets`: `name TEXT NOT NULL`, `category TEXT NOT NULL` (`fun`|`comp`), Turnier-Konfig-Spalten gespiegelt aus `tournaments` (team_size, bracket_format, series_format, final_series_format, tournament_mode, tournament_game_mode, match_objective, invite_mode, reminder_offsets, start_reminder_offsets, rules, description_template TEXT), `active INTEGER NOT NULL DEFAULT 1`, `created_by TEXT NOT NULL`, updated_at.
- `tournament_proposals`: `preset_id INTEGER REFERENCES tournament_presets(id)`, `source TEXT NOT NULL` (`bot`|`manual`), `proposed_start TEXT`, `config_json TEXT NOT NULL`, `state TEXT NOT NULL DEFAULT 'draft'` (`draft`|`pending_approval`|`approved`|`rejected`|`expired`), `proposal_message_id TEXT`, `channel_id TEXT`, `tournament_id INTEGER REFERENCES tournaments(id)`, decided_at.
- `tournament_proposal_votes`: `proposal_id INTEGER NOT NULL REFERENCES tournament_proposals(id) ON DELETE CASCADE`, `caster_discord_id TEXT NOT NULL`, `decision TEXT NOT NULL` (`approve`|`reject`), created_at, `UNIQUE(proposal_id, caster_discord_id)`.
- `tournament_proposal_feedback`: `proposal_id INTEGER NOT NULL REFERENCES ... ON DELETE CASCADE`, `caster_discord_id TEXT NOT NULL`, `raw_text TEXT NOT NULL`, `applied_change_json TEXT`, created_at.
- `tournament_dm_optout`: `discord_id TEXT NOT NULL`, `scope TEXT NOT NULL` (`fun`|`comp`|`all`), created_at, `UNIQUE(discord_id, scope)`.
- `tournament_signals`: `tournament_id INTEGER NOT NULL REFERENCES tournaments(id) ON DELETE CASCADE`, `participants INTEGER`, `teams INTEGER`, `no_shows INTEGER`, `poll_up INTEGER`, `poll_down INTEGER`, `poll_message_id TEXT`, `feedback_summary TEXT`, `collected_at TEXT`.
- `ALTER TABLE tournaments ADD COLUMN scheduled_event_id TEXT;` · `... ADD COLUMN source TEXT DEFAULT 'manual';` · `... ADD COLUMN preset_id INTEGER;` (jeweils nur falls Spalte fehlt — SQLite kennt kein `ADD COLUMN IF NOT EXISTS`, daher defensiv: Migration läuft auf frischer Test-DB; auf Live ist die DB bereits via 0001 da — additive Spalten sind neu, also unkritisch. Falls Re-Run-Sicherheit nötig: separate idempotente Strategie dokumentieren.)

### Crate `turnier-automatik` (neu, im Workspace `members = ["crates/*"]` automatisch)
Abhängigkeiten: `turnier-core`, `turnier-db`, sqlx, serde, thiserror, chrono. Muster: runtime-checked `query_as` + `FromRow`-Structs wie `turnier-engine/src/persist/*`. Kein `.unwrap()` in Prod-Pfaden.

Module + öffentliche Funktionen (Signaturen idiomatisch nach Vorbild turnier-engine; im Bericht zurückgeben):
- `presets`: `create/list/get/update/set_active/delete` über `tournament_presets`; `Preset`-Struct; Kategorie als Enum `Category { Fun, Comp }`.
- `proposals`: `Proposal`-Struct + `ProposalState`-Enum; `create_proposal(pool, preset_id?, source, proposed_start, config_json) -> id`; `record_vote(pool, proposal_id, caster_id, decision)`; `record_feedback(...)`; `approvals_count(...)`; reine Zustandsübergänge `transition(state, event) -> Result<state>` (pure, unit-testbar) + Persistenz `set_state`. NOCH KEINE Discord-Calls.
- `optout`: `set_optout(pool, discord_id, scope)`, `clear_optout(...)`, `is_opted_out(pool, discord_id, category) -> bool`; **reine Funktion** `compute_recipients(role_members: &[String], optouts: &[(String,Scope)], category: Category) -> Vec<String>` (Rolle minus Opt-out(category) minus Opt-out(all)) — voll unit-testbar.
- `signals`: `snapshot_signals(pool, tournament_id, ...)` schreibt `tournament_signals`; Lesefunktionen für Phase 2.

### Verdrahtung
- Crate in `turnier-bot`-AppState referenzierbar machen (Pool teilen). Noch keine Endpunkte/Scheduler — nur Bibliothek + Tests.

### DoD Phase 1a
- `cargo build --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` sauber.
- `cargo test --workspace` grün inkl. neuer Tests: Presets-CRUD-Roundtrip, Proposal-State-Übergänge (alle gültigen + ungültige), `compute_recipients` (Opt-out je Kategorie + all), optout set/clear/is_opted_out, signals-snapshot.
- Migration läuft sauber gegen frische Test-DB (sqlx-Testharness wie bestehende DB-Tests).
- Stale-Migrator-Vorsorge: falls Build die neue Migration nicht zieht, `touch rust/crates/turnier-db/src/pool.rs` vor dem Build.
- KEINE Discord-/Broker-/Scheduler-/Endpunkt-Logik (kommt in 1b–1d).
