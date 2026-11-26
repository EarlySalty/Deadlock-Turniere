# 03-draft-performance – P1 · Draft-Arena und Performance

## Abhängigkeiten und Schreibbereich
Nach 02; Turniere-Frontend/API, keine Änderungen an Caddy (04).

## Vorhandene Arbeit / Quellen
[PR #34](https://github.com/EarlySalty/Deadlock-Turniere/pull/34), feat/draft-arena-ux-20261123 @ 862d17c, [Performance-Dokumente](https://github.com/EarlySalty/Deadlock-Turniere/tree/862d17c7d3f64e54115bef23473deecd7f5cc8a5/docs/performance); sechs bekannte Konflikte gegen PUG (historische Liste in ../BESTANDSAUFNAHME.md).
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Portiere Draft-Arena, Hero-UX, vollständige öffentliche Roster, Route-Splitting, Polling und Query-Optimierungen aus PR34 konfliktbewusst auf aktuellen Main. Erhalte alle PUG-Serververträge und 02-UI. Löse die zwei historischen central_routes-Fehler vollständig, falls 01 sie nicht bereits nachweislich geschlossen hat: solo_balance_preview_apply_rollback_and_next_tournament_priority (42P01) und test_mode_seed_simulate_and_wipe_only_touch_test_data (500). Ursache des zweiten Fehlers tatsächlich isolieren. Keine vorschnellen Caches/Pool-Erhöhungen.

## Abnahmekriterien
Volle Rust- und Frontend-Suite; isolierter Draft mit beiden Team-Tokens/Seiten, Pick/Ban/Refresh/Reconnect, Observer und Hintergrundtab; Profil/Roster-Vertrag, Fehlerfälle; begrenzte Read-only Benchmarks auf Staging mit Ausgangsvergleich; Gate/Abnahme. PR #34 erst nach nachweislicher Übernahme schließen oder aktualisieren.

## Aktueller Übergabestand und Ergänzung
Agent 01 meldete central_routes am unveränderten PR34 862d17c mit Bots e65d3805 und frischer DB 15/15; beide früheren Fehler nicht reproduzierbar. Auf integriertem Stand wiederholen, keine künstlichen Fixes. PUG-Verträge priorisieren.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
