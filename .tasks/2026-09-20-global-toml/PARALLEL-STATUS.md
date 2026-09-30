# Abstimmung zwischen den laufenden Sitzungen

Stand: 20.09.2026, 13:32 Europe/Berlin.

In der weiteren ChatGPT-Sitzung zum gleichen Nutzerauftrag wurde zunächst derselbe bereits vorhandene Worktree geprüft. Änderungen an file.rs, Config-Vorlage und TODO.md zwischen 13:18 und 13:28 wurden nicht von dieser Sitzung geschrieben. Der Überschneidungshinweis ist deshalb keine Abnahme dieser Änderungen.

Diese Sitzung hat bisher folgende Dateien bearbeitet:

- ops/systemd/60-global-toml.conf neu: expliziter kanonischer TOML-Pfad, geleertes EnvironmentFile, kein ExecReload.
- ops/systemd/deadlock-turniere.service.example: TOML-Start und bestehender Infisical-Bootstrap.
- start_backend.ps1: explizite TOML, Prüfung vor Start, Build mit -j 2, keine vorgezogene Datenverzeichnis-Erstellung.

Kein Merge, kein Restart und keine Produktionsdatei wurde von dieser Sitzung verändert. Ab jetzt keine parallelen Kerncode-Änderungen dieser Sitzung im gemeinsamen Worktree. Die Startpfad-Ergänzungen werden zusätzlich auf einem getrennten Feature-Branch gesichert. Bitte diese Datei zur Koordination lesen und den eigenen Integrationsstand hier ergänzen, bevor derselbe Dienst deployt wird.

Prüfungen dieser Sitzung:

1. test-workspace-resume.log: Cargo ohne Postgres-Harness endet erwartbar mit fehlender CENTRAL_TEST_DSN. Das ist keine fachliche Abnahme.
2. test-postgres-resume.log: vorhandener central_test_db.sh, cargo test --workspace --features testing --no-fail-fast -j 2 -- --include-ignored. Laufende Prüfung, am Zwischenstand Fehler in invalid_proposal_transition_returns_conflict, den drei scrim_lobby_flow-Tests und notifier::tests::mentions_baut_korrekt. Ein Vergleich gegen unverändertes main steht noch aus. Keine Testausnahme als bewiesenen Altfehler behandeln.
3. Graphify funktioniert über /home/nathanael/.local/bin/graphify. Der kurze Programmname fehlt im PATH des Connectors.
4. Produktions-PID 1109, NRestarts 0, ausgeführtes Binary /home/nathanael/repos/Deadlock-Turniere/rust/target/release/turnier-bot. Produktiver Branch main, SHA c9fad4346cd0d86fce542dcb2da6f2dc0cadf047.

Die weitere Sitzung übernimmt zunächst den unabhängigen Baseline-Vergleich und die sichere Startpfad-Prüfung. Kernimplementierung und Deploy-Verantwortung müssen vor einer gemeinsamen Abnahme zusammengeführt werden.

## 13:36: unabhängige Sicherung und Baseline

Eigener Ergänzungsbranch: feat/turniere-global-toml-ops-20260920 unter /home/nathanael/.worktrees/turniere-global-toml-ops-20260920, Basis a721693.

Die vollständige erste PostgreSQL-Suite ist beendet: sechs fehlgeschlagene Tests in vier Targets. Zusätzlich zu den oben genannten Tests schlägt turnier-draft/tests/lobby_db.rs::rematch_nach_abschluss_tauscht_die_seiten fehl. Nichts davon wurde ausgelassen.

Die identische Gesamtsuite läuft jetzt gegen den unveränderten Produktions-SHA c9fad4346cd0d86fce542dcb2da6f2dc0cadf047 in einem eigenen detached Baseline-Worktree /home/nathanael/.worktrees/turniere-global-toml-baseline-20260920. Log: /home/nathanael/.worktrees/turniere-global-toml-ops-20260920/.tasks/2026-09-20-global-toml/test-main-baseline.log. Dadurch bleibt der Produktionscheckout unberührt. Bitte keinen zweiten Baseline-Gesamtlauf starten.

## Ergänzung gepusht

Commit `37594e7` auf `origin/feat/turniere-global-toml-ops-20260920` enthält die sieben grün geprüften Launcher-Vertragsfälle sowie die Startpfade. Die im Kern-Worktree bereits vorliegenden drei Startpfad-Dateien sind identisch mit dieser Sicherung; sie dürfen nicht doppelt unabhängig weiterentwickelt werden. Neue eigene Datei im Ergänzungsbranch: `scripts/test_config_launcher.sh`. Keine Rust-Kernänderungen durch diese Sitzung.

Der Baseline-Gesamtlauf ist abgeschlossen: main 503 bestanden/6 fehlgeschlagen, erster gemeinsamer TOML-Zwischenstand 535/6. In beiden Läufen 0 ignorierte Tests und Exit 101. Dieselben sechs Assertion-Paare einschließlich Rematch-Fehler sind belegt; JSON mit Rohlog-Hashes ist eingecheckt. Ein finaler Lauf am unveränderlichen Abnahme-Commit bleibt erforderlich.

Neuester gepushter Ergänzungsstand: `4998f0728a404195a05416525d719ba7f02cbda0`. Draft-PR #8 zielt auf `feat/turniere-global-toml-20260920`, nicht auf main. Die separate Kernabnahme unter `feat/turniere-global-toml-abnahme-20260920` wurde erkannt. Kein konkurrierender Main-Merge oder Deploy durch die Ergänzungssitzung.

Wichtiger bestehender Live-Befund: GET /api/tournaments lokal und GET /turnier/api/tournaments öffentlich liefern HTTP 500. Health und UI-Dateien liefern HTTP 200; die Health enthält noch keinen TOML-Anker. PID 1109, NRestarts 0 und Produktions-main c9fad434 unverändert. Keine Turnieraktionen als Test. Ursache des API-Fehlers noch nicht geklärt.

Der Teilreview-Werkzeugaufruf wurde vor Ausführung blockiert, daher kein Kritikerurteil und keine Main-Freigabe. Detaillierte Übergabe im gepushten Ergänzungsbranch: `.tasks/2026-09-20-global-toml/OPS-UEBERGABE.md`. Produktionsabnahme und zentrale Abschlussdokumentation sind dadurch nicht ersetzt.
