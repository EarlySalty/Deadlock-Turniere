status: aktiv | 2026-10-01

# Auftrag: Fail-closed-CodeQL-Policy auf Branch prüfen und vervollständigen

## Ziel

Der ursprüngliche PR-Gate-Auftrag ist nicht vollständig in `main` enthalten, solange SARIF-Berichte nicht fail-closed gegen fehlende, unvollständige oder fehlerhafte CodeQL-Daten validiert werden. Nur die dafür nötigen, bereits committed Änderungen übernehmen und den Vertrag ohne schwere Hostläufe prüfen.

## Fundstellen

- Ausgangs-Head: `6c2a0ed79cdb0252f45d87cb632fcae608fd2e72`.
- Direkter Folgecommit: `076e4015243c2ef147c2b7c6c1ec554c040a066e`, CodeQL-Regelauflösung und Report-Prüfung.
- Zielpfade: `.github/ci/codeql-policy.jq`, `.github/ci/codeql-policy-probe.rs`, `.github/workflows/security.yml`, `.github/SECURITY-CI.md`.
- Aktuelles `main` enthält Security- und Funktionsprüfungen, aber keinen SARIF-Policy-Parser. Die Actions sind laut Projektanweisung kein Merge-Gate.

## Arbeit

1. Den direkten, thematisch passenden Folgecommit `076e401` auf diesem isolierten Branch übernehmen.
2. Uncommittete Dateien im Source-Worktree nicht übernehmen oder ändern. Insbesondere Änderungen an den Rust-Lobby-/API-Dateien bleiben außerhalb dieses Scopes.
3. Die CodeQL-Policy und den SARIF-Vertrag statisch prüfen. Keine Cargo-, Node-Bundle- oder Release-Läufe bis zur Host-Ressourcenfreigabe.
4. Den lokalen Merge-Gate-Review gegen `main` ausführen und unabhängige Intent-Abnahme organisieren.

## Fertig-Kriterium

Nur CodeQL-Berichtsvalidierung aus dem ursprünglichen Auftrag ist ergänzt; Source-Worktree bleibt unverändert. Merge-Gate meldet ALLOW, unabhängige Intent-Abnahme liegt vor. Kein Merge, Build, Deploy oder Produktionsschritt während des TokenDB-Holds.
