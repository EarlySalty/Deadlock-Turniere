# Architektur — Deadlock-Turniere (Rust)

Das Backend ist ein Cargo-Workspace aus kleinen Crates mit je einer klaren
Zuständigkeit. Die Abhängigkeiten zeigen strikt nach „unten" (keine Zyklen): die
Domänen-Crates kennen das Fundament, die Web-/App-Schicht kennt die Domäne.

## Schichten

```
                         tb-app  (Binary, Composition-Root)
                            │
                         tb-web  (axum: Router, Extractoren, Fehler-Mapping)
        ┌───────────────────┼─────────────────────────────┐
   tb-scheduler          tb-match        tb-tournament   tb-draft
   (Loop/Phasen)        (Lobby/Result)   (Engine/Punkte) (Pick/Ban)
        └─────────┬─────────┴───────┬───────────┴────────────┘
              tb-discord        tb-steam        tb-auth
              (Broker)          (Ränge)         (Session/RBAC)
                            │
            tb-core   ·   tb-config   ·   tb-db          (Fundament)
```

- **tb-core** — Domänen-Enums + Wire-DTOs (1:1 zu den Pydantic-Modellen). Kein I/O.
- **tb-config** — geschichtete Konfiguration (Datei → Env → Default) + Ableitungen
  (Rollen-Sets, CORS, allowed_hosts).
- **tb-db** — `SqlitePool`, PRAGMA-Setup (WAL/FK/busy_timeout), konsolidierte
  Migration (aus der Live-DB generiert), Fehler-Typ.
- **tb-auth** — opake Session-Tokens (kein JWT), RBAC (`User < Mod < Admin`),
  delegierter OAuth-Client gegen den Master-Broker.
- **tb-steam** — dreistufiger Rang-Resolver (Cache → Steam-Bridge → Discord-Rollen);
  EINE `rank_score`-Formel als Quelle der Wahrheit.
- **tb-discord** — Master-Broker-Client (Channels, Embeds, DMs, Voice), entkoppelt
  von axum; `discord_tasks`-Queue.
- **tb-tournament** — die Engine: Bracket-Generierung (Single/Double-Elim),
  Seeding, Mini-Groups, Standings, Status-Übergänge, idempotente Punkte. Strikt
  getrennt: reine Algorithmen (`engine/`) vs. sqlx-Persistenz (`persist/`).
- **tb-match** — Match-Lebenszyklus: Lobby, Ergebnisverarbeitung (Bracket+Group
  vereinheitlicht), Bo-N-Serien, Spielmodi, Auto-Lobby, Steam-Bridge-Queue.
  `MatchKind`-Enum + Repository statt stringly-typed `match_type`.
- **tb-draft** — Pick/Ban-Zustandsmaschine + Repository (CAS auf den Aktionsindex).
- **tb-scheduler** — Hintergrund-Loop (Phasenübergänge + Reminder) und die geteilte
  Orchestrierung `advance_tournament_status` (auch von tb-web genutzt).
- **tb-web** — axum-HTTP-Schicht: `AppState`, Extractoren, Fehler→Response,
  Middleware (CORS, TrustedHost) und die Router aller ~213 Endpunkte.
- **tb-app** — Composition-Root + Binary: Config → Pool → Migration → AppState →
  Scheduler → `axum::serve`. Mit `--check` bootet der Prozess vollständig, ohne zu
  servieren (Smoke-Test).

## Request-Lebenszyklus (tb-web)

1. **Middleware**: TrustedHost prüft den `Host`-Header gegen `allowed_hosts`; CORS
   spiegelt Methoden/Header mit `allow_credentials`.
2. **Extractor**: `AuthUser`/`ModUser`/`AdminUser` lesen das Token (Bearer-Header
   vor Cookie `session_token`), lösen die Session über `tb_auth::resolve_session`
   auf und setzen die RBAC-Flags. Public-Routen nutzen keinen Extractor.
3. **Handler**: ruft die Domänen-Crates (kein Geschäftslogik-Code in tb-web),
   formt die Antwort als Wire-DTO oder ad-hoc-JSON.
4. **Fehler**: jeder Domänenfehler wird über `?` in einen `WebError` mit passendem
   Status übersetzt; die Response-Form ist FastAPI-kompatibel (`{"detail": …}`).
   Wo eine Route einen abweichenden Status braucht, konstruiert sie den `WebError`
   explizit (per-Route-Mapping wie im Original).

## Geteilter Zustand (`AppState`)

`AppState` hält billig-klonbare Handles: den `SqlitePool`, die `Arc<Config>`, die
`RoleSets`, den `OAuthClient`, den `Arc<MatchManager>`, den `Arc<dyn RankResolver>`
und den `Arc<DiscordNotifier>`. Effekt-Dienste (Discord/Steam) degradieren sauber,
wenn sie nicht konfiguriert sind.

## Datenbank

Rust und der Python-Stand teilen sich dieselbe SQLite-Datei
(`backend/data/tournament.db`). Das Schema ist der Vertrag (siehe
[`db-contract.md`](db-contract.md)); die Migration ist 1:1 aus dem effektiven
Live-Schema generiert und idempotent (No-op auf der bestehenden DB).

## Persistenz-Stil

Laufzeit-geprüfte sqlx-Queries (`query`/`query_as` + `FromRow`), keine compile-
time-Makros (siehe [`adr/0002`](adr/0002-runtime-checked-sqlx.md)). Mutierende
Operationen laufen in einer `sqlx::Transaction`.
