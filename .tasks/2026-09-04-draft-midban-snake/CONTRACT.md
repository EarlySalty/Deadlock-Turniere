# Contract: Draft-Format Mirror-Snake mit Ban-Trade in der Mitte

## Ziel
Der Community-Draft (`/turnier/draft`, freie Lobbys) nutzt ein Draft-Format mit
je 2 Bans und 6 Picks pro Team: Ban-Runde vorne, Pick-Block, Ban-Trade in der
Mitte, gespiegelter Pick-Block. Seitenwahl per unsichtbarem Flip.

## REQ
- REQ1: Neues Sequenz-Preset `competitive_2ban_mid` (16 Schritte) in
  `rust/crates/turnier-draft/src/sequence.rs`, exakt:
  ```
  Ban One, Ban Two,
  Pick One, Pick Two, Pick Two, Pick One, Pick One, Pick Two,
  Ban Two, Ban One,
  Pick Two, Pick One, Pick One, Pick Two, Pick Two, Pick One
  ```
  Verteilung: je Team 2 Bans, je Team 6 Picks. Zweite Pick-Haelfte ist der
  exakte Spiegel (One<->Two) der ersten.
- REQ2: `preset("competitive_2ban_mid")` liefert dieses Array.
- REQ3: Das Frontend bietet das Preset an und nutzt es als Default fuer die
  Lobby-Erstellung unter `/turnier/draft`. Die bestehenden Presets bleiben
  waehlbar.
- REQ4: Unsichtbarer Seiten-Flip: bei Lobby-Erstellung wird zufaellig
  entschieden, welches der beiden eingegebenen Teams Slot One (die startende
  Seite) belegt. Kein sichtbarer Coin-Flip, keine UI-Anzeige des Ergebnisses;
  Teamnamen bleiben korrekt an ihre Tokens gebunden.
- REQ5: Unit-Test in `sequence.rs`, der die 16 Schritte positionsgenau prueft
  (inkl. Ban-Positionen 0,1,8,9 und der Spiegel-Eigenschaft) und in
  `presets_haben_die_erwartete_verteilung` das neue Preset mit (4, 12) auffuehrt.

## INV
- INV1: Bestehende Presets (`competitive_2ban`, `competitive_1ban`,
  `quick_no_ban`) und `DEFAULT_SEQUENCE` bleiben unveraendert.
- INV2: Alle bestehenden Tests bleiben gruen; keine rote Baseline vorhanden.
- INV3: Keine neuen Code-Kommentare; vorhandene im angefassten Diff-Bereich
  entfernen, wenn es den Diff nicht aufblaeht.

## Nicht-Ziele
- Kein sichtbarer Coin-Flip / keine Flip-Animation.
- Keine Aenderung an Bracket-Match-Drafts (`start_match_draft`).
- Keine neuen Presets ausser dem einen.

## Erlaubter Bereich
- rust/crates/turnier-draft/src/sequence.rs
- rust/crates/turnier-draft/src (create_lobby / Slot-Zuweisung fuer den Flip)
- rust/crates/turnier-api/src/draft.rs (nur falls Flip dort sitzt)
- frontend/src (Preset-Auswahl + Default)
