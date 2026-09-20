# Übergabe der Startpfad- und Baseline-Prüfung

Stand: 20.09.2026. Dieser Ergänzungsstand ist gepusht, aber nicht auf main integriert und nicht produktiv abgenommen.

## Integration

Branch: `feat/turniere-global-toml-ops-20260920`.

`37594e7` sichert den expliziten TOML-Launcher, die systemd-Startvorlagen, den Windows-Start und sieben isolierte Launcher-Vertragstests. `8c2fc63` dokumentiert die vollständige fachliche Gegenprüfung. Beide Commits sind auf origin veröffentlicht.

Die parallel entstandene Kernabnahme liegt laut ihrer Koordinationsakte unter `/home/nathanael/.worktrees/turniere-global-toml-abnahme-20260920` auf `feat/turniere-global-toml-abnahme-20260920`. Kein konkurrierender Main-Merge oder Produktionsdeploy aus diesem Ergänzungs-Worktree. Der ursprüngliche Worktree `/home/nathanael/.worktrees/turniere-global-toml-20260920` und seine ungemergte Arbeit bleiben bestehen.

Der vorhandene Abnahmestand muss die Startpfade und `scripts/test_config_launcher.sh` zusammenführen und anschließend als unveränderlicher Commit geprüft werden. Ein grüner Teiltest oder eine Health-Antwort ist keine Produktionsfreigabe.

## Fachtests

Befehl in beiden Läufen: vorhandener `central_test_db.sh`, danach `cargo test --workspace --features testing --no-fail-fast -j 2 -- --include-ignored`. Es wurde die Wegwerf-Testdatenbank verwendet, nicht die Produktion.

| Stand | Bestanden | Fehlgeschlagen | Ignoriert | Exit |
| --- | ---: | ---: | ---: | ---: |
| Produktions-SHA c9fad4346cd0d86fce542dcb2da6f2dc0cadf047 | 503 | 6 | 0 | 101 |
| Erster TOML-Zwischenstand | 535 | 6 | 0 | 101 |

Die sechs fehlschlagenden Assertions sind identisch. Das Vergleichs-JSON enthält die Fehlerpaare und Rohlog-Hashes. Wegen der damals noch parallel veränderten Kandidatenquellen ist ein neuer Lauf am finalen Commit erforderlich. Keine der sechs Ausnahmen wurde aus dem Testaufruf entfernt.

Die Rohlogs sind zusätzlich unter `/home/nathanael/.local/state/turniere-toml-ops-evidence-20260920-8c2fc63/` mit Dateimodus 0600 gesichert. Ihre SHA-256-Werte stimmen mit dem eingecheckten Vergleichs-JSON überein.

## Bestehender Live-Fehler

Die lesende Prüfung in `live-baseline-status.json` fand:

- Lokale und öffentliche Health: HTTP 200, weiterhin ohne TOML-Anker.
- UI-Dokument und zugehöriges JavaScript: HTTP 200. Das ist kein vollständiger Browser-Funktionstest.
- `/api/tournaments` lokal und `/turnier/api/tournaments` öffentlich: HTTP 500.
- Nicht angemeldeter Zugriff auf `/turnier/api/me`: HTTP 401.

PID weiterhin 1109, NRestarts 0. Kein Dienstneustart, keine Turnier-, Rollen-, Anmelde- oder Benachrichtigungsaktion durch diese Prüfung. Der Fehler der Turnierliste bestand vor einem TOML-Deploy und darf nicht als neue Config-Regression oder als funktionierender API-Pfad dargestellt werden. Seine Ursache ist hier nicht bestätigt.

## Offene Gates und Nachweise

Der angeforderte Teilreview der Startpfade über den installierten `gate_hook.py --review` wurde als Werkzeugaufruf vor der Ausführung blockiert. Ein Kritikerurteil liegt deshalb nicht vor. Eine weitere redigierte Journal-Auswertung wurde ebenfalls vor der Ausführung blockiert. Daraus folgt weder ALLOW noch BLOCK des Kritikers. Kein alternativer Umgehungspfad wurde benutzt.

Weiter offen: Gesamtinventar und abschließender Statusvergleich, Verbraucher- und Gesamttests am finalen Commit, reguläres Test-/Merge-Gate, Main-Integration, Release-Build mit -j 2, kontrollierte Einrichtung der aktiven TOML, Dienstdeploy, PID-/Binary-/Journal-/Heartbeat-Nachweise, authentifizierter lesender Broker-Test und erfolgreiche API-Funktion. Ebenso sind die abschließende Dev-/Support-Dokumentation nach Deadlock-Docs und die operativen Einträge nach dem 2nd-Brain-Schema noch nicht als erledigt bestätigt.

MERGEPROTOKOLL[MS-1]: Feature-Commits einzeln geprüft und gepusht | Main-Anläufe: 0 | Gate: keine Main-Freigabe

LIVEBEWEIS[DV-1]: nicht erbracht | PID 1109 unverändert | kein Deploy | Turnierliste vor Migration HTTP 500

TEXTNACHWEIS[DR-1]: Gedankenstriche 0 | ae/oe/ue/ss-Ersatz 0 | Absolutwörter 0 belegt | Senke: Status dieser Task-Übergabe
