# DDC Plattform: systemweites Performance-Audit (23.11.2026)

**Geltungsbereich:** `Deadlock-Turniere` mit Rust/Axum/Tokio, SQLx/PostgreSQL, React/TypeScript/Vite und Caddy. Ergänzt den [kontrollierten Baseline-Lasttest](loadtest-2026-11-23.md).

**Wichtig:** Die Zahlen wurden gegen den **bestehenden Live-Dienst vor PR-Merge** gemessen; die Codeoptimierungen in PR #34 wurden nicht auf Live-Rust deployed. Die Messungen stammen **vom Server selbst** mit vorgewärmten, wiederverwendeten HTTPS-Verbindungen. Sie sind keine geografisch repräsentative Browser-/Mobilmessung.

## 1. Was heute bereits schnell ist

Public HTTPS, 8 parallele Clients, jeweils 96 gemessene GET-Requests; p95 in Millisekunden:

| Route | p50 | p95 | p99 |
|---|---:|---:|---:|
| `/api/leaderboard` | 6,3 | **12,7** | 15,1 |
| `/api/draft/heroes` | 7,6 | **27,5** | 37,2 |
| `/api/tournaments` | 9,1 | **30,3** | 43,6 |
| `/api/tournaments/17` | 44,7 | **75,9** | 81,1 |
| `/api/health` | 18,4 | **71,8** | 128,1 |

Der komplette `site`-Test hat **880/880 HTTP 200**, keinen Timeout und keine Anfrage über 300 ms geliefert.

Separat: 60 gültige öffentliche Spielerprofil-Requests mit 4 Clients: **p50 15,1 ms / p95 64,4 ms / p99 101,6 ms**. Das Profil war der *alte* Handler mit 6 nacheinander ausgeführten SQL-Requests.

Es gibt daher **keine Grundlage**, Rangliste und Turnierliste pauschal mit zusätzlichen Redis-/Memory-Caches zu versehen oder den PostgreSQL-Pool blind zu vergrößern. Skalierungsbedarf zuerst messen.

## 2. Änderungen in PR #34

### Datenzugriffe

- Die bestehenden Team-Roster-Optimierungen laden öffentliche Mitglieder/Bewerbungen gebündelt.
- **Spielerprofil:** Die Identität (Discord-ID zum Namen) ist eine eigene Abfrage (`db.rs:72`, aufgerufen in `leaderboard.rs:116`). Die Kopfabfrage danach verbindet Avatar, User-Profil und Rank-Cache in einer parametrisierten SQL-Abfrage. Die Punkte stammen aus `player_standings`, der einzigen Ranglisten-Aggregation, die alle Spieler berechnet und dann den einen Eintrag liefert. Die Turnierhistorie folgt in einer eigenen Abfrage. Pro Profil laufen damit drei Abfragen plus die volle Ranglisten-Aggregation, ohne Änderung an JSON-Struktur, anonymen Zugriffen oder gespeicherten Rangdaten.
- Der Profil-Integrationstest prüft weiterhin Rank, Punkte und Historie und wurde um einen Nutzer ohne Profil/Punkte sowie um 404-Semantik ergänzt. Der Test lief am 23.11.2026 auf einer isolierten, temporären PostgreSQL-16-Datenbank mit zentralen Migrationen erfolgreich (1/1); dafür musste in dieser Wegwerf-Instanz die NOLOGIN-Rolle `deadlock` angelegt werden. Am 28.11.2026 lief er erneut als Teil der kompletten `central_routes`-Datei über `test-db-run` auf ionos, wo der Runner diese Rolle selbst anlegt, und bestand.

### Weitere Integrationssuite

Erster Lauf am 23.11.2026 mit derselben Wegwerf-DB: **13 von 15 Tests bestanden, 2 fehlgeschlagen** (`solo_balance_preview_apply_rollback_and_next_tournament_priority` mit `42P01: relation "turnier.solo_balance_runs" does not exist`, `test_mode_seed_simulate_and_wipe_only_touch_test_data` mit HTTP 500 beim Erstellen des Testturniers). Das Debug-Logging dieser Fehlersuche wurde vollständig zurückgenommen; Produktionsschema und Produktionsdienste blieben unverändert.

Nachprüfung am 28.11.2026 über `test-db-run` auf ionos, je eine frische Wegwerf-Datenbank, Befehl `cargo test -p turnier-api --features testing --test central_routes`:

| Stand Turniere | Migrationsquelle `Deadlock-Bots` | Ergebnis |
|---|---|---|
| `origin/main` 032ae8f | `origin/main` 53402f6b | **19/19 bestanden** |
| Branch mit `origin/main` 032ae8f zusammengeführt | `origin/main` 53402f6b | **19/19 bestanden** |
| derselbe Branch | veralteter Nachbar-Checkout e65d3805 | 13/19 bestanden, 6 mit HTTP 500 oder `ColumnNotFound("tournament_mode_auto")` |

Ursache des roten Laufs vom 28.11.2026: Die Tests wenden die Migrationen aus dem Nachbar-Checkout `../../Deadlock-Bots` an. Dem veralteten Stand e65d3805 fehlt `20261127120000_turnier_tournament_mode_auto.sql`, also fehlt die Spalte und die Routen antworten mit 500. Der Lauf vom 23.11.2026 lässt sich nicht mehr nachstellen; sein `42P01` passt zum selben Muster (Migrationsquelle ohne `20261011120000_turnier_solo_balance.sql`), ist dafür aber nicht belegt. Belegt ist: Mit aktuellem `Deadlock-Bots` `origin/main` bestehen beide damals roten Tests, auf `origin/main` wie auf dem Branch. Die Fehler sind also weder Basislinie von `main` noch durch diesen Branch verursacht. Ein Merge-Hindernis aus dieser Suite besteht nicht mehr.

- Der globale `Server-Timing`-Layer loggt statt konkreter URL-Segmente das Axum-Routenmuster (`MatchedPath`). Für Requests über 100 ms werden bei aktiviertem Debug-Logging die SQLx-Poolgröße und die Zahl freier Verbindungen erfasst; über 300 ms mit Warnstufe. Die öffentliche Antwort erhält weiterhin nur `rust;dur=…`, **keine Pool-internen Informationen**.

### Live-Verhalten

- **Draft:** Der nötige 1-Sekunden-Takt bleibt für sichtbare aktive Räume, wird jedoch in Hintergrund-Tabs gestoppt. Rückkehr in den Tab löst weiterhin einen aktuellen Fetch aus.
- **Observer:** Sessions werden bei aktiven Sessions alle 2 s, sonst alle 10 s abgefragt; Bot-Lease bei Reservierung alle 3 s, sonst alle 15 s; Sitzungsdetails nur bei echten Live-Sessions alle 1 s, bei wartenden alle 5 s und bei abgeschlossenen nicht mehr. Hintergrund-Tabs pausieren.
- **Schriftladen:** Google-Fonts-Aufruf nur in `index.html` mit `preconnect`, der doppelte CSS-`@import` entfällt.
- **Route-Splitting:** Auch Home und Draft-Lobby werden bei Bedarf geladen; die gemeinsame Header-Leiste bleibt beim Lazy-Load über ein Suspense innerhalb des Layouts sichtbar. Der Vite-Hauptchunk sank dabei von ca. **76 KB gzip auf 24 KB gzip** (Build vom 28.11.2026: 23,9 KB gzip). Teilweise wanderte Code in einen separaten Shared-Chunk (ca. 41 KB gzip), sodass dies **nicht** einer garantierten Reduktion der gesamten Seitenlast um 52 KB entspricht.
- Vorherige Optimierungen: React Query Stale-Times und Lazy-Loads für Admin, Turnier, Rangliste und Draft-Board.

### Reproduzierbarer Performance-Test

```bash
node scripts/perf/bench-readonly.mjs local site
node scripts/perf/bench-readonly.mjs public site
# nur wenn eine isolierte Rust-Staging-Instanz auf 127.0.0.1:8901 läuft:
node scripts/perf/bench-readonly.mjs staging site
```

Der Test verwendet ausschließlich fest definierte GET-Routen, max. acht parallele Clients und stoppt bei Fehlern oder p95 über 300 ms. `core` bleibt Default und misst das bisherige 3-Endpoint-Szenario.

## 3. Größter offener Frontend-/Caddy-Fund

**Gemessen am 23.11.2026:** Die öffentlich ausgelieferte Preview-`index.html` und die gehashten JS-Dateien (`/turnier/_preview/assets/index-…js`) antworten **ohne Cache-Control**, obwohl ETags vorhanden sind. ETags sparen unter Umständen Bytes, aber nicht einen erneuten Conditional-Request. Die gehashten Vite-Dateien können langfristig unveränderlich gecacht werden.

Empfohlene Änderung **im separaten Repository `EarlySalty/caddy-config`**, gezielt auf `/turnier` und nur bei gesichert gehashten JS-/CSS-/Woff2-Dateinamen:

```caddyfile
# Im Turnier-Siteblock, vor den allgemeinen handle-Zweigen;
# @turnier_immutable als benannten Matcher in dieser Site verwenden.
@turnier_immutable path_regexp turnier_immutable ^/turnier/(?:_preview/)?assets/[^/]+-[A-Za-z0-9_-]{6,}\.(?:js|css|woff2)$
header @turnier_immutable Cache-Control "public, max-age=31536000, immutable"
```

Zusätzlich nur für die **HTML-Fallback-Antwort** `Cache-Control: no-cache, must-revalidate` setzen, nicht für die gehashten Assets. Die dynamischen `/turnier/api/`-/Auth-Routen **niemals pauschal öffentlich cachen**.

**Umgesetzt:** [EarlySalty/caddy-config#10](https://github.com/EarlySalty/caddy-config/pull/10) enthält genau diesen Matcher sowie No-Cache für das SPA-HTML und wurde am 27.11.2026 gemergt. Live geprüft am 28.11.2026: `/turnier/assets/index-…js` antwortet mit `Cache-Control: public, max-age=31536000, immutable`, `/turnier/` und `/turnier/_preview/` mit `private, no-cache, must-revalidate`.

## 4. Weitere Skalierungs-Funde (vor Schema-/Architekturänderungen messen)

1. **Session-Namensauflösung:** Mehrere SQL-Abfragen verwenden `LEFT JOIN (SELECT discord_id, MAX(discord_name) FROM turnier.sessions ... GROUP BY discord_id)`. Bei großer Session-Tabelle kann das zu unnötiger globaler Aggregation führen. Mit `EXPLAIN (ANALYZE, BUFFERS)` auf **Test-/Staging-Datenbank** prüfen und erst dann per `LATERAL` oder geeigneten `(discord_id, discord_name)`-Index ändern.
2. **Auth:** Jeder authentifizierte API-Request überprüft das Session-Token in PostgreSQL. Kein transparenter Session-L1-Cache ohne verbindliche Logout-/Rollen-/Revocation-Semantik; die sichere Korrektheit geht vor ein paar Millisekunden.
3. **Steam-/Discord-Rangauflösung:** Im Admin-Read-Model können bei Cache-Misses externe Aufrufe im kritischen Pfad entstehen. Nach Messung der `Server-Timing`- und Pool-Werte ist das nächste große Architekturprojekt ein strikt cachebasierter Read-Pfad mit kontrollierter asynchroner Aktualisierung. Damit dürfen aktuelle Rangwerte aber nicht stillschweigend verschwinden; hierfür separates Integrationsdesign.
4. **Database Pool:** Der Code-Default ist `max_connections=8`, `acquire_timeout=10s`. Nicht erhöhen, bevor Pool-Wartezeiten, DB-CPU, aktive Queries und Konflikte gemessen wurden.
5. **Browser-Nutzbarkeit:** Die Preview-Startseite lieferte am 23.11.2026 vor dem Route-Splitting ein Hauptbundle von ca. **76 KB gzip**, React ca. **66 KB gzip** und globale CSS ca. **22 KB gzip**, dazu routeabhängige Chunks. Daher können 50 ms als Ziel für einen vollständigen Kaltstart auf mobilen Netzen nicht garantiert werden. **LCP, INP, JS-Ausführungszeit, Font-Paint und Navigationszeit** separat per Browser-RUM messen.
6. **Cache/Datenschutz:** Öffentliche statische Assets dürfen lang gecacht werden. Live-Drafts, Admin- und User-Daten dürfen niemals über denselben Shared-Cache-Pfad laufen.

## 5. Definition einer belastbaren Freigabe

- **Ideal:** p95 unter 50 ms bei kleinen, warmen API-Requests.
- **Gut:** p95 unter 100 ms bei typischen öffentlichen API-Requests.
- **Akzeptabel:** p95 unter 300 ms bei den komplexen Routen unter der vereinbarten Last.
- **Nicht als dieselbe Zahl zusammenrechnen:** Rust-Verarbeitungszeit, Caddy/TLS-Aufbau, Client-Netz und Browser-Interaktion.
- Vor Produktiv-Rollout: Staging-Rust mit separater, repräsentativer SQL-Testdatenbank, 1/4/8/16-Client-Vergleich, echte eingeloggte Admin-Ansicht und externe Browser-Messung.
- Kein Wechsel des Stacks, keine Produktionstests mit ungebremster Last, keine Änderung an live laufenden Diensten.
