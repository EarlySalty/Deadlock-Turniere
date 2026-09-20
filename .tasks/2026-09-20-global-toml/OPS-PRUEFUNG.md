# Startpfade und unabhängige Prüfung

Stand: 20.09.2026. Ergänzungsbranch `feat/turniere-global-toml-ops-20260920`, Basis `a7216930b8226ac2e608c4829a1184c407f1a2f3`.

Diese Ergänzung gehört zum laufenden Auftrag für Deadlock-Turniere. Sie ist nicht separat produktionsreif: Die vollständige Kernmigration auf `feat/turniere-global-toml-20260920` und deren Gates fehlen auf dieser Basis noch. Kein anderer Bot wird geändert.

## Änderungen

Die systemd-Vorlage verwendet einen absoluten `--config`-Pfad und keine ENV-Betriebswerte. Das kanonische Drop-in `ops/systemd/60-global-toml.conf` setzt denselben Pfad, leert die alte EnvironmentFile-Liste und bietet keinen Reload an. Vorhandene Infisical-Credentials bleiben in ihrem bestehenden Drop-in.

Das Windows-Startskript übergibt die aufgelöste TOML an das Binary, prüft sie vor dem Start und baut bei Bedarf mit `-j 2`. Es legt vor der Prüfung kein Datenverzeichnis an. Ein Windows-Lauf wurde in dieser Linux-Sitzung nicht behauptet.

Der bereits vorhandene, vollständig gelesene TOML-Launcher wurde aus dem Kern-Worktree übernommen. Die Betriebsdatei wird weder gesourct noch in ENV exportiert. Nur die bestehende Infisical-Bootstrap-Datei wird weiterhin geladen. Die Prüfung erfolgt davor. Kein eigener Secret-Store und keine neue Infisical-Implementierung.

## Startskript-Vertrag

`bash scripts/test_config_launcher.sh`: sieben Fälle bestanden, Exit 0.

Die isolierte Prüfung verwendet einen ausdrücklich dokumentierten Binary-Stub für die Argumentgrenze, keine echte Secret-Anbindung. Sie prüft fehlende und relative Pfade, unzulässige und mehrfache Prüfmodi, exakte Weitergabe an `--check-config` und `--print-config` sowie Abbruch des normalen Starts vor Infisical, wenn das Binary die Datei ablehnt. Die echte TOML-Deserialisierung gehört zu den Rust-CLI-Tests der Kernmigration, nicht zu diesem Stub-Test.

`git diff --check`: Exit 0. Kein Merge, kein Build für die Produktion und kein Dienstneustart durch diese Ergänzung.

## Baseline der Fachtests

Beide Gesamtsuiten wurden ohne ausgelassene Tests im vorhandenen isolierten PostgreSQL-Harness ausgeführt. Der unveränderte Produktions-SHA `c9fad4346cd0d86fce542dcb2da6f2dc0cadf047` hat 503 bestandene und sechs fehlgeschlagene Tests. Der erste TOML-Zwischenstand hat 535 bestandene und dieselben sechs fehlgeschlagenen Tests. Die sechs Assertion-Paare stimmen exakt überein. Beide Prozesse endeten mit Exit 101, null Tests wurden ignoriert. Das ist ein belegter roter Altbestand, keine vollständig grüne Suite.

Der TOML-Lauf entstand im noch nicht unveränderlich festgehaltenen gemeinsamen Worktree. Er ersetzt daher ausdrücklich nicht den erneuten Lauf am finalen Abnahme-Commit. Der Baseline-Lauf erfolgte in einem eigenen sauberen detached Worktree. Die Produktionsdatenbank wurde nicht für Tests verwendet.

`baseline-test-comparison.json` hält Zähler, Fehler und SHA-256 der beiden Rohlogs fest. Logorte: `.tasks/2026-09-20-global-toml/test-postgres-resume.log` im Kern-Worktree und `.tasks/2026-09-20-global-toml/test-main-baseline.log` im Ergänzungs-Worktree.

## Koordinationsgrenze

Die zweite Sitzung hat inzwischen einen eigenen Abnahme-Worktree `/home/nathanael/.worktrees/turniere-global-toml-abnahme-20260920` auf `feat/turniere-global-toml-abnahme-20260920` angelegt. Dieser Ergänzungsbranch startet keinen konkurrierenden Main-Merge oder Produktionsdeploy. Die Startpfade und Prüfnachweise sind gepusht und können in dem Abnahmestand zusammengeführt werden. Der ursprüngliche gemeinsame Worktree bleibt erhalten.
