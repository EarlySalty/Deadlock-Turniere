# Architektur — Deadlock-Turniere (Rust)

Das Backend ist ein Cargo-Workspace aus kleinen Crates mit je einer klaren
Zuständigkeit. Die Abhängigkeiten zeigen strikt nach „unten" (keine Zyklen): die
Domänen-Crates kennen das Fundament, die Web-/App-Schicht kennt die Domäne.

## Schichten

```
                         turnier-bot  (Binary, Composition-Root)
                            │
                         turnier-api  (axum: Router, Extractoren, Fehler-Mapping)
        ┌───────────────────┼─────────────────────────────┐
   turnier-scheduler          turnier-match        turnier-engine   turnier-draft
   (Loop/Phasen)        (Lobby/Result)   (Engine/Punkte) (Pick/Ban)
        └─────────┬─────────┴───────┬───────────┴────────────┘
              turnier-discord        turnier-steam        turnier-auth
              (Broker)          (Ränge)         (Session/RBAC)
                            │
            turnier-core   ·   turnier-config   ·   turnier-db          (Fundament)
```

- **turnier-core** — Domänen-Enums, Wire-DTOs (1:1 zu den Pydantic-Modellen)
  und Grenzkonventions-Helfer fuer Discord-ID-, Zeit- und JSONB-Mapping.
- **turnier-config** — geschichtete Konfiguration (Datei → Env → Default) + Ableitungen
  (Rollen-Sets, CORS, allowed_hosts).
- **turnier-db** — `PgPool`-Foundation zur zentralen Postgres/TimescaleDB ueber
  `dl-central-db`, Fehler-Typ, Testpool-Adapter und Whitelist-Anker fuer
  erlaubtes dynamisches SQL. Produktive Migrationen laufen zentral ueber
  `dl-central-migrate`, nicht durch diese Crate.
- **turnier-auth** — opake Session-Tokens (kein JWT), RBAC (`User < Mod < Admin`),
  delegierter OAuth-Client gegen den Master-Broker.
- **turnier-steam** — dreistufiger Rang-Resolver (Cache → Steam-Bridge → Discord-Rollen);
  EINE `rank_score`-Formel als Quelle der Wahrheit.
- **turnier-discord** — Master-Broker-Client (Channels, Embeds, DMs, Voice), entkoppelt
  von axum; `discord_tasks`-Queue.
- **turnier-engine** — die Engine: Bracket-Generierung (Single/Double-Elim),
  Seeding, Mini-Groups, Standings, Status-Übergänge, idempotente Punkte. Strikt
  getrennt: reine Algorithmen (`engine/`) vs. sqlx-Persistenz (`persist/`).
- **turnier-match** — Match-Lebenszyklus: Lobby, Ergebnisverarbeitung (Bracket+Group
  vereinheitlicht), Bo-N-Serien, Spielmodi, Auto-Lobby, Steam-Bridge-Queue.
  `MatchKind`-Enum + Repository statt stringly-typed `match_type`.
- **turnier-draft** — Pick/Ban-Zustandsmaschine + Repository (CAS auf den Aktionsindex).
- **turnier-scheduler** — Hintergrund-Loop (Phasenübergänge + Reminder) und die geteilte
  Orchestrierung `advance_tournament_status` (auch von turnier-api genutzt).
- **turnier-api** — axum-HTTP-Schicht: `AppState`, Extractoren, Fehler→Response,
  Middleware (CORS, TrustedHost) und die Router aller ~213 Endpunkte.
- **turnier-bot** — Composition-Root + Binary: Config → zentraler PgPool →
  AppState → Scheduler → `axum::serve`. Mit `--check` baut der Prozess Config,
  Pool, AppState, Scheduler und Router, ohne zu servieren oder Scheduler-Checks
  auszufuehren (Smoke-Test).

## Request-Lebenszyklus (turnier-api)

1. **Middleware**: TrustedHost prüft den `Host`-Header gegen `allowed_hosts`; CORS
   spiegelt Methoden/Header mit `allow_credentials`.
2. **Extractor**: `AuthUser`/`ModUser`/`AdminUser` lesen das Token (Bearer-Header
   vor Cookie `session_token`), lösen die Session über `turnier_auth::resolve_session`
   auf und setzen die RBAC-Flags. Public-Routen nutzen keinen Extractor.
3. **Handler**: ruft die Domänen-Crates (kein Geschäftslogik-Code in turnier-api),
   formt die Antwort als Wire-DTO oder ad-hoc-JSON.
4. **Fehler**: jeder Domänenfehler wird über `?` in einen `WebError` mit passendem
   Status übersetzt; die Response-Form ist FastAPI-kompatibel (`{"detail": …}`).
   Wo eine Route einen abweichenden Status braucht, konstruiert sie den `WebError`
   explizit (per-Route-Mapping wie im Original).

## Geteilter Zustand (`AppState`)

`AppState` hält billig-klonbare Handles: den `PgPool`, die `Arc<Config>`, die
`RoleSets`, den `OAuthClient`, den `Arc<MatchManager>`, den `Arc<dyn RankResolver>`
und den `Arc<DiscordNotifier>`. Effekt-Dienste (Discord/Steam) degradieren sauber,
wenn sie nicht konfiguriert sind.

## Datenbank

Das Rust-Backend nutzt als Zielzustand die zentrale Postgres/TimescaleDB. Die
fachlichen Turnierdaten liegen in `turnier.*`; zentrale Cross-Schema-Lookups
nutzen explizit qualifizierte Tabellen wie `core.*` oder `voice.*`. Das Schema
ist der Vertrag (siehe [`db-contract.md`](db-contract.md)); produktive
Migrationen besitzt `dl-central-db` im Schwesterrepo.
`DEADLOCK_CENTRAL_DSN` ist fuer den Rust-Backend-Start Pflicht. `DATABASE_PATH`
ist nur noch Python-/SQLite-Legacy und wird vom Rust-Backend ignoriert.

Python unter `backend/` ist Legacy-Flaeche und kein stiller produktiver
SQLite-Schreibpfad fuer den Rust-Cutover; Python-Starts sind nur noch explizite
Rollback-/Dev-Starts. Die Steam-Bridge-SQLite-Flaeche wird separat in SP4/T6
entfernt oder abgegrenzt.

## Persistenz-Stil

Statische PG-Queries sollen compile-checked sein
(`sqlx::query!`/`query_as!` + `rust/.sqlx`-Offline-Cache; siehe
[`adr/0002`](adr/0002-runtime-checked-sqlx.md)). Runtime-/Builder-SQL bleibt nur
fuer echte dynamische Struktur erlaubt: variable `IN`-Listen,
Reminder-Dedupe-Tabellennamen aus Whitelist und Patch-Update-Builder mit
statischer Spalten-Whitelist. Mutierende Operationen laufen in einer
`sqlx::Transaction<'_, Postgres>` oder ueber einen kompatiblen PG-Executor.
