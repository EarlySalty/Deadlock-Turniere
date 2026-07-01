# Turnier-Bot Upgrade – WORKFLOW

---

## Neue Aufgabe (2026-07-01): SP4 T9 turnier-scheduler zentrale Postgres

### Ziel
- `turnier-scheduler` auf zentrale `turnier.*`-Postgres-Tabellen portieren.
- Reminder-Dedupe, Reminder-Offsets, Phasen-CAS, Punkte-Recompute und Test-Turnier-Skips mit PG-Typen absichern.
- Keine Secrets ausgeben, kein Commit/Push.

### Status (2026-07-01)
-> **Abgeschlossen fuer GPT-Worker** — Review durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md`, T9-Planabschnitt, Scheduler-Code, Scheduler-Tests und zentrales `0009_turnier.sql` gelesen.
- Ausgangsstand: Branch `central-postgres-sp4`; nur vorbestehende untracked Audit-Datei ausserhalb T9 sichtbar.
- Scheduler-Queries auf `turnier.*`, `$n`-Binds, PG-Bools, JSONB-Reminder-Offsets und `DateTime<Utc>` umgestellt; keine String-Zeitstempel/`datetime('now')` im Scheduler-Persistenzpfad.
- Reminder-Dedupe nutzt `ON CONFLICT DO NOTHING`; dynamische Reminder-Tabellennamen laufen weiter nur ueber `turnier_db::dynamic_sql::ReminderDedupeTable`.
- Statuswechsel behaelt den CAS `WHERE id = $n AND status = $n`; Audit schreibt `user_id BIGINT`, JSONB-Details und `created_at`.
- `completed`-Recompute bleibt in der Scheduler-Transaktion und nutzt die in T8 serialisierte Engine-Funktion ohne zusaetzliche Scheduler-Locks.
- Scheduler-Tests auf zentrale Wegwerf-PG-DB migriert und erweitert: Registration-/Start-/Match-Reminder-Dedupe, Test-Turnier-Skip, completed->points-Recompute und Recompute-Rollback.
- Verifikation: `cargo build -p turnier-scheduler` gruen.
- Verifikation: `cargo clippy -p turnier-scheduler --all-targets -- -D warnings` gruen.
- Verifikation: `cargo fmt --check -p turnier-scheduler` gruen.
- Verifikation: `../Deadlock-Bots/rust/scripts/central_test_db.sh bash -lc 'cd /home/naniadm/Documents/Deadlock-Turniere/rust && cargo test -p turnier-scheduler --features testing -- --include-ignored'` gruen.
- Kein Commit/Push, keine Dienste neu gestartet, keine Secret-Werte ausgegeben.

---

## Neue Aufgabe (2026-07-01): Rework SP4 T8 Tiebreaker + Concurrency-Guards

### Ziel
- Kritiker-Befunde in `turnier-engine::persist` beheben: deterministische Gruppen-Top-2, Check-in-Zeilenlock und globaler Points-Recompute-Lock.
- Bestehende uncommitted T8-Aenderungen erhalten; kein Commit/Push.

### Status (2026-07-01)
-> **Abgeschlossen fuer GPT-Worker** — Review durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md`, Git-Status und relevante Persistenz-/Testdateien gelesen.
- Ranking-Queries in `bracket.rs`/`groups.rs`/`double_elim.rs` per `rg` geprueft; einzige unvollstaendige Top-2-DB-Sortierung liegt in `bracket.rs`.
- `bracket.rs`: Gruppen-IDs und Gruppen-Top-2 deterministisch geordnet (`seeding_order, id` sowie `points DESC, wins DESC, id ASC, team_id ASC`); flache Qualifier-Sortierung nutzt explizit die urspruengliche Reihenfolge als weiteren Fallback.
- `checkin.rs`: initialer Tournament-Snapshot in `finalize_checkin` nimmt jetzt `FOR UPDATE` innerhalb der Transaktion.
- `points.rs`: globaler `player_points`-Recompute nimmt am Anfang der Transaktion einen benannten `pg_advisory_xact_lock`.
- Regressionstest ergaenzt: Punkte-/Wins-Gleichstand in einer Gruppe waehlt deterministisch die ersten `group_teams`-Zeilen.
- Verifikation: `cargo build -p turnier-engine` gruen.
- Verifikation: `cargo clippy -p turnier-engine --all-targets -- -D warnings` gruen.
- Verifikation: `cargo fmt --check -p turnier-engine` gruen.
- Verifikation: `../Deadlock-Bots/rust/scripts/central_test_db.sh bash -lc 'cd /home/naniadm/Documents/Deadlock-Turniere/rust && cargo test -p turnier-engine --features testing -- --include-ignored'` gruen (40 Tests).
- Zusatzcheck: `git diff --check -- rust/crates/turnier-engine/src/persist/bracket.rs rust/crates/turnier-engine/src/persist/checkin.rs rust/crates/turnier-engine/src/persist/points.rs rust/crates/turnier-engine/tests/engine_cross_seeding.rs WORKFLOW.md` gruen.
- Kein Commit/Push, keine Dienste neu gestartet, keine Secret-Werte ausgegeben.

---

## Neue Aufgabe (2026-07-01): Kritiker-Review SP4 T8 turnier-engine

### Ziel
- Uncommitted T8-Diff von `turnier-engine::persist` gegen `HEAD` vollstaendig lesen und auf echte Portierungsbugs pruefen.
- Fokus: Generator-ID-Reihenfolge, Points-Recompute, Check-in-Transaktionen/Concurrency, PG-Bools/JSONB, Double-Elim-/Advance-Propagation und Testabdeckung.
- Keine Codeaenderungen an Produktiv-/Testcode, kein Commit/Push.

### Status (2026-07-01)
→ **Abgeschlossen** — Review-Befunde an Claude zurueckzugeben

### Fortschritt
- `WORKFLOW.md`, Git-Status, Branch und Basis-Commit gelesen; bestehende uncommitted Aenderungen bleiben unangetastet.
- Vollstaendigen T8-Diff der `turnier-engine::persist`-Dateien und migrierte Engine-Tests gegen `HEAD` gelesen; zentrale PG-Schema-Definition `0009_turnier.sql` fuer Typ-/Constraint-Abgleich geprueft.
- Befund 1: `finalize_checkin` validiert den Snapshot nur einmal und schuetzt die danach gelesenen Check-ins/Signups/Teams unter Postgres nicht gegen parallele Mutationen; der spaete Status-CAS deckt nur Statuswechsel ab.
- Befund 2: `recalculate_player_points_in_tx` macht globales `DELETE` + per-Spieler-`INSERT` ohne PG-weite Serialisierung; parallele Recomputes koennen auf dem `player_points`-PK kollidieren und einen Completed-Statuswechsel zurueckrollen.
- Befund 3: Bracket-Qualifikation aus `group_teams` bricht Punkte/Wins-Gleichstaende ohne stabilen Tie-Breaker (`id`/Seed); Postgres darf dadurch andere Top-2/Seeding-Reihenfolgen liefern als die faktische SQLite-Rowid-Reihenfolge.
- Geprueft: sequenzielle `RETURNING id`-Nutzung in Bracket-/Group-/Double-Elim-Generatoren, PG-Bools/JSONB, Mini-Group-JSONB, Double-Elim-Propagation, Advance-Propagation und Testabdeckung.
- Verifikation: `git diff --check HEAD -- rust/crates/turnier-engine/src/persist rust/crates/turnier-engine/tests` gruen; keine Cargo-Tests neu ausgefuehrt, da Review statisch war und externe T8-Verifikation bereits vorlag.

---

## Neue Aufgabe (2026-07-01): SP4 T8 turnier-engine zentrale Postgres

### Ziel
- `turnier-engine::persist` von SQLite auf zentrale Postgres-Tabellen unter `turnier.*` portieren.
- Check-in-Finalisierung, Gruppen-/Bracket-/Mini-Group-/Double-Elim-Generatoren und Points-Recompute mit PG-Typen testen.
- Keine Secrets ausgeben, kein Commit/Push.

### Status (2026-07-01)
→ **Abgeschlossen fuer GPT-Worker** — Review durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md`, T8-Planabschnitt, zentrale `0009_turnier.sql`, bestehende Engine-Persistenz, T7-`turnier-match`-PG-Muster und Aufrufer per `rg` gelesen.
- `turnier-engine::persist` auf `Pool<Postgres>`/`Transaction<'_, Postgres>`, `turnier.*`, `$n`-Binds, PG-Bools, JSONB und `DateTime<Utc>` umgestellt; Discord-IDs werden an der DB-Grenze `String <-> i64` konvertiert.
- Generatoren nutzen `RETURNING id` und explizite Reihenfolgen statt SQLite-Rowid-Annahmen; `on_stream` ist echtes `BOOLEAN`.
- Engine-Tests auf zentrale Wegwerf-PG-DB (`turnier-db/testing`) migriert; Rollback-Abdeckung fuer Bracket-Rebuild, Points-Recompute und Check-in-Team-Bildung vorhanden.
- `turnier-engine` behaelt `sqlx/sqlite` im `persist`-Feature nur als Uebergang fuer `turnier-steam::bridge`; Engine-Persistenz selbst enthaelt keine SQLite-Pools/Queries mehr.
- Verifikation: `cargo build -p turnier-engine` gruen.
- Verifikation: `cargo clippy -p turnier-engine --all-targets -- -D warnings` gruen.
- Verifikation: `cargo fmt --check -p turnier-engine` gruen.
- Verifikation: `../Deadlock-Bots/rust/scripts/central_test_db.sh bash -lc 'cd /home/naniadm/Documents/Deadlock-Turniere/rust && cargo test -p turnier-engine --features testing -- --include-ignored'` gruen.
- Aufrufer-Check: `cargo build -p turnier-scheduler` gruen; `cargo build -p turnier-api -p turnier-scheduler` scheitert weiter an offenen T10/T11-`turnier-api`-SQLite-Executor-/Error-Mapping-Stellen.
- Kein Commit/Push, keine Dienste neu gestartet, keine Secret-Werte ausgegeben.

---

## Neue Aufgabe (2026-07-01): Rework SP4 T7 Mini-Group-Tiebreak

### Ziel
- `complete_mini_group_round_robin_pg` in `turnier-match` wieder auf die volle Mini-Group-Tiebreaker-Kette bringen.
- Reine Logik aus `turnier_engine::mini_groups` wiederverwenden, ohne die SQLite-Persistenzschicht in `turnier-match` mitzuziehen.
- Regressionstest fuer Head-to-Head/Point-Diff gegen Seed-Fallback ergaenzen.
- `advance_bracket_winner_pg`/`advance_in_tx_pg` kurz gegen alte Engine-Propagation pruefen.

### Status (2026-07-01)
→ **Abgeschlossen fuer GPT-Worker** — Rework umgesetzt, Review durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md`, Git-Status, `turnier_match::result`, `turnier_engine::mini_groups` und alte Engine-Persistenz fuer Mini-Groups/Advance gelesen.
- `turnier-engine` per `persist`-Default-Feature aufgeteilt; `turnier-match` bindet `turnier-engine` mit `default-features = false` und nutzt nur `mini_groups`.
- `complete_mini_group_round_robin_pg` laedt jetzt `team1_id`, `team2_id`, `winner_id`, `status`, `match_stats` und wertet ueber `aggregate` + `select_mini_group_winner` aus.
- Regressionstest ergaenzt: 4er-Mini-Group mit 2er-Wins-Tie, besserem Seed/Point-Diff fuer Team 1, aber Head-to-Head-Sieg fuer Team 2.
- `advance_bracket_winner_pg`/`advance_in_tx_pg` gegen `turnier_engine::persist::advance` gegengelesen: Source-Propagation, Loser-Ziel, Grand-Final-Reset und Legacy-Fallback sind paritaer vorhanden.
- Verifikation: `cargo build -p turnier-match` gruen.
- Verifikation: `cargo clippy -p turnier-match --all-targets --all-features -- -D warnings` gruen.
- Verifikation: `cargo fmt --check -p turnier-match` gruen.
- Verifikation: `../Deadlock-Bots/rust/scripts/central_test_db.sh bash -lc 'cd /home/naniadm/Documents/Deadlock-Turniere/rust && cargo test -p turnier-match --features testing -- --include-ignored'` gruen.
- Kein Commit/Push, keine Dienste neu gestartet, keine Secret-Werte ausgegeben.

---

## Neue Aufgabe (2026-07-01): Kritiker-Review SP4 T7 turnier-match

### Ziel
- Uncommitted T7-Diff von `turnier-match` gegen `HEAD` vollstaendig lesen und aktiv auf echte Portierungsbugs pruefen.
- Fokus: `RETURNING id`, Zeitstempel/TIMESTAMPTZ, JSONB-Binds, PG-Bools, Bracket/Series-Zustandslogik, Dynamic-SQL-Whitelist und Testabdeckung.
- Keine Codeaenderungen an Produktiv-/Testcode, kein Commit/Push.

### Status (2026-07-01)
→ **Abgeschlossen** — Review-Befund an Claude zurückzugeben

### Fortschritt
- `WORKFLOW.md`, `git status`, Basis-Commit und T7-Diffstat gelesen; bestehende uncommitted Aenderungen bleiben unangetastet.
- Vollstaendigen T7-Diff gegen `HEAD` und relevante Vorversionen/Schema gelesen: `series`, `result`, `repo`, `modes`, `auto_lobby`, `lobby`, `steam_bridge`, Tests und Plan.
- Befund: `complete_mini_group_round_robin_pg` bildet die bisherige Engine-Tiebreaker-Kette nicht ab und entscheidet Gleichstaende nur nach Wins/Seed/Team-ID.
- Geprueft ohne Produktiv-/Testcodeaenderung: `RETURNING id`, TIMESTAMPTZ-Binds, JSONB-Binds/Casts, PG-Bools, Dynamic-SQL-Whitelist und Testabdeckung.
- Verifikation: `git diff --check HEAD -- rust/crates/turnier-match` gruen; keine Cargo-Tests neu ausgefuehrt, da externe T7-Verifikation bereits vorlag und Review statisch war.

---

## Neue Aufgabe (2026-07-01): SP4 Turniere T7 turnier-match

### Ziel
- T7 (`turnier-match`) aus `backend/docs/plans/2026-07-01-sp4-turniere-central-db.md` auf zentrale Postgres-Tabellen unter `turnier.*` portieren.
- MatchManager, Ergebnisverarbeitung, Series, Modes, Auto-Lobby und Repos ohne SQLite-Fachqueries betreiben.
- Tests/Build/Clippy/Fmt fuer `turnier-match` ausfuehren; keine Secrets ausgeben, kein Commit/Push.

### Status (2026-07-01)
→ **Abgeschlossen fuer GPT-Worker** — T7 umgesetzt, Review durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md`, T7-Planabschnitt, Git-Status und `turnier-match`-SQLite-Stellen gelesen.
- Ausgangsstand: Branch `central-postgres-sp4`, nur untracked Audit-Datei ausserhalb T7 sichtbar; kein Commit/Push.
- `turnier-match` von der noch SQLite-typisierten `turnier-engine`-Persistenz entkoppelt: Bracket-Gewinner-Propagation und Mini-Group-Abschluss lokal in PG umgesetzt, damit T7 unabhaengig von T8 baubar ist.
- Fachqueries in `series`, `result`, `repo`, `modes` und `auto_lobby` auf `turnier.*`, `$n`-Binds, PG-Bools, `TIMESTAMPTZ` und JSONB umgestellt; variable Team-`IN`-Listen nutzen die Dynamic-SQL-Whitelist-Helfer.
- Tests von In-Memory-SQLite auf zentrale Wegwerf-PG-DB (`turnier-db/testing`) umgestellt; Caster-Fallback und JSONB-Stats abgedeckt.
- Verifikation: `cargo build -p turnier-match` gruen.
- Verifikation: `cargo test -p turnier-match --features testing -- --include-ignored` via `Deadlock-Bots/rust/scripts/central_test_db.sh` gruen; direkter Lauf ohne Test-DSN scheiterte erwartungsgemaess an fehlender DSN-Umgebung.
- Verifikation: `cargo clippy -p turnier-match --all-targets --all-features -- -D warnings` gruen.
- Verifikation: `cargo fmt --check -p turnier-match` gruen; dafuer scoped `cargo fmt -p turnier-match` ausgefuehrt.
- Kein Commit/Push, keine Dienste neu gestartet, keine Secret-Werte ausgegeben.

---

## Neue Aufgabe (2026-07-01): SP4 Turniere Welle1 T3-T5 Umsetzung

### Ziel
- T3 (`turnier-auth` + `turnier-discord`), T4 (`turnier-automatik`) und T5 (`turnier-draft`) aus `backend/docs/plans/2026-07-01-sp4-turniere-central-db.md` auf `central-postgres-sp4` umsetzen.
- SQLite-Queries in diesen Crates auf zentrale Postgres-Tabellen unter `turnier.*` portieren.
- Tests/Build/Clippy/Fmt fuer die vier Crates ausfuehren; keine Secrets ausgeben, kein Commit/Push.

### Status (2026-07-01)
→ **Abgeschlossen fuer GPT-Worker** — T3-T5 umgesetzt, Review durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md`, Git-Status und SP4-Plan gelesen.
- Scope bestaetigt: T3 Sessions/Discord-Tasks/Notifier-Optout, T4 Automatik-Presets/Proposals/Votes/Feedback/Optout/Signals, T5 Draft-Repo/CAS.
- Aktueller Arbeitsbaum war bereits schmutzig: vorhandene `WORKFLOW.md`-Aenderung und untracked Audit-Datei bleiben erhalten.
- T3 umgesetzt: `turnier.sessions` mit `BIGINT`-Discord-ID und `TIMESTAMPTZ`, `turnier.discord_tasks` mit JSONB-Payloads/`RETURNING id`, Notifier-Lookups via PG-Bools und BIGINT-ID-Konvertierung.
- T4 umgesetzt: Automatik-Tabellen auf `turnier.*`, JSONB fuer Offset-/Config-/Feedback-Felder, explizite PG-Zeitstempel, `ON CONFLICT DO NOTHING`, TEXT-Enum-Mapping fuer zentrale PG-Textspalten.
- T5 umgesetzt: Draft-Repo auf PG-Transaktionen, `RETURNING id` fuer Action-Materialisierung, `is_admin_forced BOOLEAN`, `taken_at TIMESTAMPTZ`, CAS-Konfliktgrenze ohne `BEGIN IMMEDIATE`.
- Tests auf echte zentrale Wegwerf-PG-DBs umgestellt: Sessions, Discord-Tasks, DM-Optout, Approval-Gate, Vote-Upsert, JSONB-Roundtrip, Optout, Signals, doppelte Hero-Auswahl, CAS, Admin-forced und parallele Aktion.
- Verifikation via Infisical-geladener `DEADLOCK_CENTRAL_DSN` als `DATABASE_URL` ohne Secret-Ausgabe: `cargo build -p turnier-auth -p turnier-discord -p turnier-automatik -p turnier-draft` gruen.
- Verifikation: `cargo test -p turnier-auth -p turnier-discord -p turnier-automatik -p turnier-draft --features testing -- --include-ignored` gruen.
- Verifikation: `cargo clippy -p turnier-auth -p turnier-discord -p turnier-automatik -p turnier-draft --all-targets --all-features -- -D warnings` gruen.
- Verifikation: `cargo fmt --check -p turnier-auth -p turnier-discord -p turnier-automatik -p turnier-draft` gruen; dafuer scoped `cargo fmt` ueber die vier Ziel-Crates ausgefuehrt.
- Kein Commit/Push, keine Dienste neu gestartet, keine Secret-Werte ausgegeben.

---

## Neue Aufgabe (2026-07-01): Review SP4 Turniere T0-T2 Kritiker

### Ziel
- Uncommitted Diff auf `central-postgres-sp4` gegen `backend/docs/plans/2026-07-01-sp4-turniere-central-db.md`, Tickets T0-T2, kritisch prüfen.
- Keine Codeänderungen, kein Commit/Push, keine Live-Daten anfassen.

### Status (2026-07-01)
→ **Abgeschlossen** — Review-Befunde an Claude zurückzugeben

### Fortschritt
- `WORKFLOW.md`, `git status --short --branch` und `git diff --stat` gelesen.
- Review-Fokus bestätigt: SQLite-Entfernung, Startpfad-Migrationen, ID-Helfer, JSONB-Mapping, Dynamic-SQL-Whitelist, DSN-Ausgabe, Tests und Doku/ADR.
- Gezielter `cargo check -p turnier-bot` ohne DB-Zugriff ausgeführt: Build bricht bereits wegen fehlendem `sqlx::sqlite` in `turnier-steam`/`turnier-draft`.
- Blocker notiert: Workspace-SQLx-Features entfernen SQLite global, produktiver `turnier-bot`-Startpfad nutzt weiter `connect_str(&config.database_path, ...)`, Dynamic-SQL-Whitelist ist nicht in die bestehenden Builder/Reminder-Pfade integriert.
- Kein Commit/Push, keine Dienste neu gestartet, keine Live-Daten gelesen oder geschrieben.

---

## Neue Aufgabe (2026-07-01): SP4 Turniere T2 Typ-/Query-Konventionen

### Ziel
- T2 aus `backend/docs/plans/2026-07-01-sp4-turniere-central-db.md` umsetzen.
- Zentrale Helfer fuer Discord-ID-Casts, UTC-Zeit, nullable JSONB und erlaubte dynamische SQL-Muster bereitstellen.
- ADR/DB-Vertrag/Architektur auf zentrale PG-Konventionen aktualisieren.
- Kein Commit/Push; Änderungen bleiben uncommitted fuer Claude-Review.

### Status (2026-07-01)
→ **Abgeschlossen fuer GPT-Worker** — T2 umgesetzt, Review durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md`, SP4-Plan, T1-Stand, zentrale `0009_turnier.sql` und Rust-Doku gelesen.
- Helfer umgesetzt: `parse_discord_id`, `discord_id_to_string`, `now_utc`, nullable JSONB-Mapper und `turnier-db::dynamic_sql`-Whitelist fuer variable IN-Listen, Reminder-Dedupe-Tabellen und Patch-Update-Builder.
- ADR 0002, `rust/docs/db-contract.md` und `rust/docs/architecture.md` auf zentrale PG-Konventionen aktualisiert: static SQL compile-checked bevorzugt, dynamisches SQL nur begruendet/whitelisted.
- Verifikation: direkter Testlauf ohne Test-DSN scheiterte erwartungsgemaess an fehlender Test-DSN-Umgebung; danach `cargo test -p turnier-core -p turnier-db --features testing` via `Deadlock-Bots/rust/scripts/central_test_db.sh` gruen.
- Verifikation: `cargo fmt --check -p turnier-core -p turnier-db` gruen.

---

## Neue Aufgabe (2026-07-01): SP4 Turniere T0+T1 zentrale Postgres-Foundation

### Ziel
- Nur T0 und T1 aus `backend/docs/plans/2026-07-01-sp4-turniere-central-db.md` umsetzen.
- T0: Boundary-Entscheidungen dokumentieren; keine Live-Daten schreiben, keine Secrets ausgeben.
- T1: `turnier-db` von SQLite-Fassade auf zentrale `sqlx::PgPool`-Foundation mit `dl-central-db` umstellen.
- Kein Commit/Push; Änderungen bleiben uncommitted fuer Claude-Review.

### Status (2026-07-01)
→ **Abgeschlossen fuer GPT-Worker** — T0/T1 umgesetzt, Review durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md` und SP4-Plan gelesen; vorhandene uncommitted `WORKFLOW.md`-Aenderung und untracked Audit-Datei nicht zurueckgesetzt.
- T0 Live-Codepfad geprueft: `deadlock-turniere.service` startet `scripts/run_turniere_backend_rust.sh` und damit `rust/target/release/turnier-bot`.
- T0 Boundary-Entscheidungen: `dl-central-db` als relative Path-Dependency `../../Deadlock-Bots/rust/crates/dl-central-db`; SQLx-Cache-Ort `rust/.sqlx`; Steam-Bridge-SQLite in T6 auf zentrale `core/voice`-Tabellen umstellen.
- T0 Schema-Oracle read-only geprueft: `turnier` vorhanden, 37/37 erwartete `turnier`-Tabellen vorhanden, `core` und `voice` vorhanden; keine DSN ausgegeben.
- T1 umgesetzt: `turnier-db::Pool = sqlx::PgPool`, `connect_central()`, `test_pool()` hinter Feature `testing`, `CentralDbError`-Mapping und `run_migrations()` als bewusster PG-No-op.
- T1 Tests ersetzt: lokale SQLite-Migrationstests raus, PG-Schema-Smokes gegen `dl_central_db::testing::test_pool()` rein.
- Verifikation: `cargo build -p turnier-db --features testing`, `SQLX_OFFLINE=true cargo build -p turnier-db --features testing`, `cargo test -p turnier-db --features testing -- --include-ignored`, `cargo clippy -p turnier-db --all-targets --all-features -- -D warnings`, `cargo fmt --check -p turnier-db` gruen.
- Hinweis: ungescoptes `cargo fmt --check` scheitert an vorhandenen Format-Diffs ausserhalb T0/T1; nicht auto-formatiert, um Scope nicht zu erweitern.

---

## Neue Aufgabe (2026-07-01): SP4 Turniere zentrale Postgres-Planung

### Ziel
- Nur Plan schreiben fuer die Umstellung von lokaler SQLite-`tournament.db` auf zentrale Postgres/TimescaleDB (`turnier.*`).
- Referenzen aus `Deadlock-Bots` lesen: zentrale Migrationen, Ledger/Data-Landscape, SP1-Methodik.
- Keine Live-Daten anfassen, keine Dienste neu starten, kein Push/Commit durch GPT-Worker.

### Status (2026-07-01)
→ **Abgeschlossen fuer GPT-Worker** — Plan geschrieben, Review/Commit durch Claude ausstehend

### Fortschritt
- `WORKFLOW.md` gelesen; vorhandener Arbeitsbaum hatte bereits eine untracked Audit-Datei, nicht angefasst.
- Zentrales Schema `turnier.*` aus `Deadlock-Bots/rust/crates/dl-central-db/migrations/0009_turnier.sql` gelesen; `0001`-`0011` per Suche auf Schema-/Cross-FK-Kontext geprueft.
- Mapping/Ledger gelesen: `rust/docs/_work/sp1/data-landscape.md` und `crates/dl-central-etl/ledger/tournament/turnier.toml`; `_sqlx_migrations` ist Meta, fachliche Turnier-Tabellen liegen in `turnier`.
- Bestehende lokale Rust-/Python-Persistenzstellen inventarisiert: Rust nutzt `turnier-db::SqlitePool`, Python/Legacy nutzt weiter `aiosqlite`; Steam-Bridge-SQLite als eigenes Risiko notiert.
- Neuer Plan angelegt: `backend/docs/plans/2026-07-01-sp4-turniere-central-db.md`.

---

## Neue Aufgabe (2026-06-30): Fix Enforcement-Gaps Turniere

### Ziel
- HIGH 1: Proposal darf nur nach echter Caster-Freigabe auf `approved` wechseln.
- HIGH 2: Live-DM-Sendepfade muessen `tournament_dm_optout` vor dem Versand respektieren.
- Kein Commit/Push, kein Deploy.

### Status (2026-06-30)
→ **Abgeschlossen** — beide HIGH-Fixes umgesetzt, Build/Clippy/Tests gruen

### Fortschritt
- Audit `rust/docs/audit/2026-06-30-enforcement-gap-audit.md` vollstaendig gelesen.
- Root-Cause bestaetigt: `proposals::apply_event()` prueft beim `Approve` keine gespeicherte Freigabe; `record_proposal_vote()` nutzt `caster_id` aus dem Body.
- Root-Cause bestaetigt: `DiscordNotifier::notify_users()` und `notify_casters_match_created()` senden ohne Lookup in `tournament_dm_optout`.
- Rote Tests belegt: Approval ohne Vote wurde akzeptiert, Vote-Spoofing moeglich, DM-Opt-out in beiden Notifier-Pfaden ignoriert.
- Fix: `apply_event()` verlangt vor `approved` mindestens einen Approval-Vote; Proposal-Votes nutzen die authentifizierte Actor-ID und verlangen die Caster-Rolle.
- Fix: `DiscordNotifier` laedt `tournament_dm_optout` zentral und skippt Opt-out-User vor `send_dm_inner()` in `notify_users()` und `notify_casters_match_created()`.
- Platzhalter gesetzt fuer neue user-sichtbare Fehlertexte: Caster-Rollenfehler und Missing-Approval-HTTP-Mapping.
- Verifikation: `cargo build --release -p turnier-bot --bin turnier-bot`, `cargo clippy -p turnier-automatik -p turnier-discord --all-targets -- -D warnings`, `cargo clippy -p turnier-api --all-targets -- -D warnings`, `cargo test -p turnier-automatik -p turnier-api -p turnier-discord`, `cargo test --workspace` gruen.

---

## Neue Aufgabe (2026-06-29): Phase0 Task4c Rework Offset-Null + Test-Haertung

### Ziel
- Kritiker-Befund fuer `reminder_offsets` und `start_reminder_offsets` erst gegen Python pruefen, dann Rust-Paritaet herstellen.
- Scheduler-Rollback-Test um Sentinel-Pruefung haerten; optional Admin-Rebuild-Integrationstest nur bei praktikablem Scope.
- Kein Commit/Push, kein Service-Restart, kein Release-Build.

### Status (2026-06-29)
→ **Abgeschlossen** — Rework umgesetzt, Debug-Build/Clippy/Tests gruen

### Fortschritt
- WORKFLOW gelesen; vorhandener Arbeitsbaum enthaelt bereits Phase0-Task4-Aenderungen.
- Python-Pruefung: explizites JSON-`null` bleibt in `TournamentUpdate` fuer beide Offset-Felder `None`, wird durch `model_dump(exclude_unset=True)` als gesetztes Feld behalten und im Handler nicht serialisiert; damit schreibt Python SQL-NULL.
- Rust-Rework: `reminder_offsets` und `start_reminder_offsets` in `TournamentUpdate` auf `Patch<Vec<i64>>` umgestellt; Admin-Update schreibt bei `Patch::Null` SQL-NULL und bereinigt Offsets nur im Value-Fall.
- Tests: DTO-Missing/Null/Value fuer beide Offset-Felder ergaenzt; Scheduler-Rollback-Test prueft die `player_points`-Sentinel-Zeile nach fehlgeschlagenem Recompute.
- Optionaler Admin-Rebuild-Integrationstest nicht ergaenzt: keine vorhandene `turnier-api/tests`-Struktur und kein Router-Testmuster; bestehender Engine-Tx-Test deckt die direkte Rebuild-Transaktionsgrenze ab.
- Verifikation: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` gruen.

---

## Neue Aufgabe (2026-06-29): Phase0 Task4 Paritäts-Fixes Py→Rust

### Ziel
- Drei Rust-Paritätsfixes aus `rust/docs/audit/2026-06-29/REVIEW_B.md`: explizites JSON-`null` bei Turnier-Updates, atomarer `group_phase -> bracket_only`-Rebuild, atomarer Scheduler-`completed`+Punkte-Recompute.
- Zwei bewusst zurückgestellte Punkte in `rust/docs/known-issues.md` dokumentieren.
- Kein Commit/Push, kein Service-Restart, kein Release-Build.

### Status (2026-06-29)
→ **Abgeschlossen** — Implementierung und Abschluss-Verifikation grün

### Fortschritt
- Audit, Schema und betroffene Rust-Dateien gelesen.
- `TournamentUpdate` nutzt für nullable Felder `Patch<T>` mit Missing/Null/Value-Semantik; DTO-Tests ergänzt.
- `generate_bracket_in_tx` ergänzt und Admin-Rebuild in die bestehende Update-Transaktion gezogen; Rollback-Test ergänzt.
- `recalculate_player_points_in_tx` ergänzt und Scheduler-`completed`-Pfad transaktional gemacht; Positiv- und Rollback-Test ergänzt.
- Known-Issues für Avatar-Header und generische Steam-Ops-Timeouttexte ergänzt; behobene A1/A3-Doku aktualisiert.
- Verifikation: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` grün (225 passed, 0 failed).

---

## Neue Aufgabe (2026-06-29): Phase 0 Task 1 Crate-Rename tb-* -> turnier-*

### Ziel
- Mechanischer, verhaltensneutraler Rename der Rust-Cargo-Crates unter `rust/`: `tb-*` -> `turnier-*`.
- Keine Funktionsänderung, kein Release-Build, kein Commit/Push.

### Status (2026-06-29)
→ **Abgeschlossen** — Rename umgesetzt, Debug-Build/Clippy/Tests grün

### Fortschritt
- Plan gelesen: `docs/plans/2026-06-29-turnier-automatik-phase0.md`, Task 1.
- Workflow gelesen; Arbeitsbaum vor Start geprüft.
- Referenz-Build vor Rename: `cargo build --workspace` grün.
- Crates unter `rust/crates/` per `git mv` umbenannt: `tb-*` -> `turnier-*`, inklusive `tb-tournament` -> `turnier-engine`.
- Cargo-Manifeste, Rust-Importe, Tests und Rust-Doku-Verweise mechanisch auf neue Namen angepasst; Binary-Name ist `turnier-bot`.
- Verifikation nach Rename: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` grün.
- Rückstands-Grep für `*.rs`/`*.toml` leer; zusätzlicher breiter Rust-Scan ohne `target` ebenfalls leer.

---

## Neue Aufgabe (2026-04-25, Teil 2): Tournament-Caster, Live/Archiv-Trennung, Test-Modus

### Ziel
- **Caster pro Turnier statt pro Match**: Pool kommt bereits aus Discord-Rolle `1495154811799077067` (`DISCORD_CASTER_ROLE_ID`), Zuweisung wird auf Turnier-Ebene gehoben. Wer fürs Turnier eingetragen ist, wird automatisch in jedes Match übernommen.
- **Admin-Übersicht entwirren**: Live und Archiv klar getrennt, abgeschlossene Turniere mischen sich nicht mehr in die aktive Verwaltung. Read-Only-Detailansicht für archivierte Turniere.
- **Test-Modus**: `is_test`-Flag pro Turnier; Endpoints zum Erstellen von Test-Usern, Test-Turnieren, Match-Ergebnis-Simulation und Wipe. UI-Section, in der Test-Daten generiert/gelöscht werden.

### Status (2026-04-25)
→ **In Arbeit** — Backend-Implementierung durch GPT-Worker läuft

### Scope (Backend, an GPT delegiert)
1. **`backend/db.py`**:
   - Neue Tabelle `tournament_casters (tournament_id, discord_id, assigned_at, assigned_by, UNIQUE(tournament_id, discord_id))` + Migration in `_ensure_schema_upgrades`.
   - Neue Spalte `tournaments.is_test INTEGER NOT NULL DEFAULT 0` + Migration.
2. **Caster-Layer (`backend/tournament/admin_routes.py` + `backend/match/manager.py`)**:
   - Neue Endpoints: `GET/POST /admin/tournaments/{id}/casters`, `DELETE /admin/tournaments/{id}/casters/{discord_id}`.
   - Per-Match-Endpoints (`/tournaments/{id}/matches/{match_id}/casters`) als Read-Only erhalten — sie geben zukünftig die Tournament-Caster zurück (Backwards-Compat fürs Frontend übergangsweise).
   - `_load_match_casters(match_type, match_id)` in `manager.py`: Lookup tournament_id über match → liest aus `tournament_casters`. Falls Tournament-Liste leer, fallback auf alte `match_casters`-Einträge (Backwards-Compat).
   - Audit-Log-Einträge `tournament_caster_assign` / `tournament_caster_remove`.
3. **Test-Modus (`backend/admin/test_mode.py` neu, eingebunden in `main.py`)**:
   - `is_test` in `TournamentCreate/Update/TournamentDetailPublic` Pydantic-Modellen.
   - Neue Module-Routen `POST /admin/test/users` (body `{count}`), `GET /admin/test/users`, `DELETE /admin/test/users` — erzeugt/löscht User mit Discord-IDs `test_<6-stellig>` in `user_profiles` + simulierter `rank_cache`-Eintrag.
   - `POST /admin/test/tournaments` (body `{name, team_size, num_teams, mode, game_mode}`) — erstellt komplettes Test-Turnier mit `is_test=1`, Teams + Members aus Test-User-Pool, Captain-Naming, optional auto-checkin → Status auf `bracket` setzen für sofortigen Test.
   - `POST /admin/test/tournaments/{id}/simulate-round` — würfelt Ergebnisse für alle offenen Matches der aktuellen Runde, ruft `apply_bracket_match_result` / `apply_group_match_result`.
   - `DELETE /admin/test/wipe` — löscht alle Test-User + alle `is_test=1` Turniere + abhängige Daten (signups, teams, matches, mini_groups …).
   - Endpoints alle `require_owner` (oder `require_mod` wenn keine Owner-Trennung existiert).
4. **Tests**: Smoke-Test pro neuem Endpoint via `httpx.AsyncClient` (falls pytest-Setup vorhanden).

### Fortschritt GPT-Worker Backend (2026-04-25)
- Relevante Backend-Stellen geprüft: `db.py`, `tournament/models.py`, `tournament/admin_routes.py`, `tournament/routes.py`, `tournament/engine.py`, `match/manager.py`, `match/auto_lobby.py`, `notifications/discord_notifier.py`, `main.py`, `auth/permissions.py`
- Architekturentscheidung bestätigt: kein `require_owner` vorhanden, daher neuer Test-Mode-Router mit `require_mod`
- Umsetzung läuft in drei Blöcken: Schema/`is_test`-Propagation, Tournament-Caster-Layer mit Legacy-Fallback, neuer `admin/test_mode.py`-Router inkl. Simulationslogik
- Implementiert: DB-Migration für `tournament_casters` + `tournaments.is_test`, neue Turnier-Caster-Endpoints, Legacy-Fallback in `match.manager`, neue `backend/admin/test_mode.py`-Routen, Test-Mode-Skips für Auto-Lobby/Discord-Benachrichtigungen
- Verifikation gelaufen: `.venv/bin/python -m py_compile $(git ls-files 'backend/**/*.py')`, zusätzlicher Compile für neue untracked Dateien `backend/admin/test_mode.py` + `backend/__init__.py`, `.venv/bin/python -c "from backend import main; print('imports ok')"`, `.venv/bin/python -c "import asyncio; from backend.db import init_db; asyncio.run(init_db())"`
- Umgebungshinweis: `pytest` ist in `.venv` aktuell nicht verfügbar; `init_db()` hat ein lokales `data/`-Artefakt gemäß aktueller Config erzeugt

### Scope (Frontend, von Claude)
1. **`frontend/src/types/tournament.ts`**: `is_test`, `tournamentCasters`-Modelle.
2. **`frontend/src/api/client.ts` + `hooks/useTournament.ts`**: `useTournamentCasters`, `useAssignTournamentCaster`, `useRemoveTournamentCaster`, `useTestUsers`, `useCreateTestUsers`, `useWipeTestUsers`, `useCreateTestTournament`, `useSimulateRound`, `useWipeTestData`.
3. **`pages/Admin.tsx`**: Top-Level-Mode-State `'live' | 'archive' | 'test'` als Tabs oben; Sidebar zeigt nur passende Liste; Archiv-Detail-View mit Banner "Read-Only" und reduzierten Phase-Tabs; Test-Tab mit Test-Mode-Tools-Panel.
4. **Neue Komponenten**:
   - `TournamentCasterPanel` — wird in der Voice/Caster-Phase angezeigt (Live + Archiv), Multi-Select aus Discord-Rollen-Pool.
   - `ArchivedTournamentView` — gedimmter Read-Only-Container für archivierte Turniere.
   - `TestModePanel` — Buttons: Seed N Users, Erstelle Test-Turnier, Simuliere Runde, Wipe.
5. **`MatchAdminPanel.tsx`**: Per-Match-`CasterPanel` entfernen oder als Read-Only-Anzeige der Tournament-Caster lassen (Default: entfernen).

### Verifikation
- `cd backend && .venv/bin/python -m py_compile $(git ls-files 'backend/**/*.py')`
- `cd frontend && node_modules/.bin/tsc --noEmit -p tsconfig.app.json && node_modules/.bin/vite build`
- Bot-Restart `systemctl --user restart deadlock-turniere.service`
- Manueller Test: Test-Turnier mit 5 Test-Usern erstellen → Mini-RR + Caster-Auswahl + Round-Simulation prüfen.

---

## Neue Aufgabe (2026-05-09): Turnier-Engine - Auto-Gruppen, Cross-Seed, Double Elim

### Ziel
- Group Stage erst ab **16 Teams**
- Gruppen-Anzahl automatisch aus Team-Anzahl ableiten
- Gruppen-Qualifier im Bracket per Cross-Seed statt flacher Punktesortierung paaren
- Double-Elimination im Backend tatsächlich erzeugen und Loser-Routing persistieren

### Status (2026-05-09)
→ **Abgeschlossen** — Engine/DB/Tests + Frontend-Bracket-Visualisierung umgesetzt

### Fortschritt
- Relevante Stellen geprüft: `backend/tournament/engine.py`, `backend/db.py`, `backend/tournament/admin_routes.py`, `backend/tournament/models.py`, `backend/match/result_processor.py`, bestehende Tests in `backend/tests/`
- Schema ergänzt: `bracket_matches.loser_to_match_id` und `bracket_matches.loser_to_slot` für Neuinstallationen und idempotente Migrationen
- Engine erweitert: Auto-Gruppen-Helfer, Auto-Default für `generate_groups`, DB-gesteuertes `bracket_format`, Cross-Seeding aus Gruppenphase, Double-Elimination-Builder inkl. GF-Reset-Logik
- Neue Tests ergänzt: `test_engine_auto_num_groups.py`, `test_engine_cross_seeding.py`, `test_engine_double_elim.py`
- Verifikation: gezielter Lauf der drei neuen Tests erfolgreich; vollständiger `backend/tests/`-Lauf ausgeführt, dabei zusätzliche bestehende fachfremde Fehler in `profile`, `security` und `match_manager` sichtbar

---

## Neue Aufgabe (2026-04-25): Mini-RR-Bracket, Captain-Teamnamen, Game-Modes, Auto-Lobby

### Ziel
- BYE-freies Single-Elimination-Bracket via rekursiven Mini-Round-Robin-Slots
- Auto-generierte Teams nach Captain benennen statt Phonetic-Fallback
- Turnierweite Game-Modes mit Hero-Zuteilung und Lobby-Ankündigung
- Auto-Lobby-Erstellung nach Bracket-/Gruppen-Generierung und beim Advance
- API/Modelle/Tests für Mini-Groups und neue Match-Metadaten

### Status (2026-04-25)
→ **In Arbeit** — Backend-Refactor läuft

### Fortschritt
- Plan-Datei `/home/naniadm/.claude/plans/wir-m-ssen-den-turnier-sprightly-lemon.md` vollständig gelesen
- Relevante Backend-Dateien geprüft: `db.py`, `engine.py`, `models.py`, `manager.py`, `result_processor.py`, `scheduler.py`, `admin_routes.py`, `routes.py`, `discord_notifier.py`
- Umsetzung startet mit Schema-/Modell-Änderungen, danach Engine/Mini-Groups, dann Match-Flow/Routes/Tests
- Backend umgesetzt: DB-Migrationen, Mini-RR-Bracket, Captain-Teamnamen, Mini-Group-Resolver, Game-Modes, Auto-Lobby, Admin/Public-Routen
- Neue Backend-Dateien: `backend/tournament/mini_groups.py`, `backend/match/game_modes.py`, `backend/match/heroes.py`, `backend/match/auto_lobby.py`
- Neue Tests ergänzt: `test_engine_mini_group_seeding.py`, `test_engine_team_naming.py`, `test_auto_lobby_hooks.py`, `test_game_modes.py`
- Verifikation: `.venv/bin/python -m py_compile` für Ziel-Dateien + neue Tests erfolgreich; `.venv/bin/pytest` in dieser Umgebung nicht vorhanden

---

## Neue Aufgabe (2026-04-18): Feature-Erweiterungen — Voice, Bo3, Draft, Stats, Lobby

### Ziel
- **A** Voice-Kanal-Runden-Management: Start/Nächste Runde, Split Teams → VC1/VC2 → Sammelpunkt
- **B** Best of 3: series_format pro Turnier (Bo1/Bo3/Bo5), match_games Tabelle
- **C** Hero-Draft: Pick/Ban System (6 Bans + 12 Picks)
- **D** Match-Stats: Deadlock Match-ID + K/D/A in Discord + Dashboard
- **E** Lobby-Announcement: Lobby-Code immer in Kanal 1412411665713987635 mit User-Pings

### Status (2026-04-18)
→ **Gestartet** — GPT-Worker werden dispatched

### Fortschritt GPT-Worker A1 (2026-04-18)
- `service/master_broker.py` in `Deadlock-Bots` um `move-voice` und `voice-channel/members` erweitert
- Routing, Auth, Idempotency und 404/502-Fehlerpfade an bestehendes Broker-Muster angeglichen
- Verifikation: `python3 -m py_compile /home/naniadm/Documents/Deadlock-Bots/service/master_broker.py` erfolgreich
- Übergabe: Änderungen bleiben absichtlich uncommitted für Claude-Review

### Kritische Erkenntnis
Broker hat KEINEN move-voice Endpoint → muss in Deadlock-Bots ergänzt werden.

### Reihenfolge
1. Parallel: A1 (Broker) + B1/C1 (DB-Schema)
2. Parallel: A2-A4+E1-E3 (Backend Voice+Lobby) + B2-B4 (Series) + C2-C4 (Draft) + D1-D2 (Stats)
3. Frontend: A5 + B5 + C5 + D3

### Offene Punkte
- [x] A1: Broker move-voice + get-voice-members (Deadlock-Bots)
- [ ] B1+C1: DB schema_upgrades
- [x] A2-A4: config.py + notifier + admin_routes Voice
- [x] E1-E3: config + send_lobby_announcement + match/manager.py
- [x] B2-B4: models + series_manager.py + Routen
- [x] C2-C4: draft/heroes.py + draft/engine.py + draft/routes.py + main.py
- [x] D1-D2: send_match_stats + result_processor
- [ ] Frontend (A5, B5, C5, D3)

### Fortschritt GPT-Worker B1+C1+B2 (2026-04-18)
- `backend/db.py`: `series_format` in `tournaments` ergänzt, neue Tabellen `match_games`, `draft_sessions`, `draft_actions` im Schema und in `_ensure_schema_upgrades()` hinzugefügt
- `backend/tournament/models.py`: `series_format` in Tournament-Modelle aufgenommen, `MatchGame` ergänzt, `BracketMatch` um Series-Tracking erweitert
- Verifikation: `.venv/bin/python -m py_compile backend/db.py backend/tournament/models.py` erfolgreich
- Übergabe: Änderungen bleiben absichtlich uncommitted für Claude-Review

### Fortschritt GPT-Worker B3+B4+C2+C3+C4 (2026-04-18)
- `backend/match/series_manager.py` neu erstellt: Serien-Spiele anlegen, Ergebnisse pro Game speichern, Serienstand auswerten
- `backend/tournament/admin_routes.py` um Series-Start/Result-Endpunkte ergänzt; Match wird gegen `tournament_id` validiert und der Serien-Slot `1/2` auf die bestehende Bracket-Konvention `0/1` gemappt
- Neues Paket `backend/draft/` mit `heroes.py`, `engine.py`, `routes.py` erstellt; `backend/main.py` bindet den Draft-Router ein
- Verifikation steht als nächster Schritt an; Änderungen bleiben absichtlich uncommitted für Claude-Review

### Fortschritt GPT-Worker A2+A3+A4+E1+E2+E3 (2026-04-18)
- `backend/config.py`: Voice- und Turnier-Lobby-Channel-Settings ergänzt
- `backend/notifications/discord_notifier.py`: Voice-Moves, Voice-Member-Abfrage, Lobby-Announcement und Match-Stats-Posting ergänzt
- `backend/tournament/admin_routes.py`: Admin-Endpoints für VC-Split, Sammelpunkt, Einzel-Move und Channel-Member ergänzt
- `backend/match/manager.py`: Zentrales Lobby-Announcement nach Lobby-Erstellung ergänzt
- `backend/match/result_processor.py`: Non-blocking Stats-Posting in Discord-Match-Channels ergänzt
- Verifikation: `.venv/bin/python -m py_compile backend/config.py backend/notifications/discord_notifier.py backend/tournament/admin_routes.py backend/match/manager.py backend/match/result_processor.py` erfolgreich
- Übergabe: Änderungen bleiben absichtlich uncommitted für Claude-Review

### Fortschritt GPT-Review Backend kritisch (2026-04-18)
- Review-Scope gelesen: `backend/db.py`, `backend/tournament/models.py`, `backend/config.py`, `backend/notifications/discord_notifier.py`, `backend/tournament/admin_routes.py`, `backend/match/manager.py`, `backend/match/result_processor.py`, `backend/match/series_manager.py`, `backend/draft/*`, `backend/main.py`
- Syntax-Check: `.venv/bin/python -m py_compile` auf allen genannten Backend-Dateien erfolgreich, keine Syntax-Fehler
- Fokus des Reviews: Series-Flow, Discord-Notifier, Voice-Endpoints, Draft-Routen/Engine, DB-Migrationen
- Ergebnis wird als reine Issue-Liste ohne Fixes an Claude zurückgegeben

### Fortschritt GPT-Worker Backend Fix Review-Issues (2026-04-18)
- Scope strikt auf 4 Fixes begrenzt: `series_format` Validator, Pre-Write-Matchvalidierung in Series-Endpunkten, `match_id`-Check im Draft-Start, engeres Exception-Handling in `_ensure_schema_upgrades()`
- Betroffene Dateien aktualisiert: `backend/tournament/models.py`, `backend/tournament/admin_routes.py`, `backend/draft/routes.py`, `backend/db.py`
- Verifikation: `.venv/bin/python -m py_compile backend/tournament/models.py backend/tournament/admin_routes.py backend/draft/routes.py backend/db.py` erfolgreich
- Übergabe: Änderungen bleiben uncommitted für Claude-Review

### Fortschritt GPT-Worker Frontend A5+B5+C5+D3 (2026-04-18)
- `frontend/src/types/tournament.ts`: Series-, Voice- und Draft-Typen ergänzt; `BracketMatch` und `TournamentCreate/Update` erweitert
- `frontend/src/api/client.ts` + `frontend/src/hooks/useTournament.ts`: neue Voice-, Draft- und Series-Requests/Hooks im bestehenden `request()`-/React-Query-Muster ergänzt
- Neue Admin-Komponenten `VoiceChannelPanel.tsx` und `DraftPanel.tsx` erstellt; `MatchAdminPanel.tsx`, `CreateTournamentForm.tsx` und `pages/Admin.tsx` integriert
- Verifikation: `cd /home/naniadm/Documents/Deadlock-Turniere/frontend && npx tsc --noEmit` erfolgreich
- Übergabe: Änderungen bleiben absichtlich uncommitted für Claude-Review

### Plan-Datei
`/home/naniadm/.claude/plans/f-r-den-turnier-bot-bright-honey.md`

---

---

## Neue Aufgabe (2026-04-15): Auto Tournament Mode Management

### Ziel
Bot entscheidet automatisch, ob Gruppen-Phase oder nur Bracket basierend auf Team-Anzahl:
- **>= 12 Teams**: Group Stage + Bracket (wie EM/WM)
- **< 12 Teams**: Nur Bracket (schnell & einfach)
- **Admin-Override**: Admins können manuell entscheiden

Zusätzlich: Hilfe-Dokumentation für Turnier-Modi.

### Status (2026-04-15)
→ **Implementierung fertig** — Syntaktisch korrekt, Review & Commit ausstehend

### Implementierungs-Schritte
1. [x] `models.py`: `TournamentMode` Enum + `force_tournament_mode` Feld in Create/Update
2. [x] `engine.py`: `determine_tournament_mode(team_count, force_mode)` Funktion
3. [x] `scheduler.py`: Auto-Logik in `_get_due_next_status()` — wenn bracket_only → skip group_phase
4. [x] `db.py`: `tournament_mode` Spalte hinzugefügt via `_ensure_schema_upgrades()`
5. [x] `admin_routes.py`: Override-Flag in Create/Update, Mode-Berechnung, Validierung
6. [x] Hilfe-Datei `HELP_TOURNAMENT_MODES.md` erstellt (Erklärungen, Unterschiede, Ablauf, FAQ)
7. [ ] Frontend: Info-Text für Turnier-Modus (optional für nächste Phase)
8. [ ] Review, Test, Commit

### Was wurde implementiert
- **Auto-Logic**: >= 12 Teams → group_stage (Standard EM/WM), < 12 Teams → bracket_only
- **Admin-Override**: `force_tournament_mode` im TournamentCreate/Update (nur in Draft-Phase änderbar)
- **Scheduler-Integration**: Wenn bracket_only, skip group_phase und springe direkt zu bracket
- **Dokumentation**: Ausführliche Hilfe-Datei mit Beispielen, Vergleichen und FAQ

### Syntax-Check
✅ Alle Python-Dateien syntaktisch korrekt (models.py, engine.py, scheduler.py, db.py, admin_routes.py)

---

## Alte Aufgabe (2026-04-15): Profil-Upload, Namensänderung, Check-in-Start, Zeitplan-Deduplizierung

### Ziel
1. Profilbild-Upload und Anzeigename-Änderung im Frontend aktivieren (Backend bereits fertig)
2. `checkin_start` als konfigurierbaren Zeitstempel für Turniere ergänzen (Backend + Frontend)
3. Doppeltes „Zeitplan anpassen"-Panel im TournamentManager entfernen

### Status (2026-04-15)
→ **In Bearbeitung** — GPT-Worker laufen

### Fortschritt GPT-Worker 2 (2026-04-15)
- Frontend-Dateien für Profilbild/Anzeigename/`checkin_start` vollständig geprüft
- Implementierung in `tournament.ts`, `client.ts`, `useTournament.ts`, `PlayerProfile.tsx`, `TournamentManager.tsx`, `CreateTournamentForm.tsx` abgeschlossen
- Verifikation: `cd frontend && npx tsc --noEmit` erfolgreich, keine TypeScript-Fehler

### Fortschritt (2026-04-15)
- GPT-Worker 1: Backend für `checkin_start` umgesetzt: DB-Migration, Modelle, Admin-Create/Update, Scheduler
- Verifikation: Syntax-Check erfolgreich, `pytest` aktuell nicht ausführbar, weil in den verfügbaren Python-Umgebungen kein `pytest` installiert ist

### Offene Punkte
- [x] Backend: `checkin_start` DB-Migration, Modelle, Scheduler, Admin-Routes
- [ ] Frontend: Typen, API-Client, Hooks, Profil-Seite, TournamentManager, CreateTournamentForm
- [ ] Review, Verifikation, Commit & Push

### Wichtige Entscheidungen
- Avatar-Upload-Backend ist vollständig implementiert → keine Backend-Änderungen nötig
- `checkin_start` Fallback: falls nicht gesetzt → `registration_end` bleibt der Trigger
- Quick-Schedule-Panel wird entfernt (dupliziert das Hauptformular)
- Ergebnis-Eintrag ist bereits implementiert, erscheint automatisch nach Match-Generierung

---


## Ziel
Großes Feature-Upgrade mit 8 Phasen: DSGVO-Consent, Discord-ID-Schutz, Rangliste, Spieler-Profile, Recruiting-Status, Pick-Zeitfenster, Invite-Flow, Admin-Cards.

## Aufteilung
- **GPT-Worker**: Gesamtes Backend (db.py, models.py, routes.py, admin_routes.py, neue Backend-Dateien)
- **Claude**: Gesamtes Frontend (types, pages, components, App.tsx)

## Status
`IN ARBEIT`

## Erledigte Schritte
- [x] Worker A: Phase 1 (db.py Schema) + Phase 2 (models.py Public-Varianten) → Schema, Invite-/Recruiting-Felder und Public-/Consent-/Profil-Modelle umgesetzt
- [x] Worker B: `backend/tournament/routes.py` erweitert: Public Tournament-Detail, Consent-Gates, Recruiting-Status, Invite-by-Signup, Bewerbungs- und Einladungs-Flow umgesetzt
- [x] Worker C: admin_routes.py (Phase 4 Recruiting + Phase 5 Pick-Fenster) → Invite-Felder in Create/Update ergänzt, Recruiting-Patch sowie Applications List/Accept/Reject umgesetzt
- [x] Worker D: Neue Backend-Dateien (Phase 3 Consent/Profile + Phase 7 Spieler-Profil + Phase 8 Rangliste + points.py) erstellt, `main.py` angebunden, Import-Check mit `./.venv/bin/python` erfolgreich
- [x] Security-Fixes: `join_team` mit Consent-Gate ergänzt, Check-in-Status auf `checked_in_names` ohne Discord-ID umgestellt, Leaderboard-Fallback auf `"Unbekannt"` geändert; Frontend-Verweis in `CheckinManager.tsx` angepasst
- [ ] Frontend: types/tournament.ts erweitern (neue Typen, discord_id aus Public entfernen)
- [ ] Frontend: ConsentModal.tsx (Phase 3)
- [ ] Frontend: Tournament.tsx Solo-Tabelle + Invite-Flow (Phase 6)
- [ ] Frontend: ParticipantManager.tsx Cards (Phase 9)
- [ ] Frontend: CreateTournamentForm + TournamentManager Invite-Modus (Phase 5)
- [ ] Frontend: Leaderboard.tsx neue Seite (Phase 8)
- [ ] Frontend: PlayerProfile.tsx neue Seite (Phase 7)
- [ ] Frontend: App.tsx neue Routen
- [ ] Review, Verifikation, Commit, Push

## Offene Punkte
- Worker A abgeschlossen, Worker B/C/D können auf dem neuen Schema und den Public-Modellen aufsetzen
- Worker B ist implementiert und per `python3 -m py_compile backend/tournament/routes.py` syntaktisch geprüft; End-to-End API-Tests stehen noch aus
- Frontend-Seiten können parallel zur Worker-Arbeit gebaut werden

## Wichtige Entscheidungen
- Invite über signup_id statt discord_id (verhindert ID-Leak)
- Zwei Response-Schemas pro Modell: Public (kein discord_id) und Admin (mit discord_id)
- Consent: globales hartes Gate, einmalig pro User, Version 1
- player_points: nach jedem Turnier-Abschluss neu berechnet

## Relevante Dateien
**Backend:**
- `backend/db.py` – Schema + _ensure_schema_upgrades
- `backend/tournament/models.py` – Neue Public-Varianten
- `backend/tournament/routes.py` – Öffentliche Endpunkte
- `backend/tournament/admin_routes.py` – Admin-Endpunkte
- `backend/tournament/points.py` – NEU: Punkte-Berechnung
- `backend/main.py` – Neue Router einbinden

**Frontend:**
- `frontend/src/types/tournament.ts`
- `frontend/src/App.tsx`
- `frontend/src/pages/Tournament.tsx`
- `frontend/src/pages/Admin.tsx`
- `frontend/src/pages/Leaderboard.tsx` (NEU)
- `frontend/src/pages/PlayerProfile.tsx` (NEU)
- `frontend/src/components/admin/ParticipantManager.tsx`
- `frontend/src/components/admin/CreateTournamentForm.tsx`
- `frontend/src/components/admin/TournamentManager.tsx`
- `frontend/src/components/ConsentModal.tsx` (NEU)
