# Human-approved Routine-Turniere Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Routine-Turniere werden ausschließlich als KI-Vorschlag im Mod-Kanal erstellt und erst nach zwei frischen, unterschiedlichen Mod-Freigaben live geschaltet.

**Architecture:** `Deadlock-Turniere` besitzt Zustand, Versionen, Stimmen und die atomare Live-Schaltung. `Deadlock-Bots` besitzt den vorhandenen KI-Client, Discord Components V2, Modals und Rollenprüfung; beide Dienste sprechen über token-geschützte interne HTTP-Endpunkte. Der Turnier-Scheduler erzeugt nur einen Draft und bittet den Master-Bot um KI-Planung und Veröffentlichung.

**Tech Stack:** Rust 1.85, Tokio, axum, SQLx/Postgres, reqwest, Serenity/`dl-discord`, `dl-ai`, Discord Components V2.

## Global Constraints

- Vorschlagskanal: `1474543558793887937`.
- Stimmberechtigte Rollen: `Moderator` (`1337518124647579661`) und `Community Moderator` (`1401891955931222110`).
- Zwei unterschiedliche Y-Stimmen sind erforderlich; Y bedeutet Zeit und Freigabe.
- N verlangt einen Grund, blockiert aber nicht.
- Jede Revision setzt alle Stimmen zurück.
- Die KI liefert nur validierte Daten/Text und führt keine Seiteneffekte aus.
- Öffentliche Ankündigungen werden niemals automatisch gesendet.
- Keine neue KI-Bibliothek, kein zweiter Discord-Gateway-Client.
- Jede KI-/Vote-Entscheidung inklusive Fehler und Timeout wird sichtbar geloggt.

---

### Task 1: Versionierte Proposal-Domäne und Zwei-Personen-Gate

**Files:**
- Create: `rust/crates/turnier-db/migrations/0003_human_approved_proposals.sql`
- Modify: `rust/crates/turnier-automatik/src/proposals.rs`
- Modify: `rust/crates/turnier-automatik/src/error.rs`
- Test: `rust/crates/turnier-automatik/tests/automatik_db.rs`
- Test: `rust/crates/turnier-db/tests/migration.rs`

**Interfaces:**
- Produces: `FeedbackKind::{Reject, Change}`, `ApprovalStatus { approvals: i64, required: i64, ready: bool }`.
- Produces: `create_revision(pool, parent_id, config_json, actor_id, feedback) -> Proposal`.
- Produces: `record_decision(pool, proposal_id, actor_id, VoteDecision, reason) -> ApprovalStatus`.
- Produces: `attach_message(pool, proposal_id, channel_id, message_id)` and `mark_materialized(pool, proposal_id, tournament_id)`.

- [ ] **Step 1: Write failing migration and domain tests**

```rust
#[tokio::test]
async fn two_distinct_approvals_are_required() {
    let proposal_id = pending_proposal(pool).await;
    let first = proposals::record_decision(pool, proposal_id, MOD_A, VoteDecision::Approve, None).await.unwrap();
    assert!(!first.ready);
    let duplicate = proposals::record_decision(pool, proposal_id, MOD_A, VoteDecision::Approve, None).await.unwrap();
    assert!(!duplicate.ready);
    let second = proposals::record_decision(pool, proposal_id, MOD_B, VoteDecision::Approve, None).await.unwrap();
    assert!(second.ready);
}

#[tokio::test]
async fn revision_resets_votes_and_preserves_feedback() {
    let revised = proposals::create_revision(pool, original, r#"{"name":"Neu"}"#, MOD_A, "Eine Stunde später").await.unwrap();
    assert_eq!(revised.revision, 2);
    assert_eq!(proposals::approvals_count(pool, revised.id).await.unwrap(), 0);
}
```

- [ ] **Step 2: Run RED**

Run: `cd rust && cargo test -p turnier-automatik --test automatik_db two_distinct_approvals_are_required revision_resets_votes_and_preserves_feedback`

Expected: compile/test failure because the new types and functions do not exist.

- [ ] **Step 3: Add the minimal schema**

```sql
ALTER TABLE turnier.tournament_proposals
  ADD COLUMN revision INTEGER NOT NULL DEFAULT 1,
  ADD COLUMN parent_proposal_id BIGINT REFERENCES turnier.tournament_proposals(id),
  ADD COLUMN announcement_draft TEXT;
ALTER TABLE turnier.tournament_proposal_feedback
  ADD COLUMN kind TEXT NOT NULL DEFAULT 'change' CHECK (kind IN ('reject', 'change'));
CREATE UNIQUE INDEX tournament_proposals_one_revision
  ON turnier.tournament_proposals (COALESCE(parent_proposal_id, id), revision);
```

- [ ] **Step 4: Implement the minimal state-machine changes**

Set `REQUIRED_APPROVALS` to `2`, keep the existing unique `(proposal_id, caster_discord_id)` constraint, require a non-empty reason for `Reject`, create a new row for every revision, and never copy votes to that row.

- [ ] **Step 5: Run GREEN**

Run: `cd rust && cargo test -p turnier-db --test migration && cargo test -p turnier-automatik --test automatik_db`

Expected: all migration and proposal tests pass.

- [ ] **Step 6: Commit and push**

```bash
git add rust/crates/turnier-db rust/crates/turnier-automatik
git commit -m "feat: Vorschläge mit Zwei-Personen-Freigabe versionieren"
git push
```

### Task 2: Scheduler darf nur noch Vorschläge erzeugen

**Files:**
- Modify: `rust/crates/turnier-automatik/src/routine.rs`
- Modify: `rust/crates/turnier-scheduler/src/loop_runner.rs`
- Modify: `rust/crates/turnier-scheduler/src/lib.rs`
- Modify: `rust/crates/turnier-config/src/lib.rs`
- Modify: `rust/crates/turnier-discord/src/notifier.rs`
- Test: `rust/crates/turnier-automatik/tests/routine_db.rs`
- Test: `rust/crates/turnier-scheduler/tests/routine_schedule.rs`
- Test: `rust/crates/turnier-discord/tests/tasks_db.rs`

**Interfaces:**
- Consumes: Proposal functions from Task 1.
- Produces: `ensure_routine_proposal(pool, preset, plan) -> EnsuredRoutineProposal`.
- Produces: `DiscordNotifier::request_proposal_publish(proposal_id, channel_id)` calling `/internal/master/v1/turnier/proposals/publish`.

- [ ] **Step 1: Write failing no-autonomy tests**

```rust
#[tokio::test]
async fn due_routine_creates_proposal_but_no_tournament() {
    scheduler.run_all_checks(due_at).await;
    assert_eq!(count("turnier.tournament_proposals").await, 1);
    assert_eq!(count("turnier.tournaments").await, 0);
}

#[tokio::test]
async fn proposal_planner_runs_at_start_then_hourly() {
    let mut cadence = RoutineCadence::new(start);
    assert!(cadence.take_if_due(start));
    assert!(!cadence.take_if_due(start + chrono::Duration::minutes(59)));
    assert!(cadence.take_if_due(start + chrono::Duration::hours(1)));
}
```

Add a notifier test asserting target channel `1474543558793887937` and proposal ID are sent to the master endpoint.

- [ ] **Step 2: Run RED**

Run: `cd rust && cargo test -p turnier-scheduler --test routine_schedule && cargo test -p turnier-discord`

Expected: current code creates and opens a tournament, so the new assertion fails.

- [ ] **Step 3: Replace the autonomous path**

Remove `ensure_routine_tournament`, `advance_tournament_status` and `announce_routine_tournament` from `check_routine_tournament`. Create/reuse one pending proposal per preset/start slot, request publication, and log `created`, `already_pending`, `publish_requested`, or `error`. Split proposal planning from the 60-second match/reminder loop: run it once at service start and then at most once per hour.

- [ ] **Step 4: Add explicit channel config**

```rust
pub routine_proposal_channel_id: i64,
// ROUTINE_PROPOSAL_CHANNEL_ID, default 1474543558793887937
```

- [ ] **Step 5: Run GREEN and regression tests**

Run: `cd rust && cargo test -p turnier-automatik --test routine_db && cargo test -p turnier-scheduler && cargo test -p turnier-discord`

Expected: all pass; no scheduler test observes a tournament before approval.

- [ ] **Step 6: Commit and push**

```bash
git add rust/crates/turnier-automatik rust/crates/turnier-scheduler rust/crates/turnier-config rust/crates/turnier-discord
git commit -m "fix: Routine-Scheduler auf Vorschläge begrenzen"
git push
```

### Task 3: Token-geschützte Turnier-Schnittstelle und atomare Live-Schaltung

**Files:**
- Create: `rust/crates/turnier-api/src/internal_automatik.rs`
- Modify: `rust/crates/turnier-api/src/lib.rs`
- Modify: `rust/crates/turnier-api/src/app.rs`
- Modify: `rust/crates/turnier-api/src/admin/tournaments.rs`
- Test: `rust/crates/turnier-api/tests/internal_automatik.rs`

**Interfaces:**
- Produces: `GET /internal/turnier/v1/proposals/{id}` with proposal, votes, feedback, preset and learned preference summary.
- Produces: `POST /internal/turnier/v1/proposals/{id}/rendered` with validated AI plan plus Discord message IDs.
- Produces: `POST /internal/turnier/v1/proposals/{id}/vote` with `actor_id`, `role_ids`, `decision`, optional `reason`.
- Produces: `POST /internal/turnier/v1/proposals/{id}/revision` with actor, feedback and validated replacement plan.
- Uses `X-Internal-Token` equal to the existing `TURNIER_INTERNAL_API_TOKEN`.

- [ ] **Step 1: Write failing HTTP contract tests**

```rust
#[tokio::test]
async fn vote_rejects_missing_token_and_unapproved_role() {
    let missing = vote_request(None, MOD_A, &[MOD_ROLE]).await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    let outsider = vote_request(Some(INTERNAL_TOKEN), MOD_A, &[OUTSIDER_ROLE]).await;
    assert_eq!(outsider.status(), StatusCode::FORBIDDEN);
    assert_eq!(count_votes().await, 0);
}

#[tokio::test]
async fn second_mod_approval_materializes_once() {
    let first = vote(MOD_A, MOD_ROLE).await;
    assert!(!first["went_live"].as_bool().unwrap());
    let second = vote(MOD_B, COMMUNITY_MOD_ROLE).await;
    assert!(second["went_live"].as_bool().unwrap());
    assert_eq!(count_tournaments().await, 1);
    vote(MOD_B, COMMUNITY_MOD_ROLE).await;
    assert_eq!(count_tournaments().await, 1);
}
```

- [ ] **Step 2: Run RED**

Run: `cd rust && cargo test -p turnier-api --test internal_automatik`

Expected: route/module missing.

- [ ] **Step 3: Implement header and role gates**

Accept only role IDs `1337518124647579661` and `1401891955931222110`; compare the internal token with the existing secret-safe configuration value using the repository's existing internal-auth pattern. Return no secret material in errors.

- [ ] **Step 4: Implement atomic finalization**

Lock the proposal row, persist the vote, recount distinct approvals, materialize through `ensure_routine_tournament`, call the existing draft→registration transition, and set `tournament_id` before commit. Repeated requests return the same tournament ID.

- [ ] **Step 5: Run GREEN**

Run: `cd rust && cargo test -p turnier-api --test internal_automatik && cargo test -p turnier-api --test automatik_routes`

Expected: all pass.

- [ ] **Step 6: Commit and push**

```bash
git add rust/crates/turnier-api
git commit -m "feat: interne Mod-Freigabe für Turniervorschläge"
git push
```

### Task 4: Master-Bot KI, Components V2, Modals und Rollenprüfung

**Files (`/home/naniadm/Documents/Deadlock-Bots` isolated worktree):**
- Create: `rust/bin/dl-bot/src/turnierglue.rs`
- Modify: `rust/bin/dl-bot/src/main.rs`
- Modify: `rust/bin/dl-bot/src/master.rs`
- Modify: `rust/bin/dl-bot/src/Cargo.toml` only if an already-workspace dependency is not yet declared.

**Interfaces:**
- Consumes: Task 3 HTTP contract through `TurnierClient` (`TURNIER_INTERNAL_BASE_URL`, default `http://127.0.0.1:8900`; `TURNIER_INTERNAL_API_TOKEN`).
- Consumes: `Arc<dyn dl_ai::TextGenerator>` from the existing OpenAI client construction.
- Produces: `POST /internal/master/v1/turnier/proposals/publish` for the scheduler.
- Produces: interaction prefix `turnier-proposal:` and custom IDs `turnier-proposal:y:{id}`, `turnier-proposal:n:{id}`, `turnier-proposal:change:{id}`.

- [ ] **Step 1: Create an isolated worktree**

Run the `superpowers:using-git-worktrees` workflow because the primary Deadlock-Bots checkout is detached and contains unrelated user edits. Use `~/.worktrees/Deadlock-Bots-human-approved-tournaments` on branch `feature/human-approved-tournaments` from verified `main`.

- [ ] **Step 2: Write failing unit tests in `turnierglue.rs`**

```rust
#[tokio::test]
async fn outsiders_cannot_vote() {
    let port = Arc::new(FakeTurnierPort::default());
    let reply = handler(port.clone()).handle(interaction_with_roles(&[OUTSIDER_ROLE], "turnier-proposal:y:7")).await;
    assert!(reply.ephemeral);
    assert_eq!(port.calls(), 0);
}

#[test]
fn proposal_card_has_three_persistent_buttons_and_gold_container() {
    let card = proposal_card(&fixture_proposal());
    assert_eq!(card["flags"], 32768);
    assert_eq!(card["components"][0]["accent_color"], 0xC8A86B);
    assert_eq!(button_custom_ids(&card).len(), 3);
}

#[tokio::test]
async fn n_and_change_open_required_text_modals() {
    for custom_id in ["turnier-proposal:n:7", "turnier-proposal:change:7"] {
        let reply = handler(fake_port()).handle(mod_interaction(custom_id)).await;
        let modal = reply.modal.expect("modal");
        assert!(modal.fields.iter().all(|field| field.required));
    }
}

#[tokio::test]
async fn went_live_posts_only_internal_announcement_draft() {
    let reply = handler(port_returning_live()).handle(mod_interaction("turnier-proposal:y:7")).await;
    let message = reply.channel_message.expect("internal draft");
    assert_eq!(message.target_channel_id, Some(1474543558793887937));
    assert!(!message.content.unwrap().is_empty());
}
```

- [ ] **Step 3: Run RED**

Run: `cd rust && cargo test -p dl-bot turnierglue`

Expected: module and tests missing.

- [ ] **Step 4: Implement the minimal ports and renderer**

Use one HTTP client implementation plus a test fake. Render one Components-V2 gold container (`flags=32768`, `accent_color=0xC8A86B`) showing version, time, plan, voters and objections. Set `allowed_mentions.parse=[]`.

- [ ] **Step 5: Implement AI generation and schema validation**

Use the existing `dl-ai` provider with temperature `0.0`. Parse exactly one JSON object containing tournament plan fields and announcement draft; reject missing/invalid values and leave the current proposal unchanged. Feed prior Y/N/change outcomes as data, not instructions.

- [ ] **Step 6: Implement interactions**

Server-side role guard every button/modal. Y records a vote; N and change open required modals; change requests a new AI plan then creates a revision. When `went_live=true`, post the generated announcement only as an internal template in channel `1474543558793887937`.

- [ ] **Step 7: Register HTTP and interaction routes**

Register `router.on_prefix("turnier-proposal:", handler)` and merge the authenticated publish route into the existing master router without starting another server or gateway.

- [ ] **Step 8: Run GREEN and workspace checks**

Run: `cd rust && cargo test -p dl-bot turnierglue && cargo test -p dl-discord && cargo clippy -p dl-bot --all-targets -- -D warnings`

Expected: all pass with no warnings.

- [ ] **Step 9: Commit and push**

```bash
git add rust/bin/dl-bot
git commit -m "feat: Turniervorschläge per Mod-Voting steuern"
git push -u origin feature/human-approved-tournaments
```

### Task 5: End-to-End-Vertrag, HTML-Doku und Changelog

**Files:**
- Modify: `docs/internal/routine-tournaments.html`
- Modify: `CHANGELOG.md`
- Test: relevant tests in Tasks 1–4.
- Modify in Deadlock-Bots: `CHANGELOG.md`.

- [ ] **Step 1: Add the cross-service contract test**

Test the serialized publish request, Components custom IDs and Turnier internal response fixtures on both sides so a field rename fails both workspaces.

- [ ] **Step 2: Document current behavior in HTML**

Explain channel, eligible roles, Y/N/change semantics, vote reset, two-person gate, internal announcement template, audit visibility and failure behavior. Remove the former autonomous flow.

- [ ] **Step 3: Update both changelogs**

Add one user-facing entry per repo: Problem → human approval flow → current behavior. No filenames, code, stack traces or marketing.

- [ ] **Step 4: Run complete verification**

Turniere:

```bash
cd rust
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Deadlock-Bots:

```bash
cd rust
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: every command exits `0`; no new warnings or failures.

- [ ] **Step 5: Commit and push verified docs/integration work**

Commit each repo separately with the required `Co-authored-by` trailer and push immediately.

### Task 6: Merge, deploy and prove live behavior

**Files:** no source changes.

- [ ] **Step 1: Review both complete diffs**

Check changed-file count, `git diff main...HEAD`, secrets scan and every decision branch. Do not merge with unresolved failures.

- [ ] **Step 2: Merge Turniere and Deadlock-Bots to `main`**

For each repo: update `main`, `git merge --no-ff feature/human-approved-tournaments`, push `main`, then verify the feature branch is an ancestor before deletion.

- [ ] **Step 3: Build release binaries**

Run `cargo build --release --workspace` in each Rust workspace. Preserve existing release directories.

- [ ] **Step 4: Restart services**

Restart `deadlock-turniere.service` and `deadlock-bot-rust.service` with `systemctl --user restart`.

- [ ] **Step 5: Live proof**

For both services prove changed PID, `/proc/<pid>/exe` points to the freshly built release binary, and the last minute of journal contains no `error|panic|fatal`.

- [ ] **Step 6: Functional proof**

Trigger or create one real pending proposal in channel `1474543558793887937`; verify non-Mod denial, first Y stays pending, a revision resets votes, N stores a reason without blocking, and two distinct allowed roles create exactly one registration tournament. Keep the public announcement manual.

- [ ] **Step 7: Cleanup**

Remove merged branches/worktree only after `git merge-base --is-ancestor` succeeds, then run `git worktree prune`.
