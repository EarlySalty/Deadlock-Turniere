# ADR 0002 -- SQLx-Query-Pruefung fuer zentrale Postgres

## Status
Akzeptiert. Ersetzt den frueheren SQLite-Grundsatz "runtime-checked statt
compile-time-Makros" fuer alle nach SP4 portierten PG-Persistenzpfade.

## Kontext
Das Rust-Turnierbackend wechselt in SP4 von der lokalen SQLite-Datei auf die
zentrale Postgres/TimescaleDB. Das fachliche Schema liegt in `turnier.*` und
wird durch `dl-central-db` migriert. Der lokale Workspace nutzt `sqlx` mit
Postgres-Features und einem eigenen Offline-Cache unter `rust/.sqlx`.

Der alte SQLite-Vertrag war bewusst laufzeitgeprueft, weil die geteilte Datei
extern war und parallele Portierungswellen keinen stabilen Compile-Oracle hatten.
Fuer die zentrale PG-DB ist das nicht mehr der Zielzustand: Schema, Typen und
Offline-Cache sind expliziter Teil des Build-Vertrags.

## Entscheidung
Statische produktive PG-Queries sollen bevorzugt mit `sqlx::query!` oder
`sqlx::query_as!` geschrieben werden. Der Build laeuft gegen den eingecheckten
Offline-Cache (`SQLX_OFFLINE=true`), nicht gegen eine Live-DB.

Runtime-gepruefte Queries (`sqlx::query`, `query_as`, `QueryBuilder<Postgres>`)
bleiben nur erlaubt, wenn die SQL-Struktur wirklich dynamisch ist oder ein
Crate noch innerhalb seines SP4-Portierungstickets umgestellt wird. Dynamische
Stellen muessen eine Whitelist fuer alle SQL-Identifier nutzen und ihre Werte
weiter als Bind-Parameter uebergeben.

## Erlaubte dynamische SQL-Stellen
- Variable `IN`-Listen: nur mit `QueryBuilder<Postgres>`; Werte werden gebunden,
  der Leerfall wird vom Aufrufer explizit behandelt.
- Reminder-Dedupe-Tabellen: nur die sortierte Whitelist
  `sent_match_reminders`, `sent_start_reminders`, `sent_tournament_reminders`
  ueber `turnier_db::dynamic_sql::ReminderDedupeTable`.
- Patch-Update-Builder: nur fuer partielle Updates mit statischer
  Spalten-Whitelist; Spaltennamen kommen nie aus Requestdaten, Werte bleiben
  gebunden.

## Typkonventionen
- Tabellen werden voll qualifiziert (`turnier."table"`, bei Cross-Schema-Lookups
  z. B. `core.*`/`voice.*`).
- `BIGINT` wird in Rust als `i64` gebunden; Discord-IDs bleiben an HTTP-/DTO-
  Grenzen Strings und werden mit `turnier_core::parse_discord_id` geprueft.
- `TIMESTAMPTZ` wird als `chrono::DateTime<Utc>` behandelt; neue Persistenzpfade
  nutzen `turnier_core::now_utc()`.
- `JSONB` wird als `serde_json::Value` behandelt; nullable JSONB-Felder nutzen
  die Mapper in `turnier_core::json`.
- `BOOLEAN` wird als `bool` gebunden, nicht als `0`/`1`.

## Konsequenzen
- SQLite-Stringzeiten, `?`-Binds, `INSERT OR ...`, `last_insert_rowid()` und
  `json_extract` sind in portierten PG-Pfaden nicht mehr erlaubt.
- Query-Fehler fuer statische SQL-Stellen sollen beim Build auffallen. Der
  Offline-Cache muss aktualisiert werden, wenn Query-Shape oder Schema wechseln.
- Dynamisches SQL bleibt reviewpflichtig und muss durch echte PG-Tests abgedeckt
  sein.
