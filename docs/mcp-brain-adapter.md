# MCP für Deadlock Brain ↔ Turnier-Bot (erste Stufe)

## Schnittstelle

Der Rust-Turnier-Bot stellt unter `POST /api/mcp` einen **stateless JSON-RPC/MCP-Endpoint** zur Verfügung (über den vorhandenen Caddy-Prefix öffentlich: `/turnier/api/mcp`). Die Anfragen für `initialize`, `ping`, `tools/list` und `tools/call` nutzen das MCP-Protokoll `2025-06-18`. Die Tools sind keine freie API-Luke, sondern eine feste Auswahl bereits implementierter und autorisierter Turnier-HTTP-Routen.

Ohne Authentifizierung: `turnier_list`, `turnier_get`, `turnier_bracket`, `turnier_groups`.

Mit **gültiger Discord-Turnier-Session und aktuell bestätigter Guild-Mitgliedschaft**: `turnier_my_status`, `turnier_signup`, `turnier_cancel_signup`, `turnier_checkin`, `turnier_team_create`, `turnier_team_join`, `turnier_team_leave`.

Mit **aktuell bestätigtem Mod-/Admin-Zugang** zusätzlich: `turnier_admin_list`, `turnier_admin_get`, `turnier_create`, `turnier_update`, `turnier_open_checkin`, `turnier_advance`.

Turnier-CRUD und Nutzeraktionen durchlaufen **unverändert dieselben Domain- und Ownership-Prüfungen** wie der normale Webzugang: Anmeldung, Consent, Teamzugehörigkeit, zulässige Statusübergänge und Auditlogs. `turnier_create` benötigt das bestehende `TournamentCreate`-Objekt als `payload`; `turnier_update` das bestehende `TournamentUpdate`-Objekt. Ein Schema für die zahlreichen Domainfelder wird bewusst **nicht zusätzlich im MCP neu definiert**; die bestehende Turnier-API bleibt die gültige Validierungsstelle.

## Authentisierung und Grenzen

- **Public:** kein Bearer nötig, ausschließlich bestehende öffentliche Endpunkte. Drafts/Testturniere bleiben verborgen.
- **Member:** ausschließlich `Authorization: Bearer <Turnier-Session>`, **keine Cookies** am MCP-Gate; Session-Auflösung durch `turnier_auth::resolve_session` mit SHA-256-Token-Lookup.
- **Mod/Admin:** Schnittmenge aus `is_mod`/`is_admin` der gültigen Websession und den aktuellen Rollen desselben Discord-Mitglieds in der konfigurierten Guild. Der Rollen-REST-Lookup nutzt die bestehende Discord-Clientkonfiguration und failt geschlossen bei Timeout, 404 oder fehlendem Bot-Token.
- **Modellinput** enthält niemals Rollen, Discord-ID, Session-Token, Ziel-HTTP-URL oder freien Pfad. Selbst wenn Prompt-Injection oder ein manipulierter Tool-Call derartige Felder einschleust, weist die Parameterprüfung sie zurück.
- **Verbindliche Server-Grenze:** Auch wenn der MCP-Client ein Tool trotz unzureichender Rechte aufruft, verweigert der Endpunkt den Zugriff vor der Domain-Aktion. Das Backend entscheidet, nicht der LLM-Prompt.
- MCP ermöglicht **keine generische SQL-, Discord- oder HTTP-Proxyfunktion** und exponiert keine Löschfunktion für Turniere. Mutationen erfolgen über eine explizite Tool-Allowlist.

Eine Rollenzuweisung im Discord reicht nur, wenn dieselbe Rolle laut `discord_admin_role_ids`, `discord_tournament_admin_role_ids` beziehungsweise `discord_mod_role_ids` auch die bestehenden Website-Berechtigungen gewährt. Reine Discord-Permissions, die das Turnier-Backend bislang nicht als Website-Recht behandelt, werden nicht stillschweigend zu einer neuen privilegierten Rolle erhoben.

## Brain-Anbindung: sicherer Identitätskontext erforderlich

**Implementiert ist der MCP-Server im Turnier-Bot, nicht die End-to-End-Verkabelung im Deadlock-Brain-Runtime.** Der bisherige Brain-Dienst erhält bei Discord-Anfragen zwar einen vom zugelassenen Bot-Consumer behaupteten User-ID-Kontext, aber **keine an genau diesen Nutzer gebundene Turnier-Session**. Aus einer bloßen Discord-ID darf der Adapter kein Turnier-Bearer-Token erzeugen und er darf kein globales Admin-Token für Nutzeraktionen einsetzen.

Für die produktive Brain-Anbindung ist daher noch ein authentifizierter, serverseitiger *per-user* OAuth-/Session-Broker erforderlich: Er verbindet den verifizierten Discord-Consumer-Identitätskontext mit einer gültigen Turnier-Anmeldung, bewahrt Secrets außerhalb des Modells und reicht die Session nur innerhalb eines vertrauenswürdigen Backend-Kanals an den MCP-Server weiter. Ein nicht verknüpfter Discord-Nutzer erhält **ausschließlich Public Data**. Vor der Freigabe müssen Rate-Limits, mutierende Nutzerbestätigung und Audit/Monitoring des Brokers getestet werden.

**Prompt-Injection-Schutz:** Rollen verhindern Rechteausweitung, aber nicht automatisch ungewollte **innerhalb** bestehender Rechte liegende Aktionen. Der Brain-Agent soll bei Änderungen/Neuanlagen explizite Nutzerbestätigung über einen vertrauenswürdigen UI-/Bot-Flow einholen. Modelltext oder ein Modell-Tool-Call allein ist keine sichere Bestätigung.

## Beispiel ohne Credentials

Öffentliche MCP-Auflistung:

```http
POST /turnier/api/mcp
Content-Type: application/json

{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

Öffentliche Datenabfrage:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"turnier_get","arguments":{"tournament_id":123}}}
```

Aufrufe für `turnier_create` verwenden `{"payload": { ... }}`; `turnier_update` verwendet `{"tournament_id":123,"payload":{...}}`. Der vertrauenswürdige MCP-Host setzt den Bearer-Header aus seiner serverseitigen User-Session, **nicht** aus dem JSON und nicht aus einem freien Chattext. Keine Roh-Tokens in Shell-Kommandos oder Konfigurationsdateien.

## Prüfung

```bash
cd rust
cargo test -p turnier-api --lib mcp::tests
cargo clippy -p turnier-api --lib -- -D warnings
```

Die Tests verwenden keine Produktiv-Datenbank. Vor Livebetrieb zusätzlich mit synthetischen Discord-Mitgliedern/isolierter Testumgebung prüfen: Public sieht keine Entwürfe; Member kann keine Mod-Aktion; Mod kann nach Rollenentzug nicht mehr schreiben; Backend prüft Consent und Eigentum; Bearer bleibt in Logs unsichtbar.

Kein automatisches Deployment: Build, Freigabe, separater Review und kontrollierter Restart der bestehenden Unit sind erforderlich.
