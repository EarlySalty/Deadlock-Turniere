# Admin-Routes

Die Admin-Oberfläche des Turnier-Backends hängt primär an `backend/tournament/admin_routes.py` und ergänzt sich mit `operations_routes.py` für das Tagesgeschäft rund um Off-Stream-Ergebnisse. Auth-seitig gibt es zwei relevante Schienen:

- `require_admin` für echte Verwaltungs- und Konfigurationsaktionen
- `require_mod` für operative Moderationsaufgaben wie Ergebnisbestätigung oder Stream-Markierung

## Route-Gruppen

Die Admin-Routen lassen sich praktisch in sieben Blöcke einteilen:

1. Turnier-Lifecycle
   - Turniere listen, anlegen, bearbeiten und löschen
   - Check-in öffnen, zurückrollen und finalisieren
   - Status manuell weiterdrehen
   - Gruppen oder Bracket generieren

2. Team- und Signup-Management
   - Teams manuell anlegen, umbenennen oder löschen
   - Recruiting-Status verwalten
   - Signups Teams zuweisen oder entfernen
   - Mitglieder verschieben, hinzufügen oder entfernen
   - Captain wechseln

3. Match-Steuerung
   - Lobby erstellen
   - Match über Steam starten
   - Ergebnis aus Steam holen
   - Lobby verlassen
   - Match zurücksetzen
   - manuelle Lobby-Codes setzen
   - manuelle Match-Ergebnisse eintragen

4. Gruppenmatch-Steuerung
   - dieselben Kernaktionen wie bei Bracket-Matches, aber auf `group_matches`

5. Caster- und Event-Steuerung
   - verfügbare Caster laden
   - Caster auf Turniere oder einzelne Matches setzen
   - Event-Presets und Convars auf Matches anwenden

6. Voice-Operations
   - ganze Teams in Voice-Channels verschieben
   - Sammelpunkt-Moves
   - einzelne Nutzer verschieben
   - Channel-Mitglieder abrufen

7. Action Items / Result Reports
   - offene Captain-Meldungen für Off-Stream-Matches abrufen
   - Meldungen bestätigen oder verwerfen
   - Stream-Flag auf Matches toggeln

## Wichtige Nebeneffekte

Viele Routen tun mehr als nur ein DB-Update. Typische Nebeneffekte sind:

- Audit-Log-Einträge
- Discord-Benachrichtigungen
- Queue-Aufträge an die Steam-Bridge
- Bracket-Advancement nach bestätigten Ergebnissen
- Cleanup von Match-Channels beim Reset

Das ist wichtig für spätere Änderungen: Wer eine Admin-Route umbaut, ändert fast nie nur die HTTP-Antwort, sondern meist auch Betrieb, Logging und Folgezustände.

## Rechte-Modell

Nicht jede Admin-nahe Aktion braucht Volladmin. Das Backend trennt bewusst:

- `require_mod` für laufenden Turnierbetrieb
- `require_admin` für Struktur- und Konfigurationsänderungen

Beispiel: Ein Match-Ergebnis bestätigen oder ein Stream-Flag setzen ist Moderationsarbeit. Ein Turnier anlegen, Serienformate ändern oder Brackets neu generieren ist Admin-Arbeit.

## Kritische Flows

Besonders sensibel sind folgende Bereiche:

- `finalize-checkin`: friert die Anmeldelage für die nächste Turnierphase ein
- Match-Reset: löscht operative Match-Spuren und kann Discord-Ressourcen entfernen
- manuelle Ergebnisse: können direkt den Bracket-Verlauf verändern
- Team-Mutationen kurz vor Turnierstart: beeinflussen Signups, Captain-Rollen und Seeding

Bei diesen Flows ist das Zusammenspiel mit `tournament.engine`, `match.manager`, `match.result_processor`, `series_manager` und `notifications.discord_notifier` zentral.

## Empfehlung für Änderungen

Neue Admin-Funktionen sollten sich an das bestehende Muster halten:

- vor dem Eingriff Rechte prüfen
- Zielobjekt mit eigenem `_load_*_or_404` laden
- Status- oder Kapazitätsregeln früh validieren
- Nebeneffekte nicht inline erfinden, sondern in bestehende Match-/Tournament-Services delegieren
- Audit-Log nicht vergessen

Kurz gesagt: `admin_routes.py` ist der Orchestrator für Admin-HTTP, aber die eigentliche Fachlogik lebt verteilt in Engine-, Match- und Notification-Modulen. Änderungen gehören daher fast nie nur in diese eine Datei.
