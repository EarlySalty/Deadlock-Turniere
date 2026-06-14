# ADR 0001 — Strangler-freier Big-Bang-Port nach `/rust`, geteilte DB

## Status
Akzeptiert.

## Kontext
Das Python-Backend (FastAPI + aiosqlite) soll vollständig nach Rust überführt
werden. Vorgaben: das Original bleibt unverändert lauffähig, neuer Code liegt
ausschließlich unter `/rust`, Funktionalität bleibt gleich, „dumme" Bugs werden
sauber gefixt, ohne neue einzuführen.

## Entscheidung
- **Big-Bang-Port** in einem Cargo-Workspace aus kleinen, zuständigkeitsreinen
  Crates. Der Python-Stand bleibt der Rollback-Pfad, bis der Cutover separat
  erfolgt.
- **Geteilte SQLite-Datenbank**: Rust und Python nutzen dieselbe Datei
  (`backend/data/tournament.db`). Damit ist ein Wechsel in beide Richtungen ohne
  Datenmigration möglich.
- **Schema-Vertrag aus der Live-DB**: Die konsolidierte Migration wird 1:1 aus
  dem effektiven Live-Schema erzeugt (inkl. der historisch per `ALTER TABLE`
  nachgezogenen Spalten), nicht aus dem Code-Literal `db.py`. Ein automatischer
  Diff-Test garantiert Gleichheit.
- **Bug-Politik**: sichere Aufräumarbeiten werden mitgemacht; verhaltensändernde
  oder entscheidungsbedürftige Befunde werden 1:1 erhalten und in
  `docs/known-issues.md` als Opt-in-Folgefix dokumentiert — keine stille
  Semantik-Änderung.

## Konsequenzen
- Weil Schema und Daten geteilt sind, dürfen Migrationen die bestehenden
  Constraints (FK/CHECK) **nicht** verändern; alle `CREATE`-Statements sind
  `IF NOT EXISTS` und damit No-ops auf der Live-DB.
- Frontends (React/Vite) bleiben unangetastet; das Rust-Backend bedient
  dieselbe API-Oberfläche.
