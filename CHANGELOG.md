## #18 — Scrim-Betrieb läuft vollständig im Turnierdienst

**Ausgangslage:** Die Scrim-Verwaltung war auf mehrere Dienste verteilt. Teile davon meldeten Erfolg, obwohl eine Discord-Nachricht gar nicht angekommen war, und abgelaufene Aushilfen liefen nie ab. **Änderung:** Kader, Terminabfragen, Ersatzsuche, Matches, Lobbys und Ankündigungen laufen jetzt an einer Stelle; jede Zustellung wird dauerhaft vermerkt, damit ein zweiter Versuch nachholt statt doppelt zu posten. **Aktuelles Verhalten:** Scheitert eine Discord-Zustellung, wird das jetzt gemeldet und der Vorgang bleibt gespeichert — ein erneuter Aufruf reicht die Nachricht nach, statt sie stillschweigend zu verlieren. Aushilfe-Anfragen kommen wieder mit Zusagen- und Absage-Knopf, und Ergebnisabrufe wie Erinnerungen werden auch dann weiterverarbeitet, wenn gerade niemand zusieht.

## #17 — Scrim-Planung bekommt einen neutralen Turnierkern

**Ausgangslage:** Scrim-Planung, Terminantworten und Match-Auswertung waren noch an den Discord-Bot gekoppelt und konnten nicht sicher schrittweise umgezogen werden. **Änderung:** Der Turnierdienst erhält einen neutralen Matchkern sowie eine interne, abgesicherte Scrim-Schnittstelle mit eindeutigen Übergaben, offenen Gegnern, gemeinsamen oder matchspezifischen Slots und nachvollziehbaren Freigaben. **Aktuelles Verhalten:** Der bestehende Betrieb bleibt unverändert aktiv; der neue Pfad kann parallel geprüft werden und übernimmt erst nach der kontrollierten Umschaltung Schreibzugriffe.

## #16 — Python-Altstand liegt nur noch im Archiv

**Ausgangslage:** Nach dem Rust-Cutover lag der alte Python-Stand weiter im Hauptrepo. Dadurch war nicht klar, was noch produktiv ist und was nur noch als Referenz dient. **Änderung:** Der alte Stand wurde im Backup festgehalten und aus dem laufenden Repo entfernt. Die Dienste laden ihre Geheimnisse jetzt über den Rust-Weg. **Aktuelles Verhalten:** Das Hauptrepo zeigt den aktuellen Rust-Betrieb, während der entfernte Altstand im Backup nachvollziehbar bleibt.

## #15 — Eigene Draft-Lobbys ohne Turnier

**Ausgangslage:** Für einen Helden-Draft brauchte man bisher ein angelegtes Turnier und musste Picks, Bans und Zugzeiten außerhalb der Seite koordinieren. **Änderung:** Unter `/turnier/draft` lässt sich eine freie Lobby mit Teamnamen, Preset und optionalem Zugtimer erstellen; beide Captains bekommen getrennte Links, und der Stand bleibt in der Datenbank erhalten. **Aktuelles Verhalten:** Zwei Captains können denselben Draft in getrennten Browsern bis zum Ende spielen, beim Wettbewerbs-Preset mit zwei Bans je Team; abgelaufene Züge werden automatisch ausgeführt und ein Backend-Neustart verliert die Lobby nicht.

## #14 — Turniere brauchen zwei Mod-Freigaben

**Ausgangslage:** Die Turnier-Automatik konnte nach einem Zeitplan selbst ein Turnier anlegen und ankündigen. **Änderung:** Sie erstellt nur noch einen internen Vorschlag; N und Änderungswünsche werden als Lernhistorie für spätere Vorschläge gespeichert. **Aktuelles Verhalten:** Der sichtbare KI-Plan wird vor dem Discord-Post verbindlich gespeichert, Änderungen wechseln nacheinander und alte Karten-Buttons führen automatisch zur aktiven Version; erst zwei unterschiedliche J von Mods legen das Turnier an. Die Ankündigung steht nur als interne Vorlage in derselben Karte, ein fehlgeschlagenes Edit bleibt offen und kann erneut versucht werden.

## #13 — Turnier-Automatik bis zur Mod-Freigabe gestoppt

**Ausgangslage:** Ein gelöschtes Routine-Turnier wurde vom laufenden Wochenplaner erneut angelegt und angekündigt. **Änderung:** Der automatische Planungspfad ist vollständig abgeschaltet, bis die neue Abstimmung mit zwei Mods bereitsteht. **Aktuelles Verhalten:** Der Bot erstellt, öffnet und veröffentlicht keine Routine-Turniere mehr selbstständig.

## #12 — Turniere laufen jetzt von allein an

**Ausgangslage:** Turniere sind nur entstanden, wenn sich jemand freiwillig um Erstellung, Freigabe und Erinnerungen gekümmert hat. Das hat kaum noch jemand gemacht, also gab es kaum noch Turniere.

**Änderung:** Der Bot legt Turniere jetzt selbst an. Im festen Wochenrhythmus entsteht aus einer freigegebenen Vorlage automatisch ein Turnier, die Anmeldung öffnet sich von selbst und die Ankündigung landet im Turnier-Channel. Die bekannten Erinnerungen greifen wie gewohnt.

**Aktuelles Verhalten:** Die Automatik ist eingebaut, aber noch nicht angeschaltet. Sobald wir sie aktivieren, gibt es jede Woche ein festes Turnier, ganz ohne Orga-Aufwand.

## #11 — Caster-Freigabe wirklich verbindlich, DM-Wunsch lückenlos respektiert

**Ausgangslage:** Mit der Turnier-Automatik (#10) sollte eine Turnier-Freigabe ausschließlich von einem Caster kommen — über einen direkten Status-Weg ließ sich das aber umgehen. Und der „keine Turnier-DMs"-Wunsch sollte auf jedem Benachrichtigungsweg greifen.

**Was wurde geändert:** Die Freigabe ist jetzt an jeder Stelle an die Caster-Rolle gebunden, auch über den direkten Status-Weg — wer kein Caster ist, kann ein Turnier nicht mehr durchwinken. Der DM-Opt-out wird vor jedem einzelnen Versand geprüft; ist der Status mal nicht eindeutig, wird im Zweifel nicht zugestellt.

**Wie es jetzt läuft:** „Nur Caster geben frei" gilt ohne Schlupfloch, und wer keine Turnier-DMs will, bekommt auch keine.

## #10 — Turnier-Automatik: Presets, Vorschläge & DM-Einstellungen

Bisher musste jedes Turnier komplett von Hand aufgesetzt werden — bei jedem Mal alle Einstellungen neu zusammenklicken. Im Admin-Bereich gibt es jetzt einen eigenen "Automatik"-Tab: wiederverwendbare Presets (Modus, Format, Regeln, Kategorie Fun/Comp) anlegen und pflegen und daraus mit wenigen Klicks ein Turnier einplanen. Solche Vorschläge haben einen eigenen Status-Verlauf (Entwurf → Freigabe offen → angenommen/abgelehnt) samt Votes und Änderungs-Feedback, der im selben Tab gesteuert wird.

Für Spieler kommt im Profil eine Einstellung dazu, ob man DMs zu Fun- bzw. Comp-Turnieren bekommen möchte — oder gar keine. Das betrifft nur die Benachrichtigungen, nie die Teilnahme.

Das ist die Grundlage für die kommende automatische Turnier-Planung: dass der Bot selbst Turniere vorschlägt und nach Freigabe durch einen Caster ankündigt, folgt im nächsten Schritt.

## #9 — Turnier-Doku jetzt im FAQ-Bot abrufbar

- Anmeldung, Modi, Leaderboard, Consent-Flow und Draft sind jetzt zentral dokumentiert — Spieler bekommen Fragen dazu direkt im Discord-FAQ beantwortet, ohne ein Ticket eröffnen zu müssen

## #8 — Erinnerungen, Selbstmeldung & aufgeräumte Admin-Seite

- Teilnehmer bekommen jetzt automatisch eine Discord-DM, wann das Turnier startet und wenn ihr Match als Nächstes dran ist — freundlicher Hinweis statt verpasster Matches
- Teams können Off-Stream-Ergebnisse selbst melden (mit Deadlock-Match-ID), ein Admin bestätigt sie per Klick — so kann das Loser-Bracket parallel zum Haupt-Bracket laufen, ohne dass alles über den Stream muss
- Erscheint ein Gegner nicht, lässt sich das melden; nach einer einstellbaren Frist kann der Admin einen Walkover bestätigen
- Neuer "Aktion erforderlich"-Leitstand auf der Admin-Seite zeigt auf einen Blick, welche Matches eine Bestätigung oder Entscheidung brauchen; die Phasen-Navigation ist jetzt eine klare Fortschritts-Schiene statt verschachtelter Tabs
- Bei jedem Match steht jetzt die Wertung in der Lobby-Ansage (z.B. "erster Walker gewinnt"), und der Bot markiert automatisch, welche Matches auf Stream und welche parallel laufen

## #7 — Dependency-Pinning erzwungen

- requirements.txt auf exakte Versionen (==) umgestellt — was pip-audit prüft ist jetzt auch was deployed wird
- CI blockiert ab sofort wenn >= oder ~= in requirements.txt auftaucht

## #6 — Tiefere Security-Scans: Trivy und Lizenz-Audit

- Trivy scannt jetzt bei jedem Push das Filesystem auf HIGH/CRITICAL CVEs
- Lizenz-Audit warnt bei Copyleft-Lizenzen (GPL/AGPL) in Python-Abhängigkeiten

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
