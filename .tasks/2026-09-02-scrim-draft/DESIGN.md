# Design-Spec: Scrim-Draft im Stil von deadlocklabs.gg

status: aktiv
datum: 2026-09-02

Quelle: neun Screenshots des Nutzers vom deadlocklabs.gg Draft-Tool (Ablage `/home/nathanael/.t3/userdata/attachments/59896288-*`). Der Look wird nachgebaut, nicht kopiert (INV-06). Marke: Schwarz `#0b0b0b`, Gold `#c8a86b` aus `Website/dl-brand/tokens.css` ersetzt das Orange von deadlocklabs. Team 1 gold, Team 2 blau (`#3b82f6`-Bereich), Ban rot (`#ef4444`-Bereich), Bereit grün.

## Grundstimmung

- Vollflächig dunkel, sehr dunkles Grau bis Schwarz, dezenter radialer Verlauf in Team-Farbe hinter der jeweils aktiven Seite (links warm gold, rechts kühl blau).
- Hintergrund der Startseite: großes, stark abgedunkeltes Raster aus Hero-Portraits (Opacity etwa 0.08).
- Typografie: enge, fette Versalien für Titel und Teamnamen (Display-Font), gesperrte kleine Versalien (letter-spacing 0.2em) für Labels wie "WARTEN AUF SPIELER", "BAN-PHASE 1/16", "DU BIST DRAN".
- Alles hat weiche Glows in der Teamfarbe: Karten-Rahmen 1px in Farbe plus box-shadow 0 0 40px Farbe bei 25 Prozent.
- Bewegung mit framer-motion: Einblenden von unten (y 12px, 200 ms), Skalieren von Splash-Art (1.02 auf 1.0, 400 ms), Ban-Stempel mit Überschwingen (scale 1.3 auf 1.0, 250 ms, ease-out-back).

## Screen 1: Start (`/turnier/draft`)

- Zentrierter Titel zweizeilig: "DEADLOCK DRAFT" (zweites Wort in Gold mit Glow) und "TOOL" darunter, Unterzeile "Scrim- und Turnier-Draft".
- Segment-Umschalter "Draft anlegen | Raum beitreten".
- Vorschau-Karte: links Team-1-Name-Eingabe (gold, Platzhalter "Name..."), mittig Kreis-Badge "6v6" mit Timer darunter, rechts Team-2-Name-Eingabe (blau). Darunter "Live-Vorschau" mit Ban-Kästchen und je 6 Slot-Punkten in Team-Farbe, Demo-Punkt grün "DEMO".
- Einstellungszeilen als Pill-Gruppen: "FORMAT" (6v6 aktiv, 4v4 und 2v2 deaktiviert), "BANS" (Strich, 1 bis 6, Default 2 aktiv), "TIMER" (Aus, 30s, 45s, 60s, 90s, Default 30s). Aktive Pill: Gold-Rahmen und Gold-Text.
- Großer Primärknopf über volle Kartenbreite: "DRAFT STARTEN" mit Play-Glyphe, Gold-Fläche, dunkler Text. Darunter kleine Zeile "Auf der nächsten Seite bekommst du einen Raum-Code zum Teilen".

## Screen 2: Warteraum (`/turnier/draft/<code>`, vor Start)

- Oben kleine Pill mit grünem Punkt "WARTEN AUF SPIELER".
- Titel "TEAM 1 vs TEAM 2" (Team 1 gold, "vs" grau klein, Team 2 blau), riesig, fett.
- Raum-Code-Box: Label "RAUM", Code gesperrt geschrieben ("2 Y V V R S"), daneben Knopf "Link kopieren". Unterzeile grau gesperrt: "6V6   2 BANS   30S TIMER".
- Zwei Team-Karten nebeneinander, dazwischen dünner vertikaler Trennstrich mit Pfeil-Glyphe.
  - Ohne Captain: gestrichelter Rahmen, Text "CAPTAIN ÜBERNEHMEN" gesperrt in Team-Farbe.
  - Eigener Captain: Punkt plus "DU BIST CAPTAIN", Knöpfe "BEREIT" (weiß auf dunkel, nach Klick grün "BEREIT" mit grünem Karten-Glow) und "Verlassen" (grau).
  - Fremder Captain: grüner Punkt "CAPTAIN DA", darunter "Wartet..." grau.
- Darunter Auge-Glyphe plus "1 schaut zu" bzw. "Keine Zuschauer".
- Hinweiszeile: "Teile den Raum-Link und übernimm deinen Captain-Platz" bzw. "Beide Captains müssen auf Bereit klicken".
- Ganz unten dezent "← NEUEN DRAFT ANLEGEN".

## Screen 3: Draft-Board (läuft)

Layout dreispaltig, volle Höhe:

- Kopf mittig: gesperrt "BAN-PHASE 1/16" (Zahl gedimmt), darunter "Team 1 · Bannen" (Team in Farbe), darunter rot gesperrt "DU BIST DRAN" nur beim aktiven Captain, darunter Countdown "0:24" monospaced, ab 10 Sekunden rot.
- Ecken oben links "TEAM 1" gold, oben rechts "TEAM 2" blau, gesperrt.
- Ban-Leiste direkt unter dem Kopf, mittig: links Team-1-Bans, Label "BANS", rechts Team-2-Bans. Leere Slots dunkle Quadrate mit Punkt, der aktive Slot pulsiert in Team-Farbe, belegte Slots zeigen das Portrait mit rotem X darüber.
- Linke Spalte (Team 1): Tab-Kopf "TEAM 1" gold unterstrichen, darunter 6 Slot-Kacheln mit Nummer; nach Pick Portrait links plus Name, Gold-Rahmen mit Glow beim frischen Pick.
- Rechte Spalte (Team 2): spiegelgleich in Blau.
- Mitte: leer bis zur Auswahl, dann große Splash-Art rechts der Mitte (Hero-Card-Bild aus `icon_hero_card`), weich eingeblendet, hinter ihr Farbwolke in Team-Farbe. Beim Ban ist die Wolke rot.
  - Links unten Beschriftung: kleines gesperrtes Label "BANNEN" bzw. "TEAM 1 WÄHLT" mit Punkt, darunter Heldenname in riesigen Versalien, halbtransparent (Opacity 0.35) solange nicht bestätigt.
  - Mittig auf Höhe der Beschriftung der Aktionsknopf: "BANNEN" rot-transparent mit rotem Rahmen, oder "EINLOGGEN" gold-transparent mit Gold-Rahmen. Breite etwa 180px, gesperrter Text.
- Untere Leiste, zentriert, dunkles Panel mit Rahmen: links Badge "BAN" (rot) oder "PICK" (gold), Suchfeld "Suchen...", dann alle Helden als quadratische Portraits in zwei Reihen (etwa 44px), Hover hebt leicht, Auswahl bekommt Rahmen in Aktionsfarbe. Gebannt: Portrait ausgegraut mit rotem X. Gepickt: ausgegraut. Rechts Aufklapp-Pfeil (Leiste einklappen) und unten rechts kleiner "Ton"-Umschalter.
- Ban-Bestätigung (Übergang, etwa 1,2 Sekunden): ganze Fläche rot getönt, zwei diagonale rote Linien über den Screen, mittig großes rotes X (zwei dicke Balken mit Glow) und darüber ein schräger Stempel "GEBANNT" (rote Fläche, dunkler Text, gesperrt). Links unten "ELIMINIERT" klein rot plus Heldenname groß mit roter Unterlinie. Splash-Art dahinter rot eingefärbt und verblassend.
- Pick-Bestätigung (Übergang): Splash-Art bleibt, links unten "TEAM 1" klein in Farbe, Heldenname riesig weiß mit Team-Farbe-Unterlinie, Team-Slot links füllt sich mit Portrait und Name.
- Auto-Pick (Timer abgelaufen): Slot bekommt kleines Badge "AUTO" in Grau.
- Zuschauer sehen alles ohne Aktionsknopf und ohne "DU BIST DRAN".

## Screen 4: Endscreen

- Oben klein gesperrt "DRAFT ABGESCHLOSSEN", Titel "TEAM 1 vs TEAM 2" in Farben.
- Darunter mittig die Bans als kleine ausgegraute Portraits mit rotem X, links Team 1, rechts Team 2.
- Zwei Kartenraster: Label-Leiste "TEAM 1" links mit goldener Kante, "TEAM 2" rechts mit blauer Kante. Je Team 3x2 Karten (etwa 190x280), Splash-Art als Kartenbild, oben links Pick-Nummer als Badge in Team-Farbe, unten Name weiß fett. Rahmen dünn, Glow in Team-Farbe.
- Lobby-Bereich unter dem Titel (neu, nicht bei deadlocklabs): Box "LOBBY" mit Status "Lobby wird erstellt..." (pulsierender Punkt) und danach Join-Code groß gesperrt plus "Code kopieren". Fehlerzustand: roter Text "Lobby konnte nicht erstellt werden, bitte selbst anlegen" und Knopf "Erneut versuchen". Nach Match-Ende: "Ergebnis: Team 1 gewinnt · 34:12 · Match 123456".
- Knopfzeile unten: "Teilen" (Glyphe), "Rematch" (hervorgehoben), "← Zurück".

## Texte (wortgleich verwenden)

"Draft anlegen", "Raum beitreten", "Draft starten", "Warten auf Spieler", "Captain übernehmen", "Du bist Captain", "Bereit", "Verlassen", "Captain da", "Wartet...", "schaut zu", "Keine Zuschauer", "Ban-Phase", "Pick-Phase", "Bannen", "Einloggen", "Du bist dran", "Gebannt", "Eliminiert", "wählt", "Auto", "Draft abgeschlossen", "Teilen", "Rematch", "Zurück", "Link kopieren", "Code kopieren", "Lobby wird erstellt...", "Lobby konnte nicht erstellt werden, bitte selbst anlegen", "Erneut versuchen", "Bans", "Timer", "Format", "Suchen...".
