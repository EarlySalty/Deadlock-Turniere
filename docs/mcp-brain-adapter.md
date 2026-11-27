# Tournament MCP read adapter for the Brain

## Scope

`turnier-api` exposes a read-only MCP endpoint (JSON-RPC 2.0, protocol `2025-06-18`) so the Deadlock Brain can answer tournament questions from live data.

- Route: `POST /internal/turnier/v1/mcp`
- Code: `rust/crates/turnier-api/src/mcp.rs`, tests in `rust/crates/turnier-api/tests/mcp_routes.rs`
- Methods: `initialize`, `ping`, `tools/list`, `tools/call`. Messages without `id` are notifications and get `202 Accepted`. Request IDs must be a number, null or a string of at most 128 bytes; any other ID gets error `-32600` `Ungültige Request-ID` with `id: null`. Bodies above 64 KiB get `413`.

## Authentication

The route uses the existing internal boundary (`require_internal_boundary`, same as the scrim internal API):

- The peer must be loopback, otherwise `403`.
- `X-Internal-Token` must equal `turnier_internal_api_token`, otherwise `401`. An empty configured token rejects every call.
- The check runs before the body is read. User sessions (Bearer or cookie) do not grant access.

No new secret is introduced. Callers use the existing `TURNIER_INTERNAL_API_TOKEN` from Infisical.

## Tools

All tools carry `readOnlyHint: true`. Input schemas reject extra fields.

| Tool | Arguments | Result |
| --- | --- | --- |
| `turnier_list` | none | up to 50 public tournaments, newest first (by `created_at`, then ID) |
| `turnier_get` | `tournament_id` | tournament summary, rules, teams, solo signup count |
| `turnier_bracket` | `tournament_id` | bracket matches with team names, winner, series score from `match_games`, schedule, stream flag |
| `turnier_groups` | `tournament_id` | groups with standings and games |

## Visibility and data minimisation

- Only tournaments with `status != draft` and `is_test = false` are visible. Draft, test, foreign and missing IDs all return the same tool error `Turnier nicht gefunden`.
- No Discord IDs, Steam IDs, member lists, signup names, party codes, Steam party IDs, match stats, hero assignments, lobby settings, reminders or `created_by` leave the adapter.
- Team names can contain a person's name, because auto-formed teams are named `<captain name> Team` and keep that name when the captain changes. A team name is replaced by `Team <id>` when any of these holds:
  - the captain or any current member is a guest,
  - the name contains, case-insensitively, the trimmed name of a guest who signed up for or plays in the tournament, for guest names of at least 2 characters,
  - the name has the auto-formed shape `<name> Team` (optionally with a `(n)` suffix) or has at least 27 characters after trimming (within 5 of the 32-character limit, so it may be a truncated auto-name), and no current non-guest member of that team matches it. A member matches when `<name>` equals the member's name, or when the name has at least 27 characters and is a prefix of `<member name> Team`. All comparisons are trimmed and case-insensitive. A truncated auto-name that also carries a `(n)` suffix matches neither test and is always masked.
- A team auto-named after a current non-guest member (for example `Nova Team`, `Nova Team (2)`, or a truncated `<member> Team` without suffix) keeps its name, as on the public site, so that name contains that member's Discord name.
- Group standings come sorted with a `rank` field: league tournaments use the league table, all others sort by points, then wins, then group entry order (`group_teams.id`), like the site.
- Group games carry a `round`. League games keep the round from their schedule. For all other groups the round is derived like the site's round-robin view: the circle method over the group's teams in entry order, a match placed in the round of its team pair, and games that fit no pair put in one extra round after the filled rounds. Games come sorted by round, then game ID; within a round the site lists them in pair order instead.
- Solo signups are reported as a count only.
- Database failures return `Turnierdaten gerade nicht verfügbar`. The log line holds only the tool name.
- Tournament names, descriptions, rules, group names and unmasked team names are organizer or captain input and pass through as on the public site. The Brain must treat tool output as data, not instructions.

## Not ported from the WIP adapter

The WIP adapter (`sicherung/turniere-main-wip-20261125`, commit `77d6f47`) also had member tools (own status, signup, check-in) and moderator tools (create, update, advance). They are not ported:

- They need a per-user identity from the Brain. No broker exists that forwards a verified Discord user to this service, so role checks and revocation could not be proven.
- Mutating tools are out of scope without that proof.

## Open items

- The Brain needs a client for `http://127.0.0.1:8900/internal/turnier/v1/mcp` that sends `X-Internal-Token`. That wiring belongs in the Deadlock-Brain repository.
- Member and moderator tools wait for a per-user broker.
