# Token storage boundary

For work on authentication and access, `docs/token-storage-db.md` applies. Existing repository rules remain in force. Do not create new token files; store account credentials only encrypted in the DB and pure session or one-time values as a lookup hash. Infrastructure keys stay in the existing secret manager.

# Code review and task flow

Every change follows `CODE-REVIEW.md`: deterministic checks first, one deep review
with fixed fan-out, only high findings block, findings frozen after round 1, then
verification only. It also contains the orchestrator start prompt and the
20-minute default-decision rule. Work on your own branch; only the maintainer
merges to `main`.
