# Endpoint-Paritaets-Audit: `routes.py` + `admin_routes.py` gegen Rust

Datum: 2026-07-02  
Branch/HEAD: `central-postgres-sp4` / `b1d44a7`  
Scope: `backend/tournament/routes.py` und `backend/tournament/admin_routes.py` gegen `rust/crates/turnier-api/src/public/*.rs` und `rust/crates/turnier-api/src/admin/*.rs`.

## Kurzfazit

- Gepruefte Python-Endpunkte: 79 total, davon 23 public und 56 admin.
- Pfad/Methode: 79/79 Rust-Pendants vorhanden.
- Erfolgscodes/Auth-Gates: keine bestaetigte Abweichung bei Normalpfaden.
- Bestaetigte Abweichungen: kritisch 0, hoch 1, mittel 3, niedrig 0.
- Rust hat zusaetzliche Admin-Automatik-Endpunkte (`/api/admin/presets`, `/api/admin/proposals`); diese sind nicht Teil dieses Python-Router-Audits.

## Methodik

- Python-Routen per AST aus `@router.get/post/put/patch/delete` extrahiert, danach Handler-Bloecke mit `nl -ba` gelesen.
- Rust-Routen ueber die `Router::new().route(...)`-Registrierungen und die jeweiligen Handler-Signaturen/Handler-Bloecke gelesen.
- Verglichen wurden effektiver Pfad inklusive Prefix (`/api` bzw. `/api/admin`), HTTP-Methode, Success-Status, wichtige Fehlerstatus, Response-Feldform und Auth-Extractor (`require_auth`/`require_mod`/`require_admin` vs. `AuthUser`/`ModUser`/`AdminUser`).
- Keine Live-HTTP-Tests ausgefuehrt; dies ist ein statischer Code-Audit.

## Findings

### F-001 - hoch - FastAPI/Pydantic-422 ist nicht durchgaengig nachgebildet

**Python:** `backend/tournament/admin_routes.py:821`, `backend/tournament/models.py:146`, `backend/tournament/models.py:156`, `backend/tournament/admin_routes.py:2318`, `backend/tournament/admin_routes.py:79`  
**Rust:** `rust/crates/turnier-api/src/admin/tournaments.rs:177`, `rust/crates/turnier-api/src/admin/tournaments.rs:179`, `rust/crates/turnier-api/src/admin/matches.rs:279`, `rust/crates/turnier-api/src/admin/matches.rs:293`, `rust/crates/turnier-api/src/admin/matches.rs:295`, `rust/crates/turnier-api/src/error.rs:64`

FastAPI liefert fuer Pydantic-/Request-Validation standardmaessig `422` mit `{"detail": [...]}`. Rust mappt mehrere fachlich gleiche Validierungen manuell auf `WebError`, also `{"detail": "<string>"}`, und in `create_tournament`/`update_tournament` sogar auf `400`.

Konkrete Beispiele:

- `POST /api/admin/tournaments` mit `series_format=2`: Python laeuft ueber den Pydantic-Validator (`models.py:146-153`) und antwortet 422; Rust ruft `body.validated().map_err(WebError::bad_request)` (`admin/tournaments.rs:177-179`) und antwortet 400.
- `POST /api/admin/tournaments/{id}/matches/{match_id}/games/{game_number}/result` mit `winner_team=3`: Python `GameResultRequest` (`admin_routes.py:79-81`) antwortet 422 mit Detail-Liste; Rust prueft im Handler (`admin/matches.rs:293-299`) und antwortet 422 mit String-Detail.
- Axum-`Json`/`Query`-Rejections laufen vor dem Handler und werden nicht in FastAPI-`{"detail": [...]}` uebersetzt. Das betrifft u. a. fehlenden/falschen JSON-Content-Type, syntaktisch defektes JSON und fehlerhafte Query-Typen (`force`, `confirm`, `match_id`).

Warum echtes Problem: Clients, Tests oder Frontend-Fehlerbehandlung, die FastAPI-422 samt Listenstruktur erwarten, sehen im Rust-Port je nach Fall 400/415 oder eine andere Body-Form. Das ist vor dem Cutover eine Contract-Abweichung, auch wenn die Erfolgsantworten passen.

Betroffene Audit-Endpunkte mit Body-/Query-Validation sind mindestens:

`POST /api/tournaments/{id}/teams`, `PATCH /api/tournaments/{id}/teams/{team_id}/recruiting`, `POST /api/admin/tournaments`, `PUT /api/admin/tournaments/{id}`, `POST /api/admin/tournaments/{id}/finalize-checkin?confirm=...`, `POST /api/admin/tournaments/{id}/teams`, `PUT /api/admin/tournaments/{id}/teams/{team_id}`, `PATCH /api/admin/tournaments/{id}/teams/{team_id}/recruiting`, `PUT /api/admin/tournaments/{id}/teams/{team_id}/captain`, `POST /api/admin/tournaments/{id}/teams/{team_id}/members/move`, `POST /api/admin/tournaments/{id}/teams/{team_id}/signups/assign`, `POST /api/admin/tournaments/{id}/teams/{team_id}/add-member`, `POST /api/admin/tournaments/{id}/matches/{match_id}/result?force=...`, `POST /api/admin/tournaments/{id}/matches/{match_id}/games/{game_number}/result`, `POST /api/admin/tournaments/{id}/matches/{match_id}/manual-lobby`, `POST /api/admin/tournaments/{id}/group-matches/{match_id}/manual-lobby`, `POST /api/admin/tournaments/{id}/casters`, `POST /api/admin/tournaments/{id}/matches/{match_id}/casters`, `POST /api/admin/tournaments/{id}/matches/{match_id}/apply-convars`, `POST /api/admin/tournaments/{id}/matches/{match_id}/apply-event-preset`, `POST /api/admin/tournaments/{id}/groups/generate`, `POST /api/admin/tournaments/{id}/voice/move-teams?match_id=...`, `POST /api/admin/voice/move-user`.

### F-002 - mittel - Admin-Discord-IDs werden Rust-seitig strikter geparst

**Python:** `backend/tournament/admin_routes.py:1733`, `backend/tournament/admin_routes.py:1741`, `backend/tournament/admin_routes.py:1752`, `backend/tournament/admin_routes.py:1757`, `backend/tournament/admin_routes.py:1992`, `backend/tournament/admin_routes.py:2000`, `backend/tournament/admin_routes.py:2002`, `backend/tournament/admin_routes.py:2948`, `backend/tournament/admin_routes.py:2956`  
**Rust:** `rust/crates/turnier-api/src/admin/teams.rs:548`, `rust/crates/turnier-api/src/admin/teams.rs:554`, `rust/crates/turnier-api/src/admin/teams.rs:558`, `rust/crates/turnier-api/src/admin/teams.rs:869`, `rust/crates/turnier-api/src/admin/teams.rs:876`, `rust/crates/turnier-api/src/admin/casters.rs:235`, `rust/crates/turnier-api/src/admin/casters.rs:245`, `rust/crates/turnier-api/src/db.rs:6`

Mehrere Python-Admin-Endpunkte behandeln `discord_id` nur als String und pruefen teilweise nur "nicht leer"; der DB-Lookup entscheidet dann ueber 404 oder die Operation laeuft. Rust parst dieselben Werte vor dem Lookup zu `i64` und liefert bei nicht numerischen oder zu grossen Werten sofort 400 `{"detail":"Ungueltige Discord-ID"}`.

Konkrete Beispiele:

- `PUT /api/admin/tournaments/{id}/teams/{team_id}/captain`: Python prueft nur `not discord_id` und liefert fuer nicht existierende/nicht numerische IDs 404 "Mitglied nicht im Team gefunden" (`admin_routes.py:1741-1759`). Rust parst vor dem Lookup (`admin/teams.rs:554-558`) und liefert 400.
- `POST /api/admin/tournaments/{id}/teams/{team_id}/add-member`: Python verlangt nur `discord_id` und `discord_name` (`admin_routes.py:2000-2006`) und koennte eine beliebige nicht leere ID annehmen; Rust lehnt nicht numerische IDs vor der Operation ab (`admin/teams.rs:869-877`).
- `DELETE /api/admin/tournaments/{id}/casters/{discord_id}`: Python fuehrt ein DELETE aus und gibt die aktualisierte Liste zurueck, auch wenn die ID nicht numerisch ist (`admin_routes.py:2948-2967`). Rust parst die Pfad-ID und antwortet bei ungueltiger ID 400 (`admin/casters.rs:235-247`).

Warum echtes Problem: Der Cutover aendert Request-Akzeptanz und Statuscodes fuer dieselben Pfade. Auch wenn echte Discord-Snowflakes numerisch sind, brechen vorhandene Tests/Tools mit Dummy-IDs oder Frontend-Fehlerbehandlung fuer 404/200-no-op.

Betroffene Endpunkte: `PUT .../captain`, `DELETE .../members/{discord_id}`, `POST .../members/move`, `POST .../add-member`, `DELETE .../casters/{discord_id}`. Public `kick`/`invite` sind davon nicht betroffen, dort prueft Python bereits streng per Regex.

### F-003 - mittel - `groups/generate` akzeptiert `num_groups` anders

**Python:** `backend/tournament/admin_routes.py:3172`, `backend/tournament/admin_routes.py:3175`, `backend/tournament/admin_routes.py:3180`  
**Rust:** `rust/crates/turnier-api/src/admin/brackets.rs:31`, `rust/crates/turnier-api/src/admin/brackets.rs:33`, `rust/crates/turnier-api/src/admin/brackets.rs:47`

Python nimmt fuer `POST /api/admin/tournaments/{id}/groups/generate` ein generisches `dict | None` und macht `int(body["num_groups"])`, danach Clamp auf 2..8. Dadurch werden JSON-Strings wie `"3"` akzeptiert; nicht numerische Strings koennen unhandled `ValueError` und damit 500 erzeugen. Rust typisiert `num_groups` als `Option<i64>`; JSON-Strings werden durch den `Json`-Extractor vor dem Handler abgelehnt (422/Extractor-Body), waehrend numerische JSON-Werte funktionieren.

Warum echtes Problem: Das ist eine Request-Form- und Fehlerstatus-Aenderung an einem Admin-Endpunkt. Admin-UIs/Form-Helfer senden numerische Werte haeufig als String.

### F-004 - mittel - `apply-event-preset.enabled` hat andere Coercion

**Python:** `backend/tournament/admin_routes.py:3097`, `backend/tournament/admin_routes.py:3105`, `backend/tournament/admin_routes.py:3106`  
**Rust:** `rust/crates/turnier-api/src/admin/matches.rs:552`, `rust/crates/turnier-api/src/admin/matches.rs:554`, `rust/crates/turnier-api/src/admin/matches.rs:557`, `rust/crates/turnier-api/src/admin/matches.rs:579`

Python liest `enabled = bool(body.get("enabled", True))`. Damit werden beliebige JSON-Werte akzeptiert; z. B. `"false"` ist in Python truthy und aktiviert das Preset, `0` deaktiviert. Rust typisiert `enabled` als `bool` mit Default `true`; nicht-boolesche Werte werden vor oder beim Handler abgelehnt.

Warum echtes Problem: Gleicher Pfad, gleicher Body-Schluessel, aber andere Request-Akzeptanz und teils andere Wirkung. Bei String-Formdaten wird Python erfolgreich sein und Rust ablehnen; bei `"false"` waere sogar die Python-Wirkung ueberraschend anders als ein boolesches `false`.

## Endpoint-Matrix Public

Effektiver Prefix Python/Rust: `/api`.

| # | Methode/Pfad | Python | Rust | Auth | Ergebnis |
|---:|---|---|---|---|---|
| 1 | `GET /api/tournaments` | `backend/tournament/routes.py:651` | `rust/crates/turnier-api/src/public/tournaments.rs:37` | public | OK |
| 2 | `GET /api/tournaments/{tournament_id}` | `backend/tournament/routes.py:666` | `rust/crates/turnier-api/src/public/tournaments.rs:48` | public | OK |
| 3 | `GET /api/tournaments/{tournament_id}/me` | `backend/tournament/routes.py:707` | `rust/crates/turnier-api/src/public/tournaments.rs:84` | auth | OK |
| 4 | `POST /api/tournaments/{tournament_id}/teams` | `backend/tournament/routes.py:759` | `rust/crates/turnier-api/src/public/teams.rs:58` | auth | OK; Validation-Rejection siehe F-001 |
| 5 | `POST /api/tournaments/{tournament_id}/teams/{team_id}/join` | `backend/tournament/routes.py:872` | `rust/crates/turnier-api/src/public/teams.rs:157` | auth | OK |
| 6 | `POST /api/tournaments/{tournament_id}/signup` | `backend/tournament/routes.py:1048` | `rust/crates/turnier-api/src/public/signups.rs:30` | auth | OK |
| 7 | `PATCH /api/tournaments/{tournament_id}/teams/{team_id}/recruiting` | `backend/tournament/routes.py:1161` | `rust/crates/turnier-api/src/public/teams.rs:304` | auth | OK; Validation-Rejection siehe F-001 |
| 8 | `POST /api/tournaments/{tournament_id}/teams/{team_id}/invite-by-signup/{signup_id}` | `backend/tournament/routes.py:1196` | `rust/crates/turnier-api/src/public/invitations.rs:70` | auth | OK |
| 9 | `GET /api/tournaments/{tournament_id}/my-invitations` | `backend/tournament/routes.py:1305` | `rust/crates/turnier-api/src/public/invitations.rs:219` | auth | OK |
| 10 | `POST /api/tournaments/{tournament_id}/invitations/{invite_id}/accept` | `backend/tournament/routes.py:1330` | `rust/crates/turnier-api/src/public/invitations.rs:275` | auth | OK |
| 11 | `POST /api/tournaments/{tournament_id}/invitations/{invite_id}/reject` | `backend/tournament/routes.py:1405` | `rust/crates/turnier-api/src/public/invitations.rs:364` | auth | OK |
| 12 | `POST /api/tournaments/{tournament_id}/teams/{team_id}/apply` | `backend/tournament/routes.py:1448` | `rust/crates/turnier-api/src/public/invitations.rs:402` | auth | OK |
| 13 | `GET /api/tournaments/{tournament_id}/teams/{team_id}/applications` | `backend/tournament/routes.py:1505` | `rust/crates/turnier-api/src/public/invitations.rs:467` | auth captain/mod | OK |
| 14 | `POST /api/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept` | `backend/tournament/routes.py:1529` | `rust/crates/turnier-api/src/public/invitations.rs:516` | auth captain/mod | OK |
| 15 | `POST /api/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject` | `backend/tournament/routes.py:1602` | `rust/crates/turnier-api/src/public/invitations.rs:601` | auth captain/mod | OK |
| 16 | `DELETE /api/tournaments/{tournament_id}/signup` | `backend/tournament/routes.py:1643` | `rust/crates/turnier-api/src/public/signups.rs:166` | auth | OK |
| 17 | `POST /api/tournaments/{tournament_id}/checkin` | `backend/tournament/routes.py:1697` | `rust/crates/turnier-api/src/public/signups.rs:210` | auth | OK |
| 18 | `GET /api/tournaments/{tournament_id}/checkin-status` | `backend/tournament/routes.py:1783` | `rust/crates/turnier-api/src/public/tournaments.rs:176` | public | OK |
| 19 | `DELETE /api/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}` | `backend/tournament/routes.py:1843` | `rust/crates/turnier-api/src/public/teams.rs:332` | auth captain | OK |
| 20 | `POST /api/tournaments/{tournament_id}/teams/{team_id}/invite/{target_discord_id}` | `backend/tournament/routes.py:1951` | `rust/crates/turnier-api/src/public/teams.rs:499` | auth captain | OK |
| 21 | `DELETE /api/tournaments/{tournament_id}/teams/{team_id}/leave` | `backend/tournament/routes.py:2091` | `rust/crates/turnier-api/src/public/teams.rs:390` | auth member | OK |
| 22 | `GET /api/tournaments/{tournament_id}/bracket` | `backend/tournament/routes.py:2238` | `rust/crates/turnier-api/src/public/tournaments.rs:136` | public | OK |
| 23 | `GET /api/tournaments/{tournament_id}/groups` | `backend/tournament/routes.py:2258` | `rust/crates/turnier-api/src/public/tournaments.rs:156` | public | OK |

## Endpoint-Matrix Admin

Effektiver Prefix Python/Rust: `/api/admin`.

| # | Methode/Pfad | Python | Rust | Auth | Ergebnis |
|---:|---|---|---|---|---|
| 1 | `GET /api/admin/tournaments` | `backend/tournament/admin_routes.py:731` | `rust/crates/turnier-api/src/admin/tournaments.rs:94` | mod | OK |
| 2 | `GET /api/admin/tournaments/{tournament_id}` | `backend/tournament/admin_routes.py:744` | `rust/crates/turnier-api/src/admin/tournaments.rs:102` | mod | OK |
| 3 | `GET /api/admin/tournaments/{tournament_id}/mini-groups` | `backend/tournament/admin_routes.py:779` | `rust/crates/turnier-api/src/admin/tournaments.rs:125` | mod | OK |
| 4 | `POST /api/admin/tournaments/{tournament_id}/auto-lobby/run` | `backend/tournament/admin_routes.py:804` | `rust/crates/turnier-api/src/admin/tournaments.rs:160` | mod | OK |
| 5 | `POST /api/admin/tournaments` | `backend/tournament/admin_routes.py:820` | `rust/crates/turnier-api/src/admin/tournaments.rs:174` | mod | Abweichung F-001 |
| 6 | `PUT /api/admin/tournaments/{tournament_id}` | `backend/tournament/admin_routes.py:913` | `rust/crates/turnier-api/src/admin/tournaments.rs:287` | mod | Abweichung F-001 |
| 7 | `DELETE /api/admin/tournaments/{tournament_id}` | `backend/tournament/admin_routes.py:1077` | `rust/crates/turnier-api/src/admin/tournaments.rs:656` | admin | OK |
| 8 | `POST /api/admin/tournaments/{tournament_id}/open-checkin` | `backend/tournament/admin_routes.py:1103` | `rust/crates/turnier-api/src/admin/tournaments.rs:681` | mod | OK |
| 9 | `POST /api/admin/tournaments/{tournament_id}/revert-checkin` | `backend/tournament/admin_routes.py:1156` | `rust/crates/turnier-api/src/admin/tournaments.rs:718` | mod | OK |
| 10 | `POST /api/admin/tournaments/{tournament_id}/finalize-checkin` | `backend/tournament/admin_routes.py:1206` | `rust/crates/turnier-api/src/admin/phases.rs:53` | mod | OK; Query/Body-Rejection siehe F-001 |
| 11 | `POST /api/admin/tournaments/{tournament_id}/advance` | `backend/tournament/admin_routes.py:1257` | `rust/crates/turnier-api/src/admin/tournaments.rs:762` | mod | OK |
| 12 | `POST /api/admin/tournaments/{tournament_id}/assign-random` | `backend/tournament/admin_routes.py:1333` | `rust/crates/turnier-api/src/admin/phases.rs:188` | mod | OK |
| 13 | `POST /api/admin/tournaments/{tournament_id}/teams` | `backend/tournament/admin_routes.py:1366` | `rust/crates/turnier-api/src/admin/teams.rs:111` | mod | OK; Validation-Rejection siehe F-001 |
| 14 | `PUT /api/admin/tournaments/{tournament_id}/teams/{team_id}` | `backend/tournament/admin_routes.py:1409` | `rust/crates/turnier-api/src/admin/teams.rs:163` | mod | OK; Validation-Rejection siehe F-001 |
| 15 | `PATCH /api/admin/tournaments/{tournament_id}/teams/{team_id}/recruiting` | `backend/tournament/admin_routes.py:1453` | `rust/crates/turnier-api/src/admin/teams.rs:218` | mod | OK; Validation-Rejection siehe F-001 |
| 16 | `GET /api/admin/tournaments/{tournament_id}/teams/{team_id}/applications` | `backend/tournament/admin_routes.py:1506` | `rust/crates/turnier-api/src/admin/teams.rs:261` | mod | OK |
| 17 | `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/accept` | `backend/tournament/admin_routes.py:1532` | `rust/crates/turnier-api/src/admin/teams.rs:301` | mod | OK |
| 18 | `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/applications/{app_id}/reject` | `backend/tournament/admin_routes.py:1645` | `rust/crates/turnier-api/src/admin/teams.rs:430` | mod | OK |
| 19 | `DELETE /api/admin/tournaments/{tournament_id}/teams/{team_id}` | `backend/tournament/admin_routes.py:1694` | `rust/crates/turnier-api/src/admin/teams.rs:475` | mod | OK |
| 20 | `PUT /api/admin/tournaments/{tournament_id}/teams/{team_id}/captain` | `backend/tournament/admin_routes.py:1733` | `rust/crates/turnier-api/src/admin/teams.rs:548` | mod | Abweichung F-001/F-002 |
| 21 | `DELETE /api/admin/tournaments/{tournament_id}/teams/{team_id}/members/{discord_id}` | `backend/tournament/admin_routes.py:1781` | `rust/crates/turnier-api/src/admin/teams.rs:599` | mod | Abweichung F-002 |
| 22 | `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/members/move` | `backend/tournament/admin_routes.py:1823` | `rust/crates/turnier-api/src/admin/teams.rs:659` | mod | Abweichung F-001/F-002 |
| 23 | `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/signups/assign` | `backend/tournament/admin_routes.py:1909` | `rust/crates/turnier-api/src/admin/teams.rs:764` | mod | OK; Validation-Rejection siehe F-001 |
| 24 | `POST /api/admin/tournaments/{tournament_id}/teams/{team_id}/add-member` | `backend/tournament/admin_routes.py:1992` | `rust/crates/turnier-api/src/admin/teams.rs:863` | admin | Abweichung F-001/F-002 |
| 25 | `DELETE /api/admin/tournaments/{tournament_id}/signups/{signup_id}` | `backend/tournament/admin_routes.py:2096` | `rust/crates/turnier-api/src/admin/teams.rs:983` | mod | OK |
| 26 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/result` | `backend/tournament/admin_routes.py:2137` | `rust/crates/turnier-api/src/admin/matches.rs:92` | mod | OK; Query/Body-Rejection siehe F-001 |
| 27 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/games/{game_number}/start` | `backend/tournament/admin_routes.py:2292` | `rust/crates/turnier-api/src/admin/matches.rs:249` | admin | OK |
| 28 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/games/{game_number}/result` | `backend/tournament/admin_routes.py:2317` | `rust/crates/turnier-api/src/admin/matches.rs:287` | admin | Abweichung F-001 |
| 29 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/create-lobby` | `backend/tournament/admin_routes.py:2383` | `rust/crates/turnier-api/src/admin/matches.rs:381` | mod | OK |
| 30 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/start` | `backend/tournament/admin_routes.py:2443` | `rust/crates/turnier-api/src/admin/matches.rs:397` | mod | OK |
| 31 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/fetch-result` | `backend/tournament/admin_routes.py:2499` | `rust/crates/turnier-api/src/admin/matches.rs:413` | mod | OK |
| 32 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/leave-lobby` | `backend/tournament/admin_routes.py:2553` | `rust/crates/turnier-api/src/admin/matches.rs:429` | mod | OK |
| 33 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/reset` | `backend/tournament/admin_routes.py:2600` | `rust/crates/turnier-api/src/admin/matches.rs:445` | admin | OK |
| 34 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/manual-lobby` | `backend/tournament/admin_routes.py:2630` | `rust/crates/turnier-api/src/admin/matches.rs:461` | mod | OK; Validation-Rejection siehe F-001 |
| 35 | `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/create-lobby` | `backend/tournament/admin_routes.py:2660` | `rust/crates/turnier-api/src/admin/group_matches.rs:49` | mod | OK |
| 36 | `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/start` | `backend/tournament/admin_routes.py:2708` | `rust/crates/turnier-api/src/admin/group_matches.rs:65` | mod | OK |
| 37 | `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/fetch-result` | `backend/tournament/admin_routes.py:2749` | `rust/crates/turnier-api/src/admin/group_matches.rs:81` | mod | OK |
| 38 | `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/leave-lobby` | `backend/tournament/admin_routes.py:2791` | `rust/crates/turnier-api/src/admin/group_matches.rs:97` | mod | OK |
| 39 | `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/reset` | `backend/tournament/admin_routes.py:2826` | `rust/crates/turnier-api/src/admin/group_matches.rs:113` | admin | OK |
| 40 | `POST /api/admin/tournaments/{tournament_id}/group-matches/{match_id}/manual-lobby` | `backend/tournament/admin_routes.py:2861` | `rust/crates/turnier-api/src/admin/group_matches.rs:129` | mod | OK; Validation-Rejection siehe F-001 |
| 41 | `GET /api/admin/casters` | `backend/tournament/admin_routes.py:2892` | `rust/crates/turnier-api/src/admin/casters.rs:155` | mod | OK |
| 42 | `GET /api/admin/tournaments/{tournament_id}/casters` | `backend/tournament/admin_routes.py:2904` | `rust/crates/turnier-api/src/admin/casters.rs:179` | mod | OK |
| 43 | `POST /api/admin/tournaments/{tournament_id}/casters` | `backend/tournament/admin_routes.py:2915` | `rust/crates/turnier-api/src/admin/casters.rs:197` | mod | OK; Validation-Rejection siehe F-001 |
| 44 | `DELETE /api/admin/tournaments/{tournament_id}/casters/{discord_id}` | `backend/tournament/admin_routes.py:2948` | `rust/crates/turnier-api/src/admin/casters.rs:235` | mod | Abweichung F-002 |
| 45 | `GET /api/admin/tournaments/{tournament_id}/matches/{match_id}/casters` | `backend/tournament/admin_routes.py:2970` | `rust/crates/turnier-api/src/admin/casters.rs:263` | mod | OK |
| 46 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/casters` | `backend/tournament/admin_routes.py:2982` | `rust/crates/turnier-api/src/admin/casters.rs:275` | mod | OK; deprecated 410, Validation-Rejection siehe F-001 |
| 47 | `DELETE /api/admin/tournaments/{tournament_id}/matches/{match_id}/casters/{discord_id}` | `backend/tournament/admin_routes.py:2997` | `rust/crates/turnier-api/src/admin/casters.rs:286` | mod | OK; deprecated 410 |
| 48 | `GET /api/admin/tournaments/{tournament_id}/matches/{match_id}/event-presets` | `backend/tournament/admin_routes.py:3014` | `rust/crates/turnier-api/src/admin/matches.rs:479` | mod | OK |
| 49 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/apply-convars` | `backend/tournament/admin_routes.py:3040` | `rust/crates/turnier-api/src/admin/matches.rs:516` | mod | OK; Validation-Rejection siehe F-001 |
| 50 | `POST /api/admin/tournaments/{tournament_id}/matches/{match_id}/apply-event-preset` | `backend/tournament/admin_routes.py:3097` | `rust/crates/turnier-api/src/admin/matches.rs:566` | mod | Abweichung F-004 |
| 51 | `POST /api/admin/tournaments/{tournament_id}/groups/generate` | `backend/tournament/admin_routes.py:3172` | `rust/crates/turnier-api/src/admin/brackets.rs:40` | mod | Abweichung F-003 |
| 52 | `POST /api/admin/tournaments/{tournament_id}/bracket/generate` | `backend/tournament/admin_routes.py:3212` | `rust/crates/turnier-api/src/admin/brackets.rs:73` | mod | OK |
| 53 | `POST /api/admin/tournaments/{tournament_id}/voice/move-teams` | `backend/tournament/admin_routes.py:3238` | `rust/crates/turnier-api/src/admin/voice.rs:42` | admin | OK; Query-Rejection siehe F-001 |
| 54 | `POST /api/admin/tournaments/{tournament_id}/voice/move-sammelpunkt` | `backend/tournament/admin_routes.py:3282` | `rust/crates/turnier-api/src/admin/voice.rs:90` | admin | OK |
| 55 | `POST /api/admin/voice/move-user` | `backend/tournament/admin_routes.py:3310` | `rust/crates/turnier-api/src/admin/voice.rs:129` | admin | OK; Validation-Rejection siehe F-001 |
| 56 | `GET /api/admin/voice/channel-members/{channel_id}` | `backend/tournament/admin_routes.py:3322` | `rust/crates/turnier-api/src/admin/voice.rs:143` | admin | OK |

Hinweis: Die Matrix enthaelt alle 56 Admin-Routen aus `admin_routes.py`; zusammen mit 23 Public-Routen wurden 79 konkrete Route-Decoratoren/Methoden geprueft.

## Schlussbewertung

Die Migration ist auf Pfad-/Methoden-/Auth-Ebene fuer die beiden Zielrouter vollstaendig. Die relevanten Cutover-Risiken liegen nicht in fehlenden Endpunkten, sondern in HTTP-Contract-Randfaellen:

1. FastAPI-Validation-Fehler muessen fuer Paritaet zentral nachgebildet werden oder bewusst als Contract-Aenderung freigegeben werden.
2. Die strengere Discord-ID-Parsing-Grenze ist fachlich nachvollziehbar wegen PG-`BIGINT`, aber nicht 1:1 zum Python-Admin-Verhalten.
3. Einzelne Admin-Request-Coercions (`num_groups`, `enabled`) sind nicht deckungsgleich und sollten vor Live gegen das reale Frontend geprueft werden.
