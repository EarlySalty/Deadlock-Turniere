# Solo-balanced tournaments

## Admin workflow

Open Admin, select a live tournament, then open `Teilnehmer`. During check-in, `Teams ausgleichen` resolves the rank batch and shows a proposal. The optional team count limits capacity; blank means as many complete teams as possible. `Teams übernehmen` confirms the unchanged proposal atomically. A changed signup, resolver-visible rank or priority requires a new preview.

Existing multi-player or empty teams block balancing. Clean one-player teams linked to solo signups can be replaced; teams with tournament history, applications or invitations are protected. A tournament can apply balancing once.

Players are selected by previous waitlist priority, then signup timestamp and signup ID. Ranked players from the entire signup pool determine the median, including players outside capacity. Scores 0 and 3 denote unranked players. An even median keeps its half point. With no ranked players, the neutral score is 0 for everyone. Snake distribution is followed by strictly gap-reducing pairwise swaps; this is a deterministic local optimum, not a claim of global optimization.

The proposal exposes effective scores separately from the resolved rank scores stored on members and signups. The existing resolver's configured L1/L2 cache lifetime remains in force; balancing does not force a remote rank fetch. Rank cache maintenance by the resolver may occur during a preview; preview does not write tournament, team, member, signup or waitlist data.

Waitlisted signups stay unassigned. Random assignment and check-in finalization exclude them from the solo pool. Admins can still assign a waitlisted signup manually. A later manual assignment removes their eligibility for waitlist priority. The next applied solo-balanced tournament uses the immediately preceding balance run in the same test/live category. Players must sign up again to receive priority.

## API

All three routes require the existing admin authentication:

- `POST /api/admin/tournaments/{id}/solo-balance/preview`, body `{ "team_count": null }` or a positive integer.
- `POST /api/admin/tournaments/{id}/solo-balance/apply`, body `{ "team_count": 2, "preview_token": "<hash returned by preview>" }`.
- `GET /api/admin/tournaments/{id}/solo-balance/waitlist`, persisted, still-unassigned waitlist entries.

## Integration prerequisite

The migration is in the Deadlock-Bots companion branch `feat/turnier-solo-balance-db`, commit `341159fb`, at `rust/crates/dl-central-db/migrations/20261011120000_turnier_solo_balance.sql`. Integrate and apply it with a freshly rebuilt central migrator before deploying this branch. The tournament service's `run_migrations` does not apply production migrations.

Pre-integration verification uses Cargo's native path override to this clean companion worktree, so throwaway test databases receive the exact central migration. No shared checkout is modified.
