# Enforcement-Gap-Audit: Turniere

Datum: 2026-06-30  
Scope: Rust-Port `rust/crates/*` plus Python-Referenz unter `backend/*` nur lesend. Keine Git-/Cargo-/Build-/Test-Befehle.

## Kurzfazit

Gefunden wurden zwei belastbare Enforcement-Gaps:

| Severity | Anzahl |
|---|---:|
| CRIT | 0 |
| HIGH | 2 |
| MED | 0 |
| LOW | 0 |

Nicht als Befund gemeldet: Auth-/RBAC-Extractor, Host-Guard, Auto-Lobby-Gates, Single-Active-Tournament-Gate, Caster-Zuweisung auf Turnier-Ebene, No-Show-Schonfrist-Anzeige. Diese Pfade wurden gegen Aufrufstellen geprüft; dort fand ich keinen überlebenden Enforcement-Gap im Sinne dieser Aufgabe.

## HIGH — Proposal kann ohne echte Caster-Freigabe auf `approved`

**Guard-Definition:** `rust/crates/turnier-automatik/src/proposals.rs:196` definiert `approvals_count()`, und die Datenmodell-/Spec-Definition verlangt mindestens einen Caster-Approve (`docs/specs/2026-06-29-turnier-automatisierung-design.md:17`, `:64`, `:87`, `:115`). Votes werden in `record_vote()` gespeichert (`rust/crates/turnier-automatik/src/proposals.rs:153`).

**Live-Aufrufstelle / fehlender Aufruf:** `POST /api/admin/proposals/{id}/event` ruft in `rust/crates/turnier-api/src/admin/automatik.rs:507` direkt `proposals::apply_event()` auf (`:515`). `apply_event()` in `rust/crates/turnier-automatik/src/proposals.rs:208` lädt nur den aktuellen State, ruft `transition()` auf und setzt den nächsten State (`:214-220`). Es gibt dort keinen Aufruf von `approvals_count()` und keinen Check `>= 1`.

**Fehlermodus:** A) PORTED BUT NEVER WIRED. Zusätzlich falsche Identitätsquelle: `record_proposal_vote()` nimmt `caster_id` aus dem Request-Body (`rust/crates/turnier-api/src/admin/automatik.rs:270`, `:526-534`) statt aus dem authentifizierten Actor oder einer Caster-Rollenprüfung.

**Repro-Szenario:**

1. Mod-User erstellt ein Preset und ein Proposal.
2. Mod-User ruft `POST /api/admin/proposals/{id}/event` mit `{"event":"submit"}` auf.
3. Ohne einen einzigen Vote ruft derselbe Mod-User `POST /api/admin/proposals/{id}/event` mit `{"event":"approve"}` auf.
4. Ergebnis: Proposal-State wird `approved`, obwohl `approvals_count(id) == 0`.

Variante: Ein Mod-User kann `POST /api/admin/proposals/{id}/votes` mit beliebigem `caster_id` senden; der Handler persistiert diese ID ungeprüft und erhöht damit den Approval-Count.

**Warum der Widerlegungsversuch fehlschlug:** Die State-Machine blockiert zwar `draft -> approved` direkt, aber nach `submit` erlaubt sie `pending_approval -> approved` bedingungslos. `approvals_count()` wird nur zur Detailausgabe geladen (`rust/crates/turnier-api/src/admin/automatik.rs:465-479`), nicht im Entscheidungspfad. Der vorhandene Caster-Rollen-Check aus `admin/casters.rs` wird nur beim Zuweisen von Tournament-Castern verwendet (`rust/crates/turnier-api/src/admin/casters.rs:187-195`), nicht bei Proposal-Votes oder Proposal-Approval.

**Py-Referenz:** Keine Python-Feature-Referenz vorhanden; das Automatik-Feature ist laut Plan neu. Der Plan markiert 1a als DB/Logik-Fundament ohne Endpunkte, spätere Phasen sollen Callback/Freigabe verdrahten (`docs/plans/2026-06-29-turnier-automatik-phase1.md:9-12`, `:35-36`).

## HIGH — `tournament_dm_optout` wird von Live-DM-Sendepfaden ignoriert

**Guard-Definition:** Opt-out wird über `/api/me/dm-optout` geschrieben (`rust/crates/turnier-api/src/account.rs:56-63`) und gelöscht (`:66-73`). Die Guard-Logik existiert in `turnier_automatik::optout`: `set_optout()` (`rust/crates/turnier-automatik/src/optout.rs:45`), `is_opted_out()` (`:65`) und `compute_recipients()` (`:94`). Die Spec definiert `tournament_dm_optout` als DM-Unterdrückung und verlangt Rollenmitglieder minus Opt-out (`docs/specs/2026-06-29-turnier-automatisierung-design.md:89`, `:94`, `:104`, `:118`).

**Live-Aufrufstelle / fehlender Aufruf:** Beim Anlegen eines Nicht-Test-Turniers lädt `create_tournament()` alle `user_profiles.discord_id` und ruft `notify_users(..., TournamentNews, ...)` auf (`rust/crates/turnier-api/src/admin/tournaments.rs:244-259`). `DiscordNotifier::notify_users()` liest nur `user_profiles.notify_discord_dm` und das Event-Flag (`rust/crates/turnier-discord/src/notifier.rs:268-320`; SQL in `load_notify_flags()` `:335-366`). Kein Pfad liest `tournament_dm_optout` oder ruft `is_opted_out()`/`compute_recipients()` auf.

**Fehlermodus:** A) PORTED BUT NEVER WIRED.

**Repro-Szenario:**

1. User `U` hat `user_profiles.notify_discord_dm = 1` und `notify_tournament_news = 1`.
2. `U` setzt `PUT /api/me/dm-optout` mit `{"scope":"all"}`. Dadurch entsteht eine Zeile in `tournament_dm_optout`.
3. Ein Mod erstellt ein Nicht-Test-Turnier über `POST /api/admin/tournaments`.
4. Der Live-Pfad ruft `notify_users()` für `U` auf und sendet eine Tournament-News-DM, weil nur `user_profiles`-Flags ausgewertet werden.

Falsche Prod-Aktion: DM trotz explizitem Turnier-DM-Opt-out.

**Warum der Widerlegungsversuch fehlschlug:** Man könnte argumentieren, dass `tournament_dm_optout` nur für den künftigen Automatik-DM-Broadcast gedacht ist und `notify_users()` ein altes Profil-Flag-System nutzt. Gegenbeleg: Die neue Self-Service-Route ist bereits live im Rust-Router (`rust/crates/turnier-api/src/app.rs:28-36` merge `account::router()`), schreibt ausschließlich `tournament_dm_optout`, und im Repo existiert kein separater `dm-broadcast`-Pfad, der diese Tabelle nutzt. Damit hat der Benutzer eine wirksame Opt-out-Oberfläche, die in den realen DM-Sendepfaden nicht durchgesetzt wird.

**Py-Referenz:** Python hatte diese neue Opt-out-Tabelle nicht im Live-Pfad; `backend/notifications/discord_notifier.py:250-285` filtert ebenfalls nur Profil-Flags. Der Guard stammt aus der neuen Rust-Automatik/Spec, wurde aber nicht an die sendende Runtime angeschlossen.

## Geprüfte Guards Ohne Befund

- **Auth/RBAC Web-API:** `AuthUser`, `ModUser`, `AdminUser` lösen Token vor Handler-Ausführung auf und brechen mit 401/403 ab (`rust/crates/turnier-api/src/extract.rs:44-80`). Die Admin-/Test-/Draft-Mutationsrouten nutzen diese Extractoren.
- **Host-Guard:** `build_router()` legt `host_guard` global auf den Router (`rust/crates/turnier-api/src/app.rs:25-39`); der Guard blockt vor Handler-Dispatch (`:71-82`).
- **Internal Broker/OAuth Tokens:** Broker- und OAuth-Clients validieren Base-URL/Token fail-closed und setzen `X-Internal-Token` unmittelbar am Request (`rust/crates/turnier-discord/src/broker.rs:47-83`, `rust/crates/turnier-auth/src/oauth.rs:129-160`).
- **Caster-Zuweisung:** Turnier-Caster-Zuweisung lädt die Caster-Rolle strikt und lehnt Nicht-Mitglieder ab (`rust/crates/turnier-api/src/admin/casters.rs:77-95`, `:187-195`). Match-Lobby-Erstellung lädt `tournament_casters` vor Legacy-`match_casters` und benachrichtigt Caster im Match-Channel-Pfad (`rust/crates/turnier-match/src/repo.rs:305-357`, `rust/crates/turnier-match/src/lobby.rs:400-415`).
- **Auto-Lobby-Gates:** `schedule_auto_lobbies_for_tournament()` und `schedule_auto_lobby_for_next_round()` prüfen `auto_lobby_enabled` und `is_test` vor Lobby-Erstellung (`rust/crates/turnier-match/src/auto_lobby.rs:21-27`, `:79-85`, `:116-130`).
- **Auto-Phasenplanung:** Der Scheduler berechnet Fälligkeit vor Statuswechsel, prüft bei Aktivierung die Single-Active-Invariante und kehrt bei Check-Fehler fail-closed zurück (`rust/crates/turnier-scheduler/src/loop_runner.rs:105-147`). Der gemeinsame Statuswechsel validiert Transitionen vor Seiteneffekten (`rust/crates/turnier-scheduler/src/transition.rs:127-145`).
- **No-Show-Schonfrist:** `grace_expired` wird nur für Action-Items berechnet; weder Rust noch Python erzwingen sie im Confirm-Endpunkt. Mangels harter Guard-Definition nicht als Enforcement-Gap gewertet.

## Verifikation

Nur lesende Prüfung mit `rg`, `find`, `sed`, `nl`; kein `git`, kein `cargo build`, kein `cargo clippy`, kein `cargo test`.
