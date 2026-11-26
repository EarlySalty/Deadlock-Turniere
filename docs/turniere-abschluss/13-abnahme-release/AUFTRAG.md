# 13-abnahme-release – P0 · Gesamtintegration, Abnahme und Abschluss

## Abhängigkeiten und Schreibbereich
Nach allen releasekritischen Paketen; 04/09/10/11 getrennt geprüft, dokumentierte Produktentscheidungen dürfen als ausdrücklich offene Erweiterungen verbleiben.

## Vorhandene Arbeit / Quellen
Alle Paket-PRs, Commit-SHAs und Review-Nachweise aus [REGISTER.md](../REGISTER.md) und [RESTAUFTRAG.md](../RESTAUFTRAG.md), aktuelle Remote-Basen sämtlicher Repos. Produktive Release-Nachweise werden vom Maintainer geliefert.
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Externer Abnahme-Agent (Maintainer bleibt Integrationsverantwortlicher): verifiziere, dass fertige Pakete tatsächlich auf Main und in kompatiblen Releases angekommen sind, nicht nur in Worktrees. Abnahme eines isolierten Testturniers: Gast/Discord, 30 Solo-Spieler, 5x6, manuelle Teams, Check-in, Draft, Match/Lobby, Caster/Voice, Ergebnisse, Rangliste, Archiv; zusätzlich reguläre Team-, Gruppen-/Double-Elim- und Liga-Pfade. Prüfe Browser, Logs, Fehler, Schema, Broker, Steam/Twitch/Caddy. Keine Live-Schreibtests mit echten Nutzern, kein Rewrite #16/#17. Für produktive Abnahme/Restart sichere laufende Events und Rollback; bei Konflikt Freigabe abwarten. Alte Branches/Worktrees erst nach Beweis archivieren, nicht löschen.

## Abnahmekriterien
Komplette Suites und unabhängige Abnahme; Gate-ALLOW, Remote-SHAs, tatsächliche Build-/Release-SHAs, Health/HTTP/UI-Nachweise, kompatible Frontend/Backend-Version, Rollback getestet oder verifiziert. Dem Maintainer die Belege je Punkt übergeben; nur er führt produktive Integrations-/Release-Aktionen aus. Abschlussbericht nach lokaler Vorgabe.

## Aktueller Übergabestand und Ergänzung
Der externe Agent kann portable Tests und Release-/Rollback-Anleitung liefern. Nur Maintainer führt Merge/Deploy, echte Lobby-/Discord-/Twitch-Probe und produktive Datenkorrekturen aus. Fehlende Live-Beweise offen nennen; kein lokales Setup des Maintainers voraussetzen.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
