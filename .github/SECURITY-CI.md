# PR Security CI

## Status und Geltungsbereich

Diese Änderung ist PR-Testbetrieb, keine Produktionsfreigabe. Kein Workflow deployt,
startet einen Bot, schreibt Turnierdaten oder führt einen Merge aus. Ein grüner
Einzelscanner ersetzt niemals den `Required PR Gate`.

Der überprüfte Stand enthält ein npm-Projekt (`frontend/package.json`) und den
Rust-Workspace `rust/Cargo.toml` mit 16 Crates. Das Frontend ist React/TypeScript/Vite,
das aktive Backend Rust/axum/PostgreSQL. Unter `backend/` sind keine ausführbaren
Python-Module mehr versioniert. Historische Python-/SQLite-Anleitungen sind keine
CI-Inventarliste.

## Pflichtprüfungen

`security.yml` läuft auf jedem `pull_request`, einschließlich Dokumentations-PRs
und PRs auf andere Zielbranches, außerdem auf `merge_group`, den bisherigen
Main-Branch-Pushes, manuell und weiterhin sonntags um 04:17 UTC. Es gibt keine
Workflow-Pfadfilter und keine geheimnisabhängig übersprungenen Pflichtjobs.

| Job | Blockierende Entscheidung |
| --- | --- |
| Frontend | Node 24.14.1, `npm ci`, ESLint ohne Warnungen, `npm test`, `npm run build` |
| Rust | Private Abhängigkeit vorhanden; Rust 1.97.1 fmt, Clippy mit `-D warnings`, Build aller Targets, sämtliche Workspace-Tests |
| Rust dependency audit | `cargo-audit 0.22.2` prüft das vollständige `rust/Cargo.lock`; Schwachstellen blockieren |
| Secret Detection | Verbotene Geheimnisdateien und Gitleaks 8.30.1 über die vollständige erreichbare Git-Historie |
| JavaScript Dependency Audit | `npm ci`, `npm audit --audit-level=high`, einschließlich Entwicklungsabhängigkeiten |
| Semgrep SAST | Semgrep 1.173.0 mit `--error --strict --severity ERROR`; ERROR-Findings und Scannerfehler blockieren |
| CodeQL Analysis | JavaScript/TypeScript mit `security-extended`; Analysefehler und Findings mit Security-Severity mindestens 7.0 blockieren |
| Trivy Filesystem Scan | Trivy 0.73.0, `vuln,misconfig,secret`, HIGH/CRITICAL, Entwicklungsabhängigkeiten eingeschlossen, `--exit-code 1`; auch Scanner-/Downloadfehler blockieren |
| Workflow policy | actionlint 1.7.12, zizmor 1.30.1 ab LOW, Projektinventar und negative Gate-Gegenproben |

Das Frontend definiert `lint`, `test` und `build`, aber derzeit kein separates
`typecheck`. Der verpflichtende Build führt tatsächlich `tsc -b && vite build`
aus. Wird ein eigenes `typecheck`-Script ergänzt, wird auch dieses ausgeführt.
Ein neues npm-Manifest lässt die Inventarprüfung scheitern, bis dessen CI ergänzt
wurde. Die vorhandenen Dependabot-Einträge für `/frontend` und GitHub Actions `/`
bleiben erhalten; es gibt keine erfundenen Manifestpfade.

Der abschließende Job heißt exakt **Required PR Gate**. Er hat `if: always()` und
alle neun Pflichtjobs als direkte `needs`. `.github/ci/gate.rs` akzeptiert nur
vollständig vorhandene `success`-Ergebnisse. Fehler, Abbruch, `skipped`, leere oder
fehlende Ergebnisse sowie ein abgebrochener Workflow sind nicht erfolgreich.
Die gleiche Entscheidungsfunktion wird mit positiven und negativen Fällen getestet.

## Private Rust-Abhängigkeit: bewusst keine Freigabe durch Auslassen

`dl-central-db` ist eine echte Pfadabhängigkeit aus `EarlySalty/Deadlock-Bots`.
Das Schwester-Repository ist privat; **Deadlock-Turniere ist öffentlich**. Der
geprüfte Commit steht in `.github/ci/central-db-revision.txt`. Der vorhandene
Produktionspfad bleibt unverändert und es gibt weder einen Ersatz-Stub noch eine
öffentliche Kopie der privaten Implementierung oder ihrer Migrationen.

Der GitHub-Rust-Job legt beide Checkouts in der benötigten Geschwisterstruktur an.
Er verwendet ausschließlich den normalen, lesenden Repository-Token. Dieser
verleiht keinen automatischen Zugriff auf das private Schwester-Repository.
Solange keine ausdrücklich freigegebene, isolierte Lösung für diese Abhängigkeit
vorliegt, muss der Checkout fehlschlagen und damit der Required PR Gate rot bleiben.
Das ist ein offener Infrastruktur-/Vertraulichkeitsblocker, kein erfolgreicher
Rust-Testlauf. Produktionssecrets oder private Quellcode-Artefakte in einem
öffentlichen PR-Run sind keine zulässige Abkürzung.

Ein Maintainer muss separat die Bereitstellung einer ausdrücklich zur Veröffentlichung
freigegebenen Bibliothek oder eine sicher isolierte private Build-Integration
entscheiden. Bis dahin darf die Rust-Prüfung nicht per `if`, `continue-on-error`,
fehlendem Secret oder Dummy-Implementierung neutralisiert werden.

## Isolierte Datenbanktests

Rust-Tests benutzen eine frische TimescaleDB-2.17.2/PostgreSQL-16-Instanz auf dem
GitHub-Runner mit zufälligem Host-Port und ausschließlich Wegwerf-Zugangsdaten.
`CENTRAL_TEST_DSN` und `DEADLOCK_CENTRAL_DSN` zeigen auf diese Instanz.
`TURNIER_TEST_DB_CONFIRM=throwaway-only` ist ausdrücklich gesetzt. Das bestehende
Test-Harness erstellt pro Fachtest eine eigene Datenbank mit den echten zentralen
Migrationen und räumt sie wieder auf. `--include-ignored --test-threads=2` verhindert,
dass historische Ignore-Markierungen als vollständiger Testlauf erscheinen.

Der Startup-Schemavertrag wurde auf das vorhandene `test_pool()` umgestellt,
statt eine vorab migrierte oder möglicherweise produktive Verbindung zu verwenden.
Kein Test ruft die öffentliche Turnierseite auf. Web-/DAST-Scans wurden nicht
hinzugefügt; insbesondere gibt es keinen Scan des produktiven Dienstes. Die
zusätzlichen PR-Deep-Scans sind vollständige SAST-/SCA-/Historienprüfungen, nicht DAST.

## Reproduzierbarkeit und Gegenproben

Alle externen Actions sind auf überprüfte vollständige Commit-SHAs gepinnt.
Semgrep und die Testdatenbank verwenden überprüfte Linux/amd64-Image-Digests.
`.github/ci/tools.tsv` enthält offizielle Release-URLs und SHA256-Digests aus den
jeweiligen GitHub-Releases. Der kleine Rust-Installer verweigert unbekannte Tools,
fehlgeschlagene Downloads, abweichende Checksums und uneindeutige Archive.

`.github/ci/semgrep-default.yml` ist der am 24. September 2026 abgerufene öffentliche
`https://semgrep.dev/c/p/default`-Regelstand, begrenzt auf 253 Regeln für
JavaScript, TypeScript, Rust, Generic, Bash, JSON und YAML. Die Regelmetadaten
bleiben erhalten. Eine deklarierte Sprache garantiert noch keine eigene Regel:
der aktuell blockierende ERROR-Lauf deckt insbesondere JavaScript/TypeScript,
JSON, YAML und sprachübergreifende Muster ab; Rust wird zusätzlich durch Clippy,
Cargo-Audit und die echten Backend-Tests geprüft. Updates des Regelbundles sind
reviewpflichtige Änderungen. Semgrep läuft im gepinnten Container ohne Netzwerk,
mit schreibgeschütztem Quellcode und ohne Telemetrie.

`scanner-probe.rs` erzeugt ausschließlich temporäre synthetische Eingaben und
prüft die tatsächlich gestarteten Scanner. Je Scanner müssen ein sauberer Fall,
ein Finding und ein Konfigurationsfehler die erwarteten unterschiedlichen
Exit-Codes liefern. Die künstliche Gitleaks-Zeichenfolge wird erst zur Laufzeit
zusammengesetzt, ist kein Zugangsschlüssel und wird nicht committet. Trivy prüft
ein temporäres Dockerfile, keine laufende Instanz. Alle Gegenproben laufen auch
auf PRs. Sie sind selbst Pflichtprüfungen, keine dauerhaft ignorierten Fehler.

Scanner- und Advisory-Datenbanken entwickeln sich weiter. Gepinnte Binaries,
Lockfiles und Regeln machen die ausgeführte Entscheidung reproduzierbar; sie
frieren neue Sicherheitsmeldungen nicht ein. Ein Datenbank-/Netzwerkfehler ist
kein Beleg für einen sauberen Scan.

## Reporting, Ausnahmen und Grenzen

Nur der separate SARIF-Reporting-Job erhält `security-events: write`. Er führt
keinen Checkout und keinen PR-Code aus. SARIF wird zuvor unabhängig von der
blockierenden Scannerentscheidung als kurzlebiges Artefakt gespeichert. Nur diese
Uploads haben `continue-on-error`; ein fehlgeschlagener Scanner bleibt rot.
Reporting ist bei Fork-PRs absichtlich ausgenommen und gehört nicht zum Required
PR Gate. Der Token der Prüfjobs hat nur `contents: read`; Checkouts speichern
keine Credentials. Es gibt kein `pull_request_target`, keine produktiven Secrets,
keinen Self-hosted Runner und kein LLM-/Copilot-Merge-Gate.

Bewusst nicht blockierend sind SARIF-/Artefakt-Verfügbarkeit, Semgrep WARNING/INFO,
CodeQL-Findings mit gültigem Schweregrad unter 7.0, npm-/Trivy-Schweregrade unter HIGH und die regulären
Cargo-Audit-Warnungen zu Wartungsstatus, Yank oder Unsoundness ohne als
Vulnerability klassifizierten Eintrag. Diese Grenzen sind keine Behauptung,
dass entsprechende Befunde harmlos wären.

`.gitleaksignore` enthält 14 einzeln geprüfte historische Fingerprints, jeweils
begrenzt auf Commit, Datei, Regel und Zeile: acht SHA256-Dateiprüfsummen aus
Testnachweisen, vier numerische Discord-Test-IDs, einen expliziten lokalen
Test-Sentinel und eine öffentliche Cloudflare-Analytics-Beacon-ID. Keine Datei,
kein Regeltyp und kein zukünftiger Commit wird pauschal ausgenommen. Die echte
synthetische Geheimnis-Gegenprobe muss trotz dieser Datei weiterhin anschlagen.

Weitere enge Ausnahme: Semgrep scannt sein eigenes versioniertes Regel-Datenbundle nicht
als Anwendungsquellcode. Vier dort enthaltene Erkennungsmuster für Reverse Shells
wurden sonst als ausführbare Reverse Shells gemeldet. Die Datei wird weiterhin als
Regelkonfiguration geladen und strikt validiert. Es gibt keine pauschale Ausnahme
für den Anwendungscode und keine ausgeschaltete Scannerfehlerbehandlung.

Eine befristete Cargo-Audit-Ausnahme betrifft ausschließlich `RUSTSEC-2023-0071`
(Marvin/RSA, im Lockfile `rsa 0.9.10`). Der Eintrag stammt aus der optionalen
SQLx/MySQL-Auflösung und ist in keinem ausgeführten Rust-Target vorhanden:
`cargo tree --workspace --all-features --target all --locked --invert rsa` liefert
keine Abhängigkeitskette. Der Pflichtjob prüft genau diese Bedingung erneut und
blockiert bei jeder Aktivierung von RSA. Der Audit-Job blockiert außerdem ab
1. Dezember 2026, bis die Ausnahme erneut geprüft oder entfernt wurde. Das ist
keine Behauptung, RSA sei repariert; alle anderen Advisory-IDs bleiben blockierend.
Die gefundenen Schwachstellen in quinn-proto und rustls wurden durch kompatible
Lockfile-Updates beseitigt. Auch die gemeldeten anyhow-/event-listener-Probleme
wurden aktualisiert, nicht ausgeblendet.

Die CodeQL-Entscheidung steht in `ci/codeql-policy.jq` und wird gegen 15 synthetische
SARIF-Eingaben getestet. Fehlende Runs, fehlende Ergebnisse, unbekannte Regeln,
fehlende oder ungültige Severity, Scannerfehler und kaputtes JSON sind Fehler.
Ein fehlender Severity-Wert wird nicht als Null interpretiert.

Rust-Formatierung prüft die 16 Mitglieder des aktuellen virtuellen Workspaces mit
`cargo fmt -- --check`. `--all` wird hier bewusst nicht verwendet, weil es auch
die externen Pfadabhängigkeiten und deren fremden Workspace einbeziehen würde. Generierte Abhängigkeiten, Build-Ausgaben und lokaler
CI-Scratch werden nicht als zusätzlicher Quellcode gescannt; ihre Lockfiles bleiben
im Scan. Der minimale Namensfilter für erlaubte `.env.example`-Dateien erlaubt
keine darin enthaltenen echten Secrets.

## GitHub-Schutzstatus und Abnahme

Die API-Prüfung vom 24. September 2026 ergab `rulesets: []` und für `main`
`Branch not protected` (HTTP 404). Diese Änderung behauptet daher **keinen bereits
bestehenden serverseitigen Merge-Zwang**. Nach einer genehmigten Einführung muss
`Required PR Gate` als erforderlicher GitHub-Actions-Statuscheck für `main` gesetzt
werden. Das sollte nicht vorab andere PRs aussperren, deren Basis-Workflow den
neuen Check noch nicht enthält. In diesem PR-Testauftrag wurden keine Schutzregeln
entfernt oder umgangen und kein Main-Merge ausgeführt.

Ein erfolgreicher lokaler Teiltest ist keine Gesamtabnahme. Für die Abnahme müssen
PR-URL, getesteter Head-SHA, zugehörige Actions-Run-URLs, tatsächlich ausgeführte
Tests, verbliebene Befunde und dieser Schutzstatus gemeinsam betrachtet werden.
Der private Rust-Abhängigkeitsblocker muss ausdrücklich offen bleiben, solange
der echte GitHub-Rust-Testlauf nicht möglich ist.

Referenzen: [Semgrep CLI](https://docs.semgrep.dev/cli-reference),
[Trivy Exit-Code](https://trivy.dev/docs/latest/guide/configuration/others/),
[zizmor](https://docs.zizmor.sh/usage/).
