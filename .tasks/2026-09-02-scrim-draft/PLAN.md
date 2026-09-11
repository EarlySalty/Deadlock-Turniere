# Plan: Scrim-Draft-Tool mit Lobby-Automatik (Turniere-Seite)

status: aktiv
datum: 2026-09-02
klasse: hoch
research: .tasks/2026-09-02-scrim-draft/RESEARCH.md (Evidence: EVIDENCE.md, Design: DESIGN.md, Contract: CONTRACT.md)

## Ziel

Siehe CONTRACT.md. Fertig, wenn: zwei Browser unter `/turnier/draft/<code>` einen kompletten Draft im DESIGN-Look durchspielen (Claim, Bereit, Bans, Picks, Auto-Pick bei Timer, Endscreen), der Endscreen danach einen echten Join-Code vom Steam-Bot zeigt, in #announcement-scrims genau ein Lobby-Post steht, und nach einem Testmatch genau ein Ergebnis-Post folgt. Alle 17 bestehenden Draft-DB-Tests plus die neuen sind grün, Review-Freigabe liegt vor, main ist deployt.

## Entscheidungen aus dem Research (Orchestrator, technisch)

- E1 (R1): Steam-Bot wird per HTTP-Client auf `http://127.0.0.1:8782/scrims/v1/operations` angesprochen (Vertrag `api-contract::ScrimSteamOperation`: LobbyProvision, LobbyStart, LobbyReconcile, LobbyRelease, FinalResultCollect). Kein Zugriff auf die `steam_tasks`-Queue. Die Vertragstypen werden in Turniere als schlanke serde-Structs gespiegelt (gleiche JSON-Form), kein Crate-Import über Repo-Grenzen.
- E2 (R4): Captain-Claim ist ein expliziter Server-Claim: `POST /api/draft/lobbies/{code}/claim {team: 1|2}` vergibt den Team-Token (nur wenn der Slot frei ist), `POST .../ready {token}` setzt Bereit, der Server startet den Draft (setzt `started_at` und die erste `deadline_at`), sobald beide bereit sind. `POST .../leave {token}` gibt den Slot frei, solange der Draft nicht gestartet ist.
- E3 (R5): Timer "aus" = `round_seconds = 0`: keine Deadline, kein Auto-Pick. Backend-Grenze wird zu `0 | 10..=300`.
- E4 (R3): Ban-Anzahl je Team 0 bis 6 über einen Sequenz-Generator in `sequence.rs`: Bans alternierend Team 1, Team 2 (2·n Aktionen), danach 12 Picks im Snake-Muster 1-2-2-1-1-2-2-1-1-2-2-1. Presets bleiben für Turnier-Drafts erhalten.
- E5 (R2): `Hero` bekommt zusätzlich `card_image_url` (aus `icon_hero_card_webp`), `/api/draft/heroes` liefert beide Felder.
- E6: Lobby-Anfrage läuft asynchron: Abschluss des Drafts setzt `lobby_status = 'angefordert'`, ein Hintergrund-Job in turnier-api ruft LobbyProvision (Timeout 25 s), schreibt `party_id`, `join_code`, `lobby_status = 'bereit'` oder `'fehler'` plus `lobby_error`. Reconcile alle 15 s bis `gestartet`, dann FinalResultCollect alle 30 s bis Ergebnis, dann Release. Frontend pollt den Draft-Zustand wie bisher.
- E7: Discord-Posts über `turnier-discord` (Master-Broker, `idempotency_key = draft:<code>:lobby` bzw. `draft:<code>:ergebnis`), Kanal aus Config `scrim_announce_channel_id`, Default 1521522998199324853.
- E8: Migration liegt im zentralen Migrationsordner `Deadlock-Bots/rust/crates/dl-central-db/migrations/` (braucht Contract-Amendment durch den User, siehe Plan-Review).

## API-Vertrag für das Frontend (verbindlich für Backend- und Frontend-Agent)

`GET /api/draft/lobbies/{code}` liefert zusätzlich zu den bestehenden Feldern:

```json
{
  "phase": "warteraum" | "laeuft" | "abgeschlossen",
  "bans_per_team": 2,
  "round_seconds": 30,
  "team1": {"name": "Team 1", "claimed": true, "ready": false},
  "team2": {"name": "Team 2", "claimed": false, "ready": false},
  "spectators": 1,
  "you": {"team": 1 | 2 | null},
  "sequence": [{"index": 0, "team": 1, "action": "ban"}, ...],
  "current_action_index": 3,
  "deadline_at": "2026-09-02T18:00:30Z" | null,
  "lobby": {"status": "keine" | "angefordert" | "bereit" | "fehler" | "gestartet" | "beendet", "join_code": "ABCDEF" | null, "error": null, "match_id": null, "result": null},
  "rematch_code": null
}
```

Neue Routen: `POST /lobbies/{code}/claim`, `POST /lobbies/{code}/ready`, `POST /lobbies/{code}/leave`, `POST /lobbies/{code}/rematch` (liefert neuen Code, Einstellungen gleich, Seiten getauscht), `POST /lobbies/{code}/lobby/retry`. `POST /lobbies` nimmt zusätzlich `bans_per_team` (0..6, Default 2) und `round_seconds` (0|30|45|60|90, Default 30). Zuschauer zählen über Heartbeat: `GET` mit Header `X-Draft-Viewer: <uuid>` zählt 20 s als anwesend (Prozessspeicher).

## Nicht-Ziele

Siehe CONTRACT.md. Zusätzlich: kein Umbau der Turnier-Drafts (Presets bleiben), kein Redesign anderer Seiten.

## Milestones

### M1: Draft-Kern (Sequenz-Generator, Timer aus, Warteraum, Splash-Feld)
Änderungen: `rust/crates/turnier-draft/src/sequence.rs` (Generator `sequence_for_bans(n)`), `repo.rs` (Claim/Ready/Leave/Start-Übergang, `round_seconds = 0`, Rematch, Lobby-Felder, Zustandsabfrage), `heroes_provider.rs` (`card_image_url`), Tests in `draft_db.rs`/`lobby_db.rs` erweitern. Migration `2026090201_draft_scrim_warteraum_lobby.sql` in `Deadlock-Bots/rust/crates/dl-central-db/migrations/` (additiv: `bans_per_team int not null default 2`, `team1_claimed_at`, `team2_claimed_at`, `team1_ready bool`, `team2_ready bool`, `started_at`, `lobby_status text not null default 'keine'`, `lobby_party_id`, `lobby_join_code`, `lobby_error`, `lobby_match_id`, `lobby_result jsonb`, `discord_lobby_posted_at`, `discord_result_posted_at`, `rematch_of_code`).
Erwarteter Zwischenzustand: neue Tests rot vor der Änderung (Generator liefert für n=3 genau 6 Bans plus 12 Picks; Lobby startet nicht vor beidseitigem Ready; `round_seconds=0` erzeugt keine Deadline und keinen Auto-Pick), danach grün; die 17 Alt-Tests unverändert grün.
Validierung: `DEADLOCK_CENTRAL_DSN=<test-dsn> /home/nathanael/.cargo/bin/cargo test -p turnier-draft` (Test-DB lokal per `createdb`, kein Docker)
Stop-Regel: ein Alt-Test bricht oder die Migration braucht eine nicht-additive Änderung.

### M2: API (neue Routen, erweiterter Zustand, Heroes mit Splash)
Änderungen: `rust/crates/turnier-api/src/draft.rs` (Routen claim/ready/leave/rematch/lobby-retry, `bans_per_team`, `round_seconds` 0-Sonderfall, Zuschauer-Heartbeat, Zustands-JSON wie oben), Tests neben den bestehenden API-Tests.
Erwarteter Zwischenzustand: Vertrag oben ist 1:1 per curl gegen den Dev-Server nachstellbar; alte Clients (Turnier-Draft) bekommen weiterhin ihre Felder.
Validierung: `/home/nathanael/.cargo/bin/cargo test -p turnier-api` plus curl-Skript gegen `127.0.0.1:8900` im Worktree-Start.
Stop-Regel: ein bestehendes Feld ändert Typ oder Bedeutung (INV-02).

### M3: Steam-Bot-Client, Lobby-Job, Discord-Posts
Änderungen: neues Modul `rust/crates/turnier-api/src/scrim_lobby.rs` (HTTP-Client mit reqwest, Timeouts 25 s Provision, 10 s sonst, gespiegelte Vertragstypen, Hintergrund-Job mit Zustandsautomat aus E6), Config-Felder `steam_bot_base_url` (Default `http://127.0.0.1:8782`), `steam_bot_internal_token` (Infisical, bestehender Resolver), `scrim_announce_channel_id`; `turnier-discord/src/notifier.rs` schlanke Methoden `send_scrim_lobby_post` und `send_scrim_result_post` (falls `send_lobby_announcement` eine match_id verlangt). Tests mit `wiremock` oder httptest gegen einen Fake-Steam-Bot (Provision ok, Provision Fehler, Timeout, Reconcile bis gestartet, Ergebnis).
Erwarteter Zwischenzustand: Fake-Steam-Bot-Test zeigt `lobby_status` Übergänge `angefordert -> bereit -> gestartet -> beendet` und genau je einen Discord-Post (Broker gemockt); Fehlerpfad zeigt `fehler` plus Retry.
Validierung: `/home/nathanael/.cargo/bin/cargo test -p turnier-api scrim_lobby`
Stop-Regel: Vertragstypen passen nicht zu `api-contract` im Steam-Bot-Repo (dann Abgleich mit Steam-Bot-Plan, nicht raten).

### M4: Frontend im DESIGN-Look
Änderungen: `frontend/src/pages/DraftLobbyNeu.tsx` (Screen 1), `DraftBoard.tsx` als Container für Warteraum (Screen 2), Board (Screen 3), Endscreen (Screen 4), neue Komponenten unter `frontend/src/components/draft/` (TeamKarte, RaumCode, BanLeiste, TeamSpalte, HeroLeiste, SplashBuehne, BanStempel, PickAnsage, Endkarten, LobbyBox), `hooks/useDraftLobby.ts` (neue Felder, claim/ready/leave/rematch/retry, Viewer-Header), Typen. Texte wortgleich aus DESIGN.md.
Erwarteter Zwischenzustand: Dev-Server zeigt alle vier Screens; zwei Browserfenster (Captain 1, Captain 2) plus ein drittes (Zuschauer) spielen einen Draft mit 2 Bans und Timer 30 s durch; Screenshots je Screen liegen in `.tasks/2026-09-02-scrim-draft/screenshots/`.
Validierung: `cd frontend && npm run build` fehlerfrei, `npx tsc -b`, Playwright- oder Preview-Durchlauf mit drei Fenstern, Screenshots gegen DESIGN.md geprüft.
Stop-Regel: Frontend braucht ein Feld, das der Vertrag oben nicht hat (dann Vertrag hier ergänzen und Backend-Agent informieren, nicht im Frontend rechnen).

### M5: Review, Merge, Deploy, Live-Beweis
Änderungen: keine neuen; Fixes aus dem Review als neue Commits.
Erwarteter Zwischenzustand: REVIEW.md mit FREIGABE; `diff-policy.py` sauber; main gemergt und gepusht; `deadlock-turniere` neu gestartet; `npm run build` deployt; Live-Draft unter `https://deutsche-deadlock-community.de/turnier/draft` mit zwei Browsern und echtem Join-Code (Steam-Bot live), Lobby-Post in #announcement-scrims sichtbar. Ergebnis-Post wird am Testabend mit 12 Spielern geprüft (User-Termin).
Validierung: `python3 /home/nathanael/Documents/claude-config/bin/diff-policy.py /home/nathanael/repos/Deadlock-Turniere origin/main`, Tests wie M1 bis M4, `systemctl --user status deadlock-turniere`, curl Health, Browser-Screenshot.
Stop-Regel: Steam-Bot-Seite nicht live (dann Frontend und Warteraum trotzdem deployen, Lobby-Box zeigt den Fehlertext; Lobby-Pfad folgt mit dem Steam-Bot-Deploy).

## Parallelisierung

- Agent T-BE baut M1 bis M3 im Worktree `feat/scrim-draft-backend`.
- Agent T-FE baut M4 parallel im Worktree `feat/scrim-draft-frontend` gegen den API-Vertrag oben; bis M2 fertig ist, mit einem lokalen Mock (MSW oder Fixture-JSON) entwickeln, dann gegen den BE-Worktree.
- Steam-Bot-Plan (`Deadlock-Steam-Bot/.tasks/2026-09-02-scrim-lobby-automatik/PLAN.md`) läuft parallel; M3 wird gegen dessen Fake und danach gegen den echten Dienst getestet.
- Kein Agent reviewt sich selbst; Review durch frische Agenten (rust-reviewer, plus Frontend-Review durch opus48-coder read-only).

## Verlauf

- 2026-09-02: Plan erstellt, wartet auf Plan-Review durch den User (Klasse hoch) und drei Contract-Amendments (Migrationsort, Steam-Bot-Routen, Steam-Bot-Konstanten).
- 2026-09-10 (Paket B, M1): Baseline turnier-draft grün (16 Unit, 11 draft_db, 3 heroes_provider, 6 lobby_db = 36 passed, 0 ignored). Roter Lauf mit Testnamen: 4 Unit-Tests (`generator_liefert_fuer_n3_sechs_bans_und_zwoelf_picks`, `generator_bans_alternieren_vorne_und_picks_folgen_dem_snake`, `generator_ohne_bans_entspricht_dem_quick_preset`, `generator_mit_drei_bans_entspricht_der_default_sequenz`) und 5 Lobby-DB-Tests (`raum_startet_im_warteraum_ohne_deadline`, `raum_startet_erst_nach_beidseitigem_bereit`, `leave_ohne_start_gibt_den_platz_wieder_frei`, `raum_mit_runde_0_laeuft_ohne_deadline_und_ohne_auto_pick`, `rematch_nach_abschluss_tauscht_die_seiten`) fehlgeschlagen (Stub-Panics), Altbestand unverändert grün. Grüner Lauf: 48 passed (22 Unit, 11 draft_db, 4 heroes_provider, 11 lobby_db), clippy und fmt sauber. Rot-Gegenprobe per Sabotage des Splash-Mappings: Ist 2 failed, Soll 0 failed.
- 2026-09-10 (Paket B, M1, Entscheidungen): Altes `create_lobby` bleibt unverändert (INV-04: die 17 Draft-DB-Tests), freie Scrim-Räume entstehen über neues `create_room` mit Status `warteraum`, unsichtbarem Flip (Mid-Ban REQ4) und Claim/Ready/Leave/Rematch (`claim_room`, `room_ready`, `leave_room`, `rematch_room`); der Claim liefert den beim Anlegen erzeugten Slot-Token, `you.team` kommt in M2 per Header `X-Draft-Token`. Abschluss einer Lobby setzt `lobby_status = 'angefordert'` (Anknüpfung für den M3-Job). Migration `2026091001_draft_scrim_warteraum_lobby.sql` im Bots-Worktree `feat/draft-scrim-warteraum-migration` (additiv, Felder laut Plan). Test-DB anfangs lokal (System-PG 5432, Wegwerf-Rolle `turniere_draft_test`), ab M2 auf Anweisung des Orchestrators über `rust/scripts/central_test_db.sh` (Timescale-Container). API-Validierung für `round_seconds` folgt E3 (`0 | 10..=300`), das UI bietet 0|30|45|60|90 an. Bekannter Altfehler außerhalb des Pakets: `invalid_proposal_transition_returns_conflict` (automatik_routes, erwartete 409, geliefert 400) ist auf origin/main rot.
- 2026-09-10 (Paket B, M2): Roter Lauf mit Testnamen: 8 neue API-Tests fehlgeschlagen (`raum_anlegen_mit_bans_und_timer_aus_liefert_warteraum_vertrag`, `claim_und_ready_starten_erst_nach_beiden_captains`, `leave_ohne_start_gibt_den_slot_frei_und_meldet_fremde_tokens`, `rematch_route_liefert_neuen_raum_mit_getauschten_seiten`, `lobby_retry_setzt_einen_fehler_zurueck`, `zuschauer_zaehlen_ueber_den_viewer_header`, `legacy_lobby_behaelt_ihren_vertrag`, `helden_route_liefert_objekte_mit_live_vertrag`), Altbestand grün. Grüner Lauf gegen den Timescale-Container aus `central_test_db.sh` (Migration zuerst aus dem Bots-Worktree eingespielt): draft_lobby_routes 15 passed, turnier-api gesamt 91 passed plus der bekannte Automatik-Altfehler, turnier-draft 48 passed, fmt/clippy sauber. Befund währenddessen: die `you.team`-Query lieferte INT4 statt INT8 und wurde auf `1::BIGINT/2::BIGINT` geglättet. Vertrag final: Claim-Antwort `{team, token}` (Header-Name für GET: `X-Draft-Token`), Zuschauer-Header `X-Draft-Viewer`, Ready/Leave/Retry antworten mit dem erweiterten GET-Zustand (`started` zusätzlich bei Ready), Rematch antwortet `{code}`, `POST /lobbies` mit `bans_per_team` antwortet nur `{code}` (Warteraum-Raum, Slot-Tokens werden beim Claim vergeben).
- 2026-09-10 (Paket B, M3): Neues Modul `turnier-api/src/scrim_lobby.rs`: gespiegelte, schlanke Vertragstypen für `scrims/v1/operations` (LobbyProvision, LobbyReconcile, FinalResultCollect, LobbyRelease), HTTP-Client (Timeout 25 s Provision, 10 s Rest, Header `X-Internal-Token` aus `steam_bot_internal_token`), Zustandsautomat `angefordert -> bereit -> gestartet -> beendet | fehler` mit Reconcile alle 15 s und Ergebnisabruf alle 30 s (Takt im Prozessspeicher), Hintergrund-Worker `spawn_scrim_lobby_worker` (5-s-Tick, verdrahtet in `turnier-bot/src/main.rs`). Discord-Posts über `turnier-discord` mit `idempotency_key draft:<code>:lobby` bzw. `draft:<code>:ergebnis`, Kanal aus `scrim_announce_channel_id` (Default je Contract auf 1521522998199324853 geändert, Seiteneffekt siehe Bericht), Ergebniskarten-Slot in `send_scrim_result_post` reserviert (Paket D). Config neu: `steam_bot_base_url` (Default `http://127.0.0.1:8782`), `steam_bot_internal_token`. Tests mit axum-Fake-Steam-Bot und axum-Fake-Broker statt wiremock (keine neue Abhängigkeit): `angefordert -> bereit -> gestartet -> beendet` inklusive Provision-Fehler, Retry und Einmal-Post-Nachweis, 3 passed; turnier-api gesamt 94 passed plus der bekannte Automatik-Altfehler; clippy/fmt sauber. Abweichung: `turnier-bot/src/main.rs` (eine Zeile Worker-Verdrahtung nach bestehendem Muster) und `turnier-config/src/lib.rs` (Config-Felder) liegen streng genommen außerhalb der Erlaubt-Liste des Contracts, sind aber Voraussetzung für den Betrieb des Jobs.
