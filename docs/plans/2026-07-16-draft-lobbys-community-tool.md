# Draft-Lobbys — öffentliches Pick/Ban-Tool

Stand: 2026-07-16. Vorbild: statlocker.gg Draft Tool.

## Ziel

Ein öffentliches Pick/Ban-Draft-Tool auf der Website. Jeder kann eine Lobby
aufmachen, den Link teilen, live gegeneinander draften. Der fertige Draft bleibt
unter demselben Link abrufbar — das ist gleichzeitig die Coaching-Nachbesprechung.
Turniere binden denselben Draft nativ ein, dort mit Steam-Pflicht.

Drei Einstiege, EIN Draft-Kern:

| Einstieg | Auth | Bindung |
|---|---|---|
| Freie Lobby | anonym, Captain-Token im Link | keine |
| Coaching-Nachbesprechung | keine (Link genügt) | abgeschlossener Draft, read-only |
| Turnier | Discord-Login + verknüpfter Steam | `bracket_match_id` |

## Was schon existiert (nicht neu bauen)

- `turnier-draft` — Sequenz-Zustandsmaschine, Hero-Validierung, State, sqlx-Repo
  mit Compare-and-Swap. 1122 Zeilen, DB-getestet.
- `turnier-api` — läuft live auf :8900, Discord-OAuth, `AdminUser`/`AuthUser`.
- `turnier-steam` — löst Discord-ID → Steam-Link + Rang über die Bridge auf.
  Damit ist die Steam-Pflicht ein Guard, kein neuer Login.
- Frontend — React 19, Vite, Tailwind 4, TanStack Query, framer-motion.
- `steam-flows/link.rs` — vollständiger Steam-OpenID-Flow (im Steam-Bot).

## Entscheidungen

**Live-Sync = Polling (1s), kein WebSocket/SSE.**
Der Zustand liegt vollständig in Postgres, der Server merkt sich nichts. Damit
überlebt jeder laufende Draft einen Backend-Restart, Reconnects und
Netzwechsel — bei WS/SSE lebt der Broadcast-Kanal im Prozessspeicher und jedes
Deploy killt alle Drafts. Skaliert horizontal ohne Redis/Sticky-Sessions.
Debuggen mit `curl`. Ein Draft ist ~1 Klick/30s — Push kauft nichts.

**Timer wird faul ausgewertet, kein Scheduler.**
Die Lobby speichert `deadline_at`. Jeder Lese-Request prüft, ob die Deadline
überschritten ist, und führt dann die Ablauf-Aktion aus. Kein Hintergrund-Job,
kein Prozess-Zustand. Der Browser zählt lokal runter (flüssig), der Server
bleibt die Wahrheit (Uhr-Manipulation wirkungslos).

**DB = ausschließlich zentrale Postgres via `dl-central-db`.**
`turnier-db/migrations/0001_initial.sql` ist totes SQLite-Legacy;
`run_migrations(_pool)` ignoriert den Pool. Neue Migration gehört ins
Deadlock-Bots-Repo (`dl-central-db/migrations/`), Code ins Turniere-Repo.

**Heldenliste: `deadlock-api.com` ist Quelle, `heroes.rs` nur Fallback.**
Die 26 hartkodierten Helden sind veraltet (aktuell ~38). Ohne Live-Quelle ist
das Tool beim nächsten Helden kaputt. Cache 24h, Fallback bei Ausfall.

**Ein Datenmodell für frei + Turnier.** `bracket_match_id` wird nullable,
Lobby-Bindung kommt dazu. Sonst driften zwei Drafts auseinander.

**Captain-Identität = opakes Token im Link, keine Anmeldung.**
- `/draft/{code}` → Zuschauer
- `/draft/{code}?t={token}` → Captain (Token landet in localStorage)
Turnier-Lobbys ignorieren Tokens und nutzen Discord-Session + Steam-Guard.

## Datenmodell (neue Migration in dl-central-db)

Keine eigene `draft_lobbies`-Tabelle. Eine Lobby **ist** eine Draft-Session ohne
Turnier-Bindung — zwei Tabellen hiessen: jede Abfrage joint, und der Status steht
an zwei Orten ("Lobby fertig, Session laeuft"). Also `draft_sessions` erweitern:

```
turnier.draft_sessions  (additiv, alle neuen Spalten NULL = heutiges Verhalten)
  bracket_match_id   -> NULL erlaubt   -- NULL = freie Lobby, gesetzt = Turnier
  code               TEXT              -- Lobby-Code, unique partial index
  team1_name, team2_name        TEXT
  team1_token, team2_token      TEXT   -- opake Captain-Tokens, stecken im Link
  sequence           JSONB             -- Ban/Pick-Reihenfolge DIESER Session
  round_seconds      INTEGER           -- NULL = kein Timer (Bestands-Drafts)
  reserve_seconds    INTEGER
  team1_reserve_left, team2_reserve_left  INTEGER
  deadline_at        TIMESTAMPTZ       -- wann der aktuelle Zug faellt

turnier.draft_actions
  is_auto            BOOLEAN DEFAULT FALSE  -- Zug lief ab, automatisch gesetzt
```

## Timer-Regel (Hybrid, wie statlocker)

`deadline_at = Zugbeginn + round_seconds + reserve_left(Team am Zug)`.
Der Client zeigt `round_seconds`; laeuft die ab, frisst der Zug automatisch
Reserve. Beim Ausfuehren: `verbraucht = max(0, Zugdauer - round_seconds)` geht
vom `reserve_left` des Teams ab.

`settle_expired()` wird von JEDEM Lese- und Schreibpfad in derselben Transaktion
aufgerufen und loest abgelaufene Zuege auf (zufaelliger freier Held,
`is_auto = TRUE`, Reserve auf 0) — in einer Schleife, falls lange niemand hinsah.

## Slices

### Slice 1 — freie Lobby, live, teilbar (der Kern)
1. Migration in `dl-central-db`: `draft_lobbies`, `bracket_match_id` nullable.
2. `turnier-draft`: Sequenz aus der Lobby statt `const DEFAULT_SEQUENCE`;
   `start_lobby_draft`, Timer-Auswertung in `take_action`/`get_draft_state`.
   Bestehende Turnier-Pfade bleiben grün.
3. `turnier-draft`: Heroes-Provider (API + Cache + Fallback auf `heroes.rs`).
4. `turnier-api`: `POST /api/draft/lobbies`, `GET /api/draft/lobbies/{code}`,
   `POST /api/draft/lobbies/{code}/action` — öffentlich, Token-geprüft.
5. Frontend: Route `/draft`, Lobby anlegen, Hero-Grid, Polling, Timer.
6. Caddy: `/draft*` → :8900 bzw. SPA.

**Fertig, wenn:** zwei Browser draften live gegeneinander durch, Timer läuft ab
und schaltet weiter, Backend-Restart mittendrin verliert nichts.

### Slice 2 — Nachbesprechung
Abgeschlossener Draft unter `/draft/{code}` read-only, Export JSON/CSV,
Draft-Historie. Fällt zu großen Teilen aus Slice 1 ab.

### Slice 3 — Turnier nativ
`DraftPanel.tsx` auf die Lobby-API umstellen, Captains statt Admin,
Steam-Guard über `turnier-steam`. Admin behält Override.

## Bewusst NICHT in Slice 1

- Team-Logo-Upload (statlocker hat es; braucht Storage + Missbrauchsschutz)
- Custom-Sequenz-Editor (erst die 3 Presets: 2 Bans / 1 Ban / keine Bans)
- Shadow-Picks (erst verstehen, ob das jemand nutzt)
- Lobby-Verzeichnis/Sichtbarkeit, Zuschauerzahl
- Kommentare an der Nachbesprechung

## Risiken

- **Sequenz-Umbau bricht Turnier-Draft.** Gegenmittel: bestehende DB-Tests
  müssen unverändert grün bleiben, `DEFAULT_SEQUENCE` bleibt als Preset.
- **Cross-Repo-Migration.** Migration in Deadlock-Bots muss vor dem
  Turniere-Deploy laufen (siehe Memory: sqlx-Checksum-Fallen).
- **Anonyme Lobbys = Missbrauchsfläche.** Rate-Limit auf Lobby-Erstellung,
  freie Textfelder (Teamnamen) escapen.

---

# Stand & TODO (2026-07-16, Ende des Bau-Tages)

Ehrlicher Zwischenstand. `[x]` heisst: gebaut UND verifiziert. Alles andere ist offen.

## Fertig und live

- [x] Totes SQLite-Migrationsverzeichnis entfernt (auf `main`).
      `run_migrations()` war schon ein No-op, die SQL las niemand.
- [x] Frontend auf die Marke umgestellt: Patch-Schwarz `#0b0b0b` + Gold `#c8a86b`,
      Bone-Schrift, flach (kein Holz/Metall/Nieten). SSOT = `Website/dl-brand/tokens.css`.
      Nebenbei: `danger`-Button hatte 3.1:1 Kontrast im Hover, jetzt Bone auf Rust.
- [x] Draft-Seite `/turnier/draft` (Lobby anlegen) und `/turnier/draft/:code` (Board):
      Hero-Grid mit Live-Bildern, Countdown, Captain-Links, Zuschauer-Link.

## Ebenfalls fertig und live

- [x] Migration `2026071610_draft_lobbys.sql` (additiv, auf frischer DB durchgelaufen).
- [x] Sequenz-Presets, Live-Heldenquelle mit Fallback, Lobby-Kern, fauler Timer.
      17 DB-Tests gruen (11 bestehende Turnier-Tests unveraendert + 6 neue Lobby-Tests).
- [x] HTTP-Routen in `turnier-api`, einschließlich Token-, Validierungs- und
      Rate-Limit-Verträgen, gegen die zentrale Wegwerf-DB verifiziert.

## TODO — bis Slice 1 wirklich fertig ist

- [x] **Routen-Ergebnis reviewt.** Die Änderungen am Draft-Repository gehören zum
      persistierten Lobby-, Timer- und Reconnect-Vertrag; ein frischer Kritiker fand
      nach den Korrekturen keinen weiteren Blocker.
- [x] **Migration nach `main` und live.** Die zentrale Migration `2026071610` liegt im
      Deadlock-Bots-`main` und ist produktiv erfolgreich angewendet. `turnier-bot`
      prüft diesen Vertrag beim Start und beendet sich verständlich, falls er fehlt.
- [x] **Turniere nach `main` gemergt**, `cargo build --release --workspace`,
      `systemctl --user restart deadlock-turniere`.
- [x] **Live-Beweis** (alle drei): PID-Wechsel, `/proc/<pid>/exe` zeigt auf die neue Binary,
      `journalctl` sauber. Öffentliche Health-, Seiten- und Lobby-Routen antworten erfolgreich;
      eine echte Aktion blieb über einen weiteren Backend-Neustart erhalten.
- [ ] **Echter Durchlauf zu zweit**: zwei Browser, Draft bis `completed`, Timer ablaufen
      lassen, Backend mittendrin neu starten (muss der Draft ueberleben — das ist der
      ganze Grund fuer Polling statt WebSocket).
- [x] `CHANGELOG.md` ergänzt und auf die echte Route `/turnier/draft` korrigiert.
- [x] Discord-Post nach erfolgreichem Live-Beweis in `#dev-updates` veröffentlicht.

## TODO — Luecken, die ich kenne

- [ ] **Der Captain weiss nicht, welches Team er ist.** Der Server verraet die Tokens nie
      (richtig so), also kann das Board die Zugehoerigkeit nicht ableiten. Aktuell darf
      jeder mit Token klicken und der Server lehnt ggf. mit "Das andere Team ist am Zug" ab.
      Funktioniert, ist aber unschoen. Fix: beim Anlegen `team_slot` neben dem Token in den
      Link legen (rein kosmetisch, keine Autorisierung) ODER die Aktion-Antwort das Team
      verraten lassen.
- [ ] **Sequenz-Leiste fehlt** (die `BAN BAN PICK …`-Reihe oben bei statlocker). Der Zustand
      dafuer ist da (`sequence`, `current_action_index`), nur nicht gezeichnet.
- [ ] **`is_auto` wird nicht angezeigt.** Ein vom Timer gewuerfelter Held sieht aus wie eine
      bewusste Wahl. Das Feld kommt vom Server, die Kachel muss es nur markieren.
- [ ] **Rate-Limit ist prozesslokal** (`ponytail:` im Code vermerkt). Reicht bei einem
      Prozess; bei mehreren Instanzen neu denken.
- [ ] Zwei ungetrackte SQLite-Dateien liegen noch in `backend/data/` herum, eine davon ist
      der Rollback-Snapshot von vor dem PG-Cutover. Nicht in Git — Loeschen ist endgueltig,
      deshalb offen gelassen.

## TODO — Slice 2: Nachbesprechung (Coaching)

- [ ] Abgeschlossener Draft unter `/turnier/draft/:code` read-only — greift heute schon,
      aber ungetestet und ohne eigene Ansicht.
- [ ] Export als JSON und CSV.
- [ ] Draft-Historie (Liste eigener/letzter Drafts).

## TODO — Slice 3: Turnier nativ

- [ ] `DraftPanel.tsx` von der Admin-Route auf die Lobby-API umstellen.
- [ ] Captains draften selbst statt des Admins; Admin behaelt Override.
- [ ] **Steam-Pflicht**: Guard ueber `turnier-steam` (liest die vorhandene Steam-Bridge).
      Kein neuer Login noetig — Discord-Session + verknuepfter Steam reicht als Nachweis.

## Bewusst NICHT gebaut (kein Versehen)

- Team-Logo-Upload — braucht Storage + Missbrauchsschutz, statlocker hat es, wir nicht.
- Custom-Sequenz-Editor — erst die drei Presets, dann sehen wir, ob es jemand vermisst.
- Shadow-Picks — erst verstehen, wozu.
- Lobby-Verzeichnis / oeffentliche Lobby-Liste / Zuschauerzahl.
- Kommentare an der Nachbesprechung.

## Fallen fuer den Naechsten

- **DB-Tests laufen still ins Leere**, wenn man `central_test_db.sh` ohne
  `--manifest-path .../Deadlock-Turniere/rust/Cargo.toml` benutzt: Das Skript wechselt in
  den Bots-Baum, wo es `turnier-draft` nicht gibt. Kein Fehler, kein Test, sieht gruen aus.
- **Der Bots-Checkout ist umkaempft.** Parallele Sessions mergen `main` in den gerade
  ausgecheckten Branch ("Deploy-Baum"). Fuer eigene Arbeit einen Worktree aus `origin/main`.
- **`npm run build` IST der Deploy** fuers Frontend: Caddy serviert `frontend/dist` direkt.
- Die Marken-Wahrheit ist `Website/dl-brand/tokens.css`. Die `ddc-design-tokens.css` im
  Twitch-Repo ist der aeltere Braun-Stand — nicht davon abschreiben.
