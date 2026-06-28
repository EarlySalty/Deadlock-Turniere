# Turnier-Automatik — Phase 0 Implementation Plan (Vorbedingung)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Saubere, bewiesen vollständige Rust-Turnier-Basis in `main` schaffen, bevor das Automatik-Feature drauf gebaut wird — durch Crate-Rename + Py→Rust-Vollständigkeits-Audit + Merge.

**Architecture:** Phase 0 ändert keine Funktionalität. Sie (1) benennt die Crates `tb-*` → `turnier-*` mechanisch um, (2) beweist per adversarialem Audit, dass kein Python-Feature fehlt, (3) schließt bestätigte Lücken, (4) mergt den seit Wochen live-aber-ungemergten `rust-rewrite`-Stand nach `main`.

**Tech Stack:** Rust (Cargo-Workspace, axum, sqlx/SQLite), systemd-user-Service, Codex (gpt-5.5/xhigh) als Implementierungs-Worker.

## Global Constraints

- Spec: `docs/specs/2026-06-29-turnier-automatisierung-design.md` (verbindlich).
- Keine Funktionsänderung in Phase 0 — reines Umbenennen + Lücken schließen (Verhalten 1:1).
- Crates deutsch-klar (`turnier-*`); DB-Tabellen/Enum-Werte englisch (Schema-konsistent).
- Kein `.unwrap()` in Produktionspfaden; `cargo clippy --workspace --all-targets -- -D warnings` muss sauber bleiben.
- Live-Dienst: `deadlock-turniere.service` (systemd --user) startet `scripts/run_turniere_backend_rust.sh` → Binary heute `rust/target/release/tb-app`.
- Deploy-Beweis-Regel (CLAUDE.md): nach Build/Deploy beweisen, dass Artefakt UND Live-Zustand die Änderung tragen (Binary-mtime > Commit, Prozess-Pfad prüfen) — Erfolgs-Log allein zählt nicht.
- `main` empfängt nur fertige, verifizierte Arbeit. Merge nach `main` = expliziter Checkpoint (Task 5), vorher anhalten.
- Stale-Migrator-Lektion: vor Release-Build `touch` auf die Migrator-`.rs`, falls nur `.sql` geändert wurde (hier nicht relevant, da keine Migration in Phase 0 — aber Regel notiert).

---

### Task 1: Crate-Rename `tb-*` → `turnier-*`

Mechanischer, verhaltensneutraler Rename. Ein zusammenhängender Task (Rename ohne grünen Build ist wertlos → Build-Verifikation ist der „Test").

**Files:**
- Modify: `rust/Cargo.toml` (workspace `members` + ggf. `[workspace.dependencies]`)
- Rename: alle Verzeichnisse `rust/crates/tb-*` → `rust/crates/turnier-*`
- Modify: jede `rust/crates/*/Cargo.toml` (`name = "..."` + Pfad-Deps `{ path = "../tb-*" }`)
- Modify: alle `*.rs` mit `use tb_*` / `tb_*::` / `extern crate tb_*`

**Rename-Tabelle (verbindlich):**

| Verzeichnis/Paket alt | neu | Crate-Import (underscore) |
|---|---|---|
| `tb-app` | `turnier-bot` | `turnier_bot` |
| `tb-web` | `turnier-api` | `turnier_api` |
| `tb-core` | `turnier-core` | `turnier_core` |
| `tb-db` | `turnier-db` | `turnier_db` |
| `tb-config` | `turnier-config` | `turnier_config` |
| `tb-auth` | `turnier-auth` | `turnier_auth` |
| `tb-scheduler` | `turnier-scheduler` | `turnier_scheduler` |
| `tb-discord` | `turnier-discord` | `turnier_discord` |
| `tb-tournament` | `turnier-engine` | `turnier_engine` |
| `tb-draft` | `turnier-draft` | `turnier_draft` |
| `tb-match` | `turnier-match` | `turnier_match` |
| `tb-steam` | `turnier-steam` | `turnier_steam` |

> Achtung `tb-tournament` → `turnier-engine` (nicht `turnier-tournament`): der Import wechselt von `tb_tournament` auf `turnier_engine` — beide Teile (Bindestrich-Paketname UND Unterstrich-Importname) müssen passen.

- [ ] **Step 1: Ausgangs-Build als Referenz grün stellen**

Run: `cd rust && cargo build --workspace 2>&1 | tail -5`
Expected: Build OK (sonst zuerst klären, nicht renamen).

- [ ] **Step 2: Verzeichnisse umbenennen (git-aware)**

Pro Zeile der Tabelle: `git mv rust/crates/<alt> rust/crates/<neu>`.

- [ ] **Step 3: Paketnamen + Pfad-Deps + Workspace-Member ersetzen**

In jeder `Cargo.toml`: `name = "tb-x"` → `name = "turnier-y"`, `{ path = "../tb-x" }` → `{ path = "../turnier-y" }`, Dependency-Schlüssel `tb-x = ...` → `turnier-y = ...`. In `rust/Cargo.toml` die `members`-Liste angleichen. Für `tb-app`: auch ein evtl. `[[bin]]`/`default-run` auf `turnier-bot` setzen (Binary muss `turnier-bot` heißen).

- [ ] **Step 4: Rust-Importe ersetzen (underscore-Form)**

Workspace-weit `tb_app`→`turnier_bot`, `tb_web`→`turnier_api`, `tb_core`→`turnier_core`, `tb_db`→`turnier_db`, `tb_config`→`turnier_config`, `tb_auth`→`turnier_auth`, `tb_scheduler`→`turnier_scheduler`, `tb_discord`→`turnier_discord`, `tb_tournament`→`turnier_engine`, `tb_draft`→`turnier_draft`, `tb_match`→`turnier_match`, `tb_steam`→`turnier_steam`.

- [ ] **Step 5: Build + Clippy + Tests verifizieren**

Run: `cd rust && cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace 2>&1 | tail -20`
Expected: alles grün (37 Suites wie vorher).

- [ ] **Step 6: Rückstands-Grep (kein `tb-`/`tb_` mehr)**

Run: `cd rust && grep -rnE '\btb[-_](app|web|core|db|config|auth|scheduler|discord|tournament|draft|match|steam)\b' --include='*.rs' --include='*.toml' . | grep -v target`
Expected: keine Treffer.

- [ ] **Step 7: Binary-Name prüfen**

Run: `ls rust/target/release/turnier-bot && ! ls rust/target/release/tb-app 2>/dev/null`
Expected: `turnier-bot` existiert.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(rust): Crates tb-* -> turnier-* (verhaltensneutral)

Co-authored-by: <model> <model@local>"
git push
```

---

### Task 2: Deploy-Pfade umstellen + Live-Verifikation

Der Dienst startet noch `tb-app`. Ohne diesen Task läuft der Dienst nach dem Rename ins Leere.

**Files:**
- Modify: `scripts/run_turniere_backend_rust.sh` (Binary-Pfad `target/release/tb-app` → `target/release/turnier-bot`)
- Modify: ggf. systemd-Unit (falls ExecStart direkt das Binary nennt; aktuell ruft sie das Skript — dann reicht das Skript)

- [ ] **Step 1: Binary-Referenz im Run-Script ersetzen**

In `scripts/run_turniere_backend_rust.sh` jeden `tb-app`-Pfad auf `turnier-bot` umstellen.

- [ ] **Step 2: Release bauen**

Run: `cd rust && cargo build --release --bin turnier-bot 2>&1 | tail -3`
Expected: OK, `rust/target/release/turnier-bot` aktualisiert.

- [ ] **Step 3: Dienst neu starten**

Run: `systemctl --user restart deadlock-turniere.service && sleep 2 && systemctl --user is-active deadlock-turniere.service`
Expected: `active`.

- [ ] **Step 4: Live-Zustand beweisen (nicht dem Log trauen)**

Run: `ps -o pid,etimes,cmd -C turnier-bot 2>/dev/null || ps aux | grep -E 'turnier-bot' | grep -v grep`
Expected: laufender Prozess aus `…/rust/target/release/turnier-bot`; kein `tb-app`-Prozess mehr.
Zusatz: Health-/Smoke-Check gegen einen bekannten API-Endpunkt (z. B. `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:<port>/<health>`), Erwartung 200/redirect wie vor dem Rename.

- [ ] **Step 5: Commit**

```bash
git add scripts/run_turniere_backend_rust.sh
git commit -m "chore(deploy): Run-Script auf turnier-bot-Binary

Co-authored-by: <model> <model@local>"
git push
```

---

### Task 3: Vollständigkeits-Audit Py→Rust (Codex-Delegation, read-only)

Discovery-Task — Ergebnis ist die Lücken-Liste, die Task 4 definiert. Kein Code, sondern ein verifizierter Report.

**Files:**
- Create: `rust/docs/audit/2026-06-29/REPORT.md` (Inventar + Mapping + Lücken-Liste)

**Methodik (Spec §11):**
1. Codex-Worker A erstellt Inventar aus `backend/` (alle Routen in `routes.py`/`admin_routes.py`/`*_routes.py` + alle Modul-Funktionen) und mappt jede auf ihr Rust-Pendant in `rust/crates/turnier-*`. Output: Tabelle `python_symbol | rust_pendant | status (vorhanden|abweichend|fehlt|bewusst_ausgelassen)`.
2. Codex-Worker B (frischer Kontext, adversarial) prüft Worker As Liste: Over-Reporting (fälschlich „fehlt") und Under-Reporting (übersehene Lücken). Lektion Twitch/Steam: Triage über-feuert „genuine".
3. Claude verifiziert Stichproben gegen echte Signale (Code/DB/Endpoint), nicht nur Modell-Annahmen.

- [ ] **Step 1: Worker A — Inventar + Mapping**

Codex (gpt-5.5/xhigh, Repo-Zugriff): „Erstelle vollständiges Endpunkt-/Funktions-Inventar aus `backend/` und mappe jede auf `rust/crates/turnier-*`. Schreibe `rust/docs/audit/2026-06-29/REPORT.md` mit Status-Tabelle. Kein Fix, nur Befund."

- [ ] **Step 2: Worker B — adversariale Prüfung**

Codex (frischer Kontext): „Prüfe REPORT.md gegen das echte Python + Rust. Liste Fehlklassifikationen (fälschlich fehlt / übersehene Lücke). Begründe je Befund mit Datei:Zeile."

- [ ] **Step 3: Claude-Stichprobe + Konsolidierung**

Claude prüft 5–10 strittige Befunde direkt im Code/DB/Endpoint, konsolidiert zu einer finalen Lücken-Liste (nur bestätigte `fehlt`/`abweichend`), trägt bewusste Auslassungen in `rust/docs/known-issues.md` ein.

- [ ] **Step 4: DoD-Gate**

Jede Python-Route/-Funktion hat: Rust-Pendant ODER Eintrag in der finalen Lücken-Liste ODER dokumentierte Auslassung. Endpunktweise Parität für `public`+`admin` (Request/Response-Form, Statuscodes, Auth-Gates) bewertet.

- [ ] **Step 5: Commit REPORT**

```bash
git add rust/docs/audit/2026-06-29/REPORT.md rust/docs/known-issues.md
git commit -m "docs(audit): Py->Rust Vollstaendigkeits-Audit 2026-06-29

Co-authored-by: <model> <model@local>"
git push
```

---

### Task 4: Lücken-Fix-Welle (abgeleitet aus Task 3)

> **Datenabhängig — kein Platzhalter, sondern Discovery-Ergebnis.** Diese Tickets werden **nach Task 3** aus der finalen Lücken-Liste erzeugt: pro bestätigter Lücke ein Ticket (exakte Datei + erwartetes Verhalten + Test). Der Loop pro Ticket: Codex-Implement → frischer Codex-Kritiker → Claude verifiziert externes Signal → Commit. User-sichtbare Text-Stellen: Codex setzt `"Platzhalter"` + meldet Datei:Zeile, Claude schreibt den finalen deutschen Text.

- [ ] **Step 1:** Aus finaler Lücken-Liste je Lücke ein Ticket (Scope + DoD) anlegen.
- [ ] **Step 2:** Tickets per Codex-Loop abarbeiten (TDD: erst fehlschlagender Test, dann Fix).
- [ ] **Step 3:** Nach jedem Ticket `cargo test --workspace` grün + Commit + Push.
- [ ] **Step 4:** Wenn Liste leer abgearbeitet: `cargo clippy --workspace --all-targets -- -D warnings` sauber.

---

### Task 5: Merge nach `main` (Checkpoint — irreversibel, hier anhalten)

> **Claude-Gate.** Vor dem Merge: `git log -1` + `git worktree list` (fremde HEAD-Bewegung?), `git branch --no-merged main`. Merge erst nach explizitem OK.

- [ ] **Step 1:** Finalen Stand verifizieren: Build + Clippy + Tests grün, Dienst läuft live aus `turnier-bot`.
- [ ] **Step 2:** CHANGELOG.md: Eintrag für den (user-sichtbaren) Anteil — hier v. a. „nichts kaputt, gleiche Funktion" → nur falls user-sichtbare Lücken geschlossen wurden; reiner Rename ist nicht CHANGELOG-würdig.
- [ ] **Step 3:** `rust-rewrite`/Feature-Branch nach `main` mergen, Dienst aus `main` bauen + neu starten, Live-Zustand erneut beweisen.
- [ ] **Step 4:** Branch-Hygiene: gemergte Branches löschen (`git branch -d`), `git worktree prune`.

---

## Self-Review

**Spec-Abdeckung (Phase 0):** §4 Phase-0-Zeile → Tasks 1–5 ✓; §10 Rename + Deploy-Folge → Tasks 1–2 ✓; §11 Audit-Methodik → Task 3 ✓; §15 Reihenfolge + Merge-Checkpoint → Task 5 ✓. Phase 1/2 bewusst NICHT hier (eigene Pläne nach Phase 0).

**Platzhalter-Scan:** Task 4 ist discovery-abhängig und als solcher deklariert (Tickets entstehen aus Task-3-Output) — kein verkappter „TODO", sondern der korrekte Umgang mit einem Audit-Ergebnis.

**Typ-/Namens-Konsistenz:** Rename-Tabelle in Task 1 (Paketname ↔ underscore-Import) deckt die kritische `tb-tournament → turnier-engine`-Falle ab; Task 2 nutzt denselben Binary-Namen `turnier-bot`.
