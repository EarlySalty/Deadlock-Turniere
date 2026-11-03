# Erklärnachricht "Rollen und Bereiche" (vorbereitet, nicht gepostet)

Komponenten-JSON (Components V2, Goldlook): `discord-rollen-erklaerung.json`. Bilder beim Posten als Multipart anhängen: `assets/welcome-banners/divider-information.png` und `assets/welcome-banners/logo-badge.png` aus Deadlock-Bots. Emojis live geprüft am 2026-11-02: dl_ranked 1522518271306366996, dl_brawl 1522518262708174928, dl_crown 1522518265421631538.

## Zielkanal

Vorschlag: neuer Textkanal in der Kategorie INFORMATION (`1310153243795390475`), direkt unter 🧭willkommen, mit den Rechten der Kategorie (alle lesen, niemand schreibt).

```
POST /guilds/1289721245281292288/channels
{"name": "🎭rollen-und-bereiche", "type": 0, "parent_id": "1310153243795390475", "position": 2,
 "topic": "Hier liest du, wie du dir Scrims, Custom Games und Turniere freischaltest.",
 "permission_overwrites": [
   {"id": "1289721245281292288", "type": 0, "allow": "66560", "deny": "309237647360"},
   {"id": "1527490219761471568", "type": 0, "allow": "0", "deny": "3072"}]}
```

Die Overwrites entsprechen 1:1 der Kategorie INFORMATION (@everyone lesen ohne Schreiben, Temporär Banned gesperrt). Alternative ohne neuen Kanal: die Nachricht in 🧭willkommen (`1522460852114948166`) posten. Danach optional die neue Kanal-ID in `default_channel_ids` des Onboardings aufnehmen.

`<id:customize>` ist der Discord-Link auf "Kanäle & Rollen" und funktioniert in jeder Nachricht.

## Text als Fallback (Klartext, gleiche Aussage)

# Rollen und Bereiche
Drei Bereiche auf dem Server schaltest du dir selbst frei. Nimm einfach, worauf du Lust hast. Du kannst das jederzeit wieder ändern.

## So wählst du
Öffne <id:customize> ganz oben in der Kanalliste. Unter **Worauf hast du Lust?** hakst du an, was dich interessiert. Die passenden Kanäle erscheinen danach sofort in deiner Liste.

### <:dl_ranked:1522518271306366996> Scrims
Trainingsspiele zwischen festen Teams. Nach der Auswahl trägst du dich mit `/scrim-signup` ein, damit die Orga deinen Rang, deine Rolle und deine Zeiten kennt. Alles Weitere findest du in der Kategorie **Scrims**.

### <:dl_brawl:1522518262708174928> Lustige Custom Games
Spaßrunden abseits von Ranked. Mit der Rolle **Custom Games** bekommst du einen Ping, sobald eine Runde startet. Abgesprochen wird in <#1412411665713987635>.

### <:dl_crown:1522518265421631538> Turniere
Mit der Rolle **Turnier Teilnehmer** siehst du die Kategorie **Turniere**. Lies zuerst <#1565499075682369577>, Neuigkeiten stehen in <#1565496544818372628>. Ein Team findest du in <#1565497013531967670>, Mitspieler in <#1565497064157216808>. Alles andere besprecht ihr in <#1565498999241445446>.

-# Rolle wieder loswerden: In <id:customize> den Haken entfernen.

## Hinweise

- Der Scrims-Absatz gilt für Variante A des Onboardings (Scrims als Option, siehe `discord-rollen-inventar.md`, Abschnitt 7). Bei Variante B (keine Scrims-Option) im Scrims-Absatz die Sätze ab "Nach der Auswahl" ersetzen durch: "Trag dich mit `/scrim-signup` ein. Danach hast du die Rolle **Scrims Teilnehmer** und siehst die Kategorie **Scrims**."
- Kanalnennungen auf Scrims sind bewusst ohne Link, weil der Scrim-Kanal für Nichtmitglieder unsichtbar ist und sonst als "Kein Zugriff" erscheint.
