# Known Issues — beim Port gefundene Alt-Bugs/Inkonsistenzen

Hier sammeln wir Probleme des Python-Stands, die beim Port auffallen. Grundsatz
der Neuentwicklung: **Funktionalität bleibt gleich, „dumme" Bugs werden sauber
gefixt** — aber rückwärtskompatibel zu vorhandenen Daten. Verhaltensändernde
oder entscheidungsbedürftige Befunde werden im Verhalten 1:1 erhalten und hier
als Opt-in-Folgefix dokumentiert (keine stille Semantik-Änderung).

Legende: **[erhalten]** = Verhalten bewusst 1:1 nachgebaut. **[gefixt]** = sicher
mitbereinigt (kein beobachtbarer Verhaltensunterschied).

---

## Steam / Ränge (`tb-steam`)

### KI-S01 [erhalten] — Steam-Bridge maskiert Discord-Fallback bei rang-losem Link
`rank_reader.py:258-268`: Existiert in `steam_links` eine Zeile für den Spieler,
aber `rank`/`rank_tier` sind NULL (verlinkter Account ohne Rang), liefert die
Bridge-Stufe trotzdem ein (leeres) Profil zurück und cacht es 24 h. Damit kommt
der Discord-Rollen-Fallback einen Tag lang nicht zum Zug, obwohl er einen Rang
liefern könnte.
**Erhalten in** `resolver.rs::resolve_via_bridge` (Kommentar markiert die Stelle).
**Folgefix-Vorschlag:** rang-lose Bridge-Zeilen überspringen oder nicht cachen.

### KI-S02 [erhalten] — Haupt-Tier-Rollen-Fallback rät `subrank = 3`
`rank_reader.py:240-241`: Hat ein Spieler nur eine Haupt-Rang-Rolle (ohne
Subrang-Rolle), wird `subrank = 3` angenommen und daraus ein exakter
`rank_score` berechnet — wirkt präziser als belegt. `source` bleibt
`discord_role`.
**Erhalten in** `role_rank.rs` (`DEFAULT_SUBRANK = 3`).
**Folgefix-Vorschlag:** eigener Source-Wert/Unschärfe-Flag (Produktentscheidung).

### KI-S03 [gefixt] — `rank_score` bei `subrank = 0`
Pythons `int(subrank or 3)` nutzt Falsy-Semantik: sowohl `None` als auch `0`
ergeben 3. Eine erste Port-Fassung mappte nur `None` (→ `0` wäre fälschlich zu 1
geklemmt worden). Vor dem Commit korrigiert: `clamp_subrank` behandelt `None` und
`Some(0)` identisch als 3 (`rank.rs`, Test `subrank_clamping_and_default`). Kein
Verhaltensunterschied zum Original mehr.

---

## Discord-Notifier (`tb-discord`)

### KI-D01 [erhalten] — Profil-lose User: Event-Default überschreibt DM-Schalter
`discord_notifier.py:283-284`: Fehlt einem Nutzer das `user_profiles`-Profil,
gilt für BEIDE Schalter (DM-Erlaubnis und Event-Flag) der Event-Default. Bei
einem Event mit Default `false` (z. B. `tournament_news`) werden profil-lose User
damit auch per DM nicht erreicht.
**Erhalten in** `notifier.rs::notify_users` (`None => (event.default, event.default)`).
**Folgefix-Vorschlag:** für profil-lose User die eigene DM-Default-Konstante nutzen.

### KI-D02 [erhalten] — Ungültige Snowflake-ID = stiller Nicht-Versand
`discord_notifier.py:208/225/456/499/505`: Eine nicht-numerische `channel_id`/
`discord_id` warf im Original ein unbehandeltes `int()`-`ValueError`. Im Port
löst sie **keine Panic** mehr aus, aber das beobachtbare Verhalten (kein Versand)
bleibt: die ID landet in der `failed`-Bucket (`require_snowflake` → `BrokerError`),
genau dort, wo der Original-Loop pro ID fing.

### KI-D03 [erhalten] — Verzögerte Channel-Löschung nur im Prozess
`discord_notifier.py:308-313`: `delete_match_channel_later` hält die Verzögerung
per `tokio::sleep` im Prozess. Bei Neustart/Crash geht die geplante Löschung
verloren (kein DB-gestützter `run_after`-Scheduler). Best-effort wie im Original.

### KI-D04 [erhalten] — Lückenhafte `discord_tasks`-Protokollierung
`discord_notifier.py:320 ff.`: Nur ein Teil der Aktionen schreibt einen
`discord_tasks`-Eintrag (Channel anlegen/löschen, Lobby-Info, `notify_users`).
Voice-Move, Lobby-Announcement, Stats, Caster-Embed sowie Rollen-/Voice-Reads
laufen ohne Task-Eintrag. 1:1 erhalten.

### KI-D05 [erhalten] — Channel-Slug bei exotischen Zeichen
Python nutzt `unicodedata` NFKD + ASCII-Ignore. Der Port transliteriert die
praktisch relevanten lateinischen Akzente/Umlaute deterministisch; `ß` wird (wie
im Original) verworfen. Bei exotischen Nicht-Latein-Zeichen kann der Slug minimal
abweichen — für Team-Namen unkritisch.

---

## Auth (`tb-auth`)

### KI-A01 [erhalten] — Rollen-Staleness bis zu 7 Tage
`discord_oauth.py:130` vs. `middleware.py`: Die Discord-Rollen werden beim Login
in der Session eingefroren und bis zu 7 Tage für `is_admin`/`is_mod` genutzt.
Entzogene Rollen wirken erst beim Session-Ablauf.
**Erhalten:** `create_session` speichert die Rollen-CSV; `resolve_session`
berechnet die Flags daraus, ohne Re-Validierung gegen den Broker.

### KI-A02 [erhalten] — Rollen als komma-separierter String
`discord_roles` bleibt CSV (kein JSON-Array, keine Join-Tabelle). Ein Komma in
einer Rollen-ID würde das Splitting brechen — bei numerischen Snowflakes derzeit
kein Problem.

### KI-A03 [erhalten] — CSRF/Replay-Schutz liegt beim Broker
`discord_oauth.py:105-119`: Der Schutz hängt vollständig daran, dass der
Master-Broker `state_id` bei `consume-result` genau einmal einlöst (Single-Use +
Ablauf). Der Port erzwingt das nicht selbst — als externe Invariante im
`oauth.rs`-Modul-Doc dokumentiert.

---

## Turnier-Engine (`tb-tournament`)

### KI-T01 [erhalten] — Platzierungs-Heuristik ist Single-Elim-zentriert
`points.py`: `max(round)` gilt als Finale, `round-1` als Halbfinale. Bei
Double-Elim ist das teils unscharf (Losers-Runden zählen anders).
**Folgefix-Vorschlag:** Platzierung aus der tatsächlichen Bracket-Topologie statt
aus der Rundennummer ableiten.

### KI-T02 [erhalten] — Toter Platzierungs-Schlüssel + Halbfinal-Pauschale
`points.py`: `PLACEMENT_POINTS[4]` wird nie getroffen; alle Halbfinal-Verlierer
bekommen Platz 3. 1:1 erhalten.

### KI-T03 [erhalten] — `matches_played` zählt global statt teambezogen
Ein Spieler bekommt alle abgeschlossenen Bracket-Matches gezählt, nicht nur die
seines Teams. 1:1 erhalten.

### KI-T04 [erhalten] — Win-Punkte runden ab
`int(wins * 0.5)` ≡ Integer-Division `wins / 2`: bei ungerader Win-Zahl geht ein
halber Punkt verloren. 1:1 erhalten.

### KI-T05 [erhalten] — Doppeltes Gruppen-Clamping + fehlender Sekundär-Tiebreaker
Gruppenzahl wird an zwei Stellen geklemmt (`2..8` und nochmals `Teams/2`); die
Top-2-Standings haben keinen stabilen Sekundär-Tiebreaker
(`ORDER BY points DESC, wins DESC` ohne weitere Stufe). 1:1 erhalten.

### KI-T06 [erhalten] — `bracket_format`-Argument von `generate_bracket` ignoriert
Das Format wird aus der `tournaments`-Zeile gelesen, das Funktionsargument bleibt
wirkungslos (toter Legacy-Pfad). 1:1 erhalten.

> Beim Port mitbereinigt (safe, kein Verhaltensunterschied): toter Code nicht
> mitportiert (`calc_rank_score`-Import, `_highest_power_of_two_below`,
> `team_avg_score`); `recalculate_player_points` ist jetzt idempotent
> (Voll-Recompute statt additivem Doppelzählen); jede Operation läuft in EINER
> Transaktion; Audit committet nicht mehr selbst.

---

## Draft (`tb-draft`)

### KI-DR01 [erhalten] — `taken_by` nicht an die Auth-Identität gebunden
`routes.py:51/61`: `take_action` übernimmt `taken_by` aus dem Argument, ohne zu
erzwingen, dass es der eingeloggte Admin ist. Die Signatur erlaubt tb-web, hier
`user.discord_id` durchzureichen (Empfehlung), das Default-Verhalten bleibt offen.

### KI-DR02 [erhalten] — Doppel-Pick nur applikativ geprüft (kein UNIQUE-Index)
`(session_id, hero_name)` hat keinen partiellen UNIQUE-Index; die Prüfung läuft
per SELECT — jetzt aber innerhalb `BEGIN IMMEDIATE` (nicht mehr race-anfällig).
Ein DB-Constraint wäre robuster, hätte aber eine Schema-Migration erfordert.

### KI-DR03 [erhalten] — Held nicht gegen erwarteten `action_type`/`team_slot` validiert
`engine.py:108-115`: Der Held wird blind an `current_action_index` geschrieben,
ohne zu prüfen, ob die Position einen Ban/Pick des jeweiligen Teams erwartet.
1:1 erhalten.

> Beim Port mitbereinigt (safe): die Race-Condition in `take_action`
> (Index-Lesen + Doppel-Pick-SELECT + zwei UPDATEs ohne Isolation, last-write-wins)
> ist durch eine Transaktion mit `BEGIN IMMEDIATE` + optimistischem
> Compare-and-Swap auf `current_action_index` serialisiert — belegt durch einen
> echten 2-Threads-Race-Test. Beobachtbar gültige Picks unverändert.
