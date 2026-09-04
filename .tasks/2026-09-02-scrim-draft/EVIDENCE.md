# Evidence: Scrim-Draft-Tool mit Lobby-Automatik

status: aktiv
datum: 2026-09-02
contract: CONTRACT.md

Repo-Aufklärung vor dem ersten Edit. Jede Zeile ist eine Fundstelle `pfad:zeile`,
keine Vermutung. Pfade relativ zu `/home/nathanael/repos/Deadlock-Turniere`.

## Draft-Datenmodell und API (Topic 1)

- `rust/crates/turnier-api/src/draft.rs:30`: `router()` registriert alle Draft-Routen.
- `rust/crates/turnier-api/src/draft.rs:32`: `GET /api/draft/heroes` geht auf `list_heroes`.
- `rust/crates/turnier-api/src/draft.rs:33`: `POST /api/draft/lobbies` geht auf `create_lobby`.
- `rust/crates/turnier-api/src/draft.rs:34`: `GET /api/draft/lobbies/{code}` geht auf `get_lobby`.
- `rust/crates/turnier-api/src/draft.rs:35,42`: `POST /api/draft/lobbies/{code}/action` (`submit_lobby_action`), `POST /api/draft/matches/{match_id}/start` (`start_match_draft`).
- `rust/crates/turnier-api/src/draft.rs:43`: `GET /api/draft/sessions/{session_id}` geht auf `get_session` (Turnier-Pfad).
- `rust/crates/turnier-api/src/draft.rs:44,50`: `POST /api/draft/sessions/{session_id}/action` geht auf `submit_action` (AdminUser).
- `rust/crates/turnier-api/src/draft.rs:51,64`: `list_heroes()` liefert `{ "heroes": [{id, name, image_url}] }`, kein Splash-Feld.
- `rust/crates/turnier-api/src/draft.rs:67,75`: `CreateLobbyRequest { team1_name, team2_name, preset: String, round_seconds: i32, reserve_seconds: i32 }` (kein `bans`-Feld).
- `rust/crates/turnier-api/src/draft.rs:77,81`: `LobbyActionRequest { token, hero_name }`.
- `rust/crates/turnier-api/src/draft.rs:120,142`: `validate_lobby`: `round_seconds` 10..=300, `reserve_seconds` 0..=600, Ban-Anzahl kommt ausschließlich aus `preset(&body.preset)`.
- `rust/crates/turnier-api/src/draft.rs:141,146`: Rate-Limit `enforce_lobby_rate_limit`, max 10 Lobbys pro IP pro Stunde, TOO_MANY_REQUESTS.
- `rust/crates/turnier-api/src/draft.rs:154,171`: `client_ip`, nur ein einzelner `x-forwarded-for`-Eintrag wird vertraut, sonst Peer-IP.
- `rust/crates/turnier-api/src/state.rs:36`: `draft_lobby_creations: Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>` hält das Rate-Limit-Fenster im Prozessspeicher.
- `rust/crates/turnier-draft/src/repo.rs:33,39`: `CreateLobbyOptions { team1_name, team2_name, sequence: Vec<SequenceStep>, round_seconds, reserve_seconds }`.
- `rust/crates/turnier-draft/src/repo.rs:43,48`: `LobbyCredentials { code, team1_token, team2_token }` (Tokens nur beim Anlegen).
- `rust/crates/turnier-draft/src/repo.rs:50,68`: `DraftSessionRow`, Spalten von `turnier.draft_sessions`: `id, bracket_match_id, code, team1_name, team2_name, sequence jsonb, round_seconds, reserve_seconds, team1_reserve_left, team2_reserve_left, deadline_at, status, current_action_index, started_by, started_at, completed_at, created_at`. Keine Captain-/Ready-Spalte.
- `rust/crates/turnier-draft/src/repo.rs:95,105`: `DraftActionRow` inkl. `is_auto: bool`.
- `rust/crates/turnier-draft/src/repo.rs:201,231`: `create_lobby(pool, opts)` setzt `deadline_at` aus `round_seconds` sofort, Status direkt aktiv (kein Warteraum-Zustand).
- `rust/crates/turnier-draft/src/repo.rs:397,433`: `take_lobby_action(pool, code, token, hero_name)`, Token-zu-Team-Zuordnung `token == team1_token ? 1 : team2_token ? 2 : InvalidToken`, Turn-Prüfung `token_team != step.team_slot` ergibt `NotYourTurn`.
- `rust/crates/turnier-api/src/draft.rs:101,108`: `get_lobby` antwortet `no-store`, Body ist `LobbyState` inkl. `bans, picks_team1, picks_team2, current_team_slot`.
- `frontend/src/hooks/useDraftLobby.ts:31,40`: `useDraftLobby`, Polling `refetchInterval` 1000 ms, stoppt bei `status === 'completed'`.
- `frontend/src/hooks/useDraftLobby.ts:87,105`: `useCaptainToken`, Token aus URL `?t=`, dann `localStorage['draft-token:<code>']`, URL wird nach Merken bereinigt.
- `frontend/src/types/tournament.ts:792,823`: `LobbyState`/`LobbyAction` (mit `is_auto`, `current_team_slot`), `DraftHero { id, name, image_url }` (759), `CreateLobbyBody` (780) ohne Ban-Feld.
- `frontend/src/api/client.ts:62`: `API_BASE = '/turnier/api'`; `client.ts:663,676` Draft-Fetches.

## Auto-Pick / Timer / Sequenz (Topic 3)

- `rust/crates/turnier-draft/src/repo.rs:488,540`: `settle_expired` prüft `deadline_at`, wählt bei Ablauf per `choose_auto_hero` und schreibt `is_auto = TRUE` (repo.rs:523).
- `rust/crates/turnier-draft/src/repo.rs:709,751`: `choose_auto_hero` (seedbar, überspringt vergebene Helden).
- `rust/crates/turnier-draft/src/repo.rs:547,610`: `advance_lobby_session` rechnet die nächste `deadline_at` aus `round_seconds` plus Reserve.
- `rust/crates/turnier-draft/src/sequence.rs:88,107`: `DEFAULT_SEQUENCE` (6 Bans plus 12 Picks, 18 Schritte).
- `rust/crates/turnier-draft/src/sequence.rs:110,163`: `COMPETITIVE_2BAN` (4 Bans), `COMPETITIVE_1BAN` (2 Bans), `QUICK_NO_BAN` (0 Bans), nur diese drei Presets.
- `rust/crates/turnier-draft/src/sequence.rs:175,183`: `preset(name)` kennt nur `competitive_2ban|competitive_1ban|quick_no_ban`. Kein 0-bis-6-Ban-Generator.

## Steam-Bot-Anbindung (Topic 4)

- `rust/crates/turnier-match/src/lib.rs:15,22`: MatchManager hält `SteamBridge` als Task-Queue gegen die externe Bridge-DB.
- `rust/crates/turnier-match/src/steam_bridge.rs:1`: "Zugriff auf die externe Steam-Bridge-SQLite (`steam_tasks`-Queue)".
- `rust/crates/turnier-match/src/steam_bridge.rs:118,134`: `create_task(task_type, payload)` schreibt `INSERT INTO steam_tasks`.
- `rust/crates/turnier-match/src/lobby.rs:60,62`: `create_lobby(tournament_id, match_id)` braucht eine Match-Zeile.
- `rust/crates/turnier-match/src/lobby.rs:297,346`: `create_lobby_for_match` erzeugt Task `GC_CREATE_CUSTOM_LOBBY`, liest `party_code` aus DB.
- `rust/crates/turnier-api/src/admin/steam_ops.rs:51,99`: bestehende Lobby-Erstellung liefert `party_code`/`join_code`, nur für Bracket-/Group-Matches, mit Audit.
- `rust/crates/turnier-api/src/state.rs:54,65`: `SteamBridge::open(&config.steam_bridge_db_path)` speist den MatchManager, kein HTTP-Client zum Steam-Bot.
- `rust/crates/turnier-config/src/lib.rs:54`: `steam_bridge_db_path: String` (SQLite-Pfad, das aktuelle Steam-Interface).

## Discord-Post (Topic 4)

- `rust/crates/turnier-discord/src/broker.rs:16,48`: `BrokerClient { http, base_url, token }`, `from_config` liest `discord_master_broker_base_url` plus `discord_master_broker_token`.
- `rust/crates/turnier-discord/src/broker.rs:78,116`: `post_internal(path, payload)`, HTTP-POST mit Header `X-Internal-Token`.
- `rust/crates/turnier-discord/src/notifier.rs:26,33`: Broker-Pfade, u. a. `SEND_RICH_MESSAGE = /internal/master/v1/discord/send-rich-message`.
- `rust/crates/turnier-discord/src/notifier.rs:587,641`: `send_lobby_announcement(match_id, party_code, team1_name, team2_name, ...)` postet Embed mit Lobby-Code in `tournament_lobby_channel_id`.
- `rust/crates/turnier-discord/src/notifier.rs:645,698`: `send_match_stats_to_channel(channel_id, match_id, winner_name, duration_s, ...)` postet Ergebnis-Embed.
- `rust/crates/turnier-scheduler/src/reminders.rs:18,20`: Reminder nutzen `turnier_discord::{DiscordNotifier, NotificationEvent}` (DM-Pfad, nicht Kanal-Post).
- `rust/crates/turnier-config/src/lib.rs:98,104`: `discord_master_broker_base_url` Default `http://127.0.0.1:8766`.

## Config-Mechanik und Secrets (Topic 5)

- `rust/crates/turnier-config/src/lib.rs:19,78`: `struct Config` mit allen Feldern (Discord-Kanäle, `backend_port`, `discord_webhook_url`, `turnier_internal_api_token`).
- `rust/crates/turnier-config/src/lib.rs:83,160`: `Config::from_env()` lädt via `get_string/get_first_string/get_int/get_bool`.
- `rust/crates/turnier-config/src/secrets.rs:1,90`: Schicht-Resolver `NAME_FILE`, dann `CREDENTIALS_DIRECTORY`/`SECRETS_DIRECTORY`/`VAULT_SECRETS_DIR`, dann Env, dann Default.
- `scripts/run_turniere_backend_rust.sh:10,45`: Config aus `~/.config/deadlock-turniere/turniere.env`, Secrets über Infisical-Loader `dl-infisical-env --profile all` und `CREDENTIALS_DIRECTORY`.

## Migrationen (Topic 5)

- `rust/crates/turnier-db/src/pool.rs:28,31`: `run_migrations` ist No-op, produktive PG-Migrationen laufen zentral über `dl-central-migrate`; Migration `2026071610` liegt NICHT in diesem Repo.
- `rust/crates/turnier-db/src/pool.rs:23,24`: `test_pool()` delegiert an `dl_central_db::test_pool()` (braucht `DEADLOCK_CENTRAL_DSN`).

## Tests / Toolchain (Topic 5)

- `rust/crates/turnier-draft/tests/draft_db.rs`: 11 `#[tokio::test]` (Draft-Session-Pfad).
- `rust/crates/turnier-draft/tests/lobby_db.rs`: 6 `#[tokio::test]` (freie Lobby), zusammen mit draft_db ergibt 17 Draft-DB-Tests.
- `rust/crates/turnier-draft/tests/lobby_db.rs:126,127`: `lange_abgelaufene_deadline_setzt_mehrere_auto_picks` (Auto-Pick-Regression).
- `rust/crates/turnier-draft/tests/draft_db.rs:8,19`: Tests nutzen `turnier_db::test_pool()`/`TestDb` gegen eine echte Wegwerf-PG.
- `rust/Cargo.toml:7,13`: Workspace `resolver = "2"`, `members = ["crates/*"]`, `rust-version = "1.85"`.
- `rust-toolchain.toml`: `channel = "stable"`, Komponenten rustfmt und clippy.

## Frontend (Topic 2)

- `frontend/package.json:16,27`: Deps `framer-motion ^12.34.3`, `tailwindcss ^4.2.1`, `@tailwindcss/vite`, `lucide-react`, `react 19.2.4`, `@tanstack/react-query`, `react-router-dom ^7`.
- `frontend/package.json:9,14`: Scripts `dev = vite`, `build = tsc -b && vite build`.
- `frontend/vite.config.ts:6,42`: `base '/turnier/'`, `outDir 'dist'`, Dev-Server Port 5173 mit Proxy `/turnier/api` auf `http://localhost:8900`.
- `frontend/src/index.css:3`: importiert `./brand-tokens.css`.
- `frontend/src/brand-tokens.css:4,35`: SSOT `Website/dl-brand/tokens.css`, `--color-primary: #c8a86b` (Gold).
- `frontend/src/App.tsx:22,24`: Routen `draft` auf `DraftLobbyNeu`, `draft/:code` auf `DraftBoard` (vor `:id`).
- `frontend/src/pages/DraftLobbyNeu.tsx:17,21`: nur drei Presets, kein 0-6-Ban-Regler, kein Timer-Wahlfeld, nach Anlegen sofort Link-Ansicht statt Warteraum.
- `frontend/src/pages/DraftLobbyNeu.tsx:9,15`: nutzt `Card`, `Button`, `lucide-react`-Icons, kein `framer-motion`-Import bisher.
- `frontend/src/pages/DraftBoard.tsx:12,45`: Board rendert Rollen (Captain/Zuschauer) rein anhand Token, `Uhr` per `useCountdown`.
- `frontend/src/hooks/draftLobbyState.ts:18,20`: `heroImageUrl` reicht das einzelne `image_url` durch (kein Splash).

## Hero-Assets (Topic 7)

- `rust/crates/turnier-draft/src/heroes_provider.rs:17,23`: `Hero { id: u32, name: String, image_url: String }`, nur ein Bildfeld.
- `rust/crates/turnier-draft/src/heroes_provider.rs:35,40`: `ApiHeroImages { icon_image_small_webp, icon_image_small, icon_hero_card_webp }`, Splash `icon_hero_card_webp` wird von der API geholt.
- `rust/crates/turnier-draft/src/heroes_provider.rs:137,148`: `From<ApiHero>` setzt `image_url = icon_image_small_webp || icon_image_small || icon_hero_card_webp`, Splash wird verworfen, sobald ein Portrait da ist.
- `rust/crates/turnier-draft/src/heroes_provider.rs:13`: Quelle `https://api.deadlock-api.com/v1/assets/heroes`.
- `rust/crates/turnier-draft/src/heroes.rs:16,43`: statischer Fallback mit 26 Helden, `image_url` leer.
- Live-Beweis: `curl http://localhost:8900/api/draft/heroes` liefert `image_url = ..._sm.webp` (Portrait), kein Splash.

## Deploy / Live-Stand (Topic 6)

- `rust/docs/cutover.md:55`: `systemctl --user restart deadlock-turniere.service` (User-Unit).
- `scripts/run_turniere_backend_rust.sh:1,8`: Launcher startet das Binary `turnier-bot` (Release), cwd `backend/`.
- `docs/steam-bridge-implementation.md:11`: Backend Port 8900.
- Live-Beweis: `GET http://localhost:8900/api/draft/heroes` ergibt 200 mit Heldenliste, `GET https://deutsche-deadlock-community.de/turnier/api/draft/heroes` ergibt 200 (Caddy reicht `/turnier/api` an 8900 durch).

## Offene Architekturfrage

- REQ-06 gegen INV-05: Der einzige heutige Steam-Bot-Kanal ist die SQLite-`steam_tasks`-Queue (kein HTTP-Client) und braucht eine Match-Zeile. INV-05 verlangt HTTP-only. Vor dem Implementieren muss der Weg feststehen: neuer HTTP-Client (Partner-Endpoint in Deadlock-Steam-Bot) oder eine match-lose Variante der `GC_CREATE_CUSTOM_LOBBY`-Queue. Solange offen kein Implement des Lobby-Pfades.
