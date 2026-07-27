# Result-Fetch-Payload-Vertrag – Implementierungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Result-Fetch akzeptiert nur den tatsächlich unterstützten Fetch-Verweis und meldet nie Erfolg für verworfene Ergebnisdaten.

**Architecture:** Der HTTP-Vertrag bleibt auf `match_id_ref` beschränkt. `serde(deny_unknown_fields)` lehnt die toten Felder `winner_team_id`, `score` und `notes` vor dem Service mit dem bestehenden Validierungsstatus `422` ab; Repository und Idempotenz-Hash verarbeiten damit vollständig alle akzeptierten Felder.

**Tech Stack:** Rust, Axum, Serde, SQLx, PostgreSQL

## Global Constraints

- Erst roter Regressionstest, dann minimaler Produktionsfix.
- Keine Schemaänderung: echte Steam-Ergebnisse schreibt der Bot in `scrim.match_result_refs`.
- Keine Änderung am Discord-Versand; Result-Fetch hat keinen Discord-Seiteneffekt.
- Nicht committen und nicht pushen.

---

### Task 1: Toten Result-Fetch-Payload entfernen

**Files:**
- Modify: `rust/crates/turnier-api/tests/scrim_foundation_routes.rs`
- Modify: `rust/crates/turnier-scrim/src/dto.rs`
- Modify: `rust/crates/turnier-scrim/src/service.rs`

**Interfaces:**
- Consumes: `POST /internal/turnier/v1/scrims/matches/{id}/result-fetches`
- Produces: `ResultFetchRequest { match_id_ref: Option<String> }`

- [x] **Step 1: Write the failing test**

  Ergänze den bestehenden Operator-Routentest um einen Retry mit demselben Idempotency-Key und `winner_team_id`, `score` sowie `notes`; er muss `422 UNPROCESSABLE_ENTITY` liefern.

- [x] **Step 2: Run test to verify it fails**

  Run: `/home/naniadm/Documents/Deadlock-Bots/rust/scripts/central_test_db.sh cargo test --manifest-path /home/naniadm/Documents/Deadlock-Turniere/rust/Cargo.toml -p turnier-api --test scrim_foundation_routes --features testing match_block_and_action_operator_routes_persist_the_canonical_flow -- --exact`

  Expected: FAIL, weil der alte Vertrag den geänderten Body als idempotente Wiederholung mit `200 OK` behandelt.

- [x] **Step 3: Write minimal implementation**

  Entferne `winner_team_id`, `score` und `notes` aus `ResultFetchRequest` sowie die zugehörige tote Validierung aus `ScrimService::request_result_fetch`.

- [x] **Step 4: Run test to verify it passes**

  Führe den fokussierten Test erneut aus; erwartet ist PASS.

- [x] **Step 5: Verify workspace**

  Run: `cargo fmt --all -- --check`

  Run: `cargo build --release --workspace`

  Run: `cargo clippy --workspace --all-targets -- -D warnings`

  Run: `/home/naniadm/Documents/Deadlock-Bots/rust/scripts/central_test_db.sh cargo test --manifest-path /home/naniadm/Documents/Deadlock-Turniere/rust/Cargo.toml --workspace --features testing --no-fail-fast`

  Expected: alle Prüfungen grün; ausschließlich `invalid_proposal_transition_returns_conflict` darf als vorbestehender Fehler verbleiben.
