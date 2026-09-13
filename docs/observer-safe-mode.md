# Scrim Observer: Safe Mode und Testbetrieb

Stand: 13. September 2026.

## Sicherheitsziel

Der Observer darf einen wertvollen Steam-Account nicht fuer einen Kameratest einem ungeklaerten Anti-Cheat-Risiko aussetzen. Deshalb sind Live-Datenanalyse und Spieleingaben getrennt.

Der normale Testbetrieb ist `Shadow` oder `Assist` und benoetigt **keine** automatisierte Interaktion mit dem Deadlock-Prozess.

Nicht Teil des normalen Betriebs:

- keine DLL-Injection oder Prozess-Hooks,
- kein Lesen/Schreiben von Deadlock-Prozessspeicher,
- keine Veraenderung von EXE/DLL/Spieldateien,
- kein `-insecure`, kein Abschalten von VAC,
- kein `sv_cheats` oder Dev-/Cheat-Modus,
- keine beliebigen Remote-Console-Kommandos vom Server.

Valve dokumentiert die normalen Observer-Bedienelemente und Spectator-Console-Befehle im Deadlock-Observer-Guide. Valve/Steam warnt zugleich vor Drittsoftware, die in Spielprozesse eingreift. Eine ausdrueckliche Valve-Freigabe fuer automatisierte VConsole-Kamerasteuerung liegt uns nicht vor. Daher ist diese Steuerung standardmaessig aus.

Referenzen:

- https://forums.playdeadlock.com/resources/guide-to-deadlock-esports-and-content-creation-observing-and-camera-controls.28/
- https://help.steampowered.com/en/faqs/view/571A-97DA-70E9-FF74

## Harte Gates

Es gibt mehrere voneinander unabhaengige Sperren:

1. `SCRIM_OBSERVER_ENABLED=false` ist der globale Default fuer das Observer-Subsystem.
2. `SCRIM_OBSERVER_GAME_CONTROL_ENABLED=false` ist der serverseitige Kill-Switch fuer jede automatisierte Kameraaktion.
3. `OBSERVER_GAME_CONTROL_ENABLED=false` ist der lokale Kill-Switch auf dem Observer-PC. Bei `false` verbindet sich der Agent nicht mit VConsole und bestaetigt Kamera-Kommandos mit `game_control_disabled_safe_mode` als abgelehnt.
4. Auto ist serverseitig nur fuer eine Session mit eigener Scrim-/Draft-Bindung **und** bekannter Lobby-Party-ID zulaessig. Eine Session, die nur mit einer fremden/oeffentlichen Match-ID angelegt wurde, kann niemals Auto aktivieren.
5. Auto verlangt eine aktive Bot-2-Observer-Reservierung. Die Headless-Steam-/GC-Session von `steam-core-2` darf dann nicht gleichzeitig laufen.
6. Auto verlangt frischen Agent-Heartbeat, bestaetigte Game-Session und den explizit freigegebenen Game-Control-Pfad.

## Empfohlener erster Test: oeffentliches Random-Match

Ziel: Director und Live-Feed unter Realbedingungen pruefen, ohne das Spiel automatisiert zu steuern.

1. Im Observer-Panel `Steam Bot 2 -> Fuer Observer reservieren` waehlen.
2. Warten, bis Headless Steam und GC fuer Bot 2 als `aus` angezeigt werden.
3. Auf dem dedizierten Observer-PC Steam normal mit Bot 2 starten.
4. Deadlock normal starten. Keine unsicheren Startparameter und keinen Observer-Agent mit Game-Control starten.
5. Ueber Deadlocks normale Watch-/Spectator-Oberflaeche ein laufendes Match manuell beobachten.
6. Match-ID im Observer-Panel unter `Manueller Match-Test` eintragen und `Shadow starten` waehlen.
7. Der Turnier-Server liest den Live-Matchfeed und schreibt Entscheidungen in `scrim.observer_decisions`.
8. Das Dashboard zeigt empfohlenen POV, Score, Grund und die letzten Entscheidungen. Es findet keine automatisierte Spieleingabe statt.
9. Optional auf `Assist` wechseln. Der Mensch bedient den Deadlock-Observer weiterhin ausschliesslich mit den normalen Observer-Bedienelementen und nutzt die Serverempfehlung als Regiehilfe.
10. Nach dem Match die Entscheidungstimeline gegen Replay/Highlights auswerten.

Fuer diesen Test wird `SCRIM_OBSERVER_GAME_CONTROL_ENABLED=false` beibehalten. `OBSERVER_GAME_CONTROL_ENABLED` wird auf dem PC nicht gesetzt oder bleibt `false`.

## Eigene Scrims

Nach dem Shadow-Nachweis kann derselbe Director an den bestehenden Scrim-/Draft-Flow gebunden werden:

Draft -> Custom Lobby -> Match-ID -> Live-Feed -> Director -> Shadow/Assist.

Die Lobby-, Team-, Match-ID- und Ergebnisautomation bleibt beim vorhandenen Steam-/Turnier-System. Bot 2 ist der dedizierte Observer-Account; Bot 1 bleibt der regulaere Lobby-/Steam-Controller.

## Auto-Kamera

Auto ist **nicht** Bestandteil des ban-risikoarmen Public-Tests.

Der Code fuer eine eng allowlistete VConsole-Steuerung existiert als experimenteller Transport, wird aber durch beide Game-Control-Kill-Switches standardmaessig nicht benutzt. Bevor er mit einem wertvollen Account aktiviert wird, muss die Betriebsentscheidung dokumentiert werden, einschliesslich des Nachweises, warum die konkrete Steuerungsmethode fuer den vorgesehenen Observer-Betrieb akzeptabel ist.

Bis dahin lautet die produktive Kombination: Server-Regie in `Shadow`/`Assist`, menschliche Kamera ueber Deadlocks normale Observer-Steuerung.
