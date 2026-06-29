# Phase0 Task3b Adversarial Review

Datum: 2026-06-29

Scope: skeptische Gegenpruefung von `rust/docs/audit/2026-06-29/REPORT.md`, mit Fokus auf `backend/tournament/routes.py` und `backend/tournament/admin_routes.py` gegen `rust/crates/turnier-*`. Keine Builds, keine Services, kein Git. Nur diese Review-Datei wurde geschrieben.

## NEU GEFUNDENE LUECKEN

### 1. `PUT /api/admin/tournaments/{tournament_id}`: `group_phase -> bracket_only` ist in Rust nicht atomar

- Python-Beleg: Der Handler sammelt Updates, loescht bei `group_phase` + Wechsel auf `bracket_only` die Gruppen-/Bracket-Struktur, baut direkt danach den neuen Bracket-Tree und setzt den Status auf `bracket`, bevor erst danach Audit und `commit()` laufen: `backend/tournament/admin_routes.py:1020`, `backend/tournament/admin_routes.py:1042`, `backend/tournament/admin_routes.py:1050`, `backend/tournament/admin_routes.py:1051`, `backend/tournament/admin_routes.py:1062`.
- Rust-Beleg: Rust loescht Gruppen-/Bracket-Struktur innerhalb einer Transaktion, schreibt Audit und committet bereits bei `tx.commit().await?`; `generate_bracket` und der finale Statuswechsel auf `bracket` laufen danach ausserhalb dieser Transaktion: `rust/crates/turnier-api/src/admin/tournaments.rs:435`, `rust/crates/turnier-api/src/admin/tournaments.rs:439`, `rust/crates/turnier-api/src/admin/tournaments.rs:450`, `rust/crates/turnier-api/src/admin/tournaments.rs:457`, `rust/crates/turnier-api/src/admin/tournaments.rs:458`, `rust/crates/turnier-api/src/admin/tournaments.rs:459`.
- Schweregrad: mittel.
- Begruendung: Im Erfolgsfall ist das Verhalten gleich. Im Fehlerfall nach dem Rust-Commit bleibt aber ein teilzerstoerter Zustand moeglich: `tournament_mode` geaendert, Gruppen-/Bracket-Daten geloescht, aber neuer Bracket-Tree/Status noch nicht fertig. Python haelt diese Sequenz bis zum expliziten Commit zusammen.

### 2. Steam-/Lobby-Timeouts: Rust verliert aktionsspezifische 504-Detailtexte

- Python-Beleg: Die Admin-Handler liefern je Aktion eigene 504-Meldungen, z.B. Lobby-Erstellung, Match-Start, Ergebnis-Fetch, Leave-Lobby, Gruppen-Lobby, Gruppen-Start, Gruppen-Fetch, Gruppen-Leave sowie ConVars/Event-Presets: `backend/tournament/admin_routes.py:2407`, `backend/tournament/admin_routes.py:2467`, `backend/tournament/admin_routes.py:2523`, `backend/tournament/admin_routes.py:2577`, `backend/tournament/admin_routes.py:2675`, `backend/tournament/admin_routes.py:2723`, `backend/tournament/admin_routes.py:2764`, `backend/tournament/admin_routes.py:2806`, `backend/tournament/admin_routes.py:3066`, `backend/tournament/admin_routes.py:3135`.
- Rust-Beleg: Das gemeinsame Mapping gibt fuer alle `SteamTaskError::Timeout` nur `err.to_string()` zurueck; der Display-Text ist generisch `Steam-Task {task_id}...`: `rust/crates/turnier-api/src/admin/steam_ops.rs:33`, `rust/crates/turnier-api/src/admin/steam_ops.rs:44`, `rust/crates/turnier-api/src/admin/steam_ops.rs:46`, `rust/crates/turnier-match/src/error.rs:82`.
- Schweregrad: niedrig.
- Begruendung: Statuscode und Fehlerform `{"detail": ...}` bleiben erhalten, aber clients/logs verlieren die Python-kompatible Aktionsdiagnose.

## BESTAETIGTE ABWEICHUNGEN

### A1. `PUT /api/admin/tournaments/{tournament_id}` kann explizites `null` nicht portieren

- Python-Beleg: `TournamentUpdate` erlaubt optionale Felder; `model_dump(exclude_unset=True)` und `model_fields_set` unterscheiden "nicht gesendet" von "explizit null": `backend/tournament/models.py:178`, `backend/tournament/admin_routes.py:958`, `backend/tournament/admin_routes.py:1003`, `backend/tournament/admin_routes.py:1007`.
- Rust-Beleg: `TournamentUpdate` nutzt einfache `Option<T>`-Felder; der Update-Code pushed nur `Some(...)`, z.B. fuer `description`, Zeitfenster, `rules`: `rust/crates/turnier-core/src/tournament.rs:105`, `rust/crates/turnier-core/src/tournament.rs:107`, `rust/crates/turnier-api/src/admin/tournaments.rs:311`, `rust/crates/turnier-api/src/admin/tournaments.rs:314`, `rust/crates/turnier-api/src/admin/tournaments.rs:332`, `rust/crates/turnier-api/src/admin/tournaments.rs:353`.
- Einstufung: bestaetigt, mittel. Echte API-Regression fuer das Leeren nullable Felder.

### A2. Avatar-Dateirouten haben weniger Datei-Header-Paritaet

- Python-Beleg: lokale Dateien werden als FastAPI/Starlette `FileResponse` ausgeliefert: `backend/tournament/consent_routes.py:293`, `backend/tournament/consent_routes.py:298`, `backend/tournament/consent_routes.py:315`, `backend/tournament/consent_routes.py:332`.
- Rust-Beleg: Rust liest die Datei komplett und baut eine einfache Response mit Status und `Content-Type`: `rust/crates/turnier-api/src/consent.rs:526`, `rust/crates/turnier-api/src/consent.rs:536`, `rust/crates/turnier-api/src/consent.rs:560`, `rust/crates/turnier-api/src/consent.rs:588`.
- Einstufung: bestaetigt, niedrig. Inhalt/Redirect/404 sind vorhanden; Header wie Cache/Range/Last-Modified sind nicht voll parity.

### A3. Scheduler `completed` + Punkte-Recompute ist nicht mehr eine Transaktion

- Python-Beleg: Status-Update, Audit und `recalculate_player_points(db, tournament_id)` laufen vor demselben `commit()`: `backend/tournament/scheduler.py:162`, `backend/tournament/scheduler.py:171`, `backend/tournament/scheduler.py:181`, `backend/tournament/scheduler.py:183`.
- Rust-Beleg: Rust committet den Status/Audit vor `recalculate_player_points(pool, tournament_id)`: `rust/crates/turnier-scheduler/src/transition.rs:200`, `rust/crates/turnier-scheduler/src/transition.rs:201`, `rust/crates/turnier-scheduler/src/transition.rs:204`, `rust/crates/turnier-scheduler/src/transition.rs:205`.
- Einstufung: bestaetigt, niedrig bis mittel. Recompute ist idempotent, aber ein Crash zwischen Commit und Recompute hinterlaesst temporaer falsche Punkte.

## HERABGESTUFT/HARMLOS

### H1. Test-Mode-Router optional abschaltbar

- Python-Beleg: Python mountet den Test-Router immer: `backend/main.py:66`.
- Rust-Beleg: Rust mountet dieselben sechs Routen per Default, gibt aber bei explizitem `TURNIER_ENABLE_TEST_MODE=0/false/no/off` einen leeren Router zurueck: `rust/crates/turnier-api/src/test_mode.rs:52`, `rust/crates/turnier-api/src/test_mode.rs:55`, `rust/crates/turnier-api/src/test_mode.rs:58`, `rust/crates/turnier-api/src/test_mode.rs:73`.
- Einstufung: herabgestuft/harmlos. Die sechs Test-Mode-Routen plus das Nicht-Routen-Feature sind bei Default aktiv. Der Kill-Switch ist eher Betriebsverbesserung als Regression; nur bei bewusst deaktiviertem Env-Flag stimmt die Methode/Pfad-Paritaet nicht mehr.

## FAZIT

- Neue echte Luecken gegenueber Report A: 2. Davon hoch: 0, mittel: 1, niedrig: 1.
- Von den 11 gemeldeten Abweichungen: 4 als echte Abweichungen bestaetigt, 7 als Test-Mode-Kill-Switch herabgestuft/harmlos.
- "0 fehlt" ist nur fuer Methode+Pfad haltbar. Fuer Verhaltensvollstaendigkeit ist es nicht haltbar, weil mindestens die `bracket_only`-Rebuild-Atomicity und die 504-Detailtexte unterreportet sind.

Priorisierte Fix-Liste nur fuer echte Regressionen:

1. `PUT /api/admin/tournaments/{id}`: `group_phase -> bracket_only` in Rust wieder atomar machen oder bei post-commit Rebuild-Fehlern kompensieren.
2. `TournamentUpdate`: explizites JSON-`null` fuer nullable Felder abbilden, z.B. ueber `Option<Option<T>>` oder einen raw-JSON-Patch-Layer.
3. Scheduler: entscheiden, ob `completed` + Punkte-Recompute wieder atomar sein muss; falls ja, Recompute transaktionsfaehig machen.
4. Avatar-Dateirouten: bei Bedarf Header-Paritaet zu `FileResponse` herstellen.
5. Steam-Ops: aktionsspezifische 504-Detailtexte wiederherstellen, wenn API-/Log-Paritaet relevant ist.
