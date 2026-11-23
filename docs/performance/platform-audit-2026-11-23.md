# DDC Plattform – systemweites Performance-Audit (23.11.2026)

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
- **Spielerprofil:** Identität, Avatar, User-Profil, Rank-Cache und Punkte werden nun über einen einzelnen parametrisierten SQL-Join abgefragt. Die Turnierhistorie folgt in einem zweiten SQL-Request. Damit **2 statt 6 Roundtrips pro Profil**, ohne Änderung an JSON-Struktur, anonymen Zugriffen oder gespeicherten Rangdaten.
- Der Profil-Integrationstest prüft weiterhin Rank, Punkte und Historie und wurde um einen Nutzer ohne Profil/Punkte sowie um 404-Semantik ergänzt. Der Test ist **mit zentralen Migrationen auf einer isolierten, temporären PostgreSQL-16-Datenbank erfolgreich ausgeführt (1/1)**. Für die Migrationen war eine fehlende, nur in dieser Wegwerf-Instanz angelegte NOLOGIN-Rolle `deadlock` nötig.

### Weitere Integrationssuite: offener Befund

Die komplette `central_routes`-Testdatei wurde mit derselben Wegwerf-DB ausgeführt: **13 von 15 Tests bestanden, 2 fehlgeschlagen**. Diese beiden Fehler müssen vor einem Merge untersucht werden:
- `solo_balance_preview_apply_rollback_and_next_tournament_priority`: HTTP 500. Temporäres Test-Logging identifizierte den konkreten PostgreSQL-Fehler `42P01: relation "turnier.solo_balance_runs" does not exist`. Der Solo-Balance-Read nutzt die Tabelle, die in der zugrunde liegenden zentralen Testmigration nicht existiert. **Wahrscheinlich Migrationslücke oder getrennte Schemaquelle**, vor einem neuen Migrationsskript mit zentralem DB-Verantwortlichen abgleichen.
- `test_mode_seed_simulate_and_wipe_only_touch_test_data`: HTTP 500 beim Erstellen des Testturniers. Die Ursache ist noch nicht hinreichend isoliert; **nicht** als durch die Profiloptimierung verursacht ausgeben.

Das Debug-Logging wurde nach der Fehlersuche **vollständig zurückgenommen**. Weder Produktionsschema noch Produktionsdienste wurden verändert. **Kein Merge** bei weiterhin roten Integrationstests.
- Der globale `Server-Timing`-Layer loggt statt konkreter URL-Segmente das Axum-Routenmuster (`MatchedPath`). Für Requests über 100 ms werden bei aktiviertem Debug-Logging die SQLx-Poolgröße und die Zahl freier Verbindungen erfasst; über 300 ms mit Warnstufe. Die öffentliche Antwort erhält weiterhin nur `rust;dur=…`, **keine Pool-internen Informationen**.

### Live-Verhalten

- **Draft:** Der nötige 1-Sekunden-Takt bleibt für sichtbare aktive Räume, wird jedoch in Hintergrund-Tabs gestoppt. Rückkehr in den Tab löst weiterhin einen aktuellen Fetch aus.
- **Observer:** Sessions werden bei aktiven Sessions alle 2 s, sonst alle 10 s abgefragt; Bot-Lease bei Reservierung alle 3 s, sonst alle 15 s; Sitzungsdetails nur bei echten Live-Sessions alle 1 s, bei wartenden alle 5 s und bei abgeschlossenen nicht mehr. Hintergrund-Tabs pausieren.
- **Schriftladen:** Google-Fonts-Aufruf nur in `index.html` mit `preconnect`, der doppelte CSS-`@import` entfällt.
- **Route-Splitting:** Auch Home und Draft-Lobby werden bei Bedarf geladen; die gemeinsame Header-Leiste bleibt beim Lazy-Load über ein Suspense innerhalb des Layouts sichtbar. Der Vite-Hauptchunk sank dabei von ca. **76 KB gzip auf 24 KB gzip**. Teilweise wanderte Code in einen separaten Shared-Chunk (ca. 41 KB gzip), sodass dies **nicht** einer garantierten Reduktion der gesamten Seitenlast um 52 KB entspricht.
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

**Tatsächlich gemessen:** Die öffentlich ausgelieferte Preview-`index.html` und die gehashten JS-Dateien (`/turnier/_preview/assets/index-…js`) antworten **ohne Cache-Control**, obwohl ETags vorhanden sind. ETags sparen unter Umständen Bytes, aber nicht einen erneuten Conditional-Request. Die gehashten Vite-Dateien können langfristig unveränderlich gecacht werden.

Empfohlene Änderung **im separaten Repository `EarlySalty/caddy-config`**, gezielt auf `/turnier` und nur bei gesichert gehashten JS-/CSS-/Woff2-Dateinamen:

```caddyfile
# Im Turnier-Siteblock, vor den allgemeinen handle-Zweigen;
# @turnier_immutable als benannten Matcher in dieser Site verwenden.
@turnier_immutable path_regexp turnier_immutable ^/turnier/(?:_preview/)?assets/[^/]+-[A-Za-z0-9_-]{6,}\.(?:js|css|woff2)$
header @turnier_immutable Cache-Control "public, max-age=31536000, immutable"
```

Zusätzlich nur für die **HTML-Fallback-Antwort** `Cache-Control: no-cache, must-revalidate` setzen, nicht für die gehashten Assets. Die dynamischen `/turnier/api/`-/Auth-Routen **niemals pauschal öffentlich cachen**.

**Separater Draft-PR erstellt:** [EarlySalty/caddy-config#10](https://github.com/EarlySalty/caddy-config/pull/10) enthält genau diesen Matcher sowie No-Cache für das SPA-HTML. Das ursprüngliche, bereits veränderte Arbeitsverzeichnis blieb unangetastet. `caddy adapt` war erfolgreich und zwei Matcher-Tests sind grün; `caddy validate` benötigt Berechtigung für eine vorhandene Caddy-Zugriffslogdatei. **Nicht live geändert und nicht gemergt.**

## 4. Weitere Skalierungs-Funde (vor Schema-/Architekturänderungen messen)

1. **Session-Namensauflösung:** Mehrere SQL-Abfragen verwenden `LEFT JOIN (SELECT discord_id, MAX(discord_name) FROM turnier.sessions ... GROUP BY discord_id)`. Bei großer Session-Tabelle kann das zu unnötiger globaler Aggregation führen. Mit `EXPLAIN (ANALYZE, BUFFERS)` auf **Test-/Staging-Datenbank** prüfen und erst dann per `LATERAL` oder geeigneten `(discord_id, discord_name)`-Index ändern.
2. **Auth:** Jeder authentifizierte API-Request überprüft das Session-Token in PostgreSQL. Kein transparenter Session-L1-Cache ohne verbindliche Logout-/Rollen-/Revocation-Semantik; die sichere Korrektheit geht vor ein paar Millisekunden.
3. **Steam-/Discord-Rangauflösung:** Im Admin-Read-Model können bei Cache-Misses externe Aufrufe im kritischen Pfad entstehen. Nach Messung der `Server-Timing`- und Pool-Werte ist das nächste große Architekturprojekt ein strikt cachebasierter Read-Pfad mit kontrollierter asynchroner Aktualisierung. Damit dürfen aktuelle Rangwerte aber nicht stillschweigend verschwinden; hierfür separates Integrationsdesign.
4. **Database Pool:** Der Code-Default ist `max_connections=8`, `acquire_timeout=10s`. Nicht erhöhen, bevor Pool-Wartezeiten, DB-CPU, aktive Queries und Konflikte gemessen wurden.
5. **Browser-Nutzbarkeit:** Die derzeitige Preview-Startseite liefert ein Hauptbundle von ca. **76 KB gzip**, React ca. **66 KB gzip** und globale CSS ca. **22 KB gzip**, dazu routeabhängige Chunks. Daher können 50 ms als Ziel für einen vollständigen Kaltstart auf mobilen Netzen nicht garantiert werden. **LCP, INP, JS-Ausführungszeit, Font-Paint und Navigationszeit** separat per Browser-RUM messen.
6. **Cache/Datenschutz:** Öffentliche statische Assets dürfen lang gecacht werden. Live-Drafts, Admin- und User-Daten dürfen niemals über denselben Shared-Cache-Pfad laufen.

## 5. Definition einer belastbaren Freigabe

- **Ideal:** p95 unter 50 ms bei kleinen, warmen API-Requests.
- **Gut:** p95 unter 100 ms bei typischen öffentlichen API-Requests.
- **Akzeptabel:** p95 unter 300 ms bei den komplexen Routen unter der vereinbarten Last.
- **Nicht als dieselbe Zahl zusammenrechnen:** Rust-Verarbeitungszeit, Caddy/TLS-Aufbau, Client-Netz und Browser-Interaktion.
- Vor Produktiv-Rollout: Staging-Rust mit separater, repräsentativer SQL-Testdatenbank, 1/4/8/16-Client-Vergleich, echte eingeloggte Admin-Ansicht und externe Browser-Messung.
- **PR #34 bleibt ungemerged**. Kein Wechsel des Stacks, keine Produktionstests mit ungebremster Last, keine Änderung an live laufenden Diensten.
