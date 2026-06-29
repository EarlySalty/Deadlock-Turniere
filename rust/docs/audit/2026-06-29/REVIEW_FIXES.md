# Phase0 Task4b Review der Paritaets-Fixes

Datum: 2026-06-29

Scope: adversariale Gegenpruefung der drei uncommitted Paritaets-Fixes. Nur gelesen und diese Datei geschrieben. Kein Git, kein Cargo, kein Service-Restart.

## Rework Task4c Abschluss

Status: **behoben**.

- Python-Pruefung: Explizites `null` fuer `reminder_offsets` und `start_reminder_offsets` bleibt im Update-Modell `None`, wird von den Validatoren unveraendert zurueckgegeben und bleibt durch `model_dump(exclude_unset=True)` im Update-Payload. Da der Handler nur Nicht-`None`-Werte serialisiert und die verbleibenden Werte direkt bindet, schreibt Python fuer gesetztes JSON-`null` SQL-NULL: `backend/tournament/models.py:201`, `backend/tournament/models.py:202`, `backend/tournament/models.py:225`, `backend/tournament/models.py:233`, `backend/tournament/admin_routes.py:959`, `backend/tournament/admin_routes.py:997`, `backend/tournament/admin_routes.py:999`, `backend/tournament/admin_routes.py:1022`, `backend/tournament/admin_routes.py:1040`.
- Rust-Rework: Beide Offset-Update-Felder nutzen jetzt `Patch<Vec<i64>>`; `Patch::Null` schreibt `Value::Null`, `Patch::Missing` laesst die Spalte unveraendert, `Patch::Value` serialisiert wie bisher. `clean_offsets` laeuft nur im Value-Fall.
- Tests: Die Nullable-DTO-Tests decken Missing/Null/Value fuer beide Offset-Felder ab; zusaetzlich prueft ein Offset-Validation-Test, dass nur konkrete Values bereinigt/defaulted werden. Der Scheduler-Rollback-Test prueft nun auch, dass die Sentinel-Zeile in `player_points` nach dem fehlgeschlagenen Recompute unveraendert vorhanden ist.
- Optionaler Admin-Rebuild-Integrationstest wurde nicht ergaenzt, weil `turnier-api` aktuell keine bestehende `tests/`-Struktur und kein Router-Testmuster hat; der vorhandene Engine-Transaktionstest bleibt die direkte Abdeckung der Rebuild-Transaktionsgrenze.
- Verifikation: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` gruen.

## Fix 1: `Patch<T>` fuer nullable `TournamentUpdate`-Felder

Urteil: **Problem (mittel)**.

Was OK ist:

- Die dreistufige Deserialisierung funktioniert konzeptionell: `#[serde(default)]` liefert bei fehlenden JSON-Feldern `Patch::Missing`, explizites JSON-`null` laeuft ueber `deserialize_option` nach `Patch::Null`, konkrete Werte nach `Patch::Value`: `rust/crates/turnier-core/src/tournament.rs:37`, `rust/crates/turnier-core/src/tournament.rs:45`, `rust/crates/turnier-core/src/tournament.rs:65`, `rust/crates/turnier-core/src/tournament.rs:72`, `rust/crates/turnier-core/src/tournament.rs:79`, `rust/crates/turnier-core/src/tournament.rs:87`.
- Die wichtigsten nullable Tournament-Spalten werden als `Patch` modelliert: `description`, Zeitfenster, `invite_window_*`, `lobby_settings`, `final_series_format`, `rules`: `rust/crates/turnier-core/src/tournament.rs:171`, `rust/crates/turnier-core/src/tournament.rs:181`, `rust/crates/turnier-core/src/tournament.rs:183`, `rust/crates/turnier-core/src/tournament.rs:195`, `rust/crates/turnier-core/src/tournament.rs:201`, `rust/crates/turnier-core/src/tournament.rs:221`.
- Der Admin-Update-Code schreibt fuer diese Felder bei `Patch::Null` wirklich `Value::Null`, das via `bind_json` als SQL-NULL gebunden wird: `rust/crates/turnier-api/src/admin/tournaments.rs:316`, `rust/crates/turnier-api/src/admin/tournaments.rs:333`, `rust/crates/turnier-api/src/admin/tournaments.rs:338`, `rust/crates/turnier-api/src/admin/tournaments.rs:417`, `rust/crates/turnier-api/src/admin/tournaments.rs:442`, `rust/crates/turnier-api/src/admin/tournaments.rs:495`.
- Die DTO-Tests pruefen Missing/Null/Value fuer diese `Patch`-Felder und sind nicht vacuous: `rust/crates/turnier-core/src/tournament.rs:415`, `rust/crates/turnier-core/src/tournament.rs:435`, `rust/crates/turnier-core/src/tournament.rs:469`.

Problem:

- `reminder_offsets` und `start_reminder_offsets` wurden uebersehen. Beide Tournament-Spalten sind im DB-Schema nullable, weil sie kein `NOT NULL` haben: `rust/crates/turnier-db/migrations/0001_initial.sql:20`. Python modelliert beide Update-Felder als optional und laesst explizites `null` durch die Validatoren unveraendert: `backend/tournament/models.py:201`, `backend/tournament/models.py:202`, `backend/tournament/models.py:225`, `backend/tournament/models.py:233`. Im Python-Handler landet present-but-null in `update_data = body.model_dump(exclude_unset=True)` und wird nicht entfernt; nur nicht-null Werte werden serialisiert: `backend/tournament/admin_routes.py:959`, `backend/tournament/admin_routes.py:997`, `backend/tournament/admin_routes.py:999`. Dadurch schreibt Python bei `{"reminder_offsets": null}` bzw. `{"start_reminder_offsets": null}` SQL-NULL. Rust nutzt hier dagegen weiter `Option<Vec<i64>>`; explizites `null` ist ununterscheidbar von "weggelassen" und wird nicht geschrieben: `rust/crates/turnier-core/src/tournament.rs:213`, `rust/crates/turnier-core/src/tournament.rs:215`, `rust/crates/turnier-api/src/admin/tournaments.rs:411`, `rust/crates/turnier-api/src/admin/tournaments.rs:414`.

Konkreter Rework:

- `TournamentUpdate.reminder_offsets` und `TournamentUpdate.start_reminder_offsets` ebenfalls auf `Patch<Vec<i64>>` umstellen.
- In `validated()` nur `Patch::Value` bereinigen; `Patch::Null` unveraendert lassen.
- Im Admin-Update `Patch::Null => push!(..., Value::Null)`, `Patch::Value(v) => push!(..., json!(serialize_reminder_offsets(v)))`, `Patch::Missing => {}`.
- Die drei Nullable-DTO-Tests um beide Felder erweitern. Optional zusaetzlich einen Admin-Update-DB-Test fuer `{"reminder_offsets": null}` und `{"start_reminder_offsets": null}` ergaenzen, weil die aktuellen Tests nur DTO-Deserialisierung abdecken.

## Fix 2: `generate_bracket_in_tx` fuer Admin-Rebuild

Urteil: **OK**.

Belege:

- Die Pool-Variante ist jetzt ein duenner Wrapper um dieselbe Implementierung: `begin`, `generate_bracket_in_tx`, `commit`: `rust/crates/turnier-engine/src/persist/bracket.rs:36`, `rust/crates/turnier-engine/src/persist/bracket.rs:37`, `rust/crates/turnier-engine/src/persist/bracket.rs:38`, `rust/crates/turnier-engine/src/persist/bracket.rs:39`.
- Die `_in_tx`-Variante enthaelt die funktionale Generierungslogik der alten Pool-Variante: Bracket-Clear, Format-Lookup, Gruppen-Top-2, Fallback auf Teams, Mindestgroesse, Cross-Seeding, Double-Elim/Single-Elim-Aufbau: `rust/crates/turnier-engine/src/persist/bracket.rs:51`, `rust/crates/turnier-engine/src/persist/bracket.rs:53`, `rust/crates/turnier-engine/src/persist/bracket.rs:63`, `rust/crates/turnier-engine/src/persist/bracket.rs:97`, `rust/crates/turnier-engine/src/persist/bracket.rs:116`, `rust/crates/turnier-engine/src/persist/bracket.rs:122`, `rust/crates/turnier-engine/src/persist/bracket.rs:124`.
- Im Admin-Update liegt Delete + Rebuild + Statuswechsel + Audit in derselben `tx`; der einzige Commit im Handler kommt danach: `rust/crates/turnier-api/src/admin/tournaments.rs:262`, `rust/crates/turnier-api/src/admin/tournaments.rs:463`, `rust/crates/turnier-api/src/admin/tournaments.rs:467`, `rust/crates/turnier-api/src/admin/tournaments.rs:468`, `rust/crates/turnier-api/src/admin/tournaments.rs:469`, `rust/crates/turnier-api/src/admin/tournaments.rs:477`, `rust/crates/turnier-api/src/admin/tournaments.rs:484`.
- Die aufgerufenen Engine-Build-Pfade starten keine eigene Transaktion und committen nicht; `begin/commit` tauchen in `bracket.rs` nur in der Pool-Wrapper-Funktion auf.
- Der neue Engine-Test ist sinnvoll: Er baut Matches innerhalb einer offenen Transaktion, sieht sie innerhalb der Tx und prueft nach Rollback, dass ausserhalb nichts persistiert ist: `rust/crates/turnier-engine/tests/engine_bracket_transaction.rs:36`, `rust/crates/turnier-engine/tests/engine_bracket_transaction.rs:40`, `rust/crates/turnier-engine/tests/engine_bracket_transaction.rs:47`, `rust/crates/turnier-engine/tests/engine_bracket_transaction.rs:49`.

Testluecke, aber kein Blocker:

- Es gibt keinen direkten Admin-Endpoint-/Handler-Test, der beim `group_phase -> bracket_only`-Rebuild einen Fehler im Bracket-Aufbau erzwingt und danach `tournaments.status`, geloeschte Gruppen/Matches und Audit zusammen prueft. Die Code-Struktur ist dennoch eindeutig transaktional. Als Regressionstest waere ein integrierter Admin-Update-Fall robuster als der reine Engine-Rollback-Test.

## Fix 3: `recalculate_player_points_in_tx` fuer `completed`

Urteil: **OK**.

Belege:

- Die Pool-Variante bleibt korrekt und ist jetzt ein duenner Wrapper: `begin`, `_in_tx`, `commit`: `rust/crates/turnier-engine/src/persist/points.rs:61`, `rust/crates/turnier-engine/src/persist/points.rs:65`, `rust/crates/turnier-engine/src/persist/points.rs:66`, `rust/crates/turnier-engine/src/persist/points.rs:67`.
- Die `_in_tx`-Variante verwendet ausschliesslich die uebergebene `Transaction` und startet keine eigene Transaktion: `rust/crates/turnier-engine/src/persist/points.rs:75`, `rust/crates/turnier-engine/src/persist/points.rs:83`, `rust/crates/turnier-engine/src/persist/points.rs:135`, `rust/crates/turnier-engine/src/persist/points.rs:140`.
- Der Scheduler ruft im `completed`-Pfad die `_in_tx`-Variante innerhalb derselben Transaktion wie Status-Update und Audit auf und committet erst danach: `rust/crates/turnier-scheduler/src/transition.rs:167`, `rust/crates/turnier-scheduler/src/transition.rs:169`, `rust/crates/turnier-scheduler/src/transition.rs:187`, `rust/crates/turnier-scheduler/src/transition.rs:194`, `rust/crates/turnier-scheduler/src/transition.rs:201`, `rust/crates/turnier-scheduler/src/transition.rs:205`.
- Es gibt keine verbliebene interne Rust-Nutzung der Pool-Variante ausser dem Export; damit entsteht im Scheduler keine geschachtelte Transaktion.
- Die neuen Scheduler-Tests pruefen sowohl den positiven `completed`-Pfad mit konkreten `player_points` als auch Rollback von Status/Audit bei erzwungenem Recompute-Fehler: `rust/crates/turnier-scheduler/tests/advance_status.rs:151`, `rust/crates/turnier-scheduler/tests/advance_status.rs:172`, `rust/crates/turnier-scheduler/tests/advance_status.rs:173`, `rust/crates/turnier-scheduler/tests/advance_status.rs:190`, `rust/crates/turnier-scheduler/tests/advance_status.rs:207`, `rust/crates/turnier-scheduler/tests/advance_status.rs:227`, `rust/crates/turnier-scheduler/tests/advance_status.rs:228`.

Testluecke, aber kein Blocker:

- Der Rollback-Test legt eine Sentinel-Zeile in `player_points` an, prueft aber nicht, dass sie nach dem fehlgeschlagenen Recompute noch vorhanden ist: `rust/crates/turnier-scheduler/tests/advance_status.rs:198`, `rust/crates/turnier-scheduler/tests/advance_status.rs:227`. Ein `SELECT` auf diese Zeile wuerde die Atomicity-Aussage abrunden.

## Gesamturteil

Nicht voll mergebar wegen Fix 1: Die Null-Patch-Paritaet ist fuer `reminder_offsets` und `start_reminder_offsets` unvollstaendig. Fix 2 und Fix 3 wirken funktional korrekt und transaktional sauber; ihre Tests sind sinnvoll, koennten aber mit je einem integrierten Zusatzfall staerker werden.
