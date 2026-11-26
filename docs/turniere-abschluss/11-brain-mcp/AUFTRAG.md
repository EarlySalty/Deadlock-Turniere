# 11-brain-mcp – P2 · Turniere-Brain/MCP-Adapter

## Abhängigkeiten und Schreibbereich
Nach 03 oder später; Turniere MCP separat von UI. Kein gleichzeitiger Merge in Turniere.

## Vorhandene Arbeit / Quellen
[77d6f47 MCP-WIP](https://github.com/EarlySalty/Deadlock-Turniere/tree/77d6f470b83c183c2c6fbd40a6cbfc488c5f938a): docs/mcp-brain-adapter.md und rust/crates/turnier-api/src/{mcp,app,lib}.rs; [Charakterisierungstests](https://github.com/EarlySalty/Deadlock-Turniere/tree/4004074c4b4ff1e2fc7ae98004b5eaf61ef48e9a); [Deadlock-Brain](https://github.com/EarlySalty/Deadlock-Brain).
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Prüfe, ob die 531 Zeilen MCP-Adapter inzwischen durch Company-Brain integriert oder ersetzt wurden. Falls nicht, portiere nur den noch benötigten Vertrag auf aktuellen Main. Authority/Permissions, Auth, read/write scope, Gäste/PII, Datenminimierung und Fehlerverträge an aktuelle zentrale Architektur anbinden. Kein doppelter Adapter, keine ungeprüften mutierenden Tools, keine Token-Dateien.

## Abnahmekriterien
Vertragstests gegen aktuellen Brain/Broker, 401/403, Rollenentzug/Identität, fremde Events/IDs, keine Secrets/PII in Logs, sichere Fehler; falls bereits überholt, nachweisbarer No-op statt neuer Code. Gate.

## Aktueller Übergabestand und Ergänzung
Prüfe zuerst aktuellen Remote-Code und andere Brain-PRs; falls bereits integriert, belegter No-op. Keine Abhängigkeit von nicht veröffentlichten Company-Brain-Worktrees.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
