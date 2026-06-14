# ADR 0002 — Laufzeit-geprüfte sqlx-Queries statt compile-time-Makros

## Status
Akzeptiert.

## Kontext
sqlx bietet compile-time-geprüfte Queries (`query!`/`query_as!`), die jedoch zur
Bauzeit entweder eine `DATABASE_URL` oder einen eingecheckten `.sqlx`-Offline-
Cache benötigen. Die Turnier-DB ist eine **geteilte, externe** SQLite-Datei, und
der Port wird in mehreren parallelen Wellen (mehrere Crates gleichzeitig)
gebaut.

## Entscheidung
Wir verwenden **laufzeit-geprüfte** Queries: `sqlx::query`/`query_as` mit
`#[derive(sqlx::FromRow)]`-Row-Structs. Die compile-time-Makros werden nicht
eingesetzt.

## Begründung
- Kein Koppeln des Builds an eine `DATABASE_URL` oder einen Offline-Cache, der
  bei paralleler Crate-Entwicklung ständig driftet und Build-Brüche verursacht.
- Kein Zwang, `sqlx-cli` für jede Query-Änderung neu auszuführen.
- Typsicherheit kommt aus den `FromRow`-Structs plus einer DB-gestützten
  Testsuite (echte SQLite, gegen das konsolidierte Schema).

## Konsequenzen
- SQL-Tippfehler fallen zur Laufzeit/im Test auf, nicht beim Kompilieren. Das
  wird durch Integrationstests pro Repository-Funktion aufgefangen.
- Die Migration bleibt compile-time eingebettet (`sqlx::migrate!`), das braucht
  keinen DB-Zugriff zum Bauen.
