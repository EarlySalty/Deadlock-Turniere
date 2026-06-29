# Phase1a Review: turnier-automatik + Migration

Datum: 2026-06-29

Scope: adversariale Read-only-Review der neuen Crate `rust/crates/turnier-automatik/` und der Migration `rust/crates/turnier-db/migrations/0002_automatik.sql` gegen Spec `docs/specs/2026-06-29-turnier-automatisierung-design.md` §5/§6 und Plan `docs/plans/2026-06-29-turnier-automatik-phase1.md` Abschnitt 1a. Kein Git, kein Cargo, kein Service-Restart. Nur diese Review-Datei wurde geschrieben.

## Gesamturteil

Nicht mergebar ohne Rework.

Die Crate ist sauber geschnitten und viele reine Funktionen sind korrekt, aber zwei Fundament-Invarianten sind zu schwach abgesichert: `tournaments.source` kann beliebige/NULL-Werte annehmen, obwohl das Vokabular `bot|manual` festgelegt ist, und die oeffentliche Proposal-Persistenz kann die State-Machine komplett umgehen. Zusaetzlich ist die Vote-Tabelle als "Audit" beschrieben, verliert bei Re-Votes aber Historie.

## Migration `0002_automatik.sql`

Urteil: **Problem (hoch)** fuer `tournaments.source`; sonst ueberwiegend OK mit kleineren FK-/Idempotenz-Risiken.

OK:
- Die neuen Kernwerte sind bei den neuen Tabellen per `CHECK` abgesichert: `category` `fun|comp` in `rust/crates/turnier-db/migrations/0002_automatik.sql:7`, `tournament_proposals.source` `bot|manual` in `:29`, `state` in `:32`, `decision` in `:44`, `scope` in `:61`.
- Votes und Feedback haben `proposal_id ... ON DELETE CASCADE` (`:42`, `:51`), Signals haengen mit Cascade am Turnier (`:68`).
- Presets spiegeln die in Spec §6 explizit genannten Konfig-Spalten: `team_size`, `bracket_format`, `series_format`, `final_series_format`, `tournament_mode`, `tournament_game_mode`, `match_objective`, `invite_mode`, `reminder_offsets`, `start_reminder_offsets`, `rules`, `description_template` in `:8-19`. Die Defaults passen zu `tournaments` aus `0001_initial.sql:11`, `:16`, `:20`.

Problem 1:
- `tournaments.source` wird als `TEXT DEFAULT 'manual'` ohne `NOT NULL` und ohne `CHECK(source IN ('bot','manual'))` angelegt (`rust/crates/turnier-db/migrations/0002_automatik.sql:80`). Die Spec fuehrt `source` als festes Vokabular `bot|manual` (`docs/specs/2026-06-29-turnier-automatisierung-design.md:86`, `:92`), und die Review-Aufgabe fragt diese Konsistenz explizit ab. Ergebnis: spaetere Inserts/Updates koennen `NULL`, `caster`, `auto` o.ae. speichern, ohne DB-Fehler.
- Rework: Spalte mit `TEXT NOT NULL DEFAULT 'manual' CHECK(source IN ('bot','manual'))` anlegen. Falls SQLite/Live-Migration das nicht direkt per `ALTER TABLE ADD COLUMN` leisten soll, braucht es mindestens dokumentierte Trigger/Validierung plus Test; besser ist die neue Spalte jetzt korrekt restriktiv anzulegen.

Problem 2:
- `tournaments.preset_id` ist nur `INTEGER` (`rust/crates/turnier-db/migrations/0002_automatik.sql:81`), ohne FK auf `tournament_presets(id)`. `tournament_proposals.preset_id` hat immerhin einen FK (`:28`), aber keine Loeschsemantik. Dadurch sind zwei Varianten moeglich, beide unsauber: Turniere koennen auf nicht existierende Presets zeigen, und Preset-Loeschungen koennen spaeter an alten Proposals scheitern.
- Rework: Fuer `tournaments.preset_id` FK ergaenzen, sofern SQLite-ADD-COLUMN kompatibel im Zielsetup ist. Fuer `tournament_proposals.preset_id` fachlich entscheiden: meist `ON DELETE SET NULL`, wenn Proposal-Audit erhalten bleiben soll; alternativ Presets nicht hart loeschen, sondern nur `active=0`.

Problem 3:
- Die Tabellen sind `CREATE TABLE IF NOT EXISTS`, aber die drei `ALTER TABLE tournaments ADD COLUMN ...` sind nicht SQL-idempotent (`rust/crates/turnier-db/migrations/0002_automatik.sql:79-81`). SQLx verhindert normale Wiederholung ueber `_sqlx_migrations`, aber die Migration selbst ist nicht robust gegen eine Live-DB, in der eine der Spalten schon manuell/teilweise existiert. Der vorhandene Test nennt Idempotenz, prueft aber nur den Migrator-No-op nach erfolgreicher Eintragung (`rust/crates/turnier-db/tests/migration.rs:68-89`), nicht die SQL-Re-Run-Faehigkeit.
- Rework: Entweder Plantext/Testbenennung auf "sqlx-migrator-idempotent" korrigieren oder eine echte defensive Strategie fuer bereits vorhandene Spalten dokumentieren/implementieren.

## `proposals::transition` und State-Persistenz

Urteil: **Problem (hoch)**.

OK:
- Die reine State-Machine in `transition` bildet die Spec-Zustandsmaschine ab: `draft -> pending_approval`, `pending_approval -> approved|rejected|expired`, und Feedback zurueck nach `draft` (`rust/crates/turnier-automatik/src/proposals.rs:88-98`; Spec-Diagramm `docs/specs/2026-06-29-turnier-automatisierung-design.md:62-71`).
- Unbekannte Uebergaenge werden in der reinen Funktion nicht still erlaubt, sondern mit `AutomatikError::InvalidTransition` abgelehnt (`rust/crates/turnier-automatik/src/proposals.rs:95`).

Problem:
- Die oeffentliche Persistenzfunktion `set_state(pool, proposal_id, state)` setzt jeden beliebigen Zielzustand direkt (`rust/crates/turnier-automatik/src/proposals.rs:190-209`). Damit kann Code `draft -> approved`, `approved -> draft`, `rejected -> pending_approval` usw. persistieren, ohne `transition` aufzurufen. Das verletzt die geforderte Invariante "Kann man von Endzustaenden nicht mehr weg?" auf API-Ebene, auch wenn die reine Funktion korrekt ist.
- Die Tests nutzen `set_state` direkt fuer `pending_approval` und `approved` (`rust/crates/turnier-automatik/tests/automatik_db.rs:201-223`), pruefen aber keinen illegalen persistenten Uebergang.
- Rework: Entweder `set_state` privat/nur testnah halten und eine oeffentliche `apply_transition(pool, proposal_id, event)` anbieten, die den aktuellen DB-State laedt, `transition` erzwingt und per `WHERE id = ? AND state = ?` race-sicher updated; oder `set_state` selbst mit `from_state/event` ersetzen. Tests muessen mindestens terminale Zustaende gegen weitere Aenderungen absichern.

## `optout::compute_recipients`

Urteil: **OK** mit kleinem Test-Gap.

Belege:
- Die Berechnung filtert Rollenmitglieder gegen Kategorie-Scope und `all` (`rust/crates/turnier-automatik/src/optout.rs:101-105`), dedupliziert stabil in Rollenreihenfolge (`:107-115`) und kann wegen `Scope`-Enum plus DB-`CHECK` keine unbekannten Scopes in der typisierten Funktion bekommen (`:15-19`, Migration `0002_automatik.sql:61`).
- Leere Rollenliste ergibt durch die Schleife natuerlich `Vec::new()` (`rust/crates/turnier-automatik/src/optout.rs:107-117`).
- Tests decken Fun-vs-Comp-vs-All und "keine Optouts" ab (`rust/crates/turnier-automatik/tests/automatik_db.rs:226-253`).

Rework optional:
- Einen expliziten Test fuer doppelte `role_members` und leere Rolle ergaenzen. Die Implementierung wirkt korrekt, aber die Kanten sind im Test nicht sichtbar.

## `record_vote` / `approvals_count`

Urteil: **Problem (mittel)**.

OK:
- `UNIQUE(proposal_id, caster_discord_id)` ist in der Migration vorhanden (`rust/crates/turnier-db/migrations/0002_automatik.sql:46`).
- `approvals_count` zaehlt nur `decision = 'approve'` (`rust/crates/turnier-automatik/src/proposals.rs:177-185`).
- Der Test beweist die aktuelle Upsert-Semantik: Approve von `caster-1`, danach Reject desselben Casters reduziert den Count auf 0 und `list_votes` bleibt bei einer Zeile (`rust/crates/turnier-automatik/tests/automatik_db.rs:150-175`).

Problem:
- Die Spec beschreibt `tournament_proposal_votes` als "Min-1-Caster + Audit" (`docs/specs/2026-06-29-turnier-automatisierung-design.md:87`). `record_vote` macht aber ein Upsert und ueberschreibt `decision` sowie `created_at` (`rust/crates/turnier-automatik/src/proposals.rs:139-145`). Damit ist die Tabelle kein belastbarer Audit-Log mehr: die alte Entscheidung und der urspruengliche Zeitpunkt verschwinden.
- Rework: Fachentscheidung festziehen. Wenn nur der aktuelle Vote zaehlt, Tabelle/Doc als Current-Vote-Store benennen und `updated_at` statt `created_at`-Rewrite einfuehren. Wenn "Audit" ernst gemeint ist, Duplicate-Votes per UNIQUE-Fehler/Domainfehler ablehnen oder eine separate Vote-Events-Tabelle nutzen.

## Allgemeine Crate-Qualitaet

Urteil: **OK** mit Testluecken.

OK:
- Keine `.unwrap()`/`expect()` in Nicht-Test-Pfaden der neuen Crate gefunden; Vorkommen liegen in `rust/crates/turnier-automatik/tests/automatik_db.rs`.
- Fehler laufen ueber `thiserror` und `AutomatikError` (`rust/crates/turnier-automatik/src/error.rs:8-31`).
- Die Crate nutzt runtime-checked `query_as`/`FromRow` wie im Plan gefordert (`docs/plans/2026-06-29-turnier-automatik-phase1.md:31`), z.B. Presets `rust/crates/turnier-automatik/src/presets.rs:66-96`, Proposals `rust/crates/turnier-automatik/src/proposals.rs:51-64`, Signals `rust/crates/turnier-automatik/src/signals.rs:21-48`.
- Keine Discord-/Broker-/Scheduler-/HTTP-Logik in `turnier-automatik`; `lib.rs` exportiert nur `error`, `optout`, `presets`, `proposals`, `signals` (`rust/crates/turnier-automatik/src/lib.rs:8-14`).

Test-Probleme:
- Proposal-Transition-Tests pruefen nur zwei ungueltige Uebergaenge (`rust/crates/turnier-automatik/tests/automatik_db.rs:124-127`), obwohl der Plan "alle gueltigen + ungueltige" verlangt (`docs/plans/2026-06-29-turnier-automatik-phase1.md:44`). Besonders terminale Zustaende sollten gegen alle Events abgesichert werden.
- Migrationstest prueft Tabellen/Spalten-Anwesenheit, aber nicht die CHECK-Constraints/Foreign-Key-Semantik der neuen Tabellen (`rust/crates/turnier-db/tests/migration.rs:27-62`).

## Liste echter Probleme

1. Hoch: `tournaments.source` ohne `NOT NULL`/`CHECK(bot|manual)` laesst ungueltige Persistenzwerte zu (`rust/crates/turnier-db/migrations/0002_automatik.sql:80`).
2. Hoch: `set_state` erlaubt oeffentlich beliebige persistente Proposal-Zustandswechsel und kann terminale Zustaende wieder verlassen (`rust/crates/turnier-automatik/src/proposals.rs:190-209`).
3. Mittel: Vote-Upsert widerspricht der Audit-Erwartung, weil Entscheidungen und Zeitpunkte ueberschrieben werden (`rust/crates/turnier-automatik/src/proposals.rs:139-145`).
4. Mittel: Preset-/Tournament-FKs und Loeschsemantik sind nicht sauber entschieden (`rust/crates/turnier-db/migrations/0002_automatik.sql:28`, `:35`, `:81`).
5. Niedrig bis mittel: Migration ist nur durch SQLx-Migrationsbuchhaltung idempotent, nicht als SQL-Skript gegen teilweise vorveraenderte DBs (`rust/crates/turnier-db/migrations/0002_automatik.sql:79-81`).
