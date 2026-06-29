# Phase0 Task3a Audit Inventar+Mapping

Datum: 2026-06-29  
Scope: Python-Original unter `backend/` ohne `.venv/`, `__pycache__/`, fuer das Routen-Inventar ohne `backend/tests/`; Rust-Rewrite unter `rust/crates/turnier-*`.

## Kopfzaehlung

| Inventar | Total | vorhanden | abweichend | fehlt | bewusst_ausgelassen | unsicher |
|---|---:|---:|---:|---:|---:|---:|
| Python-HTTP-Routen | 109 | 100 | 9 | 0 | 0 | 0 |
| Nicht-Routen-Features | 35 | 32 | 2 | 0 | 1 | 0 |

Automatisierte Methode/Pfad-Gegenprobe: 109 Python-Routen und 109 Rust-Routen; keine fehlende oder zusaetzliche Methode/Pfad-Kombination gefunden.

## HTTP-Routen: Auth/System

| Python | Rust | Status | Auth/Zweck/Notiz |
|---|---|---|---|
| `GET /auth/discord/login` `discord_login` @ `backend/auth/discord_oauth.py:84` | `login` @ `rust/crates/turnier-api/src/auth.rs:32` | vorhanden | public; Redirect zum zentralen Discord-OAuth-Service. |
| `GET /auth/discord/complete` `discord_complete` @ `backend/auth/discord_oauth.py:106` | `complete` @ `rust/crates/turnier-api/src/auth.rs:33` | vorhanden | public; OAuth-Ergebnis konsumieren, lokale Session/Cookie erzeugen. |
| `GET /auth/discord/logout` `discord_logout` @ `backend/auth/discord_oauth.py:165` | `logout` @ `rust/crates/turnier-api/src/auth.rs:34` | vorhanden | public; lokale Session/Cookie loeschen. |
| `GET /api/me` `get_me` @ `backend/main.py:74` | `me` @ `rust/crates/turnier-api/src/app.rs:26` | vorhanden | `get_current_user`/`AuthUser`; aktuelle User-Session. |
| `GET /api/health` `health` @ `backend/main.py:81` | `health` @ `rust/crates/turnier-api/src/app.rs:27` | vorhanden | public; Health-Check. |

## HTTP-Routen: Public Tournament

| Python | Rust | Status | Auth/Zweck/Notiz |
|---|---|---|---|
| `GET /api/tournaments` `list_tournaments` @ `backend/tournament/routes.py:652` | `list_tournaments` @ `rust/crates/turnier-api/src/public/tournaments.rs:21` | vorhanden | public; nicht-Draft-Turniere listen. |
| `GET /api/tournaments/{tournament_id}` `get_tournament` @ `backend/tournament/routes.py:667` | `get_tournament` @ `rust/crates/turnier-api/src/public/tournaments.rs:22` | vorhanden | public; Turnier-Detail mit Teams/Gruppen/Bracket. |
| `GET /api/tournaments/{tournament_id}/me` `get_my_tournament_status` @ `backend/tournament/routes.py:708` | `get_my_tournament_status` @ `rust/crates/turnier-api/src/public/tournaments.rs:23` | vorhanden | `require_auth`/`AuthUser`; eigener Turnierstatus. |
| `POST /api/tournaments/{tournament_id}/teams` `create_team` @ `backend/tournament/routes.py:760` | `create_team` @ `rust/crates/turnier-api/src/public/teams.rs:23` | vorhanden | `require_auth`; Team erstellen, aktueller User wird Captain. |
| `POST /api/tournaments/{tournament_id}/teams/{team_id}/join` `join_team` @ `backend/tournament/routes.py:873` | `join_team` @ `rust/crates/turnier-api/src/public/teams.rs:24` | vorhanden | `require_auth`; Team beitreten. |
| `POST /api/tournaments/{tournament_id}/signup` `solo_signup` @ `backend/tournament/routes.py:1049` | `solo_signup` @ `rust/crates/turnier-api/src/public/signups.rs:17` | vorhanden | `require_auth`; Solo-Anmeldung. |
| `PATCH /api/tournaments/{tournament_id}/teams/{team_id}/recruiting` `update_team_recruiting` @ `backend/tournament/routes.py:1162` | `update_team_recruiting` @ `rust/crates/turnier-api/src/public/teams.rs:25` | vorhanden | `require_auth`; Recruiting-Status setzen. |
| `POST /api/tournaments/{tournament_id}/teams/{team_id}/invite-by-signup/{signup_id}` `invite_to_team_by_signup` @ `backend/tournament/routes.py:1197` | `invite_to_team_by_signup` @ `rust/crates/turnier-api/src/public/invitations.rs:21` | vorhanden | `require_auth`; Solo-Signup einladen. |
| `GET /api/tournaments/{tournament_id}/my-invitations` `get_my_invitations` @ `backend/tournament/routes.py:1306` | `get_my_invitations` @ `rust/crates/turnier-api/src/public/invitations.rs:25` | vorhanden | `require_auth`; eigene Einladungen listen. |
| `POST /api/tournaments/{tournament_id}/invitations/{invite_id}/accept` `accept_team_invitation` @ `backend/tournament/routes.py:1331` | `accept_team_invitation` @ `rust/crates/turnier-api/src/public/invitations.rs:26` | vorhanden | `require_auth`; Einladung annehmen. |
| `POST /api/tournaments/{tournament_id}/invitations/{invite_id}/reject` `reject_team_invitation` @ `backend/tournament/routes.py:1406` | `reject_team_invitation` @ `rust/crates/turnier-api/src/public/invitations.rs:30` | vorhanden | `require_auth`; Einladung ablehnen. |
| `POST /api/tournaments/{tournament_id}/teams/{team_id}/apply` `apply_to_team` @ `backend/tournament/routes.py:1449` | `apply_to_team` @ `rust/crates/turnier-api/src/public/invitations.rs:34` | vorhanden | `require_auth`; Teambewerbung erstellen. |
| `GET /api/tournaments/{tournament_id}/teams/{team_id}/applications` `get_team_applications` @ `backend/tournament/routes.py:1506` | `get_team_applications` @ `rust/crates/turnier-api/src/public/invitations.rs:35` | vorhanden | `require_auth`; Teambewerbungen listen. |
| `POST /api/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept` `accept_team_application` @ `backend/tournament/routes.py:1530` | `accept_team_application` @ `rust/crates/turnier-api/src/public/invitations.rs:39` | vorhanden | `require_auth`; Bewerbung annehmen. |
| `POST /api/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject` `reject_team_application` @ `backend/tournament/routes.py:1603` | `reject_team_application` @ `rust/crates/turnier-api/src/public/invitations.rs:43` | vorhanden | `require_auth`; Bewerbung ablehnen. |
| `DELETE /api/tournaments/{tournament_id}/signup` `cancel_solo_signup` @ `backend/tournament/routes.py:1644` | `cancel_solo_signup` @ `rust/crates/turnier-api/src/public/signups.rs:17` | vorhanden | `require_auth`; Solo-Anmeldung zurueckziehen. |
| `POST /api/tournaments/{tournament_id}/checkin` `checkin_player` @ `backend/tournament/routes.py:1698` | `checkin_player` @ `rust/crates/turnier-api/src/public/signups.rs:21` | vorhanden | `require_auth`; Spieler-Check-in. |
| `GET /api/tournaments/{tournament_id}/checkin-status` `get_checkin_status` @ `backend/tournament/routes.py:1784` | `get_checkin_status` @ `rust/crates/turnier-api/src/public/tournaments.rs:26` | vorhanden | public; Check-in-Stand. |
| `DELETE /api/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}` `kick_team_member` @ `backend/tournament/routes.py:1844` | `kick_team_member` @ `rust/crates/turnier-api/src/public/teams.rs:27` | vorhanden | `require_auth`; Captain kickt Mitglied. |
| `POST /api/tournaments/{tournament_id}/teams/{team_id}/invite/{target_discord_id}` `invite_to_team` @ `backend/tournament/routes.py:1952` | `invite_to_team` @ `rust/crates/turnier-api/src/public/teams.rs:31` | vorhanden | `require_auth`; Direkt-Einladung/Direct-Add. |
| `DELETE /api/tournaments/{tournament_id}/teams/{team_id}/leave` `leave_team` @ `backend/tournament/routes.py:2092` | `leave_team` @ `rust/crates/turnier-api/src/public/teams.rs:26` | vorhanden | `require_auth`; Team verlassen. |
| `GET /api/tournaments/{tournament_id}/bracket` `get_bracket` @ `backend/tournament/routes.py:2239` | `get_bracket` @ `rust/crates/turnier-api/src/public/tournaments.rs:24` | vorhanden | public; Bracket lesen. |
| `GET /api/tournaments/{tournament_id}/groups` `get_groups` @ `backend/tournament/routes.py:2259` | `get_groups` @ `rust/crates/turnier-api/src/public/tournaments.rs:25` | vorhanden | public; Gruppen lesen. |

## HTTP-Routen: Admin Tournament

| Python | Rust | Status | Auth/Zweck/Notiz |
|---|---|---|---|
| `GET /api/admin/tournaments` `list_tournaments_admin` @ `backend/tournament/admin_routes.py:732` | `list_tournaments` @ `rust/crates/turnier-api/src/admin/tournaments.rs:38` | vorhanden | `require_mod`; Admin-Turnierliste inkl. Drafts. |
| `GET /api/admin/tournaments/{tournament_id}` `get_tournament_admin` @ `backend/tournament/admin_routes.py:745` | `get_tournament` @ `rust/crates/turnier-api/src/admin/tournaments.rs:39` | vorhanden | `require_mod`; Admin-Detail. |
| `GET /api/admin/tournaments/{tournament_id}/mini-groups` `get_tournament_mini_groups_admin` @ `backend/tournament/admin_routes.py:780` | `get_mini_groups` @ `rust/crates/turnier-api/src/admin/tournaments.rs:43` | vorhanden | `require_mod`; Mini-Groups lesen. |
| `POST /api/admin/tournaments/{tournament_id}/auto-lobby/run` `trigger_auto_lobby_for_tournament` @ `backend/tournament/admin_routes.py:805` | `run_auto_lobby` @ `rust/crates/turnier-api/src/admin/tournaments.rs:44` | vorhanden | `require_mod`; Auto-Lobby fuer Turnier triggern. |
| `POST /api/admin/tournaments` `create_tournament` @ `backend/tournament/admin_routes.py:821` | `create_tournament` @ `rust/crates/turnier-api/src/admin/tournaments.rs:38` | vorhanden | `require_mod`; Turnier erstellen, 201. |
| `PUT /api/admin/tournaments/{tournament_id}` `update_tournament` @ `backend/tournament/admin_routes.py:914` | `update_tournament` @ `rust/crates/turnier-api/src/admin/tournaments.rs:252` | abweichend | Python kann explizites `null` via `model_dump(exclude_unset=True)` in nullbare Spalten schreiben (`backend/tournament/admin_routes.py:967`); Rust `TournamentUpdate` nutzt `Option<T>` und kann Feld-fehlt vs. `null` nicht unterscheiden (`rust/crates/turnier-core/src/tournament.rs:107`). Dokumentiert in `rust/docs/known-issues.md:316`. |
| `DELETE /api/admin/tournaments/{tournament_id}` `delete_tournament` @ `backend/tournament/admin_routes.py:1078` | `delete_tournament` @ `rust/crates/turnier-api/src/admin/tournaments.rs:39` | vorhanden | `require_admin`; Turnier loeschen. |
| `POST /api/admin/tournaments/{tournament_id}/open-checkin` `open_checkin` @ `backend/tournament/admin_routes.py:1104` | `open_checkin` @ `rust/crates/turnier-api/src/admin/tournaments.rs:45` | vorhanden | `require_mod`; Check-in oeffnen. |
| `POST /api/admin/tournaments/{tournament_id}/revert-checkin` `revert_checkin` @ `backend/tournament/admin_routes.py:1157` | `revert_checkin` @ `rust/crates/turnier-api/src/admin/tournaments.rs:46` | vorhanden | `require_mod`; Check-in auf Registration zuruecksetzen. |
| `POST /api/admin/tournaments/{tournament_id}/finalize-checkin` `finalize_checkin_endpoint` @ `backend/tournament/admin_routes.py:1207` | `finalize_checkin_route` @ `rust/crates/turnier-api/src/admin/phases.rs:27` | vorhanden | `require_mod`; Check-ins bereinigen/finalisieren. |
| `POST /api/admin/tournaments/{tournament_id}/advance` `advance_tournament` @ `backend/tournament/admin_routes.py:1258` | `advance_tournament` @ `rust/crates/turnier-api/src/admin/tournaments.rs:47` | vorhanden | `require_mod`; Phase weiterschalten. |
| `POST /api/admin/tournaments/{tournament_id}/assign-random` `assign_random` @ `backend/tournament/admin_routes.py:1334` | `assign_random` @ `rust/crates/turnier-api/src/admin/phases.rs:28` | vorhanden | `require_mod`; Solo-Signups zufaellig Teams zuweisen. |
| `POST /api/admin/tournaments/{tournament_id}/teams` `create_team_admin` @ `backend/tournament/admin_routes.py:1367` | `create_team` @ `rust/crates/turnier-api/src/admin/teams.rs:29` | vorhanden | `require_mod`; leeres Team anlegen, 201. |
| `PUT /api/admin/tournaments/{tournament_id}/teams/{team_id}` `rename_team_admin` @ `backend/tournament/admin_routes.py:1410` | `rename_team` @ `rust/crates/turnier-api/src/admin/teams.rs:30` | vorhanden | `require_mod`; Team umbenennen. |
| `PATCH /api/admin/tournaments/{tournament_id}/teams/{team_id}/recruiting` `update_team_recruitment_status_admin` @ `backend/tournament/admin_routes.py:1454` | `update_recruiting` @ `rust/crates/turnier-api/src/admin/teams.rs:31` | vorhanden | `require_mod`; Recruiting-Status setzen. |
| `GET /api/admin/tournaments/{tournament_id}/teams/{team_id}/applications` `list_team_applications_admin` @ `backend/tournament/admin_routes.py:1510` | `list_applications` @ `rust/crates/turnier-api/src/admin/teams.rs:32` | vorhanden | `require_mod`; Bewerbungen listen. |
| `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept` `accept_team_application_admin` @ `backend/tournament/admin_routes.py:1536` | `accept_application` @ `rust/crates/turnier-api/src/admin/teams.rs:33` | vorhanden | `require_mod`; Bewerbung annehmen. |
| `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject` `reject_team_application_admin` @ `backend/tournament/admin_routes.py:1649` | `reject_application` @ `rust/crates/turnier-api/src/admin/teams.rs:37` | vorhanden | `require_mod`; Bewerbung ablehnen. |
| `DELETE /api/admin/tournaments/{tournament_id}/teams/{team_id}` `delete_team_admin` @ `backend/tournament/admin_routes.py:1695` | `delete_team` @ `rust/crates/turnier-api/src/admin/teams.rs:30` | vorhanden | `require_mod`; Team loeschen. |
| `PUT /api/admin/tournaments/{tournament_id}/teams/{team_id}/captain` `change_team_captain_admin` @ `backend/tournament/admin_routes.py:1734` | `change_captain` @ `rust/crates/turnier-api/src/admin/teams.rs:41` | vorhanden | `require_mod`; Captain wechseln. |
| `DELETE /api/admin/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}` `remove_team_member_admin` @ `backend/tournament/admin_routes.py:1782` | `remove_member` @ `rust/crates/turnier-api/src/admin/teams.rs:42` | vorhanden | `require_mod`; Mitglied entfernen. |
| `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/members/move` `move_team_member_admin` @ `backend/tournament/admin_routes.py:1824` | `move_member` @ `rust/crates/turnier-api/src/admin/teams.rs:46` | vorhanden | `require_mod`; Mitglied verschieben. |
| `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/signups/assign` `assign_signup_to_team_admin` @ `backend/tournament/admin_routes.py:1910` | `assign_signup` @ `rust/crates/turnier-api/src/admin/teams.rs:47` | vorhanden | `require_mod`; Signup Team zuweisen. |
| `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/add-member` `add_team_member_admin` @ `backend/tournament/admin_routes.py:1993` | `add_member` @ `rust/crates/turnier-api/src/admin/teams.rs:48` | vorhanden | `require_admin`; Ersatzspieler direkt hinzufuegen. |
| `DELETE /api/admin/tournaments/{tournament_id}/signups/{signup_id}` `delete_signup_admin` @ `backend/tournament/admin_routes.py:2097` | `delete_signup` @ `rust/crates/turnier-api/src/admin/teams.rs:49` | vorhanden | `require_mod`; Solo-Signup loeschen. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/result` `set_match_result` @ `backend/tournament/admin_routes.py:2138` | `set_match_result` @ `rust/crates/turnier-api/src/admin/matches.rs:27` | vorhanden | `require_mod`; manuelles Bracket-Ergebnis. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/games/{game_number}/start` `start_series_game` @ `backend/tournament/admin_routes.py:2293` | `start_series_game` @ `rust/crates/turnier-api/src/admin/matches.rs:28` | vorhanden | `require_admin`; Serien-Spiel sicherstellen. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/games/{game_number}/result` `submit_series_game_result` @ `backend/tournament/admin_routes.py:2318` | `submit_series_game_result` @ `rust/crates/turnier-api/src/admin/matches.rs:32` | vorhanden | `require_admin`; Serien-Spielergebnis werten. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/create-lobby` `create_match_lobby` @ `backend/tournament/admin_routes.py:2384` | `create_lobby` @ `rust/crates/turnier-api/src/admin/matches.rs:36` | vorhanden | `require_mod`; Bracket-Lobby erstellen. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/start` `start_match_via_steam` @ `backend/tournament/admin_routes.py:2444` | `start_match` @ `rust/crates/turnier-api/src/admin/matches.rs:37` | vorhanden | `require_mod`; Bracket-Match starten. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/fetch-result` `fetch_match_result_via_steam` @ `backend/tournament/admin_routes.py:2500` | `fetch_result` @ `rust/crates/turnier-api/src/admin/matches.rs:38` | vorhanden | `require_mod`; Bracket-Ergebnis aus Steam/Deadlock holen. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/leave-lobby` `leave_match_lobby` @ `backend/tournament/admin_routes.py:2554` | `leave_lobby` @ `rust/crates/turnier-api/src/admin/matches.rs:39` | vorhanden | `require_mod`; Lobby verlassen. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/reset` `reset_match` @ `backend/tournament/admin_routes.py:2601` | `reset_match` @ `rust/crates/turnier-api/src/admin/matches.rs:40` | vorhanden | `require_admin`; Bracket-Match resetten. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/manual-lobby` `set_manual_match_lobby` @ `backend/tournament/admin_routes.py:2631` | `manual_lobby` @ `rust/crates/turnier-api/src/admin/matches.rs:41` | vorhanden | `require_mod`; manuelle Lobby setzen. |
| `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/create-lobby` `create_group_match_lobby` @ `backend/tournament/admin_routes.py:2661` | `create_lobby` @ `rust/crates/turnier-api/src/admin/group_matches.rs:22` | vorhanden | `require_mod`; Gruppen-Lobby erstellen. |
| `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/start` `start_group_match_via_steam` @ `backend/tournament/admin_routes.py:2709` | `start_match` @ `rust/crates/turnier-api/src/admin/group_matches.rs:23` | vorhanden | `require_mod`; Gruppen-Match starten. |
| `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/fetch-result` `fetch_group_match_result_via_steam` @ `backend/tournament/admin_routes.py:2750` | `fetch_result` @ `rust/crates/turnier-api/src/admin/group_matches.rs:24` | vorhanden | `require_mod`; Gruppen-Ergebnis holen. |
| `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/leave-lobby` `leave_group_match_lobby` @ `backend/tournament/admin_routes.py:2792` | `leave_lobby` @ `rust/crates/turnier-api/src/admin/group_matches.rs:25` | vorhanden | `require_mod`; Gruppen-Lobby verlassen. |
| `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/reset` `reset_group_match` @ `backend/tournament/admin_routes.py:2827` | `reset_match` @ `rust/crates/turnier-api/src/admin/group_matches.rs:26` | vorhanden | `require_admin`; Gruppen-Match resetten. |
| `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/manual-lobby` `set_manual_group_match_lobby` @ `backend/tournament/admin_routes.py:2862` | `manual_lobby` @ `rust/crates/turnier-api/src/admin/group_matches.rs:27` | vorhanden | `require_mod`; manuelle Gruppen-Lobby setzen. |
| `GET /api/admin/casters` `list_available_casters` @ `backend/tournament/admin_routes.py:2893` | `list_available_casters` @ `rust/crates/turnier-api/src/admin/casters.rs:34` | vorhanden | `require_mod`; Caster-Rollenmitglieder listen. |
| `GET /api/admin/tournaments/{tournament_id}/casters` `list_tournament_casters` @ `backend/tournament/admin_routes.py:2905` | `list_tournament_casters` @ `rust/crates/turnier-api/src/admin/casters.rs:35` | vorhanden | `require_mod`; Turnier-Caster listen. |
| `POST /api/admin/tournaments/{tournament_id}/casters` `assign_tournament_caster` @ `backend/tournament/admin_routes.py:2916` | `assign_tournament_caster` @ `rust/crates/turnier-api/src/admin/casters.rs:35` | vorhanden | `require_mod`; Turnier-Caster zuweisen. |
| `DELETE /api/admin/tournaments/{tournament_id}/casters/{discord_id}` `remove_tournament_caster` @ `backend/tournament/admin_routes.py:2949` | `remove_tournament_caster` @ `rust/crates/turnier-api/src/admin/casters.rs:39` | vorhanden | `require_mod`; Turnier-Caster entfernen. |
| `GET /api/admin/tournaments/{tournament_id}/matches/{match_id}/casters` `list_match_casters` @ `backend/tournament/admin_routes.py:2971` | `list_match_casters` @ `rust/crates/turnier-api/src/admin/casters.rs:43` | vorhanden | `require_mod`; validiert Match und gibt Turnier-Caster zurueck. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/casters` `assign_match_caster` @ `backend/tournament/admin_routes.py:2983` | `assign_match_caster_gone` @ `rust/crates/turnier-api/src/admin/casters.rs:43` | vorhanden | `require_mod`; 410-Gone-Stub wie Python, dokumentiert in `rust/docs/known-issues.md:277`. |
| `DELETE /api/admin/tournaments/{tournament_id}/matches/{match_id}/casters/{discord_id}` `remove_match_caster` @ `backend/tournament/admin_routes.py:2998` | `remove_match_caster_gone` @ `rust/crates/turnier-api/src/admin/casters.rs:47` | vorhanden | `require_mod`; 410-Gone-Stub wie Python, dokumentiert in `rust/docs/known-issues.md:277`. |
| `GET /api/admin/tournaments/{tournament_id}/matches/{match_id}/event-presets` `get_match_event_presets` @ `backend/tournament/admin_routes.py:3015` | `event_presets` @ `rust/crates/turnier-api/src/admin/matches.rs:42` | vorhanden | `require_mod`; Event-Presets lesen. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/apply-convars` `apply_match_convars` @ `backend/tournament/admin_routes.py:3041` | `apply_convars` @ `rust/crates/turnier-api/src/admin/matches.rs:43` | vorhanden | `require_mod`; freie ConVars anwenden. |
| `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/apply-event-preset` `apply_match_event_preset` @ `backend/tournament/admin_routes.py:3098` | `apply_event_preset` @ `rust/crates/turnier-api/src/admin/matches.rs:44` | vorhanden | `require_mod`; Event-Preset anwenden. |
| `POST /api/admin/tournaments/{tournament_id}/groups/generate` `generate_groups_endpoint` @ `backend/tournament/admin_routes.py:3173` | `generate_groups_route` @ `rust/crates/turnier-api/src/admin/brackets.rs:21` | vorhanden | `require_mod`; Gruppen generieren. |
| `POST /api/admin/tournaments/{tournament_id}/bracket/generate` `generate_bracket_endpoint` @ `backend/tournament/admin_routes.py:3213` | `generate_bracket_route` @ `rust/crates/turnier-api/src/admin/brackets.rs:22` | vorhanden | `require_mod`; Bracket generieren. |
| `POST /api/admin/tournaments/{tournament_id}/voice/move-teams` `voice_move_teams` @ `backend/tournament/admin_routes.py:3239` | `move_teams` @ `rust/crates/turnier-api/src/admin/voice.rs:19` | vorhanden | `require_admin`; Match-Teams in VC1/VC2 verschieben. |
| `POST /api/admin/tournaments/{tournament_id}/voice/move-sammelpunkt` `voice_move_sammelpunkt` @ `backend/tournament/admin_routes.py:3283` | `move_sammelpunkt` @ `rust/crates/turnier-api/src/admin/voice.rs:20` | vorhanden | `require_admin`; Teilnehmer in Sammelpunkt verschieben. |
| `POST /api/admin/voice/move-user` `voice_move_user` @ `backend/tournament/admin_routes.py:3311` | `move_user` @ `rust/crates/turnier-api/src/admin/voice.rs:21` | vorhanden | `require_admin`; einzelnen User verschieben. |
| `GET /api/admin/voice/channel-members/{channel_id}` `voice_get_channel_members` @ `backend/tournament/admin_routes.py:3323` | `channel_members` @ `rust/crates/turnier-api/src/admin/voice.rs:22` | vorhanden | `require_admin`; Voice-Channel-Mitglieder lesen. |

## HTTP-Routen: Operations

| Python | Rust | Status | Auth/Zweck/Notiz |
|---|---|---|---|
| `POST /api/tournaments/{tournament_id}/matches/{match_id}/report-result` `report_match_result` @ `backend/tournament/operations_routes.py:95` | `report_match_result` @ `rust/crates/turnier-api/src/operations.rs:25` | vorhanden | `require_auth`; Captain meldet Off-Stream-Ergebnis/No-Show. |
| `GET /api/admin/tournaments/{tournament_id}/action-items` `get_action_items` @ `backend/tournament/operations_routes.py:209` | `get_action_items` @ `rust/crates/turnier-api/src/operations.rs:29` | vorhanden | `require_mod`; Leitstand/Aktionsbedarf. |
| `POST /api/admin/result-reports/{report_id}/confirm` `confirm_result_report` @ `backend/tournament/operations_routes.py:295` | `confirm_result_report` @ `rust/crates/turnier-api/src/operations.rs:33` | vorhanden | `require_mod`; Ergebnisbericht bestaetigen. |
| `POST /api/admin/result-reports/{report_id}/reject` `reject_result_report` @ `backend/tournament/operations_routes.py:384` | `reject_result_report` @ `rust/crates/turnier-api/src/operations.rs:34` | vorhanden | `require_mod`; Ergebnisbericht verwerfen. |
| `PATCH /api/admin/tournaments/{tournament_id}/matches/{match_id}/stream` `set_match_stream_flag` @ `backend/tournament/operations_routes.py:418` | `set_match_stream_flag` @ `rust/crates/turnier-api/src/operations.rs:35` | vorhanden | `require_mod`; Stream-Marker setzen. |

## HTTP-Routen: Consent/Profile/Avatar

| Python | Rust | Status | Auth/Zweck/Notiz |
|---|---|---|---|
| `GET /api/consent` `get_consent` @ `backend/tournament/consent_routes.py:96` | `get_consent` @ `rust/crates/turnier-api/src/consent.rs:39` | vorhanden | `require_auth`; Einwilligungsstatus lesen. |
| `POST /api/consent` `set_consent` @ `backend/tournament/consent_routes.py:114` | `set_consent` @ `rust/crates/turnier-api/src/consent.rs:39` | vorhanden | `require_auth`; Einwilligung setzen, 201. |
| `DELETE /api/consent` `revoke_consent` @ `backend/tournament/consent_routes.py:133` | `revoke_consent` @ `rust/crates/turnier-api/src/consent.rs:39` | vorhanden | `require_auth`; Einwilligung widerrufen, 204. |
| `GET /api/profile` `get_my_profile` @ `backend/tournament/consent_routes.py:145` | `get_my_profile` @ `rust/crates/turnier-api/src/consent.rs:43` | vorhanden | `require_auth`; eigenes Profil. |
| `PUT /api/profile` `update_my_profile` @ `backend/tournament/consent_routes.py:158` | `update_my_profile` @ `rust/crates/turnier-api/src/consent.rs:43` | vorhanden | `require_auth`; Profil aktualisieren. |
| `POST /api/profile/avatar` `upload_profile_avatar` @ `backend/tournament/consent_routes.py:244` | `upload_profile_avatar` @ `rust/crates/turnier-api/src/consent.rs:44` | vorhanden | `require_auth`; Avatar hochladen. |
| `GET /api/avatars/{discord_id}` `get_avatar` @ `backend/tournament/consent_routes.py:294` | `get_avatar` @ `rust/crates/turnier-api/src/consent.rs:561` | abweichend | Python nutzt `FileResponse` fuer lokale Dateien (`backend/tournament/consent_routes.py:298`); Rust liefert Datei/Redirect funktional, aber ohne `ETag`/`Last-Modified`/`Accept-Ranges` wie dokumentiert in `rust/docs/known-issues.md:336`. |
| `GET /api/avatars/by-name/{discord_name}` `get_avatar_by_name` @ `backend/tournament/consent_routes.py:316` | `get_avatar_by_name` @ `rust/crates/turnier-api/src/consent.rs:575` | abweichend | Gleiche Datei-Header-Abweichung wie bei `/avatars/{discord_id}`; Python `FileResponse` bei `backend/tournament/consent_routes.py:328`, Rust `serve_avatar_file`-Pfad bei `rust/crates/turnier-api/src/consent.rs:584`, dokumentiert in `rust/docs/known-issues.md:336`. |

## HTTP-Routen: Leaderboard

| Python | Rust | Status | Auth/Zweck/Notiz |
|---|---|---|---|
| `GET /api/leaderboard` `get_leaderboard` @ `backend/tournament/leaderboard_routes.py:13` | `get_leaderboard` @ `rust/crates/turnier-api/src/leaderboard.rs:16` | vorhanden | public; globale Rangliste. |
| `GET /api/players/{discord_name}` `get_player_profile` @ `backend/tournament/leaderboard_routes.py:45` | `get_player_profile` @ `rust/crates/turnier-api/src/leaderboard.rs:17` | vorhanden | public; Spielerprofil. |

## HTTP-Routen: Draft

| Python | Rust | Status | Auth/Zweck/Notiz |
|---|---|---|---|
| `GET /api/draft/heroes` `list_heroes` @ `backend/draft/routes.py:17` | `list_heroes` @ `rust/crates/turnier-api/src/draft.rs:24` | vorhanden | public; Heldenliste. |
| `POST /api/draft/matches/{match_id}/start` `start_match_draft` @ `backend/draft/routes.py:22` | `start_match_draft` @ `rust/crates/turnier-api/src/draft.rs:25` | vorhanden | `require_admin`; Draft fuer Match starten. |
| `GET /api/draft/sessions/{session_id}` `get_session` @ `backend/draft/routes.py:38` | `get_session` @ `rust/crates/turnier-api/src/draft.rs:26` | vorhanden | `require_admin`; Draft-State lesen. |
| `POST /api/draft/sessions/{session_id}/action` `submit_action` @ `backend/draft/routes.py:56` | `submit_action` @ `rust/crates/turnier-api/src/draft.rs:27` | vorhanden | `require_admin`; Ban/Pick-Aktion ausfuehren. |

## HTTP-Routen: Test Mode

| Python | Rust | Status | Auth/Zweck/Notiz |
|---|---|---|---|
| `POST /api/admin/test/users` `create_test_users` @ `backend/admin/test_mode.py:232` | `create_test_users` @ `rust/crates/turnier-api/src/test_mode.rs:59` | abweichend | Python-Router ist in `backend/main.py:66` immer gemountet und nutzt `require_mod`; Rust hat `ModUser`, aber der Router kann ueber `TURNIER_ENABLE_TEST_MODE=0` leer sein (`rust/crates/turnier-api/src/test_mode.rs:13`, `:52`, `:76`). Default ist aktiv wie Python. |
| `GET /api/admin/test/users` `list_test_users` @ `backend/admin/test_mode.py:249` | `list_test_users` @ `rust/crates/turnier-api/src/test_mode.rs:59` | abweichend | Gleicher optionaler Rust-Kill-Switch; Methode/Pfad/Auth/Response bei aktivem Flag vorhanden. |
| `DELETE /api/admin/test/users` `delete_test_users` @ `backend/admin/test_mode.py:259` | `delete_test_users` @ `rust/crates/turnier-api/src/test_mode.rs:59` | abweichend | Gleicher optionaler Rust-Kill-Switch; Methode/Pfad/Auth/Response bei aktivem Flag vorhanden. |
| `POST /api/admin/test/tournaments` `create_test_tournament` @ `backend/admin/test_mode.py:276` | `create_test_tournament` @ `rust/crates/turnier-api/src/test_mode.rs:65` | abweichend | Gleicher optionaler Rust-Kill-Switch; Methode/Pfad/Auth/Response bei aktivem Flag vorhanden. |
| `POST /api/admin/test/tournaments/{tournament_id}/simulate-round` `simulate_test_tournament_round` @ `backend/admin/test_mode.py:442` | `simulate_test_tournament_round` @ `rust/crates/turnier-api/src/test_mode.rs:66` | abweichend | Gleicher optionaler Rust-Kill-Switch; Methode/Pfad/Auth/Response bei aktivem Flag vorhanden. |
| `DELETE /api/admin/test/wipe` `wipe_test_data` @ `backend/admin/test_mode.py:521` | `wipe_test_data` @ `rust/crates/turnier-api/src/test_mode.rs:70` | abweichend | Gleicher optionaler Rust-Kill-Switch; Methode/Pfad/Auth/Response bei aktivem Flag vorhanden. |

## Nicht-Routen-Features

| Python-Einheit | Rust-Pendant | Status | Notiz |
|---|---|---|---|
| DB-Schema/Init `_SCHEMA`, `_ensure_schema_upgrades`, `init_db` @ `backend/db.py:14`, `backend/db.py:420` | konsolidierte Migration + Pool @ `rust/crates/turnier-db/migrations/0001_initial.sql:1`, `rust/crates/turnier-db/src/pool.rs:21` | vorhanden | DB-Vertrag dokumentiert Live-Schema und idempotente Migration in `rust/docs/db-contract.md:1`. |
| Config/Secrets `settings` @ `backend/config.py` | `turnier-config` @ `rust/crates/turnier-config/src/lib.rs`, `rust/crates/turnier-config/src/secrets.rs` | vorhanden | Environment/Secrets in eigenes Crate ausgelagert. |
| OAuth Broker + Session-Erzeugung @ `backend/auth/discord_oauth.py:23`, `:43`, `:84` | `OAuthClient`, `create_session`, API-Router @ `rust/crates/turnier-auth/src/oauth.rs:35`, `rust/crates/turnier-auth/src/session.rs:64`, `rust/crates/turnier-api/src/auth.rs:31` | vorhanden | Rollen-Staleness/CSV-Verhalten dokumentiert und erhalten in `rust/docs/known-issues.md:88`. |
| Auth-Middleware/Permissions @ `backend/auth/middleware.py:13`, `backend/auth/permissions.py:10` | `AuthUser`/`ModUser`/`AdminUser` @ `rust/crates/turnier-api/src/extract.rs:34`, `:49`, `:66` | vorhanden | 401/403-Gates entsprechen `require_auth`/`require_mod`/`require_admin`. |
| Steam-Link-Reader @ `backend/steam/reader.py:23` | `BridgeReader` @ `rust/crates/turnier-steam/src/bridge.rs:57` | vorhanden | Steam-Bridge-DB read-only, Fallback-Verhalten dokumentiert in `rust/docs/db-contract.md:45`. |
| Rank-Reader Cache/Rollen-Fallback @ `backend/rank_reader.py:83`, `:196`, `:252` | `SteamRankResolver`, `RankCache`, `role_rank` @ `rust/crates/turnier-steam/src/resolver.rs:46`, `rust/crates/turnier-steam/src/cache.rs:64`, `rust/crates/turnier-steam/src/role_rank.rs:48` | vorhanden | bekannte Rang-Nuancen KI-S01..S03 erhalten/gefixt dokumentiert in `rust/docs/known-issues.md:13`. |
| Discord-Broker/Task-Log @ `backend/notifications/discord_notifier.py:50`, `:101`, `:115` | `BrokerClient`, `tasks` @ `rust/crates/turnier-discord/src/broker.rs:16`, `rust/crates/turnier-discord/src/tasks.rs:47` | vorhanden | Task-Protokollierung bewusst 1:1 lueckenhaft erhalten, `rust/docs/known-issues.md:63`. |
| Discord Match-Channel Name/Lifecycle @ `backend/notifications/discord_notifier.py:37`, `:156`, `:222`, `:308` | `channel_name`, `DiscordNotifier` @ `rust/crates/turnier-discord/src/channel_name.rs:14`, `rust/crates/turnier-discord/src/notifier.rs:70` | vorhanden | Channel-Slug/Delay-Delete dokumentiert in `rust/docs/known-issues.md:56`, `:70`. |
| `notify_users` Consent/Event-Prefs @ `backend/notifications/discord_notifier.py:235` | `DiscordNotifier::notify_users` @ `rust/crates/turnier-discord/src/notifier.rs:82` | vorhanden | profil-lose User Default-Verhalten erhalten, `rust/docs/known-issues.md:43`. |
| Voice/Role Helpers @ `backend/notifications/discord_notifier.py:316`, `:343`, `:352` | `move_users_to_voice_channel`, `get_voice_channel_members`, `get_role_members` @ `rust/crates/turnier-discord/src/notifier.rs:82` | vorhanden | Broker-Aufrufe vorhanden; Task-Log-Luecke bewusst erhalten. |
| Lobby Announcement, Match-Stats, Caster-Notify @ `backend/notifications/discord_notifier.py:362`, `:413`, `:465` | `DiscordNotifier` Methoden @ `rust/crates/turnier-discord/src/notifier.rs:70` | vorhanden | Embeds/Benachrichtigungen in `turnier-discord`. |
| Scheduler Zeitparser/Faelligkeiten @ `backend/tournament/scheduler.py:29`, `:45`, `:50`, `:68` | `time`, `transition` @ `rust/crates/turnier-scheduler/src/time.rs:40`, `:73`, `:85`, `rust/crates/turnier-scheduler/src/transition.rs:46` | vorhanden | TZ-Fragilitaet bewusst erhalten, `rust/docs/known-issues.md:190`. |
| Scheduler `advance_tournament_status` @ `backend/tournament/scheduler.py:130` | `advance_tournament_status` @ `rust/crates/turnier-scheduler/src/transition.rs:117` | abweichend | Python ruft Punkte-Recompute innerhalb derselben DB-Transaktion auf (`backend/tournament/scheduler.py:181`, `backend/tournament/points.py:13`); Rust committet Status und ruft `recalculate_player_points(pool, ...)` separat auf (`rust/crates/turnier-scheduler/src/transition.rs:201`, `:205`). Dokumentiert in `rust/docs/known-issues.md:208`. |
| Scheduler Reminder Registrierung/Start/Match @ `backend/tournament/scheduler.py:294`, `:387`, `:447`, `:503` | `reminders`, `loop_runner` @ `rust/crates/turnier-scheduler/src/reminders.rs:100`, `:165`, `:229`, `rust/crates/turnier-scheduler/src/loop_runner.rs:262` | vorhanden | Dedupe/Catch-up-Verhalten erhalten, `rust/docs/known-issues.md:217`, `:228`. |
| Turnierstatus/Modus-Helfer @ `backend/tournament/engine.py:43`, `:64` | `status` @ `rust/crates/turnier-engine/src/status.rs:12`, `:31`, `:45` | vorhanden | Statusuebergaenge zentralisiert. |
| Random Teams/Captain-Namen @ `backend/tournament/engine.py:73`, `:115`, `:152` | `persist/checkin`, `engine/naming` @ `rust/crates/turnier-engine/src/persist/checkin.rs:208`, `:319`, `rust/crates/turnier-engine/src/engine/naming.rs:28` | vorhanden | Captain-Teamnamen und `casefold`-Name-Key portiert. |
| Finalize Check-in/Snapshot @ `backend/tournament/engine.py:276`, `:316`; `backend/tournament/checkin.py` | `FinalizeCheckinParams`, `finalize_checkin` @ `rust/crates/turnier-engine/src/persist/checkin.rs:559`, `:570` | vorhanden | Snapshot-/Cleanup-Logik vorhanden. |
| Gruppen generieren + Round-Robin @ `backend/tournament/engine.py:729`, `:743`, `:812`, `:831` | `groups` @ `rust/crates/turnier-engine/src/engine/groups.rs:15`, `:46`, `rust/crates/turnier-engine/src/persist/groups.rs:13`, `:122` | vorhanden | Auto-Gruppen/Snake-Seeding vorhanden; bekannte Altbugs erhalten, `rust/docs/known-issues.md:120`. |
| Bracket generieren Single/Double/Cross/Mini-Groups @ `backend/tournament/engine.py:876`, `:998`, `:1011`, `:1096`, `:1413` | `persist/bracket`, `persist/double_elim`, `mini_groups` @ `rust/crates/turnier-engine/src/persist/bracket.rs:36`, `:154`, `rust/crates/turnier-engine/src/persist/double_elim.rs`, `rust/crates/turnier-engine/src/mini_groups.rs` | vorhanden | Single/Double-Elim, Cross-Seeding und Mini-RR vorhanden. |
| Bracket-Winner weiterreichen @ `backend/tournament/engine.py:1581`, `:1593` | `advance_bracket_winner` @ `rust/crates/turnier-engine/src/persist/advance.rs:24` | vorhanden | Downstream-Propagation vorhanden. |
| Mini-Group Winner/Tiebreak @ `backend/tournament/mini_groups.py:9`, `:64`, `:96`, `:148` | `mini_groups`, `mini_group_complete` @ `rust/crates/turnier-engine/src/mini_groups.rs:19`, `:30`, `:54`, `rust/crates/turnier-engine/src/persist/mini_group_complete.rs:23` | vorhanden | Head-to-head/point-diff-Tiebreaks vorhanden. |
| Punkte-Recompute @ `backend/tournament/points.py:13` | `recalculate_player_points` @ `rust/crates/turnier-engine/src/persist/points.rs:61` | vorhanden | Fachlogik vorhanden; idempotenter Voll-Recompute als safe cleanup dokumentiert `rust/docs/known-issues.md:129`. |
| Seeding `rank_score` @ `backend/tournament/seeding.py:20` | Rank helpers @ `rust/crates/turnier-steam/src/rank.rs:68`, `:81` | vorhanden | Rangscore-Formel portiert. |
| Seeding `team_avg_score` @ `backend/tournament/seeding.py:29` | kein Laufzeit-Pendant; bewusst nicht portiert | bewusst_ausgelassen | Toter Code laut Port-Doku: `rust/docs/known-issues.md:129` nennt `team_avg_score` als nicht mitportiert, ohne beobachtbaren Verhaltensunterschied. |
| Game-Modes/Hero-Assignments @ `backend/match/game_modes.py:18`, `:131`; `backend/match/heroes.py` | `modes`, `heroes` @ `rust/crates/turnier-match/src/modes.rs:68`, `:250`, `rust/crates/turnier-match/src/modes.rs:25` | vorhanden | Objectives, zufaellige Helden, Hero-ConVars vorhanden. |
| Steam Task Queue/Poll/Invite @ `backend/match/steam_bridge.py:21`, `:37`, `:55`, `:95`, `:137` | `SteamBridge` @ `rust/crates/turnier-match/src/steam_bridge.rs:77` | vorhanden | Stale-Task-Reaper inline erhalten, `rust/docs/known-issues.md:165`. |
| Match Lobby Lifecycle bracket/group @ `backend/match/manager.py:126`, `:143`, `:340`, `:479`, `:538` | `MatchManager` lobby APIs @ `rust/crates/turnier-match/src/lobby.rs:28` | vorhanden | Create/start/fetch/leave/manual/reset fuer beide Match-Arten vorhanden. |
| Event-Presets/ConVars @ `backend/match/manager.py:566`, `:583`, `:615` | `presets` @ `rust/crates/turnier-match/src/presets.rs:52`, `:59`, `:68` | vorhanden | Preset-Liste, freie ConVar-Normalisierung und Apply-Pfade vorhanden. |
| Bracket Result Processor @ `backend/match/result_processor.py:41`, `:252`, `:286` | `MatchManager::apply_bracket_match_result`, helpers @ `rust/crates/turnier-match/src/result.rs:134`, `:533`, `:568` | vorhanden | Winner-Propagation, Downstream-Reset, Stats-Persistenz vorhanden; Alt-Nuancen dokumentiert `rust/docs/known-issues.md:145`. |
| Group Match Result Processing @ `backend/match/manager.py:880` | `MatchManager::apply_group_match_result` @ `rust/crates/turnier-match/src/result.rs:134` | vorhanden | Group-Wins/Losses/Points-Pfad vorhanden; Nicht-Idempotenz erhalten, `rust/docs/known-issues.md:270`. |
| Auto-Lobby Scheduling @ `backend/match/auto_lobby.py:10`, `:76` | `auto_lobby` @ `rust/crates/turnier-match/src/auto_lobby.rs:16` | vorhanden | Turnier- und Next-Round-Planung vorhanden. |
| Series Manager @ `backend/match/series_manager.py:11`, `:34`, `:151` | `series` @ `rust/crates/turnier-match/src/series.rs:26`, `:94`, `:205` | vorhanden | BoN-Spielanlage, Ergebniswertung, Serienstand vorhanden. |
| Draft Heroes/Sequence @ `backend/draft/heroes.py:36`, `backend/draft/engine.py:33` | `turnier-draft` heroes/sequence @ `rust/crates/turnier-draft/src/heroes.rs:48`, `rust/crates/turnier-draft/src/sequence.rs:97` | vorhanden | Heldenvalidierung und Pick/Ban-Sequenz vorhanden. |
| Draft State/Actions @ `backend/draft/engine.py:33`, `:72`, `:139` | `repo`/`state` @ `rust/crates/turnier-draft/src/repo.rs:32`, `:101`, `:204`, `rust/crates/turnier-draft/src/state.rs:48` | vorhanden | Start, Take-Action, State-Load vorhanden; bekannte `taken_by`-Semantik erhalten `rust/docs/known-issues.md:134`. |
| Test-Mode Generator/Simulation/Wipe @ `backend/admin/test_mode.py:74`, `:276`, `:442`, `:521` | `test_mode` @ `rust/crates/turnier-api/src/test_mode.rs:1`, `:59`, `:65`, `:66`, `:70` | abweichend | Fachfunktionen vorhanden, aber Rust kann gesamten Test-Router per `TURNIER_ENABLE_TEST_MODE=0` deaktivieren; Python mountet immer in `backend/main.py:66`. Dokumentiert in `rust/docs/known-issues.md:282`. |

## LUECKEN-KANDIDATEN

Nur `fehlt`, `abweichend` und `unsicher`; `bewusst_ausgelassen` ist separat gezaehlt.

| Kategorie | Python-Beleg | Rust-Beleg | Status | Risiko/Notiz |
|---|---|---|---|---|
| Route | `PUT /api/admin/tournaments/{tournament_id}` @ `backend/tournament/admin_routes.py:914`; `model_dump(exclude_unset=True)` @ `backend/tournament/admin_routes.py:967` | `update_tournament` @ `rust/crates/turnier-api/src/admin/tournaments.rs:252`; DTO `Option<T>` @ `rust/crates/turnier-core/src/tournament.rs:107`; Doku @ `rust/docs/known-issues.md:316` | abweichend | Explizites `null` leert in Python nullbare Felder, in Rust nicht. |
| Route | `GET /api/avatars/{discord_id}` @ `backend/tournament/consent_routes.py:294`; `FileResponse` @ `backend/tournament/consent_routes.py:298` | `get_avatar` @ `rust/crates/turnier-api/src/consent.rs:561`; Doku @ `rust/docs/known-issues.md:336` | abweichend | Bildinhalt/Redirect vorhanden, aber Datei-Header-Paritaet fehlt. |
| Route | `GET /api/avatars/by-name/{discord_name}` @ `backend/tournament/consent_routes.py:316`; `FileResponse` @ `backend/tournament/consent_routes.py:328` | `get_avatar_by_name` @ `rust/crates/turnier-api/src/consent.rs:575`; Doku @ `rust/docs/known-issues.md:336` | abweichend | Gleiche Datei-Header-Abweichung. |
| Route | `POST /api/admin/test/users` @ `backend/admin/test_mode.py:232`; Python-Mount @ `backend/main.py:66` | Router-Gate @ `rust/crates/turnier-api/src/test_mode.rs:13`, `:52`, `:76` | abweichend | Bei `TURNIER_ENABLE_TEST_MODE=0` fehlt Route in Rust; Default aktiv wie Python. |
| Route | `GET /api/admin/test/users` @ `backend/admin/test_mode.py:249`; Python-Mount @ `backend/main.py:66` | Router-Gate @ `rust/crates/turnier-api/src/test_mode.rs:13`, `:52`, `:76` | abweichend | Wie oben. |
| Route | `DELETE /api/admin/test/users` @ `backend/admin/test_mode.py:259`; Python-Mount @ `backend/main.py:66` | Router-Gate @ `rust/crates/turnier-api/src/test_mode.rs:13`, `:52`, `:76` | abweichend | Wie oben. |
| Route | `POST /api/admin/test/tournaments` @ `backend/admin/test_mode.py:276`; Python-Mount @ `backend/main.py:66` | Router-Gate @ `rust/crates/turnier-api/src/test_mode.rs:13`, `:52`, `:76` | abweichend | Wie oben. |
| Route | `POST /api/admin/test/tournaments/{tournament_id}/simulate-round` @ `backend/admin/test_mode.py:442`; Python-Mount @ `backend/main.py:66` | Router-Gate @ `rust/crates/turnier-api/src/test_mode.rs:13`, `:52`, `:76` | abweichend | Wie oben. |
| Route | `DELETE /api/admin/test/wipe` @ `backend/admin/test_mode.py:521`; Python-Mount @ `backend/main.py:66` | Router-Gate @ `rust/crates/turnier-api/src/test_mode.rs:13`, `:52`, `:76` | abweichend | Wie oben. |
| Nicht-Route | Scheduler `advance_tournament_status` @ `backend/tournament/scheduler.py:130`; points call in Tx @ `backend/tournament/scheduler.py:181` | Rust call nach Status-Commit @ `rust/crates/turnier-scheduler/src/transition.rs:201`, `:205`; Doku @ `rust/docs/known-issues.md:208` | abweichend | Status und Punkte nicht mehr atomar; Recompute ist idempotent. |
| Nicht-Route | Test-Mode Feature @ `backend/admin/test_mode.py:22`, `:276`, `:442`, `:521`; Python-Mount @ `backend/main.py:66` | `test_mode_enabled`/leerer Router @ `rust/crates/turnier-api/src/test_mode.rs:52`, `:75` | abweichend | Fachlich vorhanden, aber Feature-Verfuegbarkeit ist konfigurierbar. |

### Fehlend

Keine `fehlt`-Befunde.

### Unsicher

Keine `unsicher`-Befunde.
