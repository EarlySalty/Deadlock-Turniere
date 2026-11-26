# Token storage boundary

For work on authentication and access, `docs/token-storage-db.md` applies. Existing repository rules remain in force. Do not create new token files; store account credentials only encrypted in the DB and pure session or one-time values as a lookup hash. Infrastructure keys stay in the existing secret manager.

# Code review and task flow

Every task follows `CODE-REVIEW.md`: one orchestrator session per task, Opus at
medium effort (high only for risky areas), deterministic checks first, one
`/code-review` run plus one confirmation run, then one pull request with the
review result in its description. It also contains the orchestrator start prompt
and the 20-minute default-decision rule. Never push to `main`; only the
maintainer merges.
