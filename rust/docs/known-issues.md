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

---

## Match-Lebenszyklus (`tb-match`)

### KI-M01 [erhalten] — `match_results.winning_team` trägt zwei Wertfamilien
`result_processor.py:162` vs. `manager.py:955`: Der Bracket-Pfad schreibt die
**winner_id** (Team-PK) in die Spalte, der Group-Pfad einen **Slot** (1/2). 1:1
erhalten (beide Pfade per Integrationstest belegt).

### KI-M02 [erhalten] — Drei uneinheitliche `winning_team`-Konventionen
Bracket-Ergebnis 0-basiert (0=team1, 1=team2), Group 1-basiert (1/2), Serien-
`winner_team` 1/2. Alle drei Konventionen 1:1 erhalten (typisiert + getestet).

### KI-M03 [erhalten] — Redundanter zweiter Status-Guard
`result_processor.py:73-81`: Die manuelle Status-Prüfung hat einen faktisch
redundanten zweiten `NOT IN`-Teil. Irreführend, aber 1:1 erhalten.

### KI-M04 [erhalten] — `reset_bracket_downstream` ohne Zyklus-/Tiefenschutz
`result_processor.py:252-283`: Bei fehlerhaften `source_match`-Verweisen droht
theoretisch Endlosrekursion. Bewusst KEIN `visited`-Set ergänzt (1:1 erhalten,
in Rust via `Box::pin`-Rekursion).

### KI-M05 [erhalten] — Stale-Task-Reaper inline statt entkoppelt
`steam_bridge.py:21-34`: `_fail_stale_running_tasks` läuft bei jedem
`create_task`/`get_task`/`has_active_task` (beim Pollen alle 0,5 s ein
Schreib-Commit). Nicht in einen eigenen Reaper-Task entkoppelt. 1:1 erhalten.

> Beim Port mitbereinigt (safe): stringly-typed `match_type` (`_match_table`/
> `_match_scope_column`-String-Hacks) durch das Enum `MatchKind` + eine
> Repository-Abstraktion mit festen Query-Zweigen ersetzt; Ergebnis-Persistenz in
> EINER Transaktion (DELETE+UPDATE+INSERT); `ensure_game_exists` wiederverwendet.

---

## Scheduler (`tb-scheduler`)

### KI-SC01 [erhalten] — TZ-Fragilität bei Zeitvergleichen
`scheduler.py:29-47`: Zeitstempel werden in lokal-naive Zeit umgerechnet und
gegen `now()` (Server-Lokalzeit) verglichen. Stimmt die Server-TZ nicht mit den
DB-Zeitstempeln überein, verschieben sich alle Reminder/Phasenwechsel um den
Offset; über DST driftet es zusätzlich. Bewusst **nicht** auf UTC umgestellt, weil
das ändern würde, **wann** Auslösungen feuern (Parität).

### KI-SC02 [erhalten] — Generierung vor dem Optimistic-Lock
`scheduler.py:153-169`: Gruppen/Bracket werden VOR dem Status-`UPDATE … WHERE
status=current` erzeugt. Bei parallelem Statuswechsel (`rows_affected = 0`) sind
die Datensätze bereits angelegt → mögliche Waisen. 1:1 erhalten.

### KI-SC03 [Abweichung, dokumentiert] — Status+Punkte nicht mehr atomar
Im Original liefen `completed`-`UPDATE` und `recalculate_player_points` in
derselben Transaktion. Da `recalculate_player_points` im Port `&pool` nimmt
(eigene Transaktion), committet der Port den Status zuerst und rechnet dann die
Punkte. Schlägt der Recompute fehl, bleibt `completed` ohne neue Punkte stehen.
Folgenarm, weil der Recompute **idempotent** ist (einfach erneut auslösbar) und
dieser Pfad nur extern (tb-web) getriggert wird. Folgefix: eine
transaktions-durchgereichte Recompute-Variante in `tb-tournament`.

### KI-SC04 [erhalten] — Reminder-Fenster ohne Catch-up
`is_within_window` prüft `reminder_at <= now <= reminder_at + 5min`. Nach einem
längeren Ausfall verpasste Fenster werden nicht nachgeholt. 1:1 erhalten.

### KI-SC05 [erhalten] — `bracket_only`-Logikfalle
Bei `tournament_mode == "bracket_only"` und Status `checkin` wird nur nach
`bracket` gewechselt, wenn `bracket_start` fällig ist; sonst bleibt das Turnier in
`checkin` hängen, auch wenn `group_phase_start` längst vorbei ist. 1:1 erhalten.

### KI-SC06 [erhalten] — Dedupe-Insert nach dem Versand + Aktiv-Check-TOCTOU
Reminder werden versendet, dann erst per `INSERT OR IGNORE` markiert (Crash
dazwischen → erneuter Versand). Der Single-Active-Check und der Statuswechsel
laufen ohne gemeinsames Lock (TOCTOU). Beides 1:1 erhalten.

> Beim Port mitbereinigt (safe): der Tot-Loop (eine Exception aus einem Check
> beendete im Original den ganzen Scheduler-Task) ist behoben — jeder der vier
> Checks ist einzeln fehlertolerant, der Loop endet nur über das Shutdown-Signal.
> `offset_label` ist Einzelquelle (statt Inline-Duplikat); `parse_reminder_offsets`
> ist müll-tolerant.

---

## Web / Routen (`tb-web`)

### KI-W01 [erhalten] — `rank_score` im Spielerprofil zeigt `matches_won`
`leaderboard_routes.py:110`: `get_player_profile` setzt `rank_score =
points_row["matches_won"]` — der Rang-Score bekommt fälschlich die Anzahl
gewonnener Matches statt des echten Rang-Scores (Copy-Paste-Fehler). 1:1 erhalten
in `leaderboard.rs` (Kommentar markiert die Stelle).

### KI-W02 [erhalten] — `set_consent` bestätigt unbedingt
`consent_routes.py`: `POST /consent` gibt `has_consent=true` und die rohe
gepostete `consent_version` zurück, auch wenn diese unter der geforderten Version
liegt. 1:1 erhalten.

### KI-W03 [erhalten] — Asymmetrische Registration-Gates (public)
`routes.py`: `accept`-Routen (Einladung/Bewerbung) haben ein
`_ensure_registration_open`-Gate, die `reject`-Pendants und
`get_team_applications` nicht. Ebenso erlauben create/join/solo
`registration+checkin`, cancel/kick/leave aber nur `registration`. 1:1 erhalten.

### KI-W04 [erhalten] — Direkt-Add umgeht Zustimmung & invite_mode
`routes.py` `invite_to_team`: fügt den Spieler sofort ohne Zustimmung und ohne
`team_invitations`-Eintrag ein, umgeht `invite_mode`/-window. Abweichend von
„invite-by-signup". 1:1 erhalten.

### KI-W05 [erhalten] — Solo-Signup-Auto-Team race-anfällig
`routes.py` `solo_signup` (team_size==1): Auto-Team-Name per Such-Schleife +
Signup-ohne-team_id-dann-UPDATE statt `ON CONFLICT`. 1:1 erhalten.

### KI-W06 [erhalten] — Group-Standings nicht idempotent
`admin_routes.py:2236`: Der Group-Ergebnis-Pfad inkrementiert wins/losses/points
blind (+3 Sieger), kein Unentschieden, bei Doppelaufruf doppelt; `force` wird für
Group ignoriert (Bracket delegiert, Group inline). 1:1 erhalten.

### KI-W07 [erhalten] — Auto-Modus aus `team_size` statt Teamanzahl
`admin_routes.py:835`: `create_tournament` ruft `determine_tournament_mode` mit
`team_size` (Spieler/Team) als Teamanzahl — der ≥16-Schwellwert ist faktisch
wirkungslos. 1:1 erhalten.

### KI-W08 [erhalten] — Match-Caster-Routen sind 410-Gone-Stubs
`admin_routes.py:2982`: Die Match-Ebene-Caster-Routen (POST/DELETE) antworten
410 Gone; `list_match_casters` validiert die `match_id`, gibt aber Turnier-Caster
zurück. 1:1 erhalten.

### KI-W09 [erhalten] — Test-Modus immer gemountet + Orphan-Cleanup
`admin/test_mode.py`: Im Original ist der Router ungated. Der Port hängt ihn an
`TURNIER_ENABLE_TEST_MODE` (Default an = wie Python), gibt aber einen Prod-Kill-
Switch. Das Löschen von Test-Usern räumt nur einen Teil der Tabellen
(Orphan-Daten in applications/invitations/checkins) — 1:1 erhalten.

> Die vollständigen Befund-Listen der parallel portierten Router (consent/public/
> admin/test_mode) liegen in den Agent-Reports der Welle 5b. Verifikation: gesamter
> Workspace kompiliert, `clippy -D warnings` sauber, 37 Test-Suites grün,
> `tb-app --check` bootet end-to-end. Ein endpunkt-genauer Paritäts-Audit der zwei
> grossen Router ist als Folge-Schritt empfohlen (siehe `cutover.md`).
