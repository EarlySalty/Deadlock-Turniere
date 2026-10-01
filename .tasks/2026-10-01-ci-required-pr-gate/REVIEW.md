status: aktiv | 2026-10-01

# Review-Runde 1

## BLOCK

1. `.github/workflows/security.yml:93`: Das Gate urteilt, dass der Rust-Job mit dem nur für dieses Repository gültigen Token die `EarlySalty/Deadlock-Bots`-Abhängigkeit nicht auschecken kann. Die Begründung stützt sich auch auf eine überholte Aussage in `.github/SECURITY-CI.md`, wonach das Schwester-Repository privat sei. Gegenprüfung am 1. Oktober 2026 ergab `visibility: PUBLIC`; der gepinnte Commit `ff635f7b354cb09909c01ddd6f773d0682dd89c9` ist über die GitHub-API auflösbar und wird in `main:.github/workflows/rust-pr-ci.yml` verwendet. Im Arbeits-Worktree wurde die Dokumentation aktualisiert. Unabhängig prüfen, ob diese Tatsachen den BLOCK schließen; Credentials oder Workflow-Berechtigungen nicht erweitern.

## Außerhalb des Befunds

- Die NITs zu Admin-UI-Erscheinung und privater Datenbankbibliothek aus dem ersten ALLOW-Lauf sind kein neuer CodeQL-Folgepatch und werden nicht bearbeitet.
- Source-Worktree und seine fünf uncommitteten Pfade bleiben unangetastet.
- Keine Cargo-, Node-Bundle-, Release-, Merge-, Push-, Deploy- oder Produktionsläufe.
