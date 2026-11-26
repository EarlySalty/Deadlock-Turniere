# 08-lobby-steam – P2 · Steam-Bot-Flag und Lobby-Automatik

## Abhängigkeiten und Schreibbereich
Nach 07; Turniere/Deadlock-Steam-Bot, koordiniertes Schnittstellenpaket.

## Vorhandene Arbeit / Quellen
[P6/P7 im Originalauftrag/REGISTER/GRILLME](https://github.com/EarlySalty/Deadlock-Turniere/tree/7ae479f1eefb87805c50d3b9ff4015779970a728/.tasks/2026-11-02-turnier-ui-pug); turnier-match/auto_lobby.rs, Steam-Bridge, Observer und Broker-Verträge.
Weitere Quellen: [QUELLEN.md](../QUELLEN.md) und [BESTANDSAUFNAHME.md](../BESTANDSAUFNAHME.md).

## Auftrag
Rekonstruiere den genauen P6-Flag-Auftrag aus Originalquellen, keine neue Semantik erfinden. Prüfe vollständige automatische Lobby-Kette inklusive geplanter/übersprungener Lobbys, Steam-Bot-Verfügbarkeit, Task-Idempotenz, Retry/Timeout, Lobby-/Matchzuordnung, Voice und Caster. Echte Lobbyprobe nur in ausdrücklich freigegebenem Testevent mit Nutzer, keine Live-Spieler verschieben oder benachrichtigen.

## Abnahmekriterien
Isolierte Integration mit Stub/Mock für Bridge, Timeout/Offline/Retry/Doppelaufruf/Restart, eindeutige sichtbare Fehler; echte P7-Probe getrennt nach Zustimmung protokollieren. Gate; ohne echte Probe als noch nicht vollständig abgenommen kennzeichnen.

## Aktueller Übergabestand und Ergänzung
Live-Steam-Lobbyprobe bleibt dem Maintainer nach gesonderter Freigabe vorbehalten. Isolierte Mock-Abnahme und genaue Live-Prüfanleitung liefern.

## Arbeitsrahmen
Lies [ARBEITSRAHMEN.md](../ARBEITSRAHMEN.md), [RESTAUFTRAG.md](../RESTAUFTRAG.md) und [REGISTER.md](../REGISTER.md). Aktuelle Repo-Review-Regeln haben Vorrang vor historischer Gate-Methodik. Eigene Umgebung, isolierte Tests, vorhandene Remote-Branches, keine T3-Delegation. Liefere Feature-Branch, PR, SHA, konkrete Testbefehle/Ergebnisse, unabhängigen Review-Status, offene Entscheidungen und gegebenenfalls fehlenden Live-Nachweis. Keine eigenmächtigen Merges oder Deployments.
