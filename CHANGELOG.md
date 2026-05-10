## #5 — CI-Optimierung und Double-Elimination-Feinschliff

- Täglichen Security-Scan-Schedule entfernt — Security läuft bei Push und PRs
- Semgrep blockiert Build nicht mehr bei Findings
- Bracket-Datenmodell erweitert: Loser-Bracket-Verlinkungen (source_match, loser_to_match) direkt im Modell
- Bracket-Ansicht zeigt Winner-/Loser-Bracket-Verbindungen klarer an

## #4 — Automatische Security-Pipeline eingerichtet

- Dependabot überwacht ab jetzt Python-Backend, npm-Frontend und GitHub Actions täglich
- Bei jedem Push wird automatisch geprüft ob Abhängigkeiten bekannte CVEs haben
- Jeder Release wird kryptografisch signiert (SBOM + Provenance-Attestierung via Sigstore)

## #3 — Sicherheitslücken in Frontend-Abhängigkeiten geschlossen

- vite und weitere Build-Pakete auf gepatchte Versionen angehoben — schließt Path-Traversal und ReDoS-Lücken

## #2 — Double Elimination & smartere Turnier-Brackets

- Double Elimination funktioniert jetzt richtig: Wer im Hauptbaum verliert, kommt in den Loser-Bracket; das Finale entscheidet wie üblich Sieger oben gegen Sieger unten, mit Bracket-Reset wenn der Loser-Bracket-Sieger das erste Finalspiel gewinnt
- Die Bracket-Anzeige zeigt jetzt Winner-Bracket, Loser-Bracket und Grand Final getrennt — bei Single Elim sieht alles aus wie bisher
- Gruppenphase rechnet die Anzahl Gruppen automatisch passend zur Team-Anzahl aus (Ziel: 4 Teams pro Gruppe), damit die Aufteilung immer Sinn ergibt
- Aus den Gruppen geht's per Cross-Seeding ins Bracket: 1A trifft 2B, 1B trifft 2C usw. — Gruppensieger spielen nicht mehr versehentlich direkt gegeneinander in Runde 1
- Gruppenphase startet jetzt erst ab 16 Teams (vorher 12). Kleinere Turniere gehen direkt ins Bracket, wie es im eSports üblich ist. Admins können das pro Turnier weiterhin überschreiben

## #1 — UI-Überarbeitung: Grünes Design, Regelwerk & Header-Verbesserungen

- Kontrastfarbe von Orange auf Deadlock-typisches Grün umgestellt (bessere Lesbarkeit auf dunklem Hintergrund)
- Der Begriff "Kodex" wurde überall in "Regelwerk" umbenannt
- Im Header oben rechts wird beim Profil nur noch der Name angezeigt — das "Spieler"-Label entfällt
- Profil- und Admin-Button im Header sind jetzt gleich groß und einheitlich gestaltet
- Die Turnier-Panels in der "Chroniken"-Sektion haben jetzt gleichmäßige Abstände und überlappen sich beim Hover nicht mehr
