status: aktiv | 2026-10-01

# Register: ci-required-pr-gate

- Intent-Thread: `ed791981-80e9-45fb-9e2d-42df6008ce59` (ursprünglicher Auftrag, aktiver T3-Thread).
- Source-Branch/Inventur-Head: `ci/required-pr-gate-20260924` / `6c2a0ed79cdb0252f45d87cb632fcae608fd2e72`.
- Arbeits-Branch: `codex/luna-dispatch/deadlock-turniere/ci-required-pr-gate-20260924-6c2a0ed7`.
- Worktree: `/home/nathanael/.worktrees/luna-dispatch-deadlock-turniere-ci-required-pr-gate-20260924-6c2a0ed7`.
- T3-Modell: `gpt-6-luna`.
- Status: CodeQL-Folgekorrektur in Commit `2de8aae`; Status-Abnahme wartet auf unabhängige Intent-Freigabe.
- Änderung: Commit `076e401` gezielt übernommen; neue Code-Kommentare entfernt. Source-Worktree unverändert gelassen.
- Leichte Vertragsprobe: 6/6 jq-Fälle bestanden, keine ignoriert. Rust-Probe und Cargo-Läufe wegen Host-Resource-Hold nicht ausgeführt.
- Merge-Gate: `gate_hook.py --review` auf Arbeitsbranch gegen `main` meldete ALLOW mit NITs zu bestehender privater Abhängigkeit und bestehender UI-Änderung außerhalb des CodeQL-Folgepatches.
- Source-Worktree: fünf inventorisierten Pfade bleiben unverändert und wurden nicht übernommen. CodeQL-Diffs dort sind uncommittet; Ownership ist nicht bestätigt. Die drei Rust-Lobby/API-Pfade sind außerhalb des CI-Auftrags.
- PR-/Branchgruppe: kein passender offener PR; `BRANCHGRUPPEN.md` enthält keine Turnier-CI-Gruppe.
- Intent-Abnahme: T3-Thread konnte wegen fehlendem freien Modell in der Rolle `intent` nicht erstellt werden. Der Koordinator wurde über den Blocker informiert.
- Merge, Build, Deploy und Produktion: gesperrt durch Holds.
