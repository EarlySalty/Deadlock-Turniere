# Match-Integration

Die Match-Integration verbindet den Turnierbaum mit der operativen Match-Abwicklung. Technisch sitzt der HTTP-Einstieg in den Admin-Routen, die eigentliche Orchestrierung aber in `backend/match/manager.py`, `backend/match/result_processor.py`, `backend/match/series_manager.py` und `backend/tournament/engine.py`.

## Lifecycle eines Matches

Für Bracket-Matches ist der Standardpfad:

1. Match existiert im Status `pending`
2. Admin erstellt eine Lobby
3. Match wechselt auf `lobby_created`
4. Admin startet das Match
5. Match wechselt auf `in_progress`
6. Ergebnis wird automatisch oder manuell eingetragen
7. Match landet auf `completed`
8. Der Gewinner wird im Bracket weitergeschoben

Zusätzlich kennt das Modell `forfeit` und `cancelled`. Diese Zustände sind relevant für Sonderfälle, auch wenn der tägliche Fokus klar auf `pending`, `lobby_created`, `in_progress` und `completed` liegt.

## Einstieg über Admin-Routen

Die Match-Integration ist aus dem API-Blick zweigeteilt:

- klassische Admin-Steuerung in `admin_routes.py`
- operative Captain-/Mod-Flows in `operations_routes.py`

Admins können Lobbys erstellen, Matches starten, Ergebnisse aus Steam abrufen, manuelle Lobbies setzen, Matches zurücksetzen und Seriengames verwalten. Captains dürfen für Off-Stream-Bracket-Matches Ergebnisse melden; Moderatoren bestätigen oder verwerfen diese Meldungen später.

## Ergebnisquellen

Es gibt drei Resultatpfade:

- automatischer Fetch über Steam
- manuelle Eintragung durch Admin
- Captain-Self-Report für Off-Stream-Matches mit nachgelagerter Mod-Bestätigung

Alle drei Wege laufen am Ende auf denselben Result-Processor hinaus. Dadurch bleibt die Folgelogik konsistent: `winner_id`, Matchdauer, Matchstats, Datenhistorie und Bracket-Fortschritt werden zentral gesetzt.

## Bracket-Advancement

Nach einem bestätigten Ergebnis wird der Gewinner nicht in der Route selbst "per Hand" in das nächste Match geschrieben. Stattdessen übernimmt `tournament.engine` die Fortschaltung. Das ist entscheidend, weil dort die Strukturregeln für Winners-, Losers- und Grand-Final-Pfade gebündelt sind.

Bei Fehlern im Advancement ist deshalb fast nie die Admin-Route die richtige Baustelle, sondern eher:

- Bracket-Generierung
- Match-Referenzen
- Result-Processor
- Engine-Logik für das nächste Zielmatch

## Serien und Spezialfälle

Neben einzelnen Match-Ergebnissen gibt es Serien-Handling für Best-of-Formate. Admin-Routen können einzelne Spiele einer Serie anlegen oder ihr Resultat verbuchen. Das bedeutet: Ein Match ist nicht immer nur eine einmalige Sieger-Markierung, sondern kann aus mehreren Games bestehen, bevor der Match-Sieger feststeht.

Zusätzlich existieren No-Show- und Stream-Marker-Mechaniken:

- No-Show-Meldungen bekommen eine Grace-Period
- offene Meldungen landen im Action-Item-Feed
- `on_stream` trennt Stream- und Parallelmatches operativ

## Daten, die mitlaufen

Zu einem Match gehören im Betrieb mehr als nur zwei Teams und ein Sieger. Relevante Felder sind unter anderem:

- Lobby-/Party-Information
- Deadlock-Match-ID
- Status
- Gewinner
- Matchdauer
- Matchstats
- Timestamps
- gegebenenfalls Match-Caster-Zuordnungen

Das erklärt, warum Resets und manuelle Korrekturen heikel sind: Sie berühren nicht nur den Baum, sondern auch Lobby- und Betriebszustand.

## Praktische Risiken

Die sensibelsten Punkte sind:

- doppelte Lobby-Erstellung
- Ergebnis doppelt setzen
- Match resetten, obwohl externe Ressourcen schon existieren
- Captains melden ein Ergebnis, während das Match eigentlich noch live ist

Deshalb arbeiten die Routen mit Statusprüfungen, Kapazitätschecks, Audit-Logs und zentralen Service-Funktionen statt mit frei formulierten Direktupdates.

Kurz: Die Match-Integration ist kein einzelnes Modul, sondern eine Kette aus Admin-HTTP, Steam-Steuerung, Ergebnisverarbeitung und Bracket-Logik. Wer daran arbeitet, muss immer alle vier Ebenen im Blick behalten.
