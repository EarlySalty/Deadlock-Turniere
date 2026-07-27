# Scrim Cutover Guard and Nullable Patch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Block every new roster/substitute write while Turniere is not runtime owner and make explicit JSON `null` clear nullable participant text fields.

**Architecture:** Keep ownership enforcement in each repository transaction, before any roster mutation or delivery lease write. Preserve the existing `PatchValue` tri-state through SQL with `CASE`, so omitted fields remain unchanged while explicit `null` writes SQL `NULL`.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL integration tests.

## Global Constraints

- Test first and observe the expected failure before production edits.
- Discord failures must not roll back committed database state and must be logged with `tracing::warn!` context.
- Do not commit or push.
- Finish with release build, Clippy with warnings denied, and the full Postgres-backed workspace tests.

---

### Task 1: Runtime ownership for roster and substitute writes

**Files:**
- Modify: `rust/crates/turnier-api/tests/scrim_foundation_routes.rs`
- Modify: `rust/crates/turnier-scrim/tests/substitute_expiry.rs`
- Modify: `rust/crates/turnier-scrim/src/repository.rs`

**Interfaces:**
- Consumes: `require_turniere_runtime(&mut Transaction<'_, Postgres>)`
- Produces: `ScrimError::RuntimeNotWritable` before any affected write

- [ ] Add a route test proving team creation is rejected before Turniere owns the runtime and leaves no team behind.
- [ ] Add a repository test proving the expiry sweep is rejected and leaves expired membership intact.
- [ ] Run both tests and verify they fail because the writes currently succeed.
- [ ] Call `require_turniere_runtime` inside every affected write transaction before mutation.
- [ ] Enable Turniere runtime in existing positive roster and expiry tests.
- [ ] Re-run the focused tests and verify they pass.

### Task 2: Explicit nullable participant fields

**Files:**
- Modify: `rust/crates/turnier-api/tests/scrim_foundation_routes.rs`
- Modify: `rust/crates/turnier-scrim/src/repository.rs`

**Interfaces:**
- Consumes: `PatchValue<String>`
- Produces: `Option<Option<String>>`, where outer `None` means omitted and inner `None` means SQL `NULL`

- [ ] Extend the roster route integration test to set rank, roles, and notes, then clear all three with explicit JSON `null`.
- [ ] Run the focused test and verify it fails because the previous values remain.
- [ ] Preserve `PatchValue` tri-state and replace `COALESCE` with conditional assignments.
- [ ] Re-run the focused test and verify response and stored values are null.

### Task 3: Verification

**Files:**
- Verify only; no new files.

**Interfaces:**
- Consumes: completed implementation
- Produces: fresh build, lint, and test evidence

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo build --release --workspace`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `/home/naniadm/Documents/Deadlock-Bots/rust/scripts/central_test_db.sh cargo test --manifest-path /home/naniadm/Documents/Deadlock-Turniere/rust/Cargo.toml --workspace --features testing --no-fail-fast`.
- [ ] Confirm only the documented pre-existing `invalid_proposal_transition_returns_conflict` failure, if it remains.
