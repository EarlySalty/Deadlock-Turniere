# Steam-Bridge-Implementation

Die Steam-Bridge koppelt das Python-Turnierbackend an den separaten Steam-Bot, ohne dass beide Prozesse direkt RPC oder Webhooks gegeneinander sprechen. Stattdessen nutzen sie eine gemeinsame Task-Queue in SQLite. Das Backend schreibt Aufträge hinein, der Steam-Bot nimmt sie auf, arbeitet gegen den Deadlock Game Coordinator und schreibt das Ergebnis wieder zurück.

## Warum die Queue?

Die Queue-Lösung erfüllt drei praktische Ziele:

- Backend und Steam-Bot können unabhängig neu starten
- hängengebliebene Aufgaben sind sichtbar und nachprüfbar
- das Turnierbackend muss keine GC-Protokolldetails direkt kennen

Die Bridge ist damit bewusst eine Entkopplungsschicht und kein fachliches Matchmodul.

## Task-Lifecycle

Ein Task durchläuft typischerweise:

- `PENDING`
- `RUNNING`
- `DONE` oder `FAILED`

Beim Anlegen schreibt das Backend Typ, Payload und Zeitstempel. Der Steam-Bot reserviert sich den Task, setzt `RUNNING`, arbeitet ihn ab und liefert entweder ein JSON-Ergebnis in `result` oder eine Fehlermeldung in `error`.

Das Backend pollt auf Abschluss. Es wartet also nicht auf einen Push vom Bot, sondern fragt aktiv nach, bis `DONE` oder `FAILED` erreicht ist.

## Relevante Task-Typen

Für Turniere sind vor allem diese Typen wichtig:

- `GC_CREATE_CUSTOM_LOBBY`
- `GC_LOBBY_SET_SPECTATOR`
- `GC_LOBBY_READY`
- `GC_LOBBY_START_MATCH`
- `GC_GET_MATCH_RESULT`
- `GC_LOBBY_LEAVE`

Der Aufbau ist immer ähnlich: Das Match- oder Party-Kontextobjekt wird als Payload übergeben, der Bot führt die passende GC-Aktion aus und liefert das Ergebnis in einem standardisierten JSON zurück.

## Nutzung im Backend

Das eigentliche Matchmodul ruft die Bridge nicht roh an jeder Ecke auf, sondern über gezielte Service-Funktionen. Typischer Ablauf:

1. Match-Manager validiert den Matchzustand
2. Bridge erstellt einen Task
3. Backend pollt auf das Resultat
4. Erfolgsdaten werden in Matchfelder übernommen
5. Fehler landen als Exception oder Fehlpfad im HTTP-Response

Dadurch bleibt GC-spezifische Technik aus den Admin-Routen weitgehend heraus.

## Schutz vor Hängern und Duplikaten

Die Bridge enthält zwei wichtige Schutzmechanismen:

- alte `RUNNING`-Tasks werden nach einem Zeitfenster als fehlgeschlagen markiert
- aktive Aufgaben für denselben Match-/Task-Typ können vor neuer Erstellung erkannt werden

Der erste Punkt verhindert, dass abgestürzte Worker die Queue dauerhaft blockieren. Der zweite Punkt schützt vor Doppelstarts wie "Lobby erstellen" zweimal hintereinander.

## Polling-Charakter

Die Bridge arbeitet bewusst mit Polling statt mit bidirektionaler Echtzeitverbindung. Das ist langsamer als ein sauberer Service-Bus, aber für den Anwendungsfall robust genug:

- Lobbies und Matchstarts sind seltene Operator-Aktionen
- Ergebnis-Fetches müssen korrekt sein, nicht millisekundenschnell
- Fehlerdiagnose ist einfacher, weil jede Aufgabe persistent sichtbar bleibt

## Grenzen

Die aktuelle Bridge ist stark auf einen einzelnen Steam-Bot und eine gemeinsame Queue ausgelegt. Risiken entstehen vor allem, wenn:

- mehrere konkurrierende Worker dieselbe Queue bedienen
- Queue-Datei nicht lokal und stabil erreichbar ist
- GC-Aufrufe länger hängen als die erwarteten Timeouts

Für ein größeres Multi-Worker-Setup wäre mittelfristig ein sauberer Dienst mit dedizierter API robuster. Für die jetzige Turnierarchitektur ist die SQLite-Bridge aber pragmatisch und nachvollziehbar.

Kurz gesagt: Die Steam-Bridge ist der technische Adapter zwischen Turnierlogik und Deadlock-Lobby-/Matchsteuerung. Fachlogik gehört nicht hier hinein; sie liefert nur transportierbare Tasks und verlässliche Ergebnisse.
