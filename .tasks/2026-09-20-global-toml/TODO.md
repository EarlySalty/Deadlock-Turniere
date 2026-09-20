# Zentrale TOML-Konfiguration für Deadlock-Turniere

Stand: 20.09.2026. Status: Bestandsaufnahme und Umsetzung offen, keine Produktionsabnahme.

## Auftrag und Grenzen

Eine zentrale `config/bot.toml` für globale Betriebswerte. Bestehendes `turnier-config` erweitern, keine zweite Konfigurationskette. Secrets verbleiben in der vorhandenen Infisical-Anbindung. Turniere, Anmeldungen, Teilnehmer, Sessions und fachlich dynamische Regeln bleiben in Postgres. Keine echten Ankündigungen als Test. Keine Änderungen anderer Bots.

## Nachgewiesener Ausgangspunkt

- Kanon: `/home/nathanael/repos/Deadlock-Turniere`; Documents-Pfad ist ein Symlink.
- Produktionsbranch laut Launcher und Git: `main`, Ausgangs-SHA `c9fad4346cd0d86fce542dcb2da6f2dc0cadf047`.
- Remote: `git@github.com:EarlySalty/Deadlock-Turniere.git`.
- Kanon zu Beginn sauber. Fremde Worktrees bleiben erhalten.
- Eigener Branch: `feat/turniere-global-toml-20260920`; Worktree `/home/nathanael/.worktrees/turniere-global-toml-20260920`.
- User-Unit: `deadlock-turniere.service`, Ausgangs-PID 1032401, NRestarts 0. Eine laufende Unit ist noch kein Binary- oder Funktionsnachweis.
- Startpfad: `scripts/run_turniere_backend_rust.sh`, bisher mit gesourcter Betriebskonfiguration und Infisical-Loader. Unit enthält zusätzlich ein gemeinsames EnvironmentFile.
- `turnier-config/src/lib.rs`: vorhandene zentrale `Config`, bisher `from_env`, unveränderlich als `Arc<Config>` an `AppState` und Module weitergegeben.
- `turnier-config/src/secrets.rs`: vermischt Betriebswerte und Secrets; dynamische `NAME_FILE`-Schlüssel und Credentials-Verzeichnisse. Fehlerpfade von Integer-/Bool-Lesern schreiben bisher den fehlerhaften Wert ins Log.
- Weitere direkte Betriebseinstellungen: Observer-Agent und Testmodus; Logging liest bislang den ENV-Filter.
- Keine zentrale TOML im untersuchten Repo gefunden; Cargo-Manifeste sind keine Betriebskonfiguration.

## Arbeitsfolge

1. Vollständige Verbraucher-/Quellenmatrix und sicheren Vorher-Status fertigstellen; aktive Werte statt angenommener Defaults übernehmen. Zugehörige Dienste und Timer, ausgeführte Binary und Startpfade abschließend prüfen.
2. Typisierten TOML-Lader im bestehenden Config-Crate implementieren. Schema-Version, strikte unbekannte Felder, Pflichtwerte, Zahlen-/ID-/URL-/Pfadvalidierung und Fehler ohne Eingabewerte. Vor Clients und Dateisystem-Nebenwirkungen laden.
3. Betriebs-ENV-Leser und dynamische Betriebs-Keys entfernen; Secret-Resolver auf echte Secret-Namen begrenzen. Nicht nach ENV zurückexportieren. Module verwenden dieselbe geprüfte Momentaufnahme.
4. Dokumentiertes Restart-only-Verhalten statt unbelegtem Hot-Reload. Prüfmodus ohne DB-, Scheduler- oder Client-Seiteneffekte.
5. Tests für Parser, Grenzen, ENV-Isolation, Redaction, Pfade, fehlende Datei, unveränderte laufende Momentaufnahme bei fehlerhafter Datei und echte Verbraucher. Bestehende Fachtests getrennt von Bestandsfehlern auswerten.
6. Test-Gate und unabhängigen Merge-Kritiker durchlaufen, Feature-Commits sofort pushen. Erst danach Integration nach main und Release-Build mit `-j 2`.
7. Deploy: PID-Wechsel, exe ohne deleted, Fehlerjournal, Binary-Anker, API-/UI-/Broker-Nachweise und periodischen Heartbeat/NRestarts erheben. Keine mutierenden Turniertests.
8. Dev-/Support-Doku nach Deadlock-Docs `internal/deadlock-turniere/`, operative Erkenntnisse nach 2nd-Brain. Gemergte eigene Branches und Worktrees erst nach Live-Abnahme entfernen.

## Unterbrechungen und Restunsicherheit

Der Worktree-Anlageaufruf lief in einen Timeout; der Connector meldete anschließend zeitweise `Session terminated`. Nach Wiederherstellung wurde der Worktree einschließlich Branch und Ausgangs-SHA bestätigt. Kein Merge oder Deploy wurde dadurch ausgelöst.

Die alten Defaults sind noch kein Nachweis der produktiv wirksamen Werte. Die 2nd-Brain-Systemakte nennt noch einen Python-Service und ist veraltet; deren Korrektur benötigt den tatsächlichen Binary-Nachweis.

## Pflichtprotokolle

MERGEPROTOKOLL[MS-1]: offen | Anläufe: 0 | Gate: noch nicht ausgeführt
LIVEBEWEIS[DV-1]: PID 1032401->offen | exe ungeprüft | journal -p err ungeprüft | Anker offen | Funktion: ungeprüft | Ort: Turnier-Oberfläche, API und Broker noch zu prüfen
TEXTNACHWEIS[DR-1]: Gedankenstriche 0 | ae/oe/ue/ss-Ersatz 0 | Absolutwörter 0 belegt | Senke: Task-Akte


## Wiederaufnahme: geprüfter Zwischenstand am 20.09.2026

Der Feature-Worktree bleibt isoliert. Produktions-HEAD ist c9fad4346cd0d86fce542dcb2da6f2dc0cadf047; der laufende Dienst hatte bei der Wiederaufnahme PID 1109 und NRestarts 0. Kein Deploy und keine Datenmigration in diesem Zwischenstand.

Der typisierte Lader, die direkte Verbraucherübergabe und die expliziten Startargumente sind implementiert. Zusätzlich sind Rangrollen, Vorschlags-Freigaberollen, Rang-/Unterrang-Caches, Bridge-Zeitgrenzen, DB-Pool, Draft-Limits und der Config-Fingerabdruck im Health-Endpunkt verdrahtet.

Prüfung: cargo check --workspace --all-targets -j 2 erfolgreich (check-r2.log, Finished nach 1m 09s). cargo test -p turnier-config -j 2: 21 bestanden, 0 fehlgeschlagen. Das ist noch keine fachliche Gesamtabnahme.

Ein isolierter Rust-Inspektor hat die bisherige Konfiguration über die bestehende produktive Bootstrap-Reihenfolge aufgelöst. Der Inspektor öffnet keine Datenbank und startet keine Clients oder Scheduler. Die Bootstrap-Ausgabe wurde unterdrückt; ausgegeben wurden nur Vergleichsergebnisse und ein nicht geheimer Fingerabdruck. 42 Alt-Felder sowie Testmodus und Logging stimmen mit dem damaligen TOML-Kandidaten überein. Beleg: baseline/comparison.json. Dessen Fingerabdruck gehört zum Zwischenstand, nicht zum noch ausstehenden Release. Die zusätzliche temporäre Bootstrap-Datei wurde entfernt. Der Inspektorquelltext ist nur Audit-Material unter baseline/, kein produktiver ENV-Lader.

Noch offen: vollständige Inventarmatrix und Grenzprüfungen, weitere Verbraucher-/Fachtests, Prüfer-Gate, endgültiger Statusvergleich, Produktionsintegration, Release-Build, Unit-Umstellung, Live-Prüfung von API/UI/Broker und Scheduler, Dokumentation sowie Cleanup. Insbesondere ist dieser Branch nicht produktiv abgenommen.
