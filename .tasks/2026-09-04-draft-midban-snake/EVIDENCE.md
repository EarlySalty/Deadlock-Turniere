# Evidence

## Sequenz-Engine (Presets)
- rust/crates/turnier-draft/src/sequence.rs:110: `COMPETITIVE_2BAN` (alle 4 Bans vorne, kein Mid-Ban), Vorlage fuer das neue Array.
- rust/crates/turnier-draft/src/sequence.rs:175: `preset(name)` mappt drei Namen auf Arrays; hier neuen Arm ergaenzen.
- rust/crates/turnier-draft/src/sequence.rs:292: Test `presets_haben_die_erwartete_verteilung`; Tabelle um `("competitive_2ban_mid", 4, 12)` erweitern.
- rust/crates/turnier-draft/src/sequence.rs:318: Test `competitive_2ban_hat_zwei_bans_je_team`; Muster fuer neuen Positions-Test.

## Preset-Auswahl (API)
- rust/crates/turnier-api/src/draft.rs:71: `CreateLobbyRequest.preset: String`.
- rust/crates/turnier-api/src/draft.rs:133: `preset(&body.preset)` validiert; neuer Name laeuft automatisch durch.

## Frontend
- frontend/src/types/tournament.ts:765: `DraftPreset`-Union; neuen Wert ergaenzen.
- frontend/src/pages/DraftLobbyNeu.tsx:18: Preset-Optionsliste (id/titel/erklaerung).
- frontend/src/pages/DraftLobbyNeu.tsx:89: `useState<DraftPreset>('competitive_2ban')` Default; auf neues Preset umstellen.

## Flip (Seiten-Zuweisung)
- rust/crates/turnier-draft/src/repo.rs:33: `CreateLobbyOptions` (team1/team2 Name).
- rust/crates/turnier-draft/src/repo.rs:130: `team1_token`; hier werden Teams zu Slots festgeschrieben. Zufaelliger Tausch von (Name, Token) vor dem Persistieren ergibt den unsichtbaren Flip.

## Baseline
- Keine rote Baseline; `sequence.rs`-Tests sind gruen (Feature, kein Bug).
