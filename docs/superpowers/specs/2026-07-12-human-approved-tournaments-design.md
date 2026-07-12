# Human-approved Routine-Turniere — Design

## Ziel

Die KI darf Routine-Turniere nur vorschlagen und aus Mod-Feedback überarbeiten. Sie darf weder selbstständig ein Turnier veröffentlichen noch eine öffentliche Ankündigung senden. Verbindliche Aktionen entstehen ausschließlich aus zwei getrennten menschlichen Freigaben.

## Zuständigkeiten

- `Deadlock-Bots` besitzt Discord-Gateway, Components-V2-Nachricht, Buttons, Modals, Rollenprüfung und den vorhandenen `dl-ai`-Client.
- `Deadlock-Turniere` bleibt alleinige Datenquelle für Vorschläge, Versionen, Stimmen, Feedback und Turniere.
- Der Master-Bot ruft dafür eng begrenzte interne Turnier-Endpunkte auf. Die Turnier-Domäne prüft den Zustand erneut; Discord allein ist keine Vertrauensgrenze.
- Vorschlagskanal ist `1474543558793887937`.

## Ablauf

1. Der Routine-Check erzeugt nur einen Vorschlag für den nächsten Termin. Er legt kein Turnier an und öffnet keine Anmeldung.
2. Der Master-Bot lässt aus Preset, Termin und gelernter Präferenz-Zusammenfassung einen strukturierten Plan erzeugen und postet ihn als goldene Components-V2-Karte.
3. Die Karte bietet:
   - `Y – Zeit & Freigabe`
   - `N`
   - `Änderung vorschlagen`
4. Abstimmen dürfen ausschließlich Mitglieder mit Mod- oder Community-Mod-Rolle.
5. `Y` bedeutet gleichzeitig: Die Person ist zum vorgeschlagenen Termin verfügbar und gibt den Plan frei.
6. `N` öffnet ein Modal mit Pflichtgrund. Ein N blockiert die Freigabe nicht, bleibt aber sichtbar und fließt in spätere Vorschläge ein.
7. `Änderung vorschlagen` öffnet ein Modal mit Pflichtkommentar. Die KI erzeugt daraus eine neue Version derselben Proposal-Kette. Alle Stimmen der alten Version werden ungültig; die neue Version benötigt erneut zwei frische Y-Stimmen.
8. Zwei Y-Stimmen von zwei unterschiedlichen berechtigten Personen planen das Turnier genau einmal ein und öffnen die Anmeldung.
9. Danach erzeugt die KI im selben internen Kanal nur eine Ankündigungsvorlage. Mods kopieren, ändern oder ersetzen sie und veröffentlichen selbst.

## Datenmodell

- Jeder Vorschlag besitzt eine unveränderliche Versionsnummer und optional die ID seiner Vorgängerversion.
- Stimmen sind pro Vorschlagsversion und Discord-Nutzer eindeutig.
- Feedback speichert Typ (`reject` oder `change`), Rohtext, Autor und Zeitpunkt.
- Die Live-Schaltung speichert Turnier-ID und Zeitpunkt, sodass Wiederholungen keine zweite Anlage erzeugen.
- Die Präferenz-Zusammenfassung wird aus bisherigen Entscheidungen und Gründen neu berechnet; rohe Kommentare bleiben für Mods nachvollziehbar.

## KI-Grenze

- Mod-Eingaben gelten als vertrauenswürdig; es wird keine zusätzliche Prompt-Injection-Infrastruktur gebaut.
- Modellantworten werden trotzdem gegen ein enges strukturiertes Schema validiert, weil ein Modell auch bei legitimen Eingaben fehlerhafte Daten liefern kann.
- Die KI liefert ausschließlich Daten und Text. Datenbankänderung, Discord-Versand und Live-Schaltung bleiben deterministische Anwendungsschritte.
- Timeout, Providerfehler oder ungültige Antwort lassen den aktuellen Zustand unverändert und werden sichtbar protokolliert.

## Sichtbarkeit und Audit

- Die Karte zeigt Version, Termin, Plan, Y/N-Zähler, Namen der Abstimmenden und vorhandene Einwände.
- Jede Entscheidung wird protokolliert: Y, N, Änderung, neue KI-Version, Timeout, Fehler und Live-Schaltung.
- Doppelte Button-Klicks überschreiben höchstens die eigene Stimme und erhöhen den Zähler nicht.
- Nicht berechtigte Klicks erhalten nur eine ephemere Ablehnung und verändern nichts.

## Bestehendes Auto-Turnier

Vor Aktivierung des neuen Flows wird das aktuell automatisch erzeugte Routine-Turnier über den bestehenden Domänen-/Admin-Löschpfad entfernt. Die Löschung wird nur ausgeführt, wenn der Datensatz eindeutig als aktuelles `source = routine`-Turnier identifiziert ist; bei mehreren Kandidaten wird nicht geraten.

## Dokumentation und Betrieb

- Die bestehende HTML-Dokumentation zu Routine-Turnieren wird auf den Human-in-the-loop-Ablauf aktualisiert.
- `CHANGELOG.md` beschreibt Problem, Änderung und aktuelles Verhalten ohne interne Implementierungsdetails.
- Beide betroffenen Rust-Workspaces werden formatiert, getestet, mit Clippy geprüft, gemergt, gebaut und ihre User-Services neu gestartet.
- Live-Nachweis umfasst neue PID, frische Binary und fehlerfreies Journal; zusätzlich wird ein realer Vorschlag im Zielkanal geprüft.

## Nicht im Scope

- Kein automatisches Veröffentlichen der Ankündigung.
- Kein Modell-Finetuning oder autonomes Online-Lernen.
- Kein zusätzlicher Discord-Bot und kein zweiter KI-Client.
- Kein allgemeines Abstimmungssystem außerhalb von Routine-Turnieren.
