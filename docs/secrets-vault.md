# Secrets on Linux and Vault rollout

This project now supports these configuration sources in this order:

1. `<NAME>_FILE`
2. files inside `CREDENTIALS_DIRECTORY`, `SECRETS_DIRECTORY`, or `VAULT_SECRETS_DIR`
3. plain environment variable `<NAME>`
4. optional local keyring fallback

That means the backend is no longer tied to Windows Credential Manager and can be
fed by `systemd` credentials, plain secret files, or files rendered by Vault Agent.

## Recommended target setup

For production on Linux:

1. Keep Vault as the source of truth.
2. Use Vault Agent to authenticate on the host and render secrets to files.
3. Start the backend with `systemd`.
4. Pass secret file paths or `systemd` credentials to the process.

This keeps secret values out of the repository and avoids using plain environment
variables for the secret content itself.

## Phase plan

### Phase 1: Project becomes Linux-ready

Done in this repo:

- backend config reads secrets from files
- `_FILE` convention is supported
- `systemd` credentials directory is supported
- Windows keyring remains an optional fallback for local/dev

### Phase 2: Single Linux host without Vault

Good first production step:

1. Store secrets in `/etc/deadlock-turniere/secrets/`
2. Limit access to root and the service user
3. Point the service at these files via `LoadCredential=` or `*_FILE`

### Phase 3: Vault integration

Recommended rollout:

1. Deploy Vault or use an existing Vault cluster.
2. Create one policy for this project only.
3. Mount secrets at a path such as `kv/data/deadlock-turniere/prod`.
4. Run Vault Agent on the host.
5. Let Vault Agent render secret files into `/run/deadlock-turniere-secrets/`.
6. Start or reload the backend through `systemd`.

## Example secret names

Suggested keys for this project:

- `DISCORD_CLIENT_ID`
- `DISCORD_CLIENT_SECRET`
- `JWT_SECRET`
- `DISCORD_WEBHOOK_URL`
- `STEAM_BRIDGE_DB_PATH`

Non-secret settings can stay in normal config:

- `BACKEND_PORT`
- `FRONTEND_URL`
- `DATABASE_PATH`
- role ids and guild ids

## Example Vault layout

Path:

`kv/data/deadlock-turniere/prod`

Fields:

- `DISCORD_CLIENT_ID`
- `DISCORD_CLIENT_SECRET`
- `JWT_SECRET`
- `DISCORD_WEBHOOK_URL`
- `STEAM_BRIDGE_DB_PATH`

## systemd options

Two clean runtime patterns are supported.

### Option A: systemd credentials

Best when you already have secret files on disk:

```ini
LoadCredential=DISCORD_CLIENT_ID:/etc/deadlock-turniere/secrets/discord_client_id
LoadCredential=DISCORD_CLIENT_SECRET:/etc/deadlock-turniere/secrets/discord_client_secret
LoadCredential=JWT_SECRET:/etc/deadlock-turniere/secrets/jwt_secret
```

The backend automatically reads files from `$CREDENTIALS_DIRECTORY`.

### Option B: Vault Agent rendered files

Best when Vault Agent writes rotating files to a runtime directory:

```ini
Environment=DISCORD_CLIENT_ID_FILE=/run/deadlock-turniere-secrets/DISCORD_CLIENT_ID
Environment=DISCORD_CLIENT_SECRET_FILE=/run/deadlock-turniere-secrets/DISCORD_CLIENT_SECRET
Environment=JWT_SECRET_FILE=/run/deadlock-turniere-secrets/JWT_SECRET
Environment=STEAM_BRIDGE_DB_PATH_FILE=/run/deadlock-turniere-secrets/STEAM_BRIDGE_DB_PATH
```

This keeps only file paths in the service environment, not the secret values.

## Operational notes

- `JWT_SECRET` must be persistent. Do not rely on the ephemeral fallback in production.
- If the Steam bot remains on Windows, do not point Linux at a SQLite database over a network share.
- If the Steam bridge also moves to Linux, keep both services on the same host or replace the SQLite queue with a proper service boundary later.
